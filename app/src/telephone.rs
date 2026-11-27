//! Le micro du téléphone par USB : le serveur de scrcpy 5.0.1 envoie le micro brut
//! (s16le stéréo 48 kHz, sans en-têtes) sur une socket que `adb forward` ramène ici.

use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use moteur::voie::ReglagesVoie;
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

// Le lien Wi-Fi avec l'appli : trames `type: u8, longueur: u16 le, contenu`, décrites dans
// telephone/rust/src/lib.rs (les deux crates ne partagent pas de code : ces fonctions en sont la copie).

fn trame(genre: u8, contenu: &[u8]) -> Vec<u8> {
    let mut o = vec![genre];
    o.extend_from_slice(&(contenu.len() as u16).to_le_bytes());
    o.extend_from_slice(contenu);
    o
}

fn lire_trame(r: &mut impl Read) -> std::io::Result<(u8, Vec<u8>)> {
    let mut tete = [0u8; 3];
    r.read_exact(&mut tete)?;
    let mut contenu = vec![0u8; u16::from_le_bytes([tete[1], tete[2]]) as usize];
    r.read_exact(&mut contenu)?;
    Ok((tete[0], contenu))
}

/// Réglages de la voie du téléphone, tels que les trames `C` et `S` les portent.
fn voie_vers_octets(v: &ReglagesVoie) -> [u8; 10] {
    let mut o = [0u8; 10];
    o[0] = v.anti_bruit as u8 | (v.coupe_cri as u8) << 1 | (v.limiteur as u8) << 2;
    o[1] = v.apprendre as u8;
    for (k, x) in [v.seuil_cri_db, v.reaction_cri_ms, v.seuil_limiteur_db, v.porte_db].into_iter().enumerate() {
        o[2 + 2 * k..4 + 2 * k].copy_from_slice(&((x * 10.0).round() as i16).to_le_bytes());
    }
    o
}

/// Applique une trame `C` ; les valeurs restent dans les bornes de la fenêtre Réglages.
fn appliquer(o: &[u8], v: &mut ReglagesVoie) {
    if o.len() != 10 {
        return;
    }
    let x = |k: usize| i16::from_le_bytes([o[2 + 2 * k], o[3 + 2 * k]]) as f32 / 10.0;
    v.anti_bruit = o[0] & 1 != 0;
    v.coupe_cri = o[0] & 2 != 0;
    v.limiteur = o[0] & 4 != 0;
    if o[1] != v.apprendre as u8 {
        v.apprendre = v.apprendre.wrapping_add(1); // « Apprendre le bruit » tapé sur le téléphone
        v.anti_bruit = true;
    }
    v.seuil_cri_db = x(0).clamp(-40.0, 0.0);
    v.reaction_cri_ms = x(1).clamp(1.0, 15.0);
    v.seuil_limiteur_db = x(2).clamp(-40.0, 0.0);
    v.porte_db = x(3).clamp(-80.0, -20.0);
}

/// Côté interface du lien Wi-Fi : les réglages demandés par le téléphone, et l'envoi des nôtres.
pub struct LienWifi {
    commande: Arc<Mutex<Option<Vec<u8>>>>,
    socket: TcpStream,
    envoye: Option<[u8; 10]>,
}

impl LienWifi {
    /// Une fois par image : applique ce que le téléphone a changé, puis lui renvoie l'état s'il a bougé.
    pub fn synchroniser(&mut self, v: &mut ReglagesVoie) {
        if let Some(c) = self.commande.lock().unwrap_or_else(|e| e.into_inner()).take() {
            appliquer(&c, v);
        }
        let o = voie_vers_octets(v);
        if self.envoye != Some(o) {
            use std::io::Write;
            let _ = (&self.socket).write_all(&trame(b'S', &o)); // téléphone perdu : la voie le dit déjà
            self.envoye = Some(o);
        }
    }
}

/// Lit les trames de l'appli jusqu'à la fin, puis marque le micro perdu.
fn lire_trames(mut flux: impl Read, mut prod: HeapProd<f32>, paquet: Arc<AtomicUsize>, perdu: Arc<AtomicBool>, commande: Arc<Mutex<Option<Vec<u8>>>>) {
    while let Ok((genre, contenu)) = lire_trame(&mut flux) {
        match genre {
            b'A' => {
                let n = contenu.len() / 4 * 4;
                for v in pcm_vers_mono(&contenu[..n]) {
                    let _ = prod.try_push(v);
                }
                paquet.store(n / 4 + 1, Ordering::Relaxed);
            }
            b'C' => *commande.lock().unwrap_or_else(|e| e.into_inner()) = Some(contenu),
            _ => {}
        }
    }
    perdu.store(true, Ordering::Relaxed);
}

pub struct Telephone {
    /// adb, numéro de série, port local, serveur scrcpy : seulement par USB.
    usb: Option<(PathBuf, String, String, Child)>,
    socket: TcpStream,
    /// Seulement par le Wi-Fi ; l'interface le prend.
    pub lien: Option<LienWifi>,
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
        Ok(Telephone { usb: Some((adb.to_path_buf(), serie, port, serveur)), socket, lien: None })
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
        // l'interface écrit l'état à chaque changement : un téléphone figé ne doit pas la bloquer
        let _ = socket.set_write_timeout(Some(Duration::from_millis(200)));
        let lecteur = socket.try_clone().map_err(|e| e.to_string())?;
        let commande = Arc::new(Mutex::new(None));
        let c = commande.clone();
        std::thread::spawn(move || lire_trames(lecteur, prod, paquet, perdu, c));
        let lien = LienWifi { commande, socket: socket.try_clone().map_err(|e| e.to_string())?, envoye: None };
        Ok(Telephone { usb: None, socket, lien: Some(lien) })
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

    /// Appli « Micro table » démarrée : `TDM_TELEPHONE="<ip> <code>" cargo test -p table-de-mixage telephone_wifi_vrai -- --ignored --nocapture`,
    /// puis toucher Limiteur sur le téléphone quand le test le demande.
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
        // le micro s'ouvre à la connexion (~200 ms de moins) ; le Redmi livre d'un coup ses 85 ms de tampon, plus 60 ms en attente
        assert!((75_000..=105_000).contains(&n), "{n}");
        assert!(!perdu.load(Ordering::Relaxed));
        // aller-retour des réglages : le PC envoie les siens, on touche Limiteur sur le téléphone,
        // le téléphone renvoie tous ses contrôles, donc aussi les valeurs reçues du PC
        let mut t = t;
        let l = t.lien.as_mut().unwrap();
        let mut v = ReglagesVoie { anti_bruit: false, coupe_cri: true, limiteur: false, seuil_cri_db: -25.0, reaction_cri_ms: 8.0, seuil_limiteur_db: -6.0, porte_db: -55.0, ..Default::default() };
        eprintln!("touchez Limiteur sur le téléphone (60 s)");
        let fin = Instant::now() + Duration::from_secs(60);
        while !v.limiteur && Instant::now() < fin {
            l.synchroniser(&mut v);
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("revenu du téléphone : {v:?}");
        assert!(v.limiteur && v.coupe_cri && !v.anti_bruit);
        assert_eq!((v.seuil_cri_db, v.reaction_cri_ms, v.seuil_limiteur_db, v.porte_db), (-25.0, 8.0, -6.0, -55.0));
        drop(t);
    }

    /// Un faux téléphone local envoie 480 échantillons et des réglages, lit l'état du PC, puis raccroche.
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
            s.write_all(&trame(b'A', &[0u8; 4 * 480])).unwrap();
            let mut v = ReglagesVoie::default();
            v.coupe_cri = true;
            v.seuil_cri_db = -99.0; // hors bornes : ramené à -40
            v.apprendre = 1;
            s.write_all(&trame(b'C', &voie_vers_octets(&v))).unwrap();
            // le PC répond par son état, puis le faux téléphone raccroche
            assert_eq!(lire_trame(&mut s).unwrap().0, b'S');
        });
        let (prod, cons) = HeapRb::<f32>::new(4096).split();
        let perdu = Arc::new(AtomicBool::new(false));
        let mut t = Telephone::wifi(&format!("{adresse} 012345"), prod, Default::default(), perdu.clone()).unwrap();
        let mut lien = t.lien.take().unwrap();
        let mut v = ReglagesVoie::default();
        let limite = Instant::now() + Duration::from_secs(1);
        while !perdu.load(Ordering::Relaxed) && Instant::now() < limite {
            lien.synchroniser(&mut v);
            std::thread::sleep(Duration::from_millis(10));
        }
        lien.synchroniser(&mut v);
        assert!(perdu.load(Ordering::Relaxed));
        assert_eq!(cons.occupied_len(), 480);
        assert!(v.coupe_cri && !v.limiteur);
        assert_eq!(v.seuil_cri_db, -40.0);
        assert_eq!((v.apprendre, v.anti_bruit), (1, true));
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
