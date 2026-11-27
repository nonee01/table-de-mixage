//! Le micro du téléphone pour la table de mixage. Le téléphone écoute sur un port TCP ; quand le PC
//! se connecte et donne le code, AAudio (basse latence) capture le micro.
//!
//! Ensuite les deux sens portent des trames `type: u8, longueur: u16 le, contenu` :
//! - téléphone → PC : `A` le son en s16le stéréo 48 kHz, `C` les réglages voulus sur le téléphone ;
//! - PC → téléphone : `S` les réglages en vigueur sur la voie du téléphone.
//! Réglages (10 octets) : blocs (1 anti-bruit, 2 coupe-cri, 4 limiteur), compteur « apprendre »,
//! puis en i16 le : seuil coupe-cri (0,1 dB), réaction coupe-cri (0,1 ms), seuil limiteur (0,1 dB), porte anti-bruit (0,1 dB).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use ringbuf::traits::{Consumer, Observer};
use ringbuf::HeapCons;

pub const ARRETE: i32 = 0;
pub const ATTENTE: i32 = 1;
pub const CONNECTE: i32 = 2;
pub const ERREUR_MICRO: i32 = -1;
pub const ERREUR_PORT: i32 = -2;

static ETAT: AtomicI32 = AtomicI32::new(ARRETE);
static ARRET: AtomicBool = AtomicBool::new(false);
static EN_MARCHE: AtomicBool = AtomicBool::new(false);
/// Code à 6 chiffres affiché sur le téléphone ; le PC l'envoie avant que le micro s'ouvre.
static CODE: AtomicI32 = AtomicI32::new(0);
/// Crête depuis la dernière lecture par l'appli, en millièmes de pleine échelle.
static CRETE: AtomicI32 = AtomicI32::new(0);
/// Réglages reçus du PC (6 valeurs, voir l'en-tête), puis 1 quand au moins une trame `S` est arrivée.
static PC: [AtomicI32; 7] = [const { AtomicI32::new(0) }; 7];
/// Réglages changés sur le téléphone, en attente d'envoi.
static A_ENVOYER: Mutex<Option<[u8; 10]>> = Mutex::new(None);

/// Ajoute une trame à `sortie`.
pub fn trame(genre: u8, contenu: &[u8], sortie: &mut Vec<u8>) {
    sortie.push(genre);
    sortie.extend_from_slice(&(contenu.len() as u16).to_le_bytes());
    sortie.extend_from_slice(contenu);
}

pub fn lire_trame(r: &mut impl Read) -> std::io::Result<(u8, Vec<u8>)> {
    let mut tete = [0u8; 3];
    r.read_exact(&mut tete)?;
    let mut contenu = vec![0u8; u16::from_le_bytes([tete[1], tete[2]]) as usize];
    r.read_exact(&mut contenu)?;
    Ok((tete[0], contenu))
}

pub fn reglages_vers_octets(v: [i32; 6]) -> [u8; 10] {
    let mut o = [0u8; 10];
    o[0] = v[0] as u8;
    o[1] = v[1] as u8;
    for k in 0..4 {
        o[2 + 2 * k..4 + 2 * k].copy_from_slice(&(v[2 + k] as i16).to_le_bytes());
    }
    o
}

pub fn octets_vers_reglages(o: &[u8]) -> Option<[i32; 6]> {
    if o.len() != 10 {
        return None;
    }
    let i = |k: usize| i16::from_le_bytes([o[2 + 2 * k], o[3 + 2 * k]]) as i32;
    Some([o[0] as i32, o[1] as i32, i(0), i(1), i(2), i(3)])
}

/// Lit les trames du PC jusqu'à la fin de la connexion.
fn recevoir(mut sock: impl Read) {
    while let Ok((genre, contenu)) = lire_trame(&mut sock) {
        if let (b'S', Some(v)) = (genre, octets_vers_reglages(&contenu)) {
            for (k, x) in v.iter().enumerate() {
                PC[k].store(*x, Ordering::Relaxed);
            }
            PC[6].store(1, Ordering::Relaxed);
        }
    }
}

/// Mono vers s16le stéréo entrelacé (les deux voies identiques).
pub fn mono_vers_stereo(m: &[i16], sortie: &mut Vec<u8>) {
    sortie.clear();
    for v in m {
        let o = v.to_le_bytes();
        sortie.extend_from_slice(&[o[0], o[1], o[0], o[1]]);
    }
}

/// Envoie l'anneau au PC jusqu'à ce qu'il raccroche ou qu'on arrête, et les réglages quand ils changent.
pub fn envoyer(mut cons: HeapCons<i16>, mut sock: impl Write, arret: &AtomicBool) {
    let mut m = [0i16; 480];
    let mut stereo = Vec::with_capacity(4 * m.len());
    let mut octets = Vec::with_capacity(4 * m.len() + 16);
    while !arret.load(Ordering::Relaxed) {
        octets.clear();
        if let Some(r) = A_ENVOYER.lock().unwrap_or_else(|e| e.into_inner()).take() {
            trame(b'C', &r, &mut octets);
        }
        // retard borné à 60 ms : après un trou du Wi-Fi on jette le vieux son plutôt que de traîner
        let trop = cons.occupied_len().saturating_sub(2880);
        cons.skip(trop);
        let n = cons.pop_slice(&mut m);
        if n > 0 {
            CRETE.fetch_max(m[..n].iter().map(|v| v.unsigned_abs() as i32).max().unwrap_or(0) * 1000 / 32768, Ordering::Relaxed);
            mono_vers_stereo(&m[..n], &mut stereo);
            trame(b'A', &stereo, &mut octets);
        }
        if octets.is_empty() {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        if sock.write_all(&octets).is_err() {
            return;
        }
    }
}

#[cfg(target_os = "android")]
fn ouvrir_micro(mut prod: ringbuf::HeapProd<i16>) -> ndk::audio::Result<ndk::audio::AudioStream> {
    use ndk::audio::*;
    use ringbuf::traits::Producer;
    let flux = AudioStreamBuilder::new()?
        .direction(AudioDirection::Input)
        .sample_rate(48_000)
        .channel_count(1)
        .format(AudioFormat::PCM_I16)
        .performance_mode(AudioPerformanceMode::LowLatency)
        .input_preset(AudioInputPreset::Unprocessed)
        .data_callback(Box::new(move |_, data, trames| {
            let x = unsafe { std::slice::from_raw_parts(data as *const i16, trames.max(0) as usize) };
            prod.push_slice(x);
            AudioCallbackResult::Continue
        }))
        .open_stream()?;
    flux.request_start()?;
    Ok(flux)
}

/// Un code neuf, tiré du générateur du noyau.
fn nouveau_code() -> i32 {
    use std::io::Read;
    let mut o = [0u8; 4];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut o));
    (u32::from_le_bytes(o) % 1_000_000) as i32
}

/// Lit les 6 chiffres du PC (2 s au plus) et les compare au code.
pub fn code_correct(sock: &mut impl std::io::Read, code: i32) -> bool {
    let mut recu = [0u8; 6];
    if sock.read_exact(&mut recu).is_err() {
        return false;
    }
    let attendu = format!("{code:06}");
    // comparaison sans sortie anticipée
    recu.iter().zip(attendu.as_bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn servir(port: u16) {
    let ecoute = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(e) => e,
        Err(_) => return ETAT.store(ERREUR_PORT, Ordering::Relaxed),
    };
    let _ = ecoute.set_nonblocking(true);
    ETAT.store(ATTENTE, Ordering::Relaxed);
    while !ARRET.load(Ordering::Relaxed) {
        let sock: TcpStream = match ecoute.accept() {
            Ok((s, _)) => s,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        let mut sock = sock;
        let _ = sock.set_nonblocking(false);
        let _ = sock.set_nodelay(true);
        let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
        if !code_correct(&mut sock, CODE.load(Ordering::Relaxed)) {
            std::thread::sleep(Duration::from_secs(1)); // deviner 10^6 codes prendrait des jours
            continue;
        }
        let (_prod, cons) = ringbuf::traits::Split::split(ringbuf::HeapRb::<i16>::new(48_000));
        #[cfg(target_os = "android")]
        let _flux = match ouvrir_micro(_prod) {
            Ok(f) => f,
            Err(_) => {
                ETAT.store(ERREUR_MICRO, Ordering::Relaxed);
                break;
            }
        };
        let _ = sock.set_read_timeout(None);
        PC[6].store(0, Ordering::Relaxed);
        if let Ok(lecteur) = sock.try_clone() {
            std::thread::spawn(move || recevoir(lecteur));
        }
        ETAT.store(CONNECTE, Ordering::Relaxed);
        envoyer(cons, &sock, &ARRET);
        let _ = sock.shutdown(std::net::Shutdown::Both); // termine aussi `recevoir`
        PC[6].store(0, Ordering::Relaxed);
        CRETE.store(0, Ordering::Relaxed);
        if ETAT.load(Ordering::Relaxed) == CONNECTE {
            ETAT.store(ATTENTE, Ordering::Relaxed);
        }
    }
    if ETAT.load(Ordering::Relaxed) > ARRETE {
        ETAT.store(ARRETE, Ordering::Relaxed);
    }
    EN_MARCHE.store(false, Ordering::Relaxed);
}

// Appelés par Micro.kt. Seulement des entiers : pas besoin de la bibliothèque JNI.
type Jni = *mut std::ffi::c_void;

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_demarrer(_: Jni, _: Jni, port: i32) {
    if EN_MARCHE.swap(true, Ordering::Relaxed) {
        return;
    }
    ARRET.store(false, Ordering::Relaxed);
    CODE.store(nouveau_code(), Ordering::Relaxed);
    std::thread::spawn(move || servir(port as u16));
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_arreter(_: Jni, _: Jni) {
    ARRET.store(true, Ordering::Relaxed);
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_etat(_: Jni, _: Jni) -> i32 {
    ETAT.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_code(_: Jni, _: Jni) -> i32 {
    CODE.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_crete(_: Jni, _: Jni) -> i32 {
    CRETE.swap(0, Ordering::Relaxed)
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_regler(_: Jni, _: Jni, blocs: i32, apprendre: i32, cri: i32, reaction: i32, lim: i32, porte: i32) {
    *A_ENVOYER.lock().unwrap_or_else(|e| e.into_inner()) = Some(reglages_vers_octets([blocs, apprendre, cri, reaction, lim, porte]));
}

#[no_mangle]
pub extern "system" fn Java_ma_tdm_micro_Micro_pc(_: Jni, _: Jni, i: i32) -> i32 {
    PC.get(i as usize).map_or(0, |v| v.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn code_verifie() {
        assert!(code_correct(&mut &b"004821"[..], 4821));
        assert!(!code_correct(&mut &b"004822"[..], 4821));
        assert!(!code_correct(&mut &b"0048"[..], 4821));
        assert!((0..1_000_000).contains(&nouveau_code()));
    }

    #[test]
    fn stereo_identique() {
        let mut o = Vec::new();
        mono_vers_stereo(&[1, -2], &mut o);
        assert_eq!(o, [1, 0, 1, 0, 0xfe, 0xff, 0xfe, 0xff]);
    }

    struct Pc(Vec<u8>, usize);
    impl Write for Pc {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.0.len() >= self.1 {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Un anneau trop plein est vidé jusqu'à 60 ms ; l'arrêt termine l'envoi.
    #[test]
    fn envoi_borne_a_60_ms() {
        let (mut prod, cons) = HeapRb::<i16>::new(48_000).split();
        prod.push_slice(&[7i16; 10_000]);
        let arret = std::sync::Arc::new(AtomicBool::new(false));
        let a = arret.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            a.store(true, Ordering::Relaxed);
        });
        let mut pc = Pc(Vec::new(), usize::MAX);
        envoyer(cons, &mut pc, &arret);
        // 2880 échantillons en 6 trames `A` de 480
        assert_eq!(pc.0.len(), 4 * 2880 + 6 * 3);
        assert_eq!(lire_trame(&mut &pc.0[..]).unwrap(), (b'A', [7, 0, 7, 0].repeat(480)));
    }

    #[test]
    fn trames_et_reglages_aller_retour() {
        let v = [5, 200, -100, 50, -120, -500];
        let mut o = Vec::new();
        trame(b'S', &reglages_vers_octets(v), &mut o);
        let (genre, contenu) = lire_trame(&mut &o[..]).unwrap();
        assert_eq!((genre, octets_vers_reglages(&contenu)), (b'S', Some(v)));
        assert!(octets_vers_reglages(&contenu[..9]).is_none());
        assert!(lire_trame(&mut &o[..5]).is_err()); // trame coupée
    }

    /// Un PC qui raccroche termine l'envoi.
    #[test]
    fn envoi_fini_quand_le_pc_raccroche() {
        let (mut prod, cons) = HeapRb::<i16>::new(48_000).split();
        prod.push_slice(&[7i16; 2_000]);
        envoyer(cons, &mut Pc(Vec::new(), 0), &AtomicBool::new(false));
    }
}
