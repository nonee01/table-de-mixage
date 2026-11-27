//! Visite guidée : une étape à la fois, la zone expliquée reste claire et le reste est assombri.
//! Elle s'ouvre seule au premier lancement, puis depuis Aide.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, Context, Id, Key, LayerId, Order, Rect, RichText, Stroke, StrokeKind, Vec2};

use crate::style;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    Voie,
    Source,
    Blocs,
    Musique,
    Voix,
    Bruit,
    Sortie,
    Trace,
    Enregistrer,
}

struct Etape {
    zone: Option<Zone>,
    titre: &'static str,
    texte: &'static str,
}

const ETAPES: &[Etape] = &[
    Etape { zone: None, titre: "Bienvenue", texte: "Cette visite montre la table en quelques étapes. Échap la passe, les flèches avancent et reculent." },
    Etape { zone: Some(Zone::Voie), titre: "Une voie", texte: "Chaque colonne est une voie : une source, son volume, son égaliseur et ses blocs. Cliquez sur son titre pour la voir sur le tracé." },
    Etape { zone: Some(Zone::Source), titre: "La source", texte: "Le micro du PC, ou le téléphone : choisissez « Téléphone (Wi-Fi) » et tapez l'adresse et le code affichés par l'appli Micro table." },
    Etape { zone: Some(Zone::Blocs), titre: "Les blocs", texte: "Anti-bruit enlève le bruit de fond de la pièce. Coupe-cri coupe un son plus fort que son seuil. Limiteur plafonne le niveau. « Réglages… » ouvre leurs seuils et leurs temps." },
    Etape { zone: Some(Zone::Musique), titre: "Musique", texte: "Ouvrez un fichier mp3, wav, flac ou mp4. « Test cri » charge un enregistrement où quelqu'un parle puis crie, pour essayer le Coupe-cri." },
    Etape { zone: Some(Zone::Voix), titre: "Voix", texte: "Tapez une phrase puis « Dire » : la voix de synthèse la lit dans la table." },
    Etape { zone: Some(Zone::Bruit), titre: "Bruit", texte: "Un bruit de test (blanc, ronflement 50 Hz, souffle) pour essayer l'Anti-bruit. Il démarre en muet." },
    Etape { zone: Some(Zone::Sortie), titre: "Sortie", texte: "La carte son où sort le mélange, son volume, et un limiteur sur tout le mélange." },
    Etape { zone: Some(Zone::Trace), titre: "Le tracé", texte: "La voie choisie : entrée en bleu, sortie en orange. Les lignes T et S sont les seuils du Coupe-cri et du Limiteur." },
    Etape { zone: Some(Zone::Enregistrer), titre: "Enregistrer", texte: "Enregistre la voie choisie avant et après ses blocs, en deux fichiers WAV." },
];

#[derive(Default)]
pub struct Visite {
    pub etape: Option<usize>,
    zones: HashMap<Zone, Rect>,
}

/// Fichier vide qui dit que la visite a déjà été vue sur ce PC.
fn marque() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("Table de mixage").join("visite-vue"))
}

impl Visite {
    /// Ouverte d'office si elle n'a jamais été vue.
    pub fn au_lancement() -> Self {
        let vue = marque().is_some_and(|m| m.exists());
        Visite { etape: (!vue).then_some(0), ..Default::default() }
    }

    pub fn noter(&mut self, z: Zone, r: Rect) {
        self.zones.insert(z, r);
    }

    fn fermer(&mut self) {
        self.etape = None;
        if let Some(m) = marque() {
            let _ = m.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(m, b"");
        }
    }

    /// À appeler après tout le reste de l'image, pour passer par-dessus.
    pub fn afficher(&mut self, ctx: &Context) {
        let Some(i) = self.etape else { return };
        let (precedent, suivant, passer) =
            ctx.input(|e| (e.key_pressed(Key::ArrowLeft), e.key_pressed(Key::ArrowRight), e.key_pressed(Key::Escape)));
        let ecran = ctx.content_rect();
        let etape = &ETAPES[i];
        let cible = etape.zone.and_then(|z| self.zones.get(&z)).map(|r| r.expand(6.0).intersect(ecran));

        let voile = Color32::from_black_alpha(165);
        let p = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("visite-voile")));
        match cible {
            Some(c) => {
                // quatre bandes autour de la zone : elle seule reste claire
                for r in [
                    Rect::from_min_max(ecran.min, egui::pos2(ecran.max.x, c.min.y)),
                    Rect::from_min_max(egui::pos2(ecran.min.x, c.max.y), ecran.max),
                    Rect::from_min_max(egui::pos2(ecran.min.x, c.min.y), egui::pos2(c.min.x, c.max.y)),
                    Rect::from_min_max(egui::pos2(c.max.x, c.min.y), egui::pos2(ecran.max.x, c.max.y)),
                ] {
                    p.rect_filled(r, 0.0, voile);
                }
                p.rect_stroke(c, 6.0, Stroke::new(2.5, style::ENTREE), StrokeKind::Outside);
            }
            None => {
                p.rect_filled(ecran, 0.0, voile);
            }
        }

        // la bulle sous la zone, ou au-dessus s'il n'y a pas la place, ou au centre
        let (pivot, pos) = match cible {
            Some(c) if ecran.max.y - c.max.y > 170.0 => (Align2::CENTER_TOP, c.center_bottom() + Vec2::new(0.0, 14.0)),
            Some(c) => (Align2::CENTER_BOTTOM, c.center_top() - Vec2::new(0.0, 14.0)),
            None => (Align2::CENTER_CENTER, ecran.center()),
        };
        let mut aller: Option<isize> = None;
        let mut fin = passer;
        egui::Area::new(Id::unique("visite-bulle")).order(Order::Tooltip).pivot(pivot).fixed_pos(pos).constrain(true).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(14.0).show(ui, |ui| {
                ui.set_max_width(330.0);
                ui.label(RichText::new(etape.titre).strong().size(17.0));
                ui.add_space(4.0);
                ui.label(RichText::new(etape.texte).size(14.0));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} / {}", i + 1, ETAPES.len())).color(style::TEXTE_GRIS));
                    if ui.button("Passer la visite").clicked() {
                        fin = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dernier = i + 1 == ETAPES.len();
                        let bouton = egui::Button::new(RichText::new(if dernier { "Terminer" } else { "Suivant" }).color(Color32::WHITE))
                            .fill(style::ENTREE);
                        if ui.add(bouton).clicked() {
                            aller = Some(1);
                        }
                        if i > 0 && ui.button("Précédent").clicked() {
                            aller = Some(-1);
                        }
                    });
                });
            });
        });
        if suivant {
            aller = Some(1);
        }
        if precedent && i > 0 {
            aller = Some(-1);
        }
        match aller {
            _ if fin => self.fermer(),
            Some(1) if i + 1 == ETAPES.len() => self.fermer(),
            Some(d) => self.etape = Some((i as isize + d) as usize),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visite_chaque_zone_une_fois() {
        let zones: Vec<_> = ETAPES.iter().filter_map(|e| e.zone).collect();
        let uniques: std::collections::HashSet<_> = zones.iter().collect();
        assert_eq!(zones.len(), uniques.len());
        assert_eq!(uniques.len(), 9);
    }
}
