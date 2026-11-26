//! Traitement du son de la table de mixage : blocs de BLOC échantillons à FE Hz, sans OS ni fenêtre,
//! pour pouvoir passer plus tard sur une carte.

pub mod limiteur;
pub mod niveau;

/// Fréquence d'échantillonnage (Hz).
pub const FE: f32 = 48_000.0;
/// Taille d'un bloc (échantillons).
pub const BLOC: usize = 256;

#[cfg(test)]
pub(crate) mod essais {
    use crate::niveau::{lin_vers_db, rms};
    use crate::FE;

    pub fn sinus(f: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * f * i as f32 / FE).sin()).collect()
    }

    /// Bruit blanc uniforme de valeur efficace `rms` (xorshift32, reproductible).
    pub fn bruit_blanc(rms_voulu: f32, n: usize, graine: u32) -> Vec<f32> {
        let mut s = graine.max(1);
        let ampl = rms_voulu * 3f32.sqrt();
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                ampl * (s as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    pub fn rms_db(x: &[f32]) -> f32 {
        lin_vers_db(rms(x))
    }

    /// Amplitude à la fréquence f (Goertzel), sur la seconde moitié du signal.
    fn amplitude_a(x: &[f32], f: f32) -> f32 {
        let x = &x[x.len() / 2..];
        let w = 2.0 * std::f64::consts::PI * f as f64 / FE as f64;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s0 = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let p = s1 * s1 + s2 * s2 - c * s1 * s2;
        (2.0 * p.max(0.0).sqrt() / x.len() as f64) as f32
    }

    /// Gain (dB) entre x_in et x_out à la fréquence f.
    pub fn gain_db_a(x_in: &[f32], x_out: &[f32], f: f32) -> f32 {
        lin_vers_db(amplitude_a(x_out, f)) - lin_vers_db(amplitude_a(x_in, f))
    }

    /// Passe un signal entier dans un traitement bloc par bloc.
    pub fn par_blocs(x: &[f32], mut traiter: impl FnMut(&mut [f32])) -> Vec<f32> {
        let mut y = x.to_vec();
        for bloc in y.chunks_mut(crate::BLOC) {
            if bloc.len() == crate::BLOC {
                traiter(bloc);
            } else {
                let mut b = [0.0; crate::BLOC];
                b[..bloc.len()].copy_from_slice(bloc);
                traiter(&mut b);
                let n = bloc.len();
                bloc.copy_from_slice(&b[..n]);
            }
        }
        y
    }
}
