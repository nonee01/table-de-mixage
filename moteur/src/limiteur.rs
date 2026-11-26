//! Limiteur : le cri ressort au niveau de la voix. y = g·x, g = min(1, S / enveloppe), anticipation 2 ms.

use crate::niveau::db_vers_lin;
use crate::FE;

const ANTICIPATION: usize = 96; // 2 ms à 48 kHz

pub struct Limiteur {
    seuil: f32,
    env: f32,
    attaque: f32,
    relache: f32,
    ligne: [f32; ANTICIPATION],
    pos: usize,
}

/// Coefficient d'un filtre à un pôle de constante de temps t (s).
pub(crate) fn coef(t: f32) -> f32 {
    (-1.0 / (t * FE)).exp()
}

impl Limiteur {
    pub fn new(seuil_db: f32) -> Self {
        Limiteur {
            seuil: db_vers_lin(seuil_db),
            env: 0.0,
            attaque: coef(0.001),
            relache: coef(0.150),
            ligne: [0.0; ANTICIPATION],
            pos: 0,
        }
    }

    pub fn set_seuil_db(&mut self, s: f32) {
        self.seuil = db_vers_lin(s);
    }

    pub fn retard(&self) -> usize {
        ANTICIPATION
    }

    pub fn traiter(&mut self, x: &mut [f32]) {
        for v in x.iter_mut() {
            let entree = if v.is_finite() { *v } else { 0.0 };
            let a = entree.abs();
            let k = if a > self.env { self.attaque } else { self.relache };
            self.env = a + k * (self.env - a);
            let retarde = std::mem::replace(&mut self.ligne[self.pos], entree);
            self.pos = (self.pos + 1) % ANTICIPATION;
            let g = if self.env > self.seuil { self.seuil / self.env } else { 1.0 };
            *v = (retarde * g).clamp(-self.seuil, self.seuil);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{par_blocs, sinus};
    use crate::niveau::{crete, lin_vers_db};

    fn rafale() -> Vec<f32> {
        let mut x = vec![0.0; 4800];
        x.extend(sinus(1000.0, 1.0, 9600));
        x.extend(vec![0.0; 4800]);
        x
    }

    #[test]
    fn rafale_pleine_echelle_limitee_au_seuil() {
        let mut l = Limiteur::new(-12.0);
        let y = par_blocs(&rafale(), |b| l.traiter(b));
        assert!(lin_vers_db(crete(&y)) <= -12.0 + 0.5, "crête {}", lin_vers_db(crete(&y)));
    }

    #[test]
    fn signal_faible_inchange() {
        let mut l = Limiteur::new(-12.0);
        let x = sinus(440.0, 0.0316, 9600); // -30 dBFS
        let y = par_blocs(&x, |b| l.traiter(b));
        let d = l.retard();
        assert_eq!(d, 96);
        for i in d..x.len() {
            assert!((y[i] - x[i - d]).abs() < 1e-4, "écart à {i}");
        }
    }

    #[test]
    fn sortie_finie_sur_nan() {
        let mut l = Limiteur::new(-12.0);
        let mut x = sinus(1000.0, 0.5, 512);
        x[10] = f32::NAN;
        x[300] = f32::INFINITY;
        let y = par_blocs(&x, |b| l.traiter(b));
        assert!(y.iter().all(|v| v.is_finite()));
    }
}
