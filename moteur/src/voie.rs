//! Une voie de la table : Anti-bruit → Coupe-cri → Limiteur → Égaliseur → volume.

use crate::anti_bruit::AntiBruit;
use crate::coupe_cri::CoupeCri;
use crate::egaliseur::Egaliseur;
use crate::limiteur::Limiteur;
use crate::niveau::{crete, db_vers_lin};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReglagesVoie {
    pub volume_db: f32,
    pub muet: bool,
    pub anti_bruit: bool,
    pub coupe_cri: bool,
    pub limiteur: bool,
    pub graves_db: f32,
    pub mediums_db: f32,
    pub aigus_db: f32,
    pub seuil_limiteur_db: f32,
    pub seuil_cri_db: f32,
    pub porte_db: f32,
    pub reaction_cri_ms: f32,
    pub relache_ms: f32,
    /// Compteur : chaque changement lance « Apprendre le bruit ».
    pub apprendre: u32,
}

impl Default for ReglagesVoie {
    fn default() -> Self {
        ReglagesVoie {
            volume_db: 0.0,
            muet: false,
            anti_bruit: false,
            coupe_cri: false,
            limiteur: false,
            graves_db: 0.0,
            mediums_db: 0.0,
            aigus_db: 0.0,
            seuil_limiteur_db: -12.0,
            seuil_cri_db: -10.0,
            porte_db: -50.0,
            reaction_cri_ms: 5.0,
            relache_ms: 150.0,
            apprendre: 0,
        }
    }
}

/// En dessous de ce volume la voie est coupée (le curseur affiche -∞).
pub const VOLUME_MIN_DB: f32 = -60.0;

pub struct Voie {
    anti_bruit: AntiBruit,
    coupe_cri: CoupeCri,
    limiteur: Limiteur,
    egaliseur: Egaliseur,
    gains_eq: [f32; 3],
    apprendre: u32,
    actifs: [bool; 3],
    crete: f32,
}

impl Voie {
    pub fn new() -> Self {
        let r = ReglagesVoie::default();
        Voie {
            anti_bruit: AntiBruit::new(),
            coupe_cri: CoupeCri::new(r.seuil_cri_db),
            limiteur: Limiteur::new(r.seuil_limiteur_db),
            egaliseur: Egaliseur::new(),
            gains_eq: [0.0; 3],
            apprendre: 0,
            actifs: [false; 3],
            crete: 0.0,
        }
    }

    /// `x` (un bloc) ressort traité et au bon volume ; `sonde` reçoit le signal avant le volume.
    pub fn traiter(&mut self, r: &ReglagesVoie, x: &mut [f32], sonde: &mut [f32]) {
        for v in x.iter_mut() {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        if r.apprendre != self.apprendre {
            self.apprendre = r.apprendre;
            self.anti_bruit.apprendre();
        }
        self.actifs = [r.anti_bruit, r.coupe_cri, r.limiteur];
        if r.anti_bruit {
            self.anti_bruit.set_porte_db(r.porte_db);
            self.anti_bruit.traiter(x);
        }
        if r.coupe_cri {
            self.coupe_cri.set_seuil_db(r.seuil_cri_db);
            self.coupe_cri.set_reaction_ms(r.reaction_cri_ms);
            self.coupe_cri.traiter(x);
        }
        if r.limiteur {
            self.limiteur.set_seuil_db(r.seuil_limiteur_db);
            self.limiteur.set_relache_ms(r.relache_ms);
            self.limiteur.traiter(x);
        }
        let gains = [r.graves_db, r.mediums_db, r.aigus_db];
        if gains != self.gains_eq {
            self.gains_eq = gains;
            self.egaliseur.set_gains_db(gains[0], gains[1], gains[2]);
        }
        self.egaliseur.traiter(x);
        sonde.copy_from_slice(x);

        let g = if r.muet || r.volume_db <= VOLUME_MIN_DB { 0.0 } else { db_vers_lin(r.volume_db) };
        x.iter_mut().for_each(|v| *v *= g);
        self.crete = crete(x);
    }

    /// Crête du dernier bloc, après le volume.
    pub fn crete(&self) -> f32 {
        self.crete
    }

    /// Retard ajouté par les blocs en marche (échantillons).
    pub fn retard(&self) -> usize {
        let [ab, cc, li] = self.actifs;
        (ab as usize) * self.anti_bruit.retard()
            + (cc as usize) * self.coupe_cri.retard()
            + (li as usize) * self.limiteur.retard()
            + self.egaliseur.retard()
    }

    /// Vrai pendant que le Coupe-cri coupe.
    pub fn coupe(&self) -> bool {
        self.actifs[1] && self.coupe_cri.coupe()
    }
}

impl Default for Voie {
    fn default() -> Self {
        Self::new()
    }
}
