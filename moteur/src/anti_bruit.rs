//! Anti-bruit : soustraction spectrale |Y| = max(|X| − α·N, β·|X|) sur une TFCT (trame 512, Hann, 50 %),
//! puis une porte qui fait taire ce qui reste sous -50 dBFS.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::limiteur::coef;
use crate::niveau::{db_vers_lin, rms};
use crate::BLOC;

const TRAME: usize = 512;
const BINS: usize = TRAME / 2 + 1;
const ALPHA: f32 = 2.0;
const BETA: f32 = 0.05;
const TRAMES_APPRENTISSAGE: usize = 187; // 1 s de trames de 256
const TRAMES_MINIMUM: usize = 187; // le minimum d'énergie est suivi sur 2 fois 1 s

pub struct AntiBruit {
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    fenetre: Vec<f32>, // racine de Hann : analyse × synthèse = Hann, qui se recouvre à 1
    entree: Vec<f32>,
    trame: Vec<f32>,
    spectre: Vec<Complex<f32>>,
    brouillon_av: Vec<Complex<f32>>,
    brouillon_ar: Vec<Complex<f32>>,
    queue: Vec<f32>,
    bruit: Vec<f32>,
    somme_apprise: Vec<f32>,
    trames_apprises: usize,
    apprend: bool,
    appris: bool,
    min_courant: f32,
    min_precedent: f32,
    trames_min: usize,
    porte: f32,
    porte_seuil: f32,
    porte_ouvre: f32,
    porte_ferme: f32,
}

impl AntiBruit {
    pub fn new() -> Self {
        let mut planif = RealFftPlanner::<f32>::new();
        let fft = planif.plan_fft_forward(TRAME);
        let ifft = planif.plan_fft_inverse(TRAME);
        let fenetre = (0..TRAME)
            .map(|i| (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / TRAME as f32).cos()).sqrt())
            .collect();
        AntiBruit {
            brouillon_av: fft.make_scratch_vec(),
            brouillon_ar: ifft.make_scratch_vec(),
            spectre: fft.make_output_vec(),
            fft,
            ifft,
            fenetre,
            entree: vec![0.0; TRAME],
            trame: vec![0.0; TRAME],
            queue: vec![0.0; TRAME - BLOC],
            bruit: vec![0.0; BINS],
            somme_apprise: vec![0.0; BINS],
            trames_apprises: 0,
            apprend: false,
            appris: false,
            min_courant: f32::INFINITY,
            min_precedent: f32::INFINITY,
            trames_min: 0,
            porte: 1.0,
            porte_seuil: db_vers_lin(-50.0),
            porte_ouvre: coef(0.005),
            porte_ferme: coef(0.100),
        }
    }

    /// La seconde qui suit devient le profil du bruit.
    pub fn apprendre(&mut self) {
        self.somme_apprise.iter_mut().for_each(|v| *v = 0.0);
        self.trames_apprises = 0;
        self.apprend = true;
    }

    pub fn apprend(&self) -> bool {
        self.apprend
    }

    pub fn retard(&self) -> usize {
        TRAME - BLOC
    }

    pub fn traiter(&mut self, x: &mut [f32]) {
        debug_assert_eq!(x.len(), BLOC);
        self.entree.copy_within(BLOC.., 0);
        for (d, v) in self.entree[TRAME - BLOC..].iter_mut().zip(x.iter()) {
            *d = if v.is_finite() { *v } else { 0.0 };
        }
        for ((t, e), w) in self.trame.iter_mut().zip(&self.entree).zip(&self.fenetre) {
            *t = e * w;
        }
        let _ = self.fft.process_with_scratch(&mut self.trame, &mut self.spectre, &mut self.brouillon_av);

        self.suivre_le_bruit();
        for (c, n) in self.spectre.iter_mut().zip(&self.bruit) {
            let m = c.norm();
            if m > 0.0 {
                *c *= (m - ALPHA * n).max(BETA * m) / m;
            }
        }
        // les cases 0 et Nyquist doivent rester réelles pour la TF inverse
        self.spectre[0].im = 0.0;
        self.spectre[BINS - 1].im = 0.0;
        let _ = self.ifft.process_with_scratch(&mut self.spectre, &mut self.trame, &mut self.brouillon_ar);

        let echelle = 1.0 / TRAME as f32;
        for (i, v) in x.iter_mut().enumerate() {
            *v = self.queue[i] + self.trame[i] * self.fenetre[i] * echelle;
        }
        for i in 0..TRAME - BLOC {
            self.queue[i] = self.trame[BLOC + i] * self.fenetre[BLOC + i] * echelle;
        }

        let cible = if rms(x) > self.porte_seuil { 1.0 } else { db_vers_lin(-30.0) };
        let k = if cible > self.porte { self.porte_ouvre } else { self.porte_ferme };
        for v in x.iter_mut() {
            self.porte = cible + k * (self.porte - cible);
            *v *= self.porte;
        }
    }

    /// Apprentissage (bouton) puis mise à jour lente sur les trames qui ressemblent à du silence.
    fn suivre_le_bruit(&mut self) {
        if self.apprend {
            for (s, c) in self.somme_apprise.iter_mut().zip(&self.spectre) {
                *s += c.norm();
            }
            self.trames_apprises += 1;
            if self.trames_apprises == TRAMES_APPRENTISSAGE {
                let n = TRAMES_APPRENTISSAGE as f32;
                for (b, s) in self.bruit.iter_mut().zip(&self.somme_apprise) {
                    *b = s / n;
                }
                self.apprend = false;
                self.appris = true;
            }
            return;
        }
        let energie: f32 = self.spectre.iter().map(|c| c.norm_sqr()).sum();
        self.min_courant = self.min_courant.min(energie);
        self.trames_min += 1;
        if self.trames_min == TRAMES_MINIMUM {
            self.min_precedent = self.min_courant;
            self.min_courant = f32::INFINITY;
            self.trames_min = 0;
        }
        let silence = if self.appris {
            energie < 2.0 * self.bruit.iter().map(|n| n * n).sum::<f32>()
        } else {
            energie > 0.0 && energie < 2.0 * self.min_courant.min(self.min_precedent)
        };
        if silence {
            for (b, c) in self.bruit.iter_mut().zip(&self.spectre) {
                *b = 0.95 * *b + 0.05 * c.norm();
            }
        }
    }
}

impl Default for AntiBruit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{bruit_blanc, gain_db_a, par_blocs, rms_db, sinus};

    const S: usize = 48_000;

    #[test]
    fn bruit_blanc_baisse_de_15_db() {
        let x = bruit_blanc(0.01, 3 * S, 7); // -40 dBFS
        let mut a = AntiBruit::new();
        a.apprendre();
        let y = par_blocs(&x, |b| a.traiter(b));
        assert!(!a.apprend());
        let fin = 2 * S..3 * S;
        assert!(rms_db(&y[fin.clone()]) <= rms_db(&x[fin.clone()]) - 15.0, "sortie {} entrée {}", rms_db(&y[fin.clone()]), rms_db(&x[fin]));
    }

    #[test]
    fn ton_garde_dans_le_bruit() {
        let mut a = AntiBruit::new();
        a.apprendre();
        par_blocs(&bruit_blanc(0.01, S, 3), |b| a.traiter(b));
        let bruit = bruit_blanc(0.01, 2 * S, 11);
        let ton = sinus(1000.0, 0.251 * 2f32.sqrt(), 2 * S); // -12 dBFS
        let x: Vec<f32> = bruit.iter().zip(&ton).map(|(b, t)| b + t).collect();
        let y = par_blocs(&x, |b| a.traiter(b));
        let g = gain_db_a(&x, &y, 1000.0);
        assert!(g.abs() < 2.0, "gain à 1 kHz {g} dB");
    }

    #[test]
    fn silence_reste_silence() {
        let x = bruit_blanc(0.000316, 3 * S, 5); // -70 dBFS
        let mut a = AntiBruit::new();
        let y = par_blocs(&x, |b| a.traiter(b));
        assert!(rms_db(&y[2 * S..]) <= -95.0, "sortie {}", rms_db(&y[2 * S..]));
    }

    #[test]
    fn retard_mesure_avec_une_impulsion() {
        let mut x = vec![0.0; 4096];
        x[1000] = 1.0;
        let mut a = AntiBruit::new();
        let y = par_blocs(&x, |b| a.traiter(b));
        let pic = y.iter().enumerate().max_by(|p, q| p.1.abs().total_cmp(&q.1.abs())).unwrap().0;
        assert_eq!(pic - 1000, a.retard());
        assert!(a.retard() <= 512);
    }

    #[test]
    fn sans_bruit_le_ton_ressort_intact() {
        let mut a = AntiBruit::new();
        a.apprendre();
        par_blocs(&bruit_blanc(0.00003, S, 9), |b| a.traiter(b)); // -90 dBFS
        let x = sinus(1000.0, 0.1 * 2f32.sqrt(), S); // -20 dBFS
        let y = par_blocs(&x, |b| a.traiter(b));
        let g = gain_db_a(&x, &y, 1000.0);
        assert!(g.abs() < 0.2, "gain à 1 kHz {g} dB");
    }
}
