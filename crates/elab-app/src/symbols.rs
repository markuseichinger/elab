//! Schaltzeichen nach IEC 60617 / DIN EN 60617.
//!
//! Alle Symbole werden in lokalen Rasterkoordinaten gezeichnet. Zweipole liegen
//! waagerecht zwischen (−2,0) und (2,0), Quellen senkrecht mit dem Pluspol bei (0,−2).

use egui::{Color32, Painter, Pos2, Shape, Stroke};
use elab_app::model::{rot_f, PartKind};
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
        self.painter
            .line_segment([self.pt(a.0, a.1), self.pt(b.0, b.1)], self.stroke());
    }

    pub fn thick(&self, a: (f32, f32), b: (f32, f32), f: f32) {
        self.painter.line_segment(
            [self.pt(a.0, a.1), self.pt(b.0, b.1)],
            Stroke::new(self.width * f, self.color),
        );
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
        self.painter
            .add(Shape::convex_polygon(v, fill, Stroke::NONE));
    }

    pub fn circle(&self, c: (f32, f32), r: f32) {
        self.painter
            .circle_stroke(self.pt(c.0, c.1), r * self.s, self.stroke());
    }

    pub fn disc(&self, c: (f32, f32), r: f32, fill: Color32) {
        self.painter
            .circle_filled(self.pt(c.0, c.1), r * self.s, fill);
    }

    fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), len: f32) {
        let n = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
        let (dx, dy) = (dir.0 / n, dir.1 / n);
        let (px, py) = (-dy, dx);
        let back = (tip.0 - dx * len, tip.1 - dy * len);
        self.fill(
            &[
                tip,
                (back.0 + px * len * 0.5, back.1 + py * len * 0.5),
                (back.0 - px * len * 0.5, back.1 - py * len * 0.5),
            ],
            self.color,
        );
    }
}

/// Zusätzliche Darstellungsinformationen aus der Simulation.
#[derive(Clone, Copy, Default)]
pub struct Live {
    /// Helligkeit 0 … 1 für LED und Lampe
    pub glow: f32,
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
                    pen.disc(
                        (0.0, 0.0),
                        rr * (0.6 + 0.4 * g),
                        Color32::from_rgba_unmultiplied(255, 200, 80, a),
                    );
                }
            }
            pen.line((-2.0, 0.0), (-0.7, 0.0));
            pen.line((0.7, 0.0), (2.0, 0.0));
            pen.disc(
                (0.0, 0.0),
                0.7,
                if g > 0.01 {
                    lerp_rgb([40, 36, 28], [255, 225, 140], g)
                } else {
                    body_fill
                },
            );
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
                    pen.disc(
                        (0.0, 0.0),
                        rr * (0.55 + 0.45 * lit),
                        Color32::from_rgba_unmultiplied(r, g, b, (a * lit) as u8),
                    );
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
            pen.poly(
                &[
                    (-0.55, 0.3),
                    (-0.55, -0.3),
                    (0.0, -0.3),
                    (0.0, 0.3),
                    (0.55, 0.3),
                    (0.55, -0.3),
                ],
                false,
            );
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
