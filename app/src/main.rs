#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Table de mixage")
            .with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("Table de mixage", options, Box::new(|_cc| Ok(Box::new(App))))
}

struct App;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        eframe::egui::Frame::central_panel(ui.style()).show(ui, |_ui| {});
    }
}
