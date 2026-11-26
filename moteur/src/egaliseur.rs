//! Égaliseur Graves / Médiums / Aigus fait avec les filtres du TP : sinus cardinal fenêtré par Hamming.
//! h = g_G·h_PB(250 Hz) + g_M·h_BP(250 Hz, 4 kHz) + g_A·h_PH(4 kHz), un seul RIF par voie.

use std::f32::consts::PI;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::niveau::db_vers_lin;
use crate::{BLOC, FE};

const M: usize = 511;
const TF: usize = 1024; // ≥ BLOC + M − 1
const COUPURE_BASSE: f32 = 250.0;
const COUPURE_HAUTE: f32 = 4000.0;

/// Coefficients T_e·h_k du passe-bas du TP : h = 2 f_c sinc(2 f_c (t − t_0)), fenêtre de Hamming, m impair.
pub fn passe_bas(fc: f32, m: usize) -> Vec<f32> {
    let te = 1.0 / FE;
    let t0 = (m - 1) as f32 / 2.0;
    (0..m)
        .map(|k| {
            let u = 2.0 * fc * (k as f32 - t0) * te;
            let sinc = if u == 0.0 { 1.0 } else { (PI * u).sin() / (PI * u) };
            let hamming = 0.54 - 0.46 * (2.0 * PI * k as f32 / (m - 1) as f32).cos();
            te * 2.0 * fc * sinc * hamming
        })
        .collect()
}

/// Passe-haut : δ(t − t_0) moins le passe-bas.
pub fn passe_haut(fc: f32, m: usize) -> Vec<f32> {
    let mut h: Vec<f32> = passe_bas(fc, m).iter().map(|v| -v).collect();
    h[(m - 1) / 2] += 1.0;
    h
}

/// Passe-bande : passe-bas f_haut moins passe-bas f_bas.
pub fn passe_bande(f_bas: f32, f_haut: f32, m: usize) -> Vec<f32> {
    passe_bas(f_haut, m).iter().zip(passe_bas(f_bas, m)).map(|(h, b)| h - b).collect()
}

pub struct Egaliseur {
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    bandes: [Vec<Complex<f32>>; 3],
    noyau: Vec<Complex<f32>>,
    spectre: Vec<Complex<f32>>,
    tampon: Vec<f32>,
    cumul: Vec<f32>,
    brouillon_av: Vec<Complex<f32>>,
    brouillon_ar: Vec<Complex<f32>>,
    actif: bool,
}

impl Egaliseur {
    pub fn new() -> Self {
        let mut planif = RealFftPlanner::<f32>::new();
        let fft = planif.plan_fft_forward(TF);
        let ifft = planif.plan_fft_inverse(TF);
        let mut brouillon_av = fft.make_scratch_vec();
        let mut spectre_de = |h: Vec<f32>| {
            let mut t = vec![0.0; TF];
            t[..M].copy_from_slice(&h);
            let mut s = fft.make_output_vec();
            let _ = fft.process_with_scratch(&mut t, &mut s, &mut brouillon_av);
            s
        };
        let bandes = [
            spectre_de(passe_bas(COUPURE_BASSE, M)),
            spectre_de(passe_bande(COUPURE_BASSE, COUPURE_HAUTE, M)),
            spectre_de(passe_haut(COUPURE_HAUTE, M)),
        ];
        Egaliseur {
            noyau: fft.make_output_vec(),
            spectre: fft.make_output_vec(),
            brouillon_ar: ifft.make_scratch_vec(),
            brouillon_av,
            tampon: vec![0.0; TF],
            cumul: vec![0.0; TF],
            bandes,
            fft,
            ifft,
            actif: false,
        }
    }

    /// Recalcule le noyau sans allouer. Les trois gains à 0 dB : l'égaliseur est sauté.
    pub fn set_gains_db(&mut self, graves: f32, mediums: f32, aigus: f32) {
        let actif = graves != 0.0 || mediums != 0.0 || aigus != 0.0;
        if actif && !self.actif {
            self.cumul.iter_mut().for_each(|v| *v = 0.0);
        }
        self.actif = actif;
        let g = [db_vers_lin(graves), db_vers_lin(mediums), db_vers_lin(aigus)];
        for (i, n) in self.noyau.iter_mut().enumerate() {
            *n = self.bandes[0][i] * g[0] + self.bandes[1][i] * g[1] + self.bandes[2][i] * g[2];
        }
    }

    pub fn retard(&self) -> usize {
        if self.actif { (M - 1) / 2 } else { 0 }
    }

    pub fn traiter(&mut self, x: &mut [f32]) {
        if !self.actif {
            return;
        }
        debug_assert_eq!(x.len(), BLOC);
        self.tampon[..BLOC].copy_from_slice(x);
        self.tampon[BLOC..].iter_mut().for_each(|v| *v = 0.0);
        let _ = self.fft.process_with_scratch(&mut self.tampon, &mut self.spectre, &mut self.brouillon_av);
        for (s, n) in self.spectre.iter_mut().zip(&self.noyau) {
            *s *= n;
        }
        let _ = self.ifft.process_with_scratch(&mut self.spectre, &mut self.tampon, &mut self.brouillon_ar);
        let echelle = 1.0 / TF as f32;
        for (c, t) in self.cumul.iter_mut().zip(&self.tampon) {
            *c += t * echelle;
        }
        x.copy_from_slice(&self.cumul[..BLOC]);
        self.cumul.copy_within(BLOC.., 0);
        let n = self.cumul.len();
        self.cumul[n - BLOC..].iter_mut().for_each(|v| *v = 0.0);
    }
}

impl Default for Egaliseur {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{gain_db_a, par_blocs, sinus};

    #[test]
    fn passe_bas_gain_continu_un() {
        assert!((passe_bas(250.0, 511).iter().sum::<f32>() - 1.0).abs() < 0.01);
    }

    #[test]
    fn passe_haut_gain_continu_nul() {
        assert!(passe_haut(4000.0, 511).iter().sum::<f32>().abs() < 0.01);
    }

    #[test]
    fn passe_bande_coupe_le_continu() {
        assert!(passe_bande(250.0, 4000.0, 511).iter().sum::<f32>().abs() < 0.01);
    }

    #[test]
    fn graves_plus_douze() {
        let mut e = Egaliseur::new();
        e.set_gains_db(12.0, 0.0, 0.0);
        for (f, attendu) in [(100.0, 12.0), (1000.0, 0.0)] {
            let x = sinus(f, 0.1, 48_000);
            let y = par_blocs(&x, |b| e.traiter(b));
            let g = gain_db_a(&x, &y, f);
            assert!((g - attendu).abs() < 1.0, "{f} Hz : {g} dB");
        }
        assert_eq!(e.retard(), 255);
    }

    #[test]
    fn plat_ne_touche_rien() {
        let mut e = Egaliseur::new();
        e.set_gains_db(0.0, 0.0, 0.0);
        let x = sinus(440.0, 0.5, 4096);
        let y = par_blocs(&x, |b| e.traiter(b));
        assert_eq!(x, y);
        assert_eq!(e.retard(), 0);
    }
}
