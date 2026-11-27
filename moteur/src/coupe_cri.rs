//! Coupe-cri : un son plus fort que le seuil T ne ressort pas. Anticipation de 20 ms, donc la coupure
//! est en place avant que le cri arrive à la sortie.

use crate::niveau::db_vers_lin;

const ANTICIPATION: usize = 960; // 20 ms
const FENETRE: usize = 480; // valeur efficace sur 10 ms
const DECLENCHE: usize = 240; // 5 ms au-dessus du seuil, par défaut
const FONDU_COUPE: f32 = 1.0 / 240.0; // 5 ms
const MAINTIEN: usize = 14_400; // 300 ms
const FONDU_RETOUR: f32 = 1.0 / 2_400.0; // 50 ms

pub struct CoupeCri {
    seuil_carre: f32,
    ligne: [f32; ANTICIPATION],
    pos_ligne: usize,
    carres: [f32; FENETRE],
    pos_carres: usize,
    somme: f32,
    dessus: usize,
    declenche: usize,
    maintien: usize,
    actif: bool,
    gain: f32,
}

impl CoupeCri {
    pub fn new(seuil_db: f32) -> Self {
        CoupeCri {
            seuil_carre: db_vers_lin(seuil_db).powi(2),
            ligne: [0.0; ANTICIPATION],
            pos_ligne: 0,
            carres: [0.0; FENETRE],
            pos_carres: 0,
            somme: 0.0,
            dessus: 0,
            declenche: DECLENCHE,
            maintien: 0,
            actif: false,
            gain: 1.0,
        }
    }

    pub fn set_seuil_db(&mut self, t: f32) {
        self.seuil_carre = db_vers_lin(t).powi(2);
    }

    /// Temps au-dessus du seuil avant de couper, 1 à 15 ms : avec le fondu de 5 ms la coupure reste
    /// en place avant que le cri sorte (anticipation de 20 ms).
    pub fn set_reaction_ms(&mut self, ms: f32) {
        self.declenche = (ms.clamp(1.0, 15.0) * 48.0) as usize;
    }

    pub fn retard(&self) -> usize {
        ANTICIPATION
    }

    /// Vrai pendant que le son est coupé.
    pub fn coupe(&self) -> bool {
        self.actif
    }

    pub fn traiter(&mut self, x: &mut [f32]) {
        for v in x.iter_mut() {
            let entree = if v.is_finite() { *v } else { 0.0 };
            let carre = entree * entree;
            self.somme += carre - std::mem::replace(&mut self.carres[self.pos_carres], carre);
            self.pos_carres = (self.pos_carres + 1) % FENETRE;
            let au_dessus = self.somme.max(0.0) / FENETRE as f32 > self.seuil_carre;

            self.dessus = if au_dessus { self.dessus + 1 } else { 0 };
            if self.dessus >= self.declenche || (self.actif && au_dessus) {
                self.actif = true;
                self.maintien = MAINTIEN;
            } else if self.actif {
                self.maintien -= 1;
                self.actif = self.maintien > 0;
            }
            self.gain = if self.actif {
                (self.gain - FONDU_COUPE).max(0.0)
            } else {
                (self.gain + FONDU_RETOUR).min(1.0)
            };

            let retarde = std::mem::replace(&mut self.ligne[self.pos_ligne], entree);
            self.pos_ligne = (self.pos_ligne + 1) % ANTICIPATION;
            *v = retarde * self.gain;
        }
        // la somme glissante accumule des erreurs d'arrondi : on la recalcule à chaque bloc
        self.somme = self.carres.iter().sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{par_blocs, rms_db, sinus};

    const MS: usize = 48;

    /// 1 kHz à -25 dBFS pendant 1 s, rafale de 200 ms à -3 dBFS à partir de 400 ms.
    fn parole_et_cri() -> Vec<f32> {
        let mut x = sinus(1000.0, 0.0562 * 2f32.sqrt(), 1000 * MS);
        let cri = sinus(1000.0, 0.708 * 2f32.sqrt(), 200 * MS);
        x[400 * MS..600 * MS].copy_from_slice(&cri);
        x
    }

    fn sortie_alignee(c: &mut CoupeCri, x: &[f32]) -> Vec<f32> {
        let mut y = par_blocs(x, |b| c.traiter(b));
        y.drain(..c.retard());
        y
    }

    #[test]
    fn cri_coupe_entierement() {
        let x = parole_et_cri();
        let mut c = CoupeCri::new(-10.0);
        assert_eq!(c.retard(), 960);
        let y = sortie_alignee(&mut c, &x);
        let (a, b) = (400 * MS, 600 * MS);
        assert!(rms_db(&y[a..b]) <= rms_db(&x[a..b]) - 30.0, "sortie {} entrée {}", rms_db(&y[a..b]), rms_db(&x[a..b]));
    }

    #[test]
    fn reaction_lente_laisse_passer_un_clic() {
        // clic de 4 ms à -3 dBFS : la valeur efficace sur 10 ms reste au-dessus du seuil environ 10 ms
        let mut x = sinus(1000.0, 0.0562 * 2f32.sqrt(), 1000 * MS);
        x[400 * MS..404 * MS].copy_from_slice(&sinus(1000.0, 0.708 * 2f32.sqrt(), 4 * MS));
        let clic = |ms: f32| {
            let mut c = CoupeCri::new(-10.0);
            c.set_reaction_ms(ms);
            let y = sortie_alignee(&mut c, &x);
            rms_db(&y[400 * MS..404 * MS])
        };
        let entree = rms_db(&x[400 * MS..404 * MS]);
        assert!(clic(5.0) < entree - 30.0, "5 ms : {} dB", clic(5.0));
        assert!((clic(15.0) - entree).abs() < 0.5, "15 ms : {} dB", clic(15.0));
    }

    #[test]
    fn parole_avant_et_apres_passe() {
        let x = parole_et_cri();
        let mut c = CoupeCri::new(-10.0);
        let y = sortie_alignee(&mut c, &x);
        let avant = 50 * MS..350 * MS;
        assert!((rms_db(&y[avant.clone()]) - rms_db(&x[avant])).abs() < 1.0);
        let apres = 960 * MS..y.len();
        assert!((rms_db(&y[apres.clone()]) - rms_db(&x[apres])).abs() < 1.0);
    }

    #[test]
    fn parole_sous_le_seuil_jamais_coupee() {
        let x = sinus(1000.0, 0.1 * 2f32.sqrt(), 2000 * MS); // -20 dBFS
        let mut c = CoupeCri::new(-10.0);
        for bloc in x.chunks(crate::BLOC) {
            let mut b = [0.0; crate::BLOC];
            b[..bloc.len()].copy_from_slice(bloc);
            c.traiter(&mut b);
            assert!(!c.coupe());
        }
    }
}
