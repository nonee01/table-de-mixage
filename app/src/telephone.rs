//! Le micro du téléphone par USB : le serveur de scrcpy 5.0.1 envoie le micro brut
//! (s16le stéréo 48 kHz, sans en-têtes) sur une socket que `adb forward` ramène ici.

use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ringbuf::traits::Producer;
use ringbuf::HeapProd;

const VERSION_SCRCPY: &str = "5.0.1"; // doit être celle du fichier scrcpy-server
const JAR: &str = "/data/local/tmp/scrcpy-server.jar";

fn a_cote_de_l_exe(nom: &str) -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("telephone").join(nom)).filter(|p| p.is_file())
}

pub fn trouver_adb() -> Option<PathBuf> {
    let nom = format!("adb{}", std::env::consts::EXE_SUFFIX);
    a_cote_de_l_exe(&nom).or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&nom)).find(|p| p.is_file())
    })
}

pub fn trouver_serveur() -> Option<PathBuf> {
    a_cote_de_l_exe("scrcpy-server").or_else(|| Some(PathBuf::from("/usr/share/scrcpy/scrcpy-server")).filter(|p| p.is_file()))
}

fn commande(adb: &Path) -> Command {
    let mut cmd = Command::new(adb);
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW : pas de console qui clignote
    }
    cmd
}

fn adb_sortie(adb: &Path, args: &[&str]) -> Result<String, String> {
    let o = commande(adb).args(args).output().map_err(|e| format!("adb introuvable: {e}"))?;
    if !o.status.success() {
        return Err(format!("adb {} a échoué", args.first().unwrap_or(&"")));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Le premier appareil autorisé sous Android 11 ou plus (la capture audio de scrcpy l'exige).
fn choisir_appareil(adb: &Path) -> Result<String, String> {
    let liste = adb_sortie(adb, &["devices"])?;
    let mut non_autorise = false;
    let mut trop_ancien = false;
    for ligne in liste.lines().skip(1) {
        let mut it = ligne.split_whitespace();
        match (it.next(), it.next()) {
            (Some(s), Some("device")) => {
                let sdk = adb_sortie(adb, &["-s", s, "shell", "getprop", "ro.build.version.sdk"])?;
                if sdk.trim().parse::<u32>().unwrap_or(0) >= 30 {
                    return Ok(s.to_string());
                }
                trop_ancien = true;
            }
            (Some(_), Some("unauthorized")) => non_autorise = true,
            _ => {}
        }
    }
    Err(if non_autorise {
        "Autorisez le débogage USB sur le téléphone".into()
    } else if trop_ancien {
        "Android 11 ou plus requis".into()
    } else {
        "Aucun téléphone branché en USB".into()
    })
}

/// s16le stéréo entrelacé vers mono f32.
pub fn pcm_vers_mono(octets: &[u8]) -> Vec<f32> {
    octets
        .chunks_exact(4)
        .map(|t| {
            let g = i16::from_le_bytes([t[0], t[1]]) as f32;
            let d = i16::from_le_bytes([t[2], t[3]]) as f32;
            (g + d) / (2.0 * 32768.0)
        })
        .collect()
}

/// Lit le flux jusqu'à sa fin, puis marque le micro perdu.
fn lire(mut flux: impl Read, mut prod: HeapProd<f32>, paquet: Arc<AtomicUsize>, perdu: Arc<AtomicBool>) {
    let mut tampon = [0u8; 8192];
    let mut reste = 0;
    loop {
        match flux.read(&mut tampon[reste..]) {
            Ok(0) => break,
            Ok(n) => {
                let n = reste + n;
                let entier = n / 4 * 4;
                for v in pcm_vers_mono(&tampon[..entier]) {
                    let _ = prod.try_push(v);
                }
                paquet.store(entier / 4 + 1, Ordering::Relaxed);
                tampon.copy_within(entier..n, 0);
                reste = n - entier;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    perdu.store(true, Ordering::Relaxed);
}

/// Port de l'appli « Micro table » (telephone/ dans ce dépôt).
pub const PORT_APPLI: u16 = 5005;

pub struct Telephone {
    /// adb, numéro de série, port local, serveur scrcpy : seulement par USB.
    usb: Option<(PathBuf, String, String, Child)>,
    socket: TcpStream,
}

impl Telephone {
    /// Lance le serveur sur le téléphone et envoie son micro dans `prod`. Rend la main en 5 s au plus.
    pub fn demarrer(
        adb: &Path,
        serveur: &Path,
        prod: HeapProd<f32>,
        paquet: Arc<AtomicUsize>,
        perdu: Arc<AtomicBool>,
    ) -> Result<Telephone, String> {
        let serie = choisir_appareil(adb)?;
        let s = serie.as_str();
        // le serveur efface son jar au démarrage : on le renvoie à chaque fois
        adb_sortie(adb, &["-s", s, "push", &serveur.to_string_lossy(), JAR])?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1);
        let scid = format!("{:08x}", nanos & 0x7fff_ffff);
        let port = adb_sortie(adb, &["-s", s, "forward", "tcp:0", &format!("localabstract:scrcpy_{scid}")])?.trim().to_string();
        let enlever_forward = || {
            let _ = adb_sortie(adb, &["-s", s, "forward", "--remove", &format!("tcp:{port}")]);
        };
        let mut serveur = match commande(adb)
            .args(["-s", s, "shell", &format!("CLASSPATH={JAR}"), "app_process", "/", "com.genymobile.scrcpy.Server", VERSION_SCRCPY])
            .args([format!("scid={scid}").as_str(), "tunnel_forward=true", "video=false", "control=false", "audio=true"])
            .args(["audio_source=mic-unprocessed", "audio_codec=raw", "raw_stream=true"])
            .stdout(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                enlever_forward();
                return Err(format!("adb introuvable: {e}"));
            }
        };

        // adb accepte la connexion avant que le serveur écoute, puis la ferme : on attend le premier octet
        let limite = Instant::now() + Duration::from_secs(5);
        let socket = loop {
            let reste = limite.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                let _ = serveur.kill();
                enlever_forward();
                return Err("Le téléphone n'envoie pas de son".into());
            }
            if let Ok(sock) = TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap_or(0))) {
                let _ = sock.set_read_timeout(Some(reste));
                if matches!(sock.peek(&mut [0u8; 1]), Ok(n) if n > 0) {
                    let _ = sock.set_read_timeout(None);
                    break sock;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let lecteur = socket.try_clone().map_err(|e| e.to_string())?;
        std::thread::spawn(move || lire(lecteur, prod, paquet, perdu));
        Ok(Telephone { usb: Some((adb.to_path_buf(), serie, port, serveur)), socket })
    }

    /// Se connecte à l'appli « Micro table » du téléphone par le Wi-Fi (le PC appelle : pas de pare-feu à ouvrir).
    /// `saisie` = « adresse code », tels qu'affichés sur le téléphone.
    pub fn wifi(saisie: &str, prod: HeapProd<f32>, paquet: Arc<AtomicUsize>, perdu: Arc<AtomicBool>) -> Result<Telephone, String> {
        use std::net::ToSocketAddrs;
        use std::io::Write;
        let mut mots = saisie.split_whitespace();
        let (Some(adresse), Some(code)) = (mots.next(), mots.next()) else {
            return Err("Tapez l'adresse et le code affichés sur le téléphone".into());
        };
        if code.len() != 6 || !code.bytes().all(|c| c.is_ascii_digit()) {
            return Err("Le code a 6 chiffres".into());
        }
        let cible = if adresse.contains(':') { adresse.to_string() } else { format!("{adresse}:{PORT_APPLI}") };
        let sa = cible.to_socket_addrs().ok().and_then(|mut it| it.next()).ok_or("Adresse illisible")?;
        let socket = TcpStream::connect_timeout(&sa, Duration::from_secs(3))
            .map_err(|_| "Téléphone injoignable : appli démarrée, même Wi-Fi ?".to_string())?;
        let _ = socket.set_nodelay(true);
        (&socket).write_all(code.as_bytes()).map_err(|e| e.to_string())?;
        // un mauvais code : le téléphone raccroche sans rien envoyer
        let _ = socket.set_read_timeout(Some(Duration::from_secs(3)));
        if !matches!(socket.peek(&mut [0u8; 1]), Ok(n) if n > 0) {
            return Err("Code refusé par le téléphone".into());
        }
        let _ = socket.set_read_timeout(None);
        let lecteur = socket.try_clone().map_err(|e| e.to_string())?;
        std::thread::spawn(move || lire(lecteur, prod, paquet, perdu));
        Ok(Telephone { usb: None, socket })
    }
}

impl Drop for Telephone {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both); // le serveur s'arrête quand sa socket se ferme
        if let Some((adb, serie, port, serveur)) = &mut self.usb {
            let _ = serveur.kill();
            let _ = adb_sortie(adb, &["-s", serie, "forward", "--remove", &format!("tcp:{port}")]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Observer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn telephone_adb_absent() {
        let (prod, _cons) = HeapRb::<f32>::new(16).split();
        let t0 = Instant::now();
        let r = Telephone::demarrer(Path::new("/nonexistent/adb"), Path::new("/nonexistent/srv"), prod, Default::default(), Default::default());
        assert!(r.is_err());
        assert!(t0.elapsed() < Duration::from_secs(1));
    }

    /// Avec un téléphone Android 11+ branché : `cargo test -p table-de-mixage telephone_vrai -- --ignored`
    #[test]
    #[ignore]
    fn telephone_vrai() {
        let (prod, cons) = HeapRb::<f32>::new(4 * 48_000).split();
        let perdu = Arc::new(AtomicBool::new(false));
        let (adb, srv) = (trouver_adb().unwrap(), trouver_serveur().unwrap());
        let t0 = Instant::now();
        let t = Telephone::demarrer(&adb, &srv, prod, Default::default(), perdu.clone()).unwrap();
        let ouvert = t0.elapsed();
        std::thread::sleep(Duration::from_secs(2));
        let n = cons.occupied_len();
        eprintln!("ouvert en {ouvert:?}, {n} échantillons en 2 s");
        assert!(ouvert < Duration::from_secs(5));
        assert!((90_000..=100_000).contains(&n), "{n}");
        assert!(!perdu.load(Ordering::Relaxed));
        let port = t.usb.as_ref().unwrap().2.clone();
        drop(t);
        std::thread::sleep(Duration::from_millis(500));
        assert!(perdu.load(Ordering::Relaxed));
        assert!(!adb_sortie(&adb, &["forward", "--list"]).unwrap().contains(&format!("tcp:{port}")));
    }

    /// Appli « Micro table » démarrée : `TDM_TELEPHONE="<ip> <code>" cargo test -p table-de-mixage telephone_wifi_vrai -- --ignored`
    #[test]
    #[ignore]
    fn telephone_wifi_vrai() {
        let (prod, cons) = HeapRb::<f32>::new(4 * 48_000).split();
        let perdu = Arc::new(AtomicBool::new(false));
        let t0 = Instant::now();
        let t = Telephone::wifi(&std::env::var("TDM_TELEPHONE").unwrap(), prod, Default::default(), perdu.clone()).unwrap();
        let ouvert = t0.elapsed();
        std::thread::sleep(Duration::from_secs(2));
        let n = cons.occupied_len();
        eprintln!("ouvert en {ouvert:?}, {n} échantillons en 2 s");
        assert!((75_000..=100_000).contains(&n), "{n}"); // le micro du téléphone s'ouvre à la connexion : ~200 ms de moins
        assert!(!perdu.load(Ordering::Relaxed));
        drop(t);
    }

    /// L'appli parle comme scrcpy : un faux téléphone local envoie 480 trames puis raccroche.
    #[test]
    fn telephone_wifi_recoit_puis_perdu() {
        let ecoute = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = ecoute.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut s, _) = ecoute.accept().unwrap();
            let mut code = [0u8; 6];
            s.read_exact(&mut code).unwrap();
            assert_eq!(&code, b"012345");
            s.write_all(&[0u8; 4 * 480]).unwrap();
        });
        let (prod, cons) = HeapRb::<f32>::new(4096).split();
        let perdu = Arc::new(AtomicBool::new(false));
        let t = Telephone::wifi(&format!("{adresse} 012345"), prod, Default::default(), perdu.clone()).unwrap();
        let limite = Instant::now() + Duration::from_secs(1);
        while !perdu.load(Ordering::Relaxed) && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(perdu.load(Ordering::Relaxed));
        assert_eq!(cons.occupied_len(), 480);
        drop(t);
        for mauvais in ["", "127.0.0.1", "127.0.0.1 12a456"] {
            assert!(Telephone::wifi(mauvais, HeapRb::<f32>::new(4).split().0, Default::default(), Default::default()).is_err());
        }
    }

    #[test]
    fn telephone_paquet_brut_vers_mono() {
        let mut o = Vec::new();
        for v in [16384i16, -16384, 32767, 32767] {
            o.extend_from_slice(&v.to_le_bytes());
        }
        let m = pcm_vers_mono(&o);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], 0.0);
        assert!((m[1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn telephone_fin_de_flux_marque_perdu() {
        // 480 trames coupées en morceaux de 7 octets : les trames à cheval doivent survivre
        let octets: Vec<u8> = (0..480).flat_map(|_| [0u8, 64, 0, 64]).collect();
        struct Morceaux(Vec<u8>, usize);
        impl Read for Morceaux {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                let n = 7.min(b.len()).min(self.0.len() - self.1);
                b[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
                self.1 += n;
                Ok(n)
            }
        }
        let (prod, cons) = HeapRb::<f32>::new(1024).split();
        let perdu = Arc::new(AtomicBool::new(false));
        lire(Morceaux(octets, 0), prod, Default::default(), perdu.clone());
        assert_eq!(cons.occupied_len(), 480);
        assert!(perdu.load(Ordering::Relaxed));
    }
}
