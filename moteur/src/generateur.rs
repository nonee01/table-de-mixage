//! Bruits de test pour la voie Bruit : blanc, ronflement 50 Hz, souffle (bruit rose). Tous à -20 dBFS.

use std::f32::consts::TAU;

use crate::FE;

const RMS: f32 = 0.1; // -20 dBFS

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TypeBruit {
    #[default]
    Blanc,
    Ronflement50,
    Souffle,
}

pub struct Generateur {
    type_bruit: TypeBruit,
    graine: u32,
    phase: f32,
    rose: [f32; 7],
    echelle_rose: f32,
}

impl Generateur {
    pub fn new(graine: u32) -> Self {
        let mut g = Generateur { type_bruit: TypeBruit::Blanc, graine: graine.max(1), phase: 0.0, rose: [0.0; 7], echelle_rose: 1.0 };
        // le filtre rose n'a pas un gain simple : on le mesure une fois sur 1 s
        let mut somme = 0.0f64;
        for _ in 0..48_000 {
            let r = g.rose_brut();
            somme += (r * r) as f64;
        }
        g.echelle_rose = RMS / (somme / 48_000.0).sqrt() as f32;
        g
    }

    pub fn set_type(&mut self, t: TypeBruit) {
        self.type_bruit = t;
    }

    fn blanc(&mut self) -> f32 {
        let s = &mut self.graine;
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        *s as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Filtre de Paul Kellet sur du blanc.
    fn rose_brut(&mut self) -> f32 {
        let w = self.blanc();
        let b = &mut self.rose;
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.1538520;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        let p = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115926;
        p
    }

    pub fn remplir(&mut self, x: &mut [f32]) {
        // ronflement : 50 Hz et ses harmoniques à -6, -12, -18 dB
        const A: f32 = RMS / 0.8149;
        for v in x.iter_mut() {
            *v = match self.type_bruit {
                TypeBruit::Blanc => self.blanc() * RMS * 3f32.sqrt(),
                TypeBruit::Souffle => self.rose_brut() * self.echelle_rose,
                TypeBruit::Ronflement50 => {
                    self.phase = (self.phase + 50.0 / FE) % 1.0;
                    let p = TAU * self.phase;
                    A * (p.sin() + 0.5 * (2.0 * p).sin() + 0.25 * (3.0 * p).sin() + 0.125 * (4.0 * p).sin())
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{par_blocs, rms_db};

    #[test]
    fn bruit_a_moins_vingt_db() {
        for t in [TypeBruit::Blanc, TypeBruit::Ronflement50, TypeBruit::Souffle] {
            let mut g = Generateur::new(1);
            g.set_type(t);
            let y = par_blocs(&vec![0.0; 48_000], |b| g.remplir(b));
            assert!((rms_db(&y) + 20.0).abs() < 1.0, "{t:?} : {} dB", rms_db(&y));
        }
    }
}
