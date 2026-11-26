//! Enregistre la voie sondée avant et après ses blocs, dans deux WAV écrits au fil de l'eau.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use hound::{SampleFormat, WavSpec, WavWriter};
use ringbuf::traits::{Consumer, Observer};
use ringbuf::HeapCons;

/// `base.wav` donne `base-avant.wav` et `base-apres.wav`.
pub fn chemins(base: &Path) -> (PathBuf, PathBuf) {
    let racine = base.with_extension("");
    let nom = |suffixe: &str| {
        let mut s = racine.clone().into_os_string();
        s.push(suffixe);
        PathBuf::from(s)
    };
    (nom("-avant.wav"), nom("-apres.wav"))
}

fn ecrivain(chemin: &Path) -> Result<WavWriter<std::io::BufWriter<std::fs::File>>, String> {
    let spec = WavSpec { channels: 1, sample_rate: moteur::FE as u32, bits_per_sample: 16, sample_format: SampleFormat::Int };
    WavWriter::create(chemin, spec).map_err(|e| format!("Enregistrement impossible: {e}"))
}

fn en_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0) as i16
}

pub struct Enregistrement {
    arret: Arc<AtomicBool>,
    fil: JoinHandle<(HeapCons<f32>, Result<(), String>)>,
    debut: Instant,
}

impl Enregistrement {
    /// `cons` reçoit des paires entrelacées (avant, après). En cas d'erreur, il est rendu.
    pub fn demarrer(base: &Path, mut cons: HeapCons<f32>) -> Result<Enregistrement, (HeapCons<f32>, String)> {
        let (a, b) = chemins(base);
        let (mut avant, mut apres) = match (ecrivain(&a), ecrivain(&b)) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(e), _) | (_, Err(e)) => return Err((cons, e)),
        };
        cons.skip(cons.occupied_len()); // pas de vieux son d'avant le clic
        let arret = Arc::new(AtomicBool::new(false));
        let stop = arret.clone();
        let fil = std::thread::spawn(move || {
            let mut tampon = [0f32; 4096];
            let mut res = Ok(());
            loop {
                let fini = stop.load(Ordering::Relaxed);
                let n = cons.pop_slice(&mut tampon) / 2 * 2;
                for p in tampon[..n].chunks_exact(2) {
                    if res.is_ok() {
                        res = avant.write_sample(en_i16(p[0])).and_then(|_| apres.write_sample(en_i16(p[1]))).map_err(|e| e.to_string());
                    }
                }
                if n == 0 {
                    if fini {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            let fin = avant.finalize().and(apres.finalize()).map_err(|e| e.to_string());
            (cons, res.and(fin).map_err(|e| format!("Enregistrement abîmé: {e}")))
        });
        Ok(Enregistrement { arret, fil, debut: Instant::now() })
    }

    pub fn secondes(&self) -> u64 {
        self.debut.elapsed().as_secs()
    }

    /// Écrit ce qui reste, ferme les fichiers et rend l'anneau.
    pub fn arreter(self) -> (HeapCons<f32>, Result<(), String>) {
        self.arret.store(true, Ordering::Relaxed);
        self.fil.join().unwrap_or_else(|_| panic!("fil d'enregistrement tombé"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn enregistrement_avant_et_apres() {
        let dossier = std::env::temp_dir().join(format!("tdm-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let base = dossier.join("Micro 1.wav");
        let (mut prod, cons) = HeapRb::<f32>::new(8192).split();
        let r = Enregistrement::demarrer(&base, cons).map_err(|e| e.1).unwrap();
        for _ in 0..480 {
            prod.try_push(1.0).unwrap();
            prod.try_push(0.5).unwrap();
        }
        let (_cons, res) = r.arreter();
        res.unwrap();
        let (a, b) = chemins(&base);
        assert!(a.ends_with("Micro 1-avant.wav") && b.ends_with("Micro 1-apres.wav"));
        let lire = |p: &Path| hound::WavReader::open(p).unwrap().into_samples::<i16>().map(|s| s.unwrap()).collect::<Vec<_>>();
        let (va, vb) = (lire(&a), lire(&b));
        assert_eq!((va.len(), vb.len()), (480, 480));
        assert!(va.iter().all(|v| *v == 32767) && vb.iter().all(|v| *v == 16383));
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
