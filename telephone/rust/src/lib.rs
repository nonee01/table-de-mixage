//! Le micro du téléphone pour la table de mixage. Le téléphone écoute sur un port TCP ; quand le PC
//! se connecte, AAudio (basse latence) capture le micro et l'envoie en s16le stéréo 48 kHz, le même
//! format que le serveur de scrcpy, sans en-têtes.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
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
/// Crête du dernier paquet envoyé, en millièmes de pleine échelle (pour le vumètre de l'appli).
static CRETE: AtomicI32 = AtomicI32::new(0);

/// Mono vers s16le stéréo entrelacé (les deux voies identiques).
pub fn mono_vers_stereo(m: &[i16], sortie: &mut Vec<u8>) {
    sortie.clear();
    for v in m {
        let o = v.to_le_bytes();
        sortie.extend_from_slice(&[o[0], o[1], o[0], o[1]]);
    }
}

/// Envoie l'anneau au PC jusqu'à ce qu'il raccroche ou qu'on arrête.
pub fn envoyer(mut cons: HeapCons<i16>, mut sock: impl Write, arret: &AtomicBool) {
    let mut m = [0i16; 480];
    let mut octets = Vec::with_capacity(4 * m.len());
    while !arret.load(Ordering::Relaxed) {
        // retard borné à 60 ms : après un trou du Wi-Fi on jette le vieux son plutôt que de traîner
        let trop = cons.occupied_len().saturating_sub(2880);
        cons.skip(trop);
        let n = cons.pop_slice(&mut m);
        if n == 0 {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        CRETE.store(m[..n].iter().map(|v| v.unsigned_abs() as i32).max().unwrap_or(0) * 1000 / 32768, Ordering::Relaxed);
        mono_vers_stereo(&m[..n], &mut octets);
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
        let _ = sock.set_nonblocking(false);
        let _ = sock.set_nodelay(true);
        let (_prod, cons) = ringbuf::traits::Split::split(ringbuf::HeapRb::<i16>::new(48_000));
        #[cfg(target_os = "android")]
        let _flux = match ouvrir_micro(_prod) {
            Ok(f) => f,
            Err(_) => {
                ETAT.store(ERREUR_MICRO, Ordering::Relaxed);
                break;
            }
        };
        ETAT.store(CONNECTE, Ordering::Relaxed);
        envoyer(cons, sock, &ARRET);
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
pub extern "system" fn Java_ma_tdm_micro_Micro_crete(_: Jni, _: Jni) -> i32 {
    CRETE.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Producer, Split};
    use ringbuf::HeapRb;

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
        assert_eq!(pc.0.len(), 4 * 2880);
    }

    /// Un PC qui raccroche termine l'envoi.
    #[test]
    fn envoi_fini_quand_le_pc_raccroche() {
        let (mut prod, cons) = HeapRb::<i16>::new(48_000).split();
        prod.push_slice(&[7i16; 2_000]);
        envoyer(cons, &mut Pc(Vec::new(), 0), &AtomicBool::new(false));
    }
}
