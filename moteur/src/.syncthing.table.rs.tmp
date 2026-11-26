//! La table : cinq voies mélangées en stéréo, puis le volume général et le limiteur de sortie.

use crate::generateur::{Generateur, TypeBruit};
use crate::limiteur::Limiteur;
use crate::niveau::{crete, db_vers_lin};
use crate::voie::{ReglagesVoie, Voie, VOLUME_MIN_DB};
use crate::BLOC;

/// 0 Micro 1, 1 Micro 2, 2 Musique, 3 Voix, 4 Bruit.
pub const NB_VOIES: usize = 5;
const SEUIL_MASTER_DB: f32 = -1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReglagesTable {
    pub voies: [ReglagesVoie; NB_VOIES],
    pub master_db: f32,
    pub master_limiteur: bool,
    pub type_bruit: TypeBruit,
    pub voie_sondee: usize,
}

impl Default for ReglagesTable {
    fn default() -> Self {
        let mut voies = [ReglagesVoie::default(); NB_VOIES];
        voies[4].muet = true; // le bruit de test ne part pas tout seul
        ReglagesTable { voies, master_db: 0.0, master_limiteur: true, type_bruit: TypeBruit::Blanc, voie_sondee: 0 }
    }
}

/// Un bloc de chaque source (BLOC échantillons chacun).
pub struct EntreesBloc<'a> {
    pub micro1: &'a [f32],
    pub micro2: &'a [f32],
    pub musique_g: &'a [f32],
    pub musique_d: &'a [f32],
    pub voix: &'a [f32],
}

pub struct Table {
    voies: [Voie; NB_VOIES],
    musique_d: Voie,
    bruit: Generateur,
    master: [Limiteur; 2],
    travail: [f32; BLOC],
    travail_d: [f32; BLOC],
    sonde_entree: [f32; BLOC],
    sonde_sortie: [f32; BLOC],
    jeter: [f32; BLOC],
    cretes_master: [f32; 2],
}

impl Table {
    pub fn new() -> Self {
        Table {
            voies: std::array::from_fn(|_| Voie::new()),
            musique_d: Voie::new(),
            bruit: Generateur::new(0x5EED),
            master: [Limiteur::new(SEUIL_MASTER_DB), Limiteur::new(SEUIL_MASTER_DB)],
            travail: [0.0; BLOC],
            travail_d: [0.0; BLOC],
            sonde_entree: [0.0; BLOC],
            sonde_sortie: [0.0; BLOC],
            jeter: [0.0; BLOC],
            cretes_master: [0.0; 2],
        }
    }

    pub fn traiter(&mut self, r: &ReglagesTable, e: &EntreesBloc, sortie_g: &mut [f32], sortie_d: &mut [f32]) {
        sortie_g.iter_mut().for_each(|v| *v = 0.0);
        sortie_d.iter_mut().for_each(|v| *v = 0.0);
        self.bruit.set_type(r.type_bruit);
        for i in 0..NB_VOIES {
            match i {
                0 => self.travail.copy_from_slice(e.micro1),
                1 => self.travail.copy_from_slice(e.micro2),
                2 => self.travail.copy_from_slice(e.musique_g),
                3 => self.travail.copy_from_slice(e.voix),
                _ => self.bruit.remplir(&mut self.travail),
            }
            let sondee = i == r.voie_sondee;
            if sondee {
                self.sonde_entree.copy_from_slice(&self.travail);
            }
            let sonde = if sondee { &mut self.sonde_sortie } else { &mut self.jeter };
            self.voies[i].traiter(&r.voies[i], &mut self.travail, sonde);
            if i == 2 {
                self.travail_d.copy_from_slice(e.musique_d);
                self.musique_d.traiter(&r.voies[2], &mut self.travail_d, &mut self.jeter);
                for k in 0..BLOC {
                    sortie_g[k] += self.travail[k];
                    sortie_d[k] += self.travail_d[k];
                }
            } else {
                for k in 0..BLOC {
                    sortie_g[k] += self.travail[k];
                    sortie_d[k] += self.travail[k];
                }
            }
        }

        let g = if r.master_db <= VOLUME_MIN_DB { 0.0 } else { db_vers_lin(r.master_db) };
        for (k, sortie) in [sortie_g, sortie_d].into_iter().enumerate() {
            for v in sortie.iter_mut() {
                *v = if v.is_finite() { *v * g } else { 0.0 };
            }
            if r.master_limiteur {
                self.master[k].traiter(sortie);
            }
            // dernier rempart : la carte son écrêterait de toute façon
            sortie.iter_mut().for_each(|v| *v = v.clamp(-1.0, 1.0));
            self.cretes_master[k] = crete(sortie);
        }
    }

    pub fn cretes(&self) -> [f32; NB_VOIES] {
        std::array::from_fn(|i| self.voies[i].crete())
    }

    pub fn cretes_master(&self) -> [f32; 2] {
        self.cretes_master
    }

    /// Entrée et sortie (avant le volume) de la voie sondée, dernier bloc.
    pub fn sonde(&self) -> (&[f32], &[f32]) {
        (&self.sonde_entree, &self.sonde_sortie)
    }

    pub fn retard(&self, voie: usize) -> usize {
        self.voies[voie].retard()
    }

    pub fn coupe(&self, voie: usize) -> bool {
        self.voies[voie].coupe()
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::sinus;
    use crate::niveau::crete;
    use crate::BLOC;

    fn tout_muet() -> ReglagesTable {
        let mut r = ReglagesTable::default();
        for v in r.voies.iter_mut() {
            v.muet = true;
        }
        r
    }

    fn un_bloc(t: &mut Table, r: &ReglagesTable, m1: &[f32], voix: &[f32]) -> ([f32; BLOC], [f32; BLOC]) {
        let z = [0.0; BLOC];
        let e = EntreesBloc { micro1: m1, micro2: &z, musique_g: &z, musique_d: &z, voix };
        let (mut g, mut d) = ([0.0; BLOC], [0.0; BLOC]);
        t.traiter(r, &e, &mut g, &mut d);
        (g, d)
    }

    #[test]
    fn bruit_muet_par_defaut() {
        assert!(ReglagesTable::default().voies[4].muet);
    }

    #[test]
    fn muet_donne_zero() {
        let mut t = Table::new();
        let x = sinus(440.0, 0.5, BLOC);
        let (g, d) = un_bloc(&mut t, &tout_muet(), &x, &[0.0; BLOC]);
        assert!(g.iter().chain(&d).all(|v| *v == 0.0));
    }

    #[test]
    fn volume_moins_six() {
        let mut t = Table::new();
        let mut r = tout_muet();
        r.voies[0].muet = false;
        r.voies[0].volume_db = -6.0206;
        let x = sinus(440.0, 0.5, 4 * BLOC);
        let mut sortie = vec![];
        for b in x.chunks(BLOC) {
            sortie.extend(un_bloc(&mut t, &r, b, &[0.0; BLOC]).0);
        }
        assert!((crete(&sortie) - 0.25).abs() < 0.0025, "crête {}", crete(&sortie));
    }

    #[test]
    fn master_en_bas_de_course_coupe() {
        let mut t = Table::new();
        let mut r = tout_muet();
        r.voies[0].muet = false;
        r.master_db = crate::voie::VOLUME_MIN_DB;
        let (g, d) = un_bloc(&mut t, &r, &sinus(440.0, 0.5, BLOC), &[0.0; BLOC]);
        assert!(g.iter().chain(&d).all(|v| *v == 0.0));
    }

    #[test]
    fn master_jamais_au_dessus_de_zero_dbfs() {
        let mut t = Table::new();
        let mut r = ReglagesTable::default();
        for v in r.voies.iter_mut() {
            v.muet = false;
            v.volume_db = 6.0;
        }
        let x = sinus(440.0, 1.0, 20 * BLOC);
        for b in x.chunks(BLOC) {
            let z = b;
            let e = EntreesBloc { micro1: z, micro2: z, musique_g: z, musique_d: z, voix: z };
            let (mut g, mut d) = ([0.0; BLOC], [0.0; BLOC]);
            t.traiter(&r, &e, &mut g, &mut d);
            assert!(crete(&g) <= 1.0 && crete(&d) <= 1.0);
        }
    }

    #[test]
    fn nan_et_inf_donnent_sortie_finie() {
        let mut t = Table::new();
        let r = ReglagesTable::default();
        let mut m1 = sinus(440.0, 0.5, BLOC);
        m1[3] = f32::NAN;
        let mut voix = [0.0; BLOC];
        voix[7] = f32::INFINITY;
        for _ in 0..4 {
            let (g, d) = un_bloc(&mut t, &r, &m1, &voix);
            assert!(g.iter().chain(&d).all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    #[test]
    fn sonde_avant_volume() {
        let mut t = Table::new();
        let mut r = tout_muet();
        r.voies[0].muet = false;
        r.voies[0].volume_db = -20.0;
        r.voie_sondee = 0;
        let x = sinus(440.0, 0.5, BLOC);
        un_bloc(&mut t, &r, &x, &[0.0; BLOC]);
        let (entree, sortie) = t.sonde();
        assert_eq!(entree, &x[..]);
        assert_eq!(sortie, &x[..]);
    }
}
