//! Schaltzeichen nach IEC 60617 / DIN EN 60617.
//!
//! Alle Symbole werden in lokalen Rasterkoordinaten gezeichnet. Zweipole liegen
//! waagerecht zwischen (−2,0) und (2,0), Quellen senkrecht mit dem Pluspol bei (0,−2).

use egui::{Color32, Painter, Pos2, Shape, Stroke};
use elab_app::model::{rot_f, PartKind};
use elab_sim::digital::Logic;
use std::f32::consts::PI;

/// Zeichenstift mit Verschiebung, Maßstab und Drehung.
pub struct Pen<'a> {
    pub painter: &'a Painter,
    pub origin: Pos2,
    /// Pixel pro Rastereinheit
    pub s: f32,
    pub rot: u8,
    pub color: Color32,
    pub width: f32,
}

impl Pen<'_> {
    pub fn pt(&self, x: f32, y: f32) -> Pos2 {
        let (rx, ry) = rot_f(x, y, self.rot);
        self.origin + egui::vec2(rx, ry) * self.s
    }

    fn stroke(&self) -> Stroke {
        Stroke::new(self.width, self.color)
    }

    pub fn line(&self, a: (f32, f32), b: (f32, f32)) {
        self.painter.line_segment([self.pt(a.0, a.1), self.pt(b.0, b.1)], self.stroke());
    }

    pub fn thick(&self, a: (f32, f32), b: (f32, f32), f: f32) {
        self.painter.line_segment([self.pt(a.0, a.1), self.pt(b.0, b.1)], Stroke::new(self.width * f, self.color));
    }

    pub fn poly(&self, pts: &[(f32, f32)], closed: bool) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        if closed {
            self.painter.add(Shape::closed_line(v, self.stroke()));
        } else {
            self.painter.add(Shape::line(v, self.stroke()));
        }
    }

    pub fn fill(&self, pts: &[(f32, f32)], fill: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.painter.add(Shape::convex_polygon(v, fill, Stroke::NONE));
    }

    /// Aufrechter Text (dreht nicht mit), Größe in Rastereinheiten.
    pub fn text(&self, at: (f32, f32), text: &str, size: f32, color: Color32) {
        self.painter.text(self.pt(at.0, at.1), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional((size * self.s).max(7.0)), color);
    }

    /// Anschlussleitung, bei Digitalbausteinen im Farbton des Pegels.
    fn lead(&self, a: (f32, f32), b: (f32, f32), v: Option<Logic>) {
        let col = v.map_or(self.color, logic_color);
        self.painter.line_segment([self.pt(a.0, a.1), self.pt(b.0, b.1)], Stroke::new(self.width, col));
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, fill: Color32) {
        let r = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        self.fill(&r, fill);
        self.poly(&r, true);
    }

    pub fn circle(&self, c: (f32, f32), r: f32) {
        self.painter.circle_stroke(self.pt(c.0, c.1), r * self.s, self.stroke());
    }

    pub fn disc(&self, c: (f32, f32), r: f32, fill: Color32) {
        self.painter.circle_filled(self.pt(c.0, c.1), r * self.s, fill);
    }

    fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), len: f32) {
        let n = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
        let (dx, dy) = (dir.0 / n, dir.1 / n);
        let (px, py) = (-dy, dx);
        let back = (tip.0 - dx * len, tip.1 - dy * len);
        self.fill(&[tip, (back.0 + px * len * 0.5, back.1 + py * len * 0.5), (back.0 - px * len * 0.5, back.1 - py * len * 0.5)], self.color);
    }
}

/// Zusätzliche Darstellungsinformationen aus der Simulation.
#[derive(Clone, Default)]
pub struct Live {
    /// Helligkeit 0 … 1 für LED und Lampe
    pub glow: f32,
    /// Logikpegel je Anschluss (leer, wenn nicht digital simuliert)
    pub pins: Vec<Logic>,
    /// Taster gedrückt
    pub pressed: bool,
}

/// Farbe eines Logikpegels (Drähte, Anschlüsse, Anzeigen).
pub fn logic_color(v: Logic) -> Color32 {
    match v {
        Logic::L1 => Color32::from_rgb(120, 220, 90),
        Logic::L0 => Color32::from_rgb(58, 96, 70),
        Logic::X => Color32::from_rgb(240, 90, 80),
        Logic::Z => Color32::from_rgb(110, 120, 170),
    }
}

/// Segmente a–g für die Hexziffern 0–F.
pub fn seven_seg(v: u8) -> [bool; 7] {
    const T: [u8; 16] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F, 0x77, 0x7C, 0x39, 0x5E, 0x79, 0x71];
    let b = T[(v & 15) as usize];
    std::array::from_fn(|k| b >> k & 1 == 1)
}

/// Zeichnet das Schaltzeichen eines Bauteils (ohne Beschriftung; Messpunkte zeichnet der Editor).
pub fn draw(pen: &Pen, kind: &PartKind, live: Live) {
    let body_fill = Color32::from_rgb(28, 30, 36);
    match kind {
        PartKind::Resistor { .. } => {
            pen.line((-2.0, 0.0), (-1.0, 0.0));
            pen.line((1.0, 0.0), (2.0, 0.0));
            let r = [(-1.0, -0.4), (1.0, -0.4), (1.0, 0.4), (-1.0, 0.4)];
            pen.fill(&r, body_fill);
            pen.poly(&r, true);
        }
        PartKind::Lamp { .. } => {
            let g = live.glow.clamp(0.0, 1.0);
            if g > 0.01 {
                for (k, rr) in [(0, 1.9), (1, 1.4), (2, 1.0)] {
                    let a = (g * [40.0, 70.0, 120.0][k]) as u8;
                    pen.disc((0.0, 0.0), rr * (0.6 + 0.4 * g), Color32::from_rgba_unmultiplied(255, 200, 80, a));
                }
            }
            pen.line((-2.0, 0.0), (-0.7, 0.0));
            pen.line((0.7, 0.0), (2.0, 0.0));
            pen.disc((0.0, 0.0), 0.7, if g > 0.01 { lerp_rgb([40, 36, 28], [255, 225, 140], g) } else { body_fill });
            pen.circle((0.0, 0.0), 0.7);
            let d = 0.7 * std::f32::consts::FRAC_1_SQRT_2;
            pen.line((-d, -d), (d, d));
            pen.line((-d, d), (d, -d));
        }
        PartKind::Capacitor { .. } => {
            pen.line((-2.0, 0.0), (-0.25, 0.0));
            pen.line((0.25, 0.0), (2.0, 0.0));
            pen.thick((-0.25, -0.8), (-0.25, 0.8), 1.7);
            pen.thick((0.25, -0.8), (0.25, 0.8), 1.7);
        }
        PartKind::Inductor { .. } => {
            pen.line((-2.0, 0.0), (-1.2, 0.0));
            pen.line((1.2, 0.0), (2.0, 0.0));
            let mut pts = Vec::new();
            for k in 0..4 {
                let cx = -0.9 + 0.6 * k as f32;
                for j in 0..=10 {
                    let t = PI * j as f32 / 10.0;
                    pts.push((cx - 0.3 * t.cos(), -0.45 * t.sin()));
                }
            }
            pen.poly(&pts, false);
        }
        PartKind::Diode { .. } => diode(pen, Color32::from_rgb(48, 52, 62)),
        PartKind::Led { color } => {
            let [r, g, b] = color.rgb();
            let lit = live.glow.clamp(0.0, 1.0);
            if lit > 0.02 {
                for (rr, a) in [(2.0, 35.0), (1.5, 60.0), (1.05, 110.0)] {
                    pen.disc((0.0, 0.0), rr * (0.55 + 0.45 * lit), Color32::from_rgba_unmultiplied(r, g, b, (a * lit) as u8));
                }
            }
            let fill = lerp_rgb([r / 4, g / 4, b / 4], [r, g, b], lit);
            diode(pen, fill);
            // Lichtpfeile nach außen
            for dx in [-0.15, 0.35] {
                let a = (dx, -0.75);
                let b2 = (dx + 0.45, -1.25);
                pen.line(a, b2);
                pen.arrow_head(b2, (0.45, -0.5), 0.22);
            }
        }
        PartKind::Switch { closed } => {
            pen.line((-2.0, 0.0), (-1.0, 0.0));
            pen.line((1.0, 0.0), (2.0, 0.0));
            pen.disc((-1.0, 0.0), 0.12, pen.color);
            pen.circle((1.0, 0.0), 0.12);
            if *closed {
                pen.line((-1.0, 0.0), (1.0, -0.12));
            } else {
                pen.line((-1.0, 0.0), (0.85, -0.8));
            }
        }
        PartKind::DcVoltage { .. } => {
            pen.line((0.0, -2.0), (0.0, -0.22));
            pen.line((0.0, 0.22), (0.0, 2.0));
            pen.thick((-0.85, -0.22), (0.85, -0.22), 1.2);
            pen.thick((-0.42, 0.22), (0.42, 0.22), 2.6);
            plus(pen, (0.75, -0.8));
        }
        PartKind::AcVoltage { .. } => {
            source_circle(pen);
            let pts: Vec<(f32, f32)> = (0..=24)
                .map(|j| {
                    let x = -0.55 + 1.1 * j as f32 / 24.0;
                    (x, -0.32 * (2.0 * PI * (x + 0.55) / 1.1).sin())
                })
                .collect();
            pen.poly(&pts, false);
            plus(pen, (0.7, -1.45));
        }
        PartKind::SquareVoltage { .. } => {
            source_circle(pen);
            pen.poly(&[(-0.55, 0.3), (-0.55, -0.3), (0.0, -0.3), (0.0, 0.3), (0.55, 0.3), (0.55, -0.3)], false);
            plus(pen, (0.7, -1.45));
        }
        PartKind::CurrentSource { .. } => {
            source_circle(pen);
            pen.line((0.0, 0.55), (0.0, -0.3));
            pen.arrow_head((0.0, -0.6), (0.0, -1.0), 0.35);
        }
        PartKind::Ground => {
            pen.line((0.0, 0.0), (0.0, 0.5));
            pen.thick((-0.7, 0.5), (0.7, 0.5), 1.3);
            pen.thick((-0.45, 0.8), (0.45, 0.8), 1.3);
            pen.thick((-0.2, 1.1), (0.2, 1.1), 1.3);
        }
        PartKind::Probe => {
            pen.disc((0.0, 0.0), 0.18, pen.color);
        }
        _ => draw_digital(pen, kind, &live),
    }
}

/// IEC-Schaltzeichen der Digitalbausteine.
fn draw_digital(pen: &Pen, kind: &PartKind, live: &Live) {
    let body = Color32::from_rgb(28, 30, 36);
    let pin = |k: usize| live.pins.get(k).copied();
    let txt = Color32::from_rgb(220, 222, 230);
    match kind {
        PartKind::Gate { gate } => {
            let n_in = gate.inputs();
            let (h, ys): (f32, &[f32]) = if n_in == 1 { (1.5, &[0.0]) } else { (2.0, &[-1.0, 1.0]) };
            for (k, &y) in ys.iter().enumerate() {
                pen.lead((-3.0, y), (-1.5, y), pin(k));
            }
            let out = pin(n_in);
            if gate.inverted() {
                pen.lead((2.0, 0.0), (3.0, 0.0), out);
            } else {
                pen.lead((1.5, 0.0), (3.0, 0.0), out);
            }
            pen.rect(-1.5, -h, 1.5, h, body);
            if gate.inverted() {
                pen.disc((1.75, 0.0), 0.25, body);
                pen.circle((1.75, 0.0), 0.25);
            }
            pen.text((0.0, -h + 0.75), gate.symbol(), 0.85, txt);
        }
        PartKind::LogicInput { on } => {
            let v = Logic::from_bool(*on);
            pen.lead((1.0, 0.0), (2.0, 0.0), pin(0).or(Some(v)));
            pen.rect(-1.8, -0.9, 1.0, 0.9, body);
            // Schiebeschalter
            let knob = if *on { 0.25 } else { -1.05 };
            pen.fill(&[(-1.4, -0.45), (0.6, -0.45), (0.6, 0.45), (-1.4, 0.45)], Color32::from_rgb(40, 43, 50));
            pen.fill(&[(knob, -0.45), (knob + 0.35, -0.45), (knob + 0.35, 0.45), (knob, 0.45)], logic_color(v));
            pen.text(if *on { (-0.6, 0.0) } else { (0.15, 0.0) }, if *on { "1" } else { "0" }, 0.75, txt);
        }
        PartKind::Button => {
            let v = Logic::from_bool(live.pressed);
            pen.lead((1.0, 0.0), (2.0, 0.0), pin(0).or(Some(v)));
            pen.rect(-1.8, -0.9, 1.0, 0.9, body);
            pen.disc((-0.4, 0.0), 0.55, if live.pressed { logic_color(Logic::L1) } else { Color32::from_rgb(70, 74, 84) });
            pen.circle((-0.4, 0.0), 0.62);
        }
        PartKind::Clock { .. } => {
            pen.lead((1.0, 0.0), (2.0, 0.0), pin(0));
            pen.rect(-1.8, -1.0, 1.0, 1.0, body);
            let c = pin(0).map_or(pen.color, logic_color);
            let w = [(-1.4, 0.45), (-1.0, 0.45), (-1.0, -0.45), (-0.4, -0.45), (-0.4, 0.45), (0.2, 0.45), (0.2, -0.45), (0.6, -0.45)];
            let v: Vec<Pos2> = w.iter().map(|&(x, y)| pen.pt(x, y)).collect();
            pen.painter.add(Shape::line(v, Stroke::new(pen.width, c)));
        }
        PartKind::LogicHigh => {
            pen.lead((0.0, 0.0), (0.0, -0.7), pin(0).or(Some(Logic::L1)));
            pen.thick((-0.6, -0.7), (0.6, -0.7), 1.3);
            pen.text((0.0, -1.2), "1", 0.7, txt);
        }
        PartKind::LogicLed { color } => {
            let v = pin(0);
            pen.lead((-2.0, 0.0), (-0.7, 0.0), v);
            let [r, g, b] = color.rgb();
            match v {
                Some(Logic::L1) => {
                    for (rr, a) in [(1.6, 40.0), (1.2, 70.0), (0.95, 120.0)] {
                        pen.disc((0.0, 0.0), rr, Color32::from_rgba_unmultiplied(r, g, b, a as u8));
                    }
                    pen.disc((0.0, 0.0), 0.7, Color32::from_rgb(r, g, b));
                }
                Some(Logic::X) | Some(Logic::Z) => {
                    pen.disc((0.0, 0.0), 0.7, Color32::from_rgb(70, 40, 40));
                    pen.text((0.0, 0.0), "?", 0.8, logic_color(Logic::X));
                }
                _ => pen.disc((0.0, 0.0), 0.7, Color32::from_rgb(r / 5, g / 5, b / 5)),
            }
            pen.circle((0.0, 0.0), 0.7);
        }
        PartKind::DFlipFlop | PartKind::JkFlipFlop => {
            let jk = matches!(kind, PartKind::JkFlipFlop);
            let ins: &[(f32, &str)] = if jk { &[(-2.0, "1J"), (0.0, "C1"), (2.0, "1K")] } else { &[(-2.0, "1D"), (2.0, "C1")] };
            for (k, &(y, _)) in ins.iter().enumerate() {
                pen.lead((-3.0, y), (-2.0, y), pin(k));
            }
            let n = ins.len();
            pen.lead((2.0, -2.0), (3.0, -2.0), pin(n));
            pen.lead((2.0, 2.0), (3.0, 2.0), pin(n + 1));
            pen.rect(-2.0, -3.2, 2.0, 3.2, body);
            for &(y, label) in ins {
                pen.text((-1.25, y), label, 0.55, txt);
                if label == "C1" {
                    pen.poly(&[(-2.0, y - 0.35), (-1.65, y), (-2.0, y + 0.35)], false);
                }
            }
            pen.text((1.4, -2.0), "Q", 0.6, txt);
            pen.text((1.4, 2.0), "Q", 0.6, txt);
            pen.line((1.22, 1.55), (1.58, 1.55)); // Querstrich: /Q
            if let Some(q) = pin(n) {
                pen.disc((0.4, -2.0), 0.28, logic_color(q));
            }
        }
        PartKind::Counter4 => {
            pen.lead((-3.0, -2.0), (-2.0, -2.0), pin(0));
            pen.lead((-3.0, 2.0), (-2.0, 2.0), pin(1));
            for k in 0..4 {
                let y = -3.0 + 2.0 * k as f32;
                pen.lead((2.0, y), (3.0, y), pin(2 + k));
            }
            pen.rect(-2.0, -4.4, 2.0, 4.4, body);
            pen.text((0.0, -3.8), "CTR4", 0.55, txt);
            pen.text((-1.35, -2.0), "+", 0.7, txt);
            pen.poly(&[(-2.0, -2.35), (-1.65, -2.0), (-2.0, -1.65)], false);
            pen.text((-1.35, 2.0), "R", 0.6, txt);
            for k in 0..4 {
                pen.text((1.45, -3.0 + 2.0 * k as f32), &k.to_string(), 0.5, txt);
            }
            let bits: Option<u8> = (0..4).map(|k| pin(2 + k)).try_fold(0u8, |acc, v| match v {
                Some(Logic::L1) => Some(acc),
                Some(Logic::L0) => Some(acc),
                _ => None,
            });
            if bits.is_some() {
                let n: u8 = (0..4).filter(|&k| pin(2 + k) == Some(Logic::L1)).map(|k| 1 << k).sum();
                pen.text((0.0, 0.0), &n.to_string(), 1.1, Color32::from_rgb(120, 220, 90));
            }
        }
        PartKind::SevenSeg => {
            for k in 0..4 {
                let y = -3.0 + 2.0 * k as f32;
                pen.lead((-3.0, y), (-2.0, y), pin(k));
            }
            pen.rect(-2.0, -4.4, 2.8, 4.4, Color32::from_rgb(14, 15, 18));
            for (k, l) in ["1", "2", "4", "8"].iter().enumerate() {
                pen.text((-1.5, -3.0 + 2.0 * k as f32), l, 0.45, Color32::from_rgb(140, 145, 155));
            }
            // Ziffer aus den Eingängen
            let vals: Vec<Option<Logic>> = (0..4).map(pin).collect();
            let valid = vals.iter().all(|v| matches!(v, Some(Logic::L0) | Some(Logic::L1)));
            let n: u8 = (0..4).filter(|&k| vals[k] == Some(Logic::L1)).map(|k| 1u8 << k).sum();
            let segs = if valid { seven_seg(n) } else { [false, false, false, false, false, false, true] };
            let on = if valid { Color32::from_rgb(255, 70, 50) } else { Color32::from_rgb(240, 90, 80).gamma_multiply(0.6) };
            let off = Color32::from_rgb(45, 22, 20);
            let (cx, w, hh, t) = (0.55, 1.5, 2.6, 0.32);
            let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);
            let (yt, ym, yb) = (-hh, 0.0, hh);
            let hseg = |y: f32| [(x0 + t * 0.6, y - t / 2.0), (x1 - t * 0.6, y - t / 2.0), (x1 - t * 0.6, y + t / 2.0), (x0 + t * 0.6, y + t / 2.0)];
            let vseg = |x: f32, ya: f32, yb2: f32| {
                [(x - t / 2.0, ya + t * 0.6), (x + t / 2.0, ya + t * 0.6), (x + t / 2.0, yb2 - t * 0.6), (x - t / 2.0, yb2 - t * 0.6)]
            };
            let shapes: [[(f32, f32); 4]; 7] = [hseg(yt), vseg(x1, yt, ym), vseg(x1, ym, yb), hseg(yb), vseg(x0, ym, yb), vseg(x0, yt, ym), hseg(ym)];
            for (k, sh) in shapes.iter().enumerate() {
                pen.fill(sh, if segs[k] { on } else { off });
            }
        }
        _ => {}
    }
}

fn diode(pen: &Pen, fill: Color32) {
    pen.line((-2.0, 0.0), (-0.5, 0.0));
    pen.line((0.5, 0.0), (2.0, 0.0));
    let tri = [(-0.5, -0.55), (-0.5, 0.55), (0.5, 0.0)];
    pen.fill(&tri, fill);
    pen.poly(&tri, true);
    pen.thick((0.5, -0.55), (0.5, 0.55), 1.3);
}

fn source_circle(pen: &Pen) {
    pen.line((0.0, -2.0), (0.0, -0.9));
    pen.line((0.0, 0.9), (0.0, 2.0));
    pen.disc((0.0, 0.0), 0.9, Color32::from_rgb(28, 30, 36));
    pen.circle((0.0, 0.0), 0.9);
}

fn plus(pen: &Pen, c: (f32, f32)) {
    let h = 0.17;
    pen.line((c.0 - h, c.1), (c.0 + h, c.1));
    pen.line((c.0, c.1 - h), (c.0, c.1 + h));
}

pub fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]))
}
