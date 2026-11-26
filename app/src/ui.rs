//! La fenêtre : menus, une boîte par voie, la boîte Sortie, la ligne d'état.

use eframe::egui::{self, Color32, RichText, Ui};
use moteur::generateur::TypeBruit;
use moteur::table::{ReglagesTable, NB_VOIES};

use crate::audio::{self, Audio, ChoixAudio, LiensUi, Retour, SourceMicro};
use crate::style::{self, boite_groupe, curseur_db, texte_db, vumetre, DB_MAX, DB_MIN};

const NOMS: [&str; NB_VOIES] = ["Micro 1", "Micro 2", "Musique", "Voix", "Bruit"];
const LARGEUR_BOITE: f32 = 142.0;
const HAUTEUR_CURSEUR: f32 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vue {
    Signal,
    Spectre,
}

pub struct EtatUi {
    pub choix: ChoixAudio,
    pub audio: Option<(Audio, LiensUi)>,
    pub erreur: Option<String>,
    pub reglages: ReglagesTable,
    pub vue: Vue,
    pub retour: Retour,
    /// Niveaux affichés (5 voies, puis gauche et droite), qui retombent doucement.
    affiches: [f32; NB_VOIES + 2],
    /// Dernier instant où chaque micro a donné autre chose que des zéros.
    dernier_signal: [f64; 2],
    entrees: Vec<String>,
    sorties: Vec<String>,
    fenetre_reglages: bool,
    fenetre_aide: bool,
}

impl EtatUi {
    pub fn new(reglages: ReglagesTable) -> Self {
        let mut e = EtatUi {
            choix: ChoixAudio::default(),
            audio: None,
            erreur: None,
            reglages,
            vue: Vue::Signal,
            retour: Retour::default(),
            affiches: [0.0; NB_VOIES + 2],
            dernier_signal: [0.0; 2],
            entrees: audio::lister_entrees(),
            sorties: audio::lister_sorties(),
            fenetre_reglages: false,
            fenetre_aide: false,
        };
        e.redemarrer();
        e
    }

    /// Rouvre les flux avec `choix` (changement de périphérique).
    pub fn redemarrer(&mut self) {
        self.audio = None; // l'ancien flux s'arrête avant d'ouvrir le nouveau
        match Audio::demarrer(&self.choix) {
            Ok(a) => {
                self.audio = Some(a);
                self.erreur = None;
            }
            Err(e) => self.erreur = Some(e),
        }
    }
}

pub fn dessiner(ui: &mut Ui, e: &mut EtatUi) {
    let maintenant = ui.input(|i| i.time);
    if let Some((_, liens)) = &mut e.audio {
        liens.reglages.write(e.reglages);
        e.retour = *liens.retour.read();
    }
    let niveaux: Vec<f32> = e.retour.cretes.iter().chain(&e.retour.master).copied().collect();
    for (a, n) in e.affiches.iter_mut().zip(niveaux) {
        *a = n.max(*a * 0.85);
    }
    for k in 0..2 {
        if e.retour.cretes[k] > 0.0 {
            e.dernier_signal[k] = maintenant;
        }
    }

    let mut redemarrer = false;
    egui::Panel::top("menus").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            menus(ui, e);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("48 000 Hz · bloc 256 · latence {:.0} ms", e.retour.latence_ms))
                        .color(style::TEXTE_GRIS),
                );
            });
        });
    });

    egui::Frame::central_panel(ui.style()).show(ui, |ui| {
        if let Some(err) = &e.erreur {
            ui.colored_label(style::ROUGE, err);
        }
        ui.horizontal_top(|ui| {
            for i in 0..NB_VOIES {
                let titre = format!("{}. {}", i + 1, NOMS[i]);
                let choisie = e.reglages.voie_sondee == i;
                let r = boite_groupe(ui, &titre, choisie, LARGEUR_BOITE, |ui| {
                    redemarrer |= boite_voie(ui, e, i, maintenant);
                });
                if r.clicked() {
                    e.reglages.voie_sondee = i;
                }
            }
            boite_groupe(ui, "Sortie", false, LARGEUR_BOITE, |ui| {
                redemarrer |= boite_sortie(ui, e);
            });
        });
        ui.add_space(8.0);
        // le tracé vient ici
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, style::BORDURE), egui::StrokeKind::Inside);
    });

    fenetres(ui, e);
    if redemarrer {
        e.redemarrer();
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

fn menus(ui: &mut Ui, e: &mut EtatUi) {
    ui.menu_button("Fichier", |ui| {
        if ui.button("Quitter").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
    ui.menu_button("Audio", |ui| {
        if ui.button("Réglages des blocs").clicked() {
            e.fenetre_reglages = true;
            ui.close();
        }
        if ui.button("Rechercher les périphériques").clicked() {
            e.entrees = audio::lister_entrees();
            e.sorties = audio::lister_sorties();
            ui.close();
        }
    });
    ui.menu_button("Affichage", |ui| {
        ui.radio_value(&mut e.vue, Vue::Signal, "Signal");
        ui.radio_value(&mut e.vue, Vue::Spectre, "Spectre");
    });
    ui.menu_button("Aide", |ui| {
        if ui.button("Les blocs").clicked() {
            e.fenetre_aide = true;
            ui.close();
        }
        ui.label("Table de mixage, TP de traitement du signal");
    });
}

fn texte_source(s: &SourceMicro) -> String {
    match s {
        SourceMicro::Aucun => "Aucun".into(),
        SourceMicro::ParDefaut => "Micro par défaut".into(),
        SourceMicro::Peripherique(n) => n.clone(),
        SourceMicro::Telephone => "Téléphone".into(),
    }
}

/// Le contenu d'une boîte de voie. Vrai si le périphérique a changé.
fn boite_voie(ui: &mut Ui, e: &mut EtatUi, i: usize, maintenant: f64) -> bool {
    let mut change = false;
    match i {
        0 | 1 => {
            let source = if i == 0 { &mut e.choix.micro1 } else { &mut e.choix.micro2 };
            let avant = source.clone();
            egui::ComboBox::from_id_salt(("source", i))
                .width(LARGEUR_BOITE - 8.0)
                .selected_text(texte_source(source))
                .show_ui(ui, |ui| {
                    ui.selectable_value(source, SourceMicro::Aucun, "Aucun");
                    ui.selectable_value(source, SourceMicro::ParDefaut, "Micro par défaut");
                    for n in &e.entrees {
                        ui.selectable_value(source, SourceMicro::Peripherique(n.clone()), n);
                    }
                });
            change = *source != avant;
        }
        4 => {
            let t = &mut e.reglages.type_bruit;
            egui::ComboBox::from_id_salt("bruit")
                .width(LARGEUR_BOITE - 8.0)
                .selected_text(match t {
                    TypeBruit::Blanc => "Bruit blanc",
                    TypeBruit::Ronflement50 => "Ronflement 50 Hz",
                    TypeBruit::Souffle => "Souffle",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(t, TypeBruit::Blanc, "Bruit blanc");
                    ui.selectable_value(t, TypeBruit::Ronflement50, "Ronflement 50 Hz");
                    ui.selectable_value(t, TypeBruit::Souffle, "Souffle");
                });
        }
        _ => {
            ui.label(RichText::new("bientôt").color(style::TEXTE_GRIS));
        }
    }

    let v = &mut e.reglages.voies[i];
    ui.horizontal(|ui| {
        curseur_db(ui, &mut v.volume_db, HAUTEUR_CURSEUR);
        vumetre(ui, e.affiches[i], HAUTEUR_CURSEUR);
    });
    ui.horizontal(|ui| {
        ui.label("Volume");
        ui.add(egui::DragValue::new(&mut v.volume_db).range(DB_MIN..=DB_MAX).speed(0.2).custom_formatter(|x, _| texte_db(x as f32)));
    });
    for (nom, g) in [("Graves", &mut v.graves_db), ("Médiums", &mut v.mediums_db), ("Aigus", &mut v.aigus_db)] {
        ui.horizontal(|ui| {
            ui.add_sized([52.0, 16.0], egui::Label::new(nom));
            ui.spacing_mut().slider_width = 60.0;
            ui.add(egui::Slider::new(g, -12.0..=12.0).step_by(1.0).show_value(false));
        });
    }
    ui.checkbox(&mut v.anti_bruit, "Anti-bruit");
    ui.checkbox(&mut v.coupe_cri, "Coupe-cri");
    ui.checkbox(&mut v.limiteur, "Limiteur");
    ui.checkbox(&mut v.muet, "Muet");

    let micro_ouvert = |s: &SourceMicro| !matches!(s, SourceMicro::Aucun);
    let etat = if i < 2 && e.retour.perdu[i] {
        Some(("Périphérique perdu", style::ROUGE))
    } else if i < 2
        && micro_ouvert(if i == 0 { &e.choix.micro1 } else { &e.choix.micro2 })
        && maintenant - e.dernier_signal[i] > 2.0
    {
        Some(("Aucun signal, micro coupé ?", style::ROUGE))
    } else if e.retour.coupe[i] {
        Some(("Cri coupé", style::ROUGE))
    } else {
        None
    };
    ui.label(match etat {
        Some((t, c)) => RichText::new(t).color(c),
        None => RichText::new(" "),
    });
    change
}

fn boite_sortie(ui: &mut Ui, e: &mut EtatUi) -> bool {
    let avant = e.choix.sortie.clone();
    egui::ComboBox::from_id_salt("sortie")
        .width(LARGEUR_BOITE - 8.0)
        .selected_text(e.choix.sortie.clone().unwrap_or_else(|| "Sortie par défaut".into()))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut e.choix.sortie, None, "Sortie par défaut");
            for n in &e.sorties {
                ui.selectable_value(&mut e.choix.sortie, Some(n.clone()), n);
            }
        });
    ui.horizontal(|ui| {
        curseur_db(ui, &mut e.reglages.master_db, HAUTEUR_CURSEUR);
        vumetre(ui, e.affiches[NB_VOIES], HAUTEUR_CURSEUR);
        vumetre(ui, e.affiches[NB_VOIES + 1], HAUTEUR_CURSEUR);
    });
    ui.horizontal(|ui| {
        ui.label("Volume");
        ui.add(
            egui::DragValue::new(&mut e.reglages.master_db)
                .range(DB_MIN..=DB_MAX)
                .speed(0.2)
                .custom_formatter(|x, _| texte_db(x as f32)),
        );
    });
    ui.checkbox(&mut e.reglages.master_limiteur, "Limiteur");
    if e.retour.sortie_perdue {
        ui.colored_label(style::ROUGE, "Sortie perdue");
    }
    e.choix.sortie != avant
}

fn fenetres(ui: &mut Ui, e: &mut EtatUi) {
    let ctx = ui.ctx().clone();
    egui::Window::new("Réglages des blocs").open(&mut e.fenetre_reglages).resizable(false).show(&ctx, |ui| {
        egui::Grid::new("seuils").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("");
            ui.label("Seuil limiteur");
            ui.label("Seuil coupe-cri");
            ui.label("");
            ui.end_row();
            for i in 0..NB_VOIES {
                let v = &mut e.reglages.voies[i];
                ui.label(NOMS[i]);
                ui.add(egui::DragValue::new(&mut v.seuil_limiteur_db).range(-40.0..=0.0).speed(0.2).suffix(" dB"));
                ui.add(egui::DragValue::new(&mut v.seuil_cri_db).range(-40.0..=0.0).speed(0.2).suffix(" dB"));
                if ui.button("Apprendre le bruit").on_hover_text("1 s de la pièce sans parler").clicked() {
                    v.apprendre = v.apprendre.wrapping_add(1);
                    v.anti_bruit = true;
                }
                ui.end_row();
            }
        });
    });
    egui::Window::new("Les blocs").open(&mut e.fenetre_aide).resizable(false).show(&ctx, |ui| {
        ui.label("Anti-bruit   |Y(f)| = max(|X(f)| − 2 N(f), 0,05 |X(f)|), puis une porte sous -50 dBFS");
        ui.label("Coupe-cri   le son est coupé quand sa valeur efficace dépasse le seuil T");
        ui.label("Limiteur   y = g x, g = min(1, S / |x|)");
        ui.label("Égaliseur   h = gG h_passe-bas + gM h_passe-bande + gA h_passe-haut");
    });
}
