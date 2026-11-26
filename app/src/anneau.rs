//! Anneaux entre les fils audio : tirer un bloc sans jamais attendre, borner le retard quand deux
//! horloges dérivent, et ramener une entrée à 48 kHz mono.

use ringbuf::traits::{Consumer, Observer, Producer};
use ringbuf::{HeapCons, HeapProd};

/// Remplit `dest` depuis l'anneau ; ce qui manque devient du silence. Faux s'il manquait des échantillons.
pub fn tirer_bloc(cons: &mut HeapCons<f32>, dest: &mut [f32]) -> bool {
    let n = cons.pop_slice(dest);
    dest[n..].iter_mut().for_each(|v| *v = 0.0);
    n == dest.len()
}

/// Jette les plus vieux échantillons au-delà de `max`, pour que le retard ne grandisse pas.
pub fn borner(cons: &mut HeapCons<f32>, max: usize) {
    let n = cons.occupied_len();
    if n > max {
        cons.skip(n - max);
    }
}

/// Ramène une entrée entrelacée à `canaux` voies et `fe_in` Hz vers du mono à 48 kHz, par interpolation
/// cubique (Catmull-Rom) sur 4 points. Aucune allocation.
// shortcut: cubic interpolation without anti-aliasing filter, fine for voice; use a sinc resampler if music
// ever goes through here at 44.1 kHz.
pub struct Reechantillonneur {
    canaux: usize,
    pas: f32,
    t: f32,
    hist: [f32; 4],
}

impl Reechantillonneur {
    pub fn new(fe_in: u32, canaux: usize) -> Self {
        Reechantillonneur { canaux: canaux.max(1), pas: fe_in as f32 / moteur::FE, t: 0.0, hist: [0.0; 4] }
    }

    pub fn pousser(&mut self, entrelace: &[f32], prod: &mut HeapProd<f32>) {
        for trame in entrelace.chunks_exact(self.canaux) {
            let m = trame.iter().sum::<f32>() / self.canaux as f32;
            self.hist = [self.hist[1], self.hist[2], self.hist[3], if m.is_finite() { m } else { 0.0 }];
            while self.t < 1.0 {
                let _ = prod.try_push(catmull_rom(&self.hist, self.t));
                self.t += self.pas;
            }
            self.t -= 1.0;
        }
    }
}

/// Valeur entre y[1] et y[2] à la fraction t.
pub fn catmull_rom(y: &[f32; 4], t: f32) -> f32 {
    let c1 = 0.5 * (y[2] - y[0]);
    let c2 = y[0] - 2.5 * y[1] + 2.0 * y[2] - 0.5 * y[3];
    let c3 = 0.5 * (y[3] - y[0]) + 1.5 * (y[1] - y[2]);
    ((c3 * t + c2) * t + c1) * t + y[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use moteur::BLOC;
    use ringbuf::traits::{Observer, Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn sous_alimentation_donne_silence() {
        let (mut p, mut c) = HeapRb::<f32>::new(1024).split();
        p.push_slice(&[0.5; 100]);
        let mut dest = [1.0; BLOC];
        assert!(!tirer_bloc(&mut c, &mut dest));
        assert!(dest[..100].iter().all(|v| *v == 0.5));
        assert!(dest[100..].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn derive_ne_fait_pas_grandir_le_retard() {
        let (mut p, mut c) = HeapRb::<f32>::new(8 * BLOC).split();
        let mut dest = [0.0; BLOC];
        for _ in 0..1000 {
            p.push_slice(&[0.1; BLOC + 1]);
            borner(&mut c, 2 * BLOC);
            tirer_bloc(&mut c, &mut dest);
        }
        assert!(c.occupied_len() <= 2 * BLOC);
    }

    #[test]
    fn reechantillonnage_garde_la_frequence() {
        let x: Vec<f32> = (0..44_100).map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / 44_100.0).sin()).collect();
        let (mut p, mut c) = HeapRb::<f32>::new(60_000).split();
        let mut r = Reechantillonneur::new(44_100, 1);
        for morceau in x.chunks(441) {
            r.pousser(morceau, &mut p);
        }
        let n = c.occupied_len();
        assert!((n as f32 - 48_000.0).abs() < 480.0, "{n} échantillons");
        let mut y = vec![0.0; n];
        ringbuf::traits::Consumer::pop_slice(&mut c, &mut y);
        let meilleure = (50..=150)
            .map(|k| k as f32 * 10.0)
            .max_by(|a, b| goertzel(&y, *a).total_cmp(&goertzel(&y, *b)))
            .unwrap();
        assert!((meilleure - 1000.0).abs() <= 50.0, "pic à {meilleure} Hz");
    }

    #[test]
    fn stereo_ramenee_en_mono() {
        let (mut p, mut c) = HeapRb::<f32>::new(4096).split();
        let mut r = Reechantillonneur::new(48_000, 2);
        r.pousser(&[0.2, 0.4].repeat(100), &mut p);
        let mut y = vec![0.0; c.occupied_len()];
        ringbuf::traits::Consumer::pop_slice(&mut c, &mut y);
        assert!(y.len() >= 95 && y[10..].iter().all(|v| (v - 0.3).abs() < 1e-5));
    }

    fn goertzel(x: &[f32], f: f32) -> f32 {
        let w = std::f32::consts::TAU * f / moteur::FE;
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &v in x {
            let s0 = v + 2.0 * w.cos() * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2
    }
}
