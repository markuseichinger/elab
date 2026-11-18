//! Farben und Grundstil – identisch zu nnlab, damit beide Lern-Apps zusammenpassen.

use egui::Color32;

pub const ACCENT: Color32 = Color32::from_rgb(255, 150, 50);
pub const BLUE: Color32 = Color32::from_rgb(70, 160, 255);
pub const GREEN: Color32 = Color32::from_rgb(120, 209, 81);
pub const RED: Color32 = Color32::from_rgb(240, 90, 80);
pub const VIOLET: Color32 = Color32::from_rgb(160, 120, 255);
/// Nebentexte: deutlich heller als egui-Grau, damit sie auf dunklem Grund lesbar bleiben
pub const MUTED: Color32 = Color32::from_rgb(170, 176, 188);
/// Fließtext
pub const TEXT: Color32 = Color32::from_rgb(222, 225, 232);
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
