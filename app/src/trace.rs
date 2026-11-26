//! Le tracé : entrée en bleu, sortie en orange, en temps (200 ms) ou en fréquence (0 à 24 kHz).

use eframe::egui::Ui;
use egui_plot::{HLine, Legend, Line, LineStyle, Plot};
use moteur::FE;
use realfft::RealFftPlanner;
use ringbuf::traits::Consumer;
use ringbuf::HeapCons;

use crate::style;
use crate::ui::Vue;

/// Longueur de la TF du spectre.
pub const TF: usize = 4096;
const GARDE: usize = 9_600; // 200 ms

/// Spectre en dB par rapport à un sinus pleine échelle, fenêtre de Hann, cases de 0 à FE/2.
pub fn spectre_db(x: &[f32]) -> Vec<f32> {
    let mut t: Vec<f32> = (0..TF)
        .map(|i| {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / TF as f32).cos();
            x.get(i).copied().unwrap_or(0.0) * w
        })
        .collect();
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(TF);
    let mut s = fft.make_output_vec();
    let _ = fft.process(&mut t, &mut s);
    // un sinus d'amplitude 1 donne |X| = somme(w) / 2 = TF / 4 dans sa case
    let reference = TF as f32 / 4.0;
    s.iter().map(|c| 20.0 * (c.norm() / reference).max(1e-7).log10()).collect()
}

pub struct Trace {
    entree: Vec<f32>,
    sortie: Vec<f32>,
}

fn garder(v: &mut Vec<f32>, c: &mut HeapCons<f32>) {
    v.extend(c.pop_iter());
    if v.len() > GARDE {
        v.drain(..v.len() - GARDE);
    }
}

fn points_temps(x: &[f32]) -> Vec<[f64; 2]> {
    let debut = GARDE.saturating_sub(x.len());
    x.iter().enumerate().map(|(i, v)| [((debut + i) as f32 / FE * 1000.0) as f64, *v as f64]).collect()
}

fn points_spectre(x: &[f32]) -> Vec<[f64; 2]> {
    let fin = &x[x.len().saturating_sub(TF)..];
    spectre_db(fin).iter().enumerate().map(|(k, db)| [(k as f32 * FE / TF as f32) as f64, *db as f64]).collect()
}

impl Trace {
    pub fn new() -> Self {
        Trace { entree: Vec::with_capacity(2 * GARDE), sortie: Vec::with_capacity(2 * GARDE) }
    }

    pub fn alimenter(&mut self, e: &mut HeapCons<f32>, s: &mut HeapCons<f32>) {
        garder(&mut self.entree, e);
        garder(&mut self.sortie, s);
    }

    /// `seuil_limiteur_db` et `seuil_cri_db` valent None quand le bloc est arrêté : pas de trait.
    pub fn afficher(&mut self, ui: &mut Ui, vue: Vue, seuil_limiteur_db: Option<f32>, seuil_cri_db: Option<f32>) {
        let plot = Plot::new(("trace", vue as u8))
            .legend(Legend::default())
            .grid_color(style::GRILLE)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false);
        let (plot, entree, sortie) = match vue {
            Vue::Signal => (
                plot.default_x_bounds(0.0, 200.0).default_y_bounds(-1.05, 1.05).x_axis_label("t (ms)"),
                points_temps(&self.entree),
                points_temps(&self.sortie),
            ),
            Vue::Spectre => (
                plot.default_x_bounds(0.0, FE as f64 / 2.0).default_y_bounds(-100.0, 0.0).x_axis_label("f (Hz)").y_axis_label("dB"),
                points_spectre(&self.entree),
                points_spectre(&self.sortie),
            ),
        };
        plot.show(ui, |p| {
            p.line(Line::new("entrée", entree).color(style::ENTREE));
            p.line(Line::new("sortie", sortie).color(style::SORTIE));
            if vue == Vue::Signal {
                for (nom, seuil) in [("S", seuil_limiteur_db), ("T", seuil_cri_db)] {
                    if let Some(db) = seuil {
                        let a = 10f64.powf(db as f64 / 20.0);
                        for y in [a, -a] {
                            p.hline(HLine::new(nom, y).color(style::TEXTE_GRIS).style(LineStyle::dashed_loose()));
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sinus_pleine_echelle_a_zero_db() {
        // sur une case (85 · FE/TF = 996 Hz) : entre deux cases, la fenêtre de Hann perd jusqu'à 1,4 dB
        let f0 = 85.0 * moteur::FE / TF as f32;
        let x: Vec<f32> = (0..TF).map(|i| (std::f32::consts::TAU * f0 * i as f32 / moteur::FE).sin()).collect();
        let s = spectre_db(&x);
        let (k, max) = s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        assert!(max.abs() < 0.5, "max {max} dB");
        let f = k as f32 * moteur::FE / TF as f32;
        assert!((f - f0).abs() < 1.0, "pic à {f} Hz");
    }

    #[test]
    fn silence_tres_bas() {
        let s = spectre_db(&[0.0; TF]);
        assert!(s.iter().all(|v| !v.is_nan() && *v <= -120.0));
    }
}
