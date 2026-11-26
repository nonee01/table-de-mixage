//! L'allure d'un outil Windows ordinaire : fond gris clair, cadres de groupe, cases à cocher, pas de décor.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, Response,
    Sense, Shadow, Stroke, Ui,
};

pub const FOND: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf0);
pub const BORDURE: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
pub const GRILLE: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xe6);
pub const BORDURE_CHOISIE: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);
pub const ENTREE: Color32 = Color32::from_rgb(0x1f, 0x77, 0xb4);
pub const SORTIE: Color32 = Color32::from_rgb(0xff, 0x7f, 0x0e);
pub const TEXTE_GRIS: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
pub const VERT: Color32 = Color32::from_rgb(0x06, 0xb0, 0x25);
pub const ROUGE: Color32 = Color32::from_rgb(0xc4, 0x2b, 0x1c);

/// Plage des curseurs de volume (dB) ; le bas est affiché -∞.
pub const DB_MIN: f32 = -60.0;
pub const DB_MAX: f32 = 6.0;

fn police() -> Vec<u8> {
    #[cfg(windows)]
    if let Ok(o) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        return o;
    }
    include_bytes!("../assets/DejaVuSans.ttf").to_vec()
}

pub fn appliquer(ctx: &egui::Context) {
    let mut polices = FontDefinitions::default();
    polices.font_data.insert("ui".into(), std::sync::Arc::new(FontData::from_owned(police())));
    for famille in [FontFamily::Proportional, FontFamily::Monospace] {
        polices.families.entry(famille).or_default().insert(0, "ui".into());
    }
    ctx.set_fonts(polices);

    let mut v = egui::Visuals::light();
    v.panel_fill = FOND;
    v.window_fill = FOND;
    v.faint_bg_color = FOND;
    v.extreme_bg_color = Color32::WHITE;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(2);
    v.menu_corner_radius = CornerRadius::same(2);
    v.window_stroke = Stroke::new(1.0, Color32::from_gray(0xa0));
    v.selection.bg_fill = Color32::from_rgb(0x00, 0x78, 0xd7);
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(2);
        w.expansion = 0.0;
    }
    v.widgets.inactive.bg_fill = Color32::from_rgb(0xe1, 0xe1, 0xe1);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xe1, 0xe1, 0xe1);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xad));
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xe5, 0xf1, 0xfb);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe5, 0xf1, 0xfb);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x78, 0xd7));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDURE);
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        for (texte, taille) in [
            (egui::TextStyle::Body, 12.0),
            (egui::TextStyle::Button, 12.0),
            (egui::TextStyle::Small, 10.0),
            (egui::TextStyle::Monospace, 12.0),
            (egui::TextStyle::Heading, 14.0),
        ] {
            s.text_styles.insert(texte, FontId::proportional(taille));
        }
        s.spacing.item_spacing = vec2(6.0, 4.0);
        s.spacing.button_padding = vec2(6.0, 2.0);
    });
}

/// Cadre de groupe Windows : bordure fine, titre posé dans le trait du haut. Cliquer dedans le choisit.
pub fn boite_groupe(ui: &mut Ui, titre: &str, choisie: bool, largeur: f32, contenu: impl FnOnce(&mut Ui)) -> Response {
    ui.add_space(6.0);
    let trait_ = Stroke::new(1.0, if choisie { BORDURE_CHOISIE } else { BORDURE });
    let interieur = egui::Frame::new()
        .stroke(trait_)
        .corner_radius(CornerRadius::same(2))
        .inner_margin(egui::Margin { left: 6, right: 6, top: 12, bottom: 6 })
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(largeur);
                contenu(ui);
            });
        });
    let rect = interieur.response.rect;
    let police = FontId::proportional(12.0);
    let galee = ui.painter().layout_no_wrap(titre.to_string(), police, Color32::BLACK);
    let fond = Rect::from_min_size(pos2(rect.left() + 6.0, rect.top() - galee.size().y / 2.0), galee.size() + vec2(6.0, 0.0));
    ui.painter().rect_filled(fond, 0.0, FOND);
    ui.painter().galley(fond.min + vec2(3.0, 0.0), galee, Color32::BLACK);
    ui.interact(rect, ui.scope_id().with(titre), Sense::click())
}

fn fraction_db(db: f32) -> f32 {
    ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0)
}

/// Curseur de volume vertical avec ses graduations +6, 0, -20, -40, -∞.
pub fn curseur_db(ui: &mut Ui, valeur: &mut f32, hauteur: f32) -> Response {
    ui.horizontal(|ui| {
        let (graduations, _) = ui.allocate_exact_size(vec2(20.0, hauteur), Sense::hover());
        ui.spacing_mut().slider_width = hauteur;
        let r = ui.add(egui::Slider::new(valeur, DB_MIN..=DB_MAX).vertical().show_value(false));
        // le rail du curseur laisse la place de la poignée à chaque bout
        let marge = r.rect.width() / 2.5;
        let (haut, bas) = (r.rect.top() + marge, r.rect.bottom() - marge);
        for (v, t) in [(6.0, "+6"), (0.0, "0"), (-20.0, "-20"), (-40.0, "-40"), (DB_MIN, "-∞")] {
            let y = bas - fraction_db(v) * (bas - haut);
            ui.painter().text(pos2(graduations.right() - 2.0, y), Align2::RIGHT_CENTER, t, FontId::proportional(9.0), TEXTE_GRIS);
        }
        r
    })
    .inner
}

/// Barre verte de niveau, sur la même échelle que le curseur. `crete` linéaire.
pub fn vumetre(ui: &mut Ui, crete: f32, hauteur: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(7.0, hauteur), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let db = 20.0 * crete.max(1e-9).log10();
    let h = fraction_db(db) * rect.height();
    let barre = Rect::from_min_max(pos2(rect.left(), rect.bottom() - h), rect.right_bottom());
    p.rect_filled(barre, 0.0, if db > -1.0 { ROUGE } else { VERT });
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(0xa0)), egui::StrokeKind::Inside);
}

/// Texte d'un niveau en dB pour les cases : -∞ en bas de course.
pub fn texte_db(v: f32) -> String {
    if v <= DB_MIN { "-∞ dB".into() } else { format!("{v:.1} dB") }
}
