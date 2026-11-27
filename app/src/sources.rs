//! Les sources qui ne sont pas des micros : un fichier de musique et la voix de synthèse (Piper).

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ringbuf::traits::{Observer, Producer};
use ringbuf::HeapProd;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as ErreurSym;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::anneau::catmull_rom;

/// Le fil de lecture garde l'anneau rempli jusque-là (échantillons), soit environ 85 ms de stéréo.
const REMPLISSAGE: usize = 8_192;
const DELAI_PIPER: Duration = Duration::from_secs(15);

/// Ramène un signal mono de `fe` Hz à 48 kHz.
pub fn reechantillonner(x: &[f32], fe: u32) -> Vec<f32> {
    if fe == moteur::FE as u32 || x.is_empty() {
        return x.to_vec();
    }
    let pas = fe as f64 / moteur::FE as f64;
    let n = (x.len() as f64 / pas) as usize;
    let dernier = x.len() as isize - 1;
    let y = |j: isize| x[j.clamp(0, dernier) as usize];
    (0..n)
        .map(|k| {
            let t = k as f64 * pas;
            let i = t as isize;
            catmull_rom(&[y(i - 1), y(i), y(i + 1), y(i + 2)], (t - i as f64) as f32)
        })
        .collect()
}

/// Décode tout le fichier en stéréo 48 kHz (le mono est recopié sur les deux côtés).
pub fn decoder_fichier(chemin: &Path) -> Result<(Vec<f32>, Vec<f32>), String> {
    let illisible = |e: &dyn std::fmt::Display| format!("Fichier illisible: {e}");
    let fichier = std::fs::File::open(chemin).map_err(|e| illisible(&e))?;
    let mss = MediaSourceStream::new(Box::new(fichier), Default::default());
    let mut indice = Hint::new();
    if let Some(ext) = chemin.extension().and_then(OsStr::to_str) {
        indice.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&indice, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| illisible(&e))?;
    let piste = format.default_track(TrackType::Audio).ok_or("Fichier illisible: pas de piste audio")?;
    let params = piste.codec_params.as_ref().and_then(|p| p.audio()).ok_or("Fichier illisible: format inconnu")?;
    let mut decodeur = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| illisible(&e))?;
    let id = piste.id;

    let (mut g, mut d) = (Vec::new(), Vec::new());
    let mut fe = 0;
    let mut entrelace: Vec<f32> = Vec::new();
    loop {
        let paquet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => return Err(illisible(&e)),
        };
        if paquet.track_id != id {
            continue;
        }
        match decodeur.decode(&paquet) {
            Ok(tampon) => {
                fe = tampon.spec().rate();
                let canaux = tampon.spec().channels().count().max(1);
                entrelace.resize(tampon.samples_interleaved(), 0.0);
                tampon.copy_to_slice_interleaved(&mut entrelace);
                for trame in entrelace.chunks_exact(canaux) {
                    g.push(trame[0]);
                    d.push(trame[canaux.min(2) - 1]);
                }
            }
            Err(ErreurSym::DecodeError(_)) => {} // un paquet abîmé : on saute
            Err(e) => return Err(illisible(&e)),
        }
    }
    if g.is_empty() {
        return Err("Fichier illisible: aucun son".into());
    }
    Ok((reechantillonner(&g, fe), reechantillonner(&d, fe)))
}

/// Pousse dans l'anneau sans dépasser REMPLISSAGE, en dormant quand il est plein. Faux si `arret` passe à vrai.
fn pousser_au_rythme(prod: &mut HeapProd<f32>, mut x: &[f32], arret: impl Fn() -> bool) -> bool {
    while !x.is_empty() {
        if arret() {
            return false;
        }
        let place = REMPLISSAGE.saturating_sub(prod.occupied_len());
        if place == 0 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        let n = prod.push_slice(&x[..place.min(x.len())]);
        x = &x[n..];
    }
    true
}

fn noter(erreur: &Mutex<Option<String>>, e: Option<String>) {
    if let Ok(mut v) = erreur.lock() {
        *v = e;
    }
}

pub struct LecteurMusique {
    envoi: Sender<PathBuf>,
    lecture: Arc<AtomicBool>,
    boucle: Arc<AtomicBool>,
    erreur: Arc<Mutex<Option<String>>>,
}

impl LecteurMusique {
    pub fn new(mut prod: HeapProd<f32>) -> Self {
        let (envoi, recu): (Sender<PathBuf>, Receiver<PathBuf>) = mpsc::channel();
        let lecture = Arc::new(AtomicBool::new(false));
        let boucle = Arc::new(AtomicBool::new(false));
        let erreur = Arc::new(Mutex::new(None));
        let (l, b, e) = (lecture.clone(), boucle.clone(), erreur.clone());
        std::thread::spawn(move || {
            let mut stereo: Vec<f32> = Vec::new();
            let mut pos = 0;
            loop {
                match recu.try_recv() {
                    Ok(chemin) => match decoder_fichier(&chemin) {
                        Ok((g, d)) => {
                            stereo = g.iter().zip(&d).flat_map(|(a, b)| [*a, *b]).collect();
                            pos = 0;
                            noter(&e, None);
                        }
                        Err(err) => {
                            l.store(false, Ordering::Relaxed);
                            noter(&e, Some(err));
                        }
                    },
                    Err(TryRecvError::Disconnected) => return, // le lecteur a été lâché
                    Err(TryRecvError::Empty) => {}
                }
                if !l.load(Ordering::Relaxed) || stereo.is_empty() {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                let fin = (pos + 2 * 1024).min(stereo.len());
                // un nouveau fichier ou une pause interrompt le morceau en cours
                if !pousser_au_rythme(&mut prod, &stereo[pos..fin], || !l.load(Ordering::Relaxed)) {
                    continue;
                }
                pos = fin;
                if pos == stereo.len() {
                    pos = 0;
                    if !b.load(Ordering::Relaxed) {
                        l.store(false, Ordering::Relaxed);
                    }
                }
            }
        });
        LecteurMusique { envoi, lecture, boucle, erreur }
    }

    pub fn ouvrir(&self, chemin: PathBuf) {
        let _ = self.envoi.send(chemin);
    }

    pub fn lecture(&self, oui: bool) {
        self.lecture.store(oui, Ordering::Relaxed);
    }

    pub fn en_lecture(&self) -> bool {
        self.lecture.load(Ordering::Relaxed)
    }

    pub fn boucle(&self, oui: bool) {
        self.boucle.store(oui, Ordering::Relaxed);
    }

    pub fn erreur(&self) -> Option<String> {
        self.erreur.lock().ok().and_then(|e| e.clone())
    }
}

fn nom_piper() -> String {
    format!("piper{}", std::env::consts::EXE_SUFFIX)
}

/// Piper et sa voix : d'abord à côté de l'exécutable, puis sur le PATH ou dans ~/.local.
pub fn trouver_piper() -> Option<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().ok()?;
    let maison = std::env::var_os("HOME").map(PathBuf::from);
    trouver_piper_dans(exe.parent()?, std::env::var_os("PATH").as_deref(), maison.as_deref())
}

fn trouver_piper_dans(dossier_exe: &Path, chemin: Option<&OsStr>, maison: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
    let binaire = dossier_exe.join("piper").join(nom_piper());
    let voix = std::fs::read_dir(dossier_exe.join("voix"))
        .ok()
        .and_then(|it| it.flatten().map(|e| e.path()).find(|p| p.extension() == Some(OsStr::new("onnx"))));
    if let (true, Some(v)) = (binaire.is_file(), voix) {
        return Some((binaire, v));
    }

    let sur_path = chemin.and_then(|c| std::env::split_paths(c).map(|d| d.join(nom_piper())).find(|p| p.is_file()));
    let local = maison.map(|m| m.join(".local/bin").join(nom_piper())).filter(|p| p.is_file());
    let voix = maison.map(|m| m.join(".local/share/piper-voices/fr_FR-siwis-medium.onnx")).filter(|p| p.is_file());
    Some((sur_path.or(local)?, voix?))
}

/// Taux d'échantillonnage écrit dans `<voix>.json` (`"sample_rate": 22050`).
fn taux_voix(voix: &Path) -> Result<u32, String> {
    let mut json = voix.as_os_str().to_owned();
    json.push(".json");
    let texte = std::fs::read_to_string(&json).map_err(|e| format!("Voix illisible: {e}"))?;
    let apres = texte.split("\"sample_rate\"").nth(1).ok_or("Voix illisible: sample_rate absent")?;
    apres
        .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| "Voix illisible: sample_rate".into())
}

/// Fait lire `texte` à Piper ; mono 48 kHz. Piper est tué au bout de 15 s.
pub fn piper_dire(texte: &str, piper: &Path, voix: &Path) -> Result<Vec<f32>, String> {
    let mut cmd = Command::new(piper);
    cmd.arg("--model").arg(voix).arg("--output_raw").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW : pas de console qui clignote
    }
    let mut enfant = cmd.spawn().map_err(|e| format!("Piper introuvable: {e}"))?;
    let fe = match taux_voix(voix) {
        Ok(fe) => fe,
        Err(e) => {
            let _ = enfant.kill();
            return Err(e);
        }
    };
    if let Some(mut entree) = enfant.stdin.take() {
        let _ = entree.write_all(texte.as_bytes());
        let _ = entree.write_all(b"\n");
    }
    let mut sortie = enfant.stdout.take().ok_or("Piper: pas de sortie")?;
    let lecteur = std::thread::spawn(move || {
        let mut octets = Vec::new();
        let _ = sortie.read_to_end(&mut octets);
        octets
    });
    let debut = Instant::now();
    let statut = loop {
        if let Some(s) = enfant.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if debut.elapsed() > DELAI_PIPER {
            let _ = enfant.kill();
            let _ = enfant.wait();
            return Err("Piper ne répond pas".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let octets = lecteur.join().map_err(|_| "Piper: lecture impossible")?;
    if !statut.success() || octets.is_empty() {
        return Err("Piper a échoué".into());
    }
    let x: Vec<f32> = octets.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]) as f32 / 32_768.0).collect();
    Ok(reechantillonner(&x, fe))
}

pub struct Voix {
    envoi: Sender<String>,
    erreur: Arc<Mutex<Option<String>>>,
}

impl Voix {
    pub fn new(mut prod: HeapProd<f32>) -> Self {
        let (envoi, recu) = mpsc::channel::<String>();
        let erreur = Arc::new(Mutex::new(None));
        let e = erreur.clone();
        std::thread::spawn(move || {
            // recv échoue quand la Voix est lâchée : le fil s'arrête
            while let Ok(texte) = recu.recv() {
                let Some((piper, voix)) = trouver_piper() else {
                    noter(&e, Some("Piper introuvable".into()));
                    continue;
                };
                match piper_dire(&texte, &piper, &voix) {
                    Ok(x) => {
                        noter(&e, None);
                        pousser_au_rythme(&mut prod, &x, || false);
                    }
                    Err(err) => noter(&e, Some(err)),
                }
            }
        });
        Voix { envoi, erreur }
    }

    pub fn dire(&self, texte: String) {
        let _ = self.envoi.send(texte);
    }

    pub fn erreur(&self) -> Option<String> {
        self.erreur.lock().ok().and_then(|e| e.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdm-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn wav_44100_vers_48000() {
        let d = dossier("wav");
        let chemin = d.join("la.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&chemin, spec).unwrap();
        for i in 0..44_100 {
            let v = 0.5 * (std::f32::consts::TAU * 1000.0 * i as f32 / 44_100.0).sin();
            w.write_sample((v * 32_767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let (g, d_) = decoder_fichier(&chemin).unwrap();
        assert_eq!(g.len(), d_.len());
        assert!((g.len() as f32 - 48_000.0).abs() < 480.0, "{} échantillons", g.len());
        let s = crate::trace::spectre_db(&g[..crate::trace::TF]);
        let k = s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let f = k as f32 * moteur::FE / crate::trace::TF as f32;
        assert!((f - 1000.0).abs() < 12.0, "pic à {f} Hz");
    }

    #[test]
    fn test_cri_parole_passe_cri_coupe() {
        let (g, _) = decoder_fichier(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/test-cri.flac"))).unwrap();
        assert!((g.len() as f32 / 48_000.0 - 7.84).abs() < 0.1, "{} échantillons", g.len());
        let mut y = g.clone();
        let mut c = moteur::coupe_cri::CoupeCri::new(-10.0);
        y.chunks_mut(256).for_each(|b| c.traiter(b));
        let r = c.retard();
        let crete = |x: &[f32]| x.iter().fold(0f32, |m, v| m.max(v.abs()));
        let parole = 48_000 * 38 / 10; // la phrase dure 3,9 s, le cri commence à 4,3 s
        let ecart = g[..parole].iter().zip(&y[r..parole + r]).fold(0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(ecart < 1e-3, "la parole est modifiée de {ecart}");
        // la montée du cri (~120 ms sous -10 dB efficaces) passe, puis la voie se tait
        let (a, b) = (48_000 * 445 / 100, 48_000 * 49 / 10);
        assert!(crete(&g[a..b]) > 0.9 && crete(&y[a + r..b + r]) < 0.01, "le cri n'est pas coupé");
    }

    /// Une vidéo mp4 (image H.264, son AAC 44,1 kHz, 1 s de la à 1 kHz) : on en garde le son.
    #[test]
    fn video_mp4_donne_son_son() {
        let (g, _) = decoder_fichier(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/essai-video.mp4"))).unwrap();
        assert!((g.len() as f32 / 48_000.0 - 1.0).abs() < 0.1, "{} échantillons", g.len());
        let s = crate::trace::spectre_db(&g[4_800..4_800 + crate::trace::TF]);
        let k = s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let f = k as f32 * moteur::FE / crate::trace::TF as f32;
        assert!((f - 1000.0).abs() < 12.0, "pic à {f} Hz");
    }

    #[test]
    fn fichier_illisible_donne_erreur() {
        let chemin = dossier("mp3").join("faux.mp3");
        std::fs::write(&chemin, "ceci n'est pas de la musique").unwrap();
        assert!(decoder_fichier(&chemin).is_err());
    }

    #[test]
    fn piper_absent_donne_erreur() {
        let t = std::time::Instant::now();
        let r = piper_dire("bonjour", Path::new("/nonexistent/piper"), Path::new("/nonexistent/v.onnx"));
        assert!(r.is_err());
        assert!(t.elapsed().as_secs_f32() < 1.0);
    }

    /// Lance le vrai Piper s'il est installé : `cargo test -p table-de-mixage piper_vrai -- --ignored`.
    #[test]
    #[ignore]
    fn piper_vrai() {
        let (piper, voix) = trouver_piper().expect("Piper introuvable");
        let x = piper_dire("Bonjour monsieur", &piper, &voix).unwrap();
        assert!(x.len() > 24_000, "{} échantillons", x.len());
        assert!(moteur::niveau::crete(&x) > 0.05);
    }

    #[test]
    fn aucun_piper_dans_dossier_vide() {
        let d = dossier("piper");
        assert_eq!(trouver_piper_dans(&d, None, Some(&d)), None);
    }
}
