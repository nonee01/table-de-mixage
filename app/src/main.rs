#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anneau;
mod audio;
mod sources;
mod style;
mod trace;
mod ui;

use eframe::egui;
use moteur::table::ReglagesTable;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Table de mixage").with_inner_size([1060.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Table de mixage",
        options,
        Box::new(|cc| {
            style::appliquer(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

struct App {
    etat: ui::EtatUi,
    debug: bool,
    dernier_debug: f64,
    /// TABLE_DE_MIXAGE_CAPTURE=fichier.ppm : la fenêtre se photographie après 3 s puis se ferme.
    capture: Option<String>,
}

fn ecrire_ppm(chemin: &str, image: &egui::ColorImage) {
    let [l, h] = image.size;
    let mut octets = format!("P6\n{l} {h}\n255\n").into_bytes();
    for p in &image.pixels {
        octets.extend_from_slice(&[p.r(), p.g(), p.b()]);
    }
    if let Err(e) = std::fs::write(chemin, octets) {
        eprintln!("capture impossible: {e}");
    }
}

impl App {
    fn new() -> Self {
        let mut reglages = ReglagesTable::default();
        // TABLE_DE_MIXAGE_DEBUG=muet : on mesure les entrées sans rien envoyer aux haut-parleurs
        if std::env::var("TABLE_DE_MIXAGE_DEBUG").as_deref() == Ok("muet") {
            reglages.master_db = moteur::voie::VOLUME_MIN_DB;
        }
        let mut etat = ui::EtatUi::new(reglages);
        // un fichier donné en argument (« Ouvrir avec » sous Windows) est lancé tout de suite
        if let Some(chemin) = std::env::args_os().nth(1) {
            etat.jouer(chemin.into());
        }
        App {
            etat,
            debug: std::env::var_os("TABLE_DE_MIXAGE_DEBUG").is_some(),
            dernier_debug: 0.0,
            capture: std::env::var("TABLE_DE_MIXAGE_CAPTURE").ok(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::dessiner(ui, &mut self.etat);
        let maintenant = ui.input(|i| i.time);
        if maintenant > 3.0 {
            if let Some(chemin) = self.capture.take() {
                let rappel = egui::ScreenshotCallback::new(move |image| {
                    ecrire_ppm(&chemin, &image);
                    std::process::exit(0);
                });
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(rappel));
            }
        }
        if self.debug && maintenant - self.dernier_debug > 1.0 {
            self.dernier_debug = maintenant;
            let r = &self.etat.retour;
            eprintln!("[debug] cretes {:?} master {:?} latence {:.1} ms perdu {:?}", r.cretes, r.master, r.latence_ms, r.perdu);
        }
    }
}
