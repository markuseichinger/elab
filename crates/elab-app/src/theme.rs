//! Farben und Grundstil – identisch zu nnlab, damit beide Lern-Apps zusammenpassen.

use egui::Color32;
use std::cell::Cell;

pub const ACCENT: Color32 = Color32::from_rgb(255, 150, 50);
pub const BLUE: Color32 = Color32::from_rgb(70, 160, 255);
pub const GREEN: Color32 = Color32::from_rgb(120, 209, 81);
pub const RED: Color32 = Color32::from_rgb(240, 90, 80);
pub const VIOLET: Color32 = Color32::from_rgb(160, 120, 255);
pub const PINK: Color32 = Color32::from_rgb(230, 90, 160);
pub const CYAN: Color32 = Color32::from_rgb(60, 210, 200);

/// Hintergrund der Zeichenfläche (wie die Netzgrafik in nnlab).
pub const CANVAS_BG: Color32 = Color32::from_rgb(16, 17, 21);

/// Farben der Messpunkte, in derselben Reihenfolge im Oszilloskop.
pub const PROBE_COLORS: [Color32; 6] = [ACCENT, BLUE, GREEN, VIOLET, PINK, CYAN];

pub fn probe_color(idx: usize) -> Color32 {
    PROBE_COLORS[idx % PROBE_COLORS.len()]
}

/// Farbe einer Knotenspannung: grau = 0 V, orange = positiv, blau = negativ.
pub fn volt_color(v: f64, vmax: f64) -> Color32 {
    let t = (v / vmax.max(1e-9)).clamp(-1.0, 1.0) as f32;
    let g = [125, 130, 142];
    if t >= 0.0 {
        crate::symbols::lerp_rgb(g, [255, 145, 45], t.powf(0.7))
    } else {
        crate::symbols::lerp_rgb(g, [70, 160, 255], (-t).powf(0.7))
    }
}

// ---------------------------------------------------------------------------
// Hell und dunkel
// ---------------------------------------------------------------------------
//
// Die Bedienleisten folgen dem System (hell oder dunkel). Zeichenfläche und Tutorial sind
// immer dunkel. Textfarben hängen deshalb davon ab, wo gerade gezeichnet wird: `set_dark`
// schaltet für den jeweiligen Bereich um.

thread_local! {
    static DARK: Cell<bool> = const { Cell::new(true) };
}

/// Wird gerade auf dunklem Grund gezeichnet?
pub fn set_dark(dark: bool) {
    DARK.with(|d| d.set(dark));
}

pub fn is_dark() -> bool {
    DARK.with(|d| d.get())
}

/// Nebentexte (Beschriftungen, Hinweise): gut lesbar, aber zurückhaltender als Fließtext
pub fn muted() -> Color32 {
    if is_dark() {
        Color32::from_rgb(178, 184, 196)
    } else {
        Color32::from_rgb(82, 88, 100)
    }
}

/// Fließtext
pub fn text() -> Color32 {
    if is_dark() {
        Color32::from_rgb(232, 234, 240)
    } else {
        Color32::from_rgb(24, 26, 32)
    }
}

/// Dunkler Stil mit hellem, kontrastreichem Text (Tutorial, Zeichenfläche)
pub fn dark_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(232, 234, 240);
    v.widgets.inactive.fg_stroke.color = Color32::from_rgb(225, 228, 235);
    v.widgets.hovered.fg_stroke.color = Color32::WHITE;
    v.panel_fill = Color32::from_rgb(24, 26, 31);
    v.window_fill = Color32::from_rgb(28, 30, 36);
    v
}

/// Heller Stil mit dunklem Text (Bedienleisten bei hellem System)
pub fn light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(24, 26, 32);
    v.widgets.inactive.fg_stroke.color = Color32::from_rgb(30, 32, 38);
    v
}
