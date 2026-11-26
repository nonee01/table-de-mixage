#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anneau;
mod audio;

use audio::{Audio, ChoixAudio, LiensUi};
use eframe::egui;
use moteur::table::ReglagesTable;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Table de mixage").with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("Table de mixage", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}

struct App {
    choix: ChoixAudio,
    audio: Option<(Audio, LiensUi)>,
    erreur: Option<String>,
    reglages: ReglagesTable,
    debug: bool,
    dernier_debug: f64,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            choix: ChoixAudio::default(),
            audio: None,
            erreur: None,
            reglages: ReglagesTable::default(),
            debug: std::env::var_os("TABLE_DE_MIXAGE_DEBUG").is_some(),
            dernier_debug: 0.0,
        };
        // TABLE_DE_MIXAGE_DEBUG=muet : on mesure les entrées sans rien envoyer aux haut-parleurs
        if std::env::var("TABLE_DE_MIXAGE_DEBUG").as_deref() == Ok("muet") {
            app.reglages.master_db = -120.0;
        }
        app.redemarrer();
        app
    }

    fn redemarrer(&mut self) {
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let maintenant = ui.input(|i| i.time);
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            if let Some(e) = &self.erreur {
                ui.label(e);
            }
            if let Some((_, liens)) = &mut self.audio {
                liens.reglages.write(self.reglages);
                let r = *liens.retour.read();
                ui.label(format!("latence {:.0} ms", r.latence_ms));
                if self.debug && maintenant - self.dernier_debug > 1.0 {
                    self.dernier_debug = maintenant;
                    eprintln!("[debug] cretes {:?} master {:?} latence {:.1} ms perdu {:?}", r.cretes, r.master, r.latence_ms, r.perdu);
                }
            }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
}
