//! Untere Leiste: Oszilloskop, Logikanalysator, Wahrheitstabelle,
//! sowie der Echtzeitbetrieb der Digitalsimulation.

use crate::symbols::logic_color;
use crate::theme::*;
use crate::tutorial::Topic;
use crate::App;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use elab_app::model::{Document, PartKind};
use elab_app::sim;
use elab_sim::digital::Logic;
use elab_sim::fmt_si;

/// Reiter der unteren Leiste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomTab {
    Scope,
    Analyzer,
    Truth,
}

const WINDOWS: [(f64, &str); 4] = [(2.0, "2 s"), (5.0, "5 s"), (10.0, "10 s"), (30.0, "30 s")];

/// Schaltplan ohne Schalterstellungen: Ändert sich nur ein Schalter, läuft die
/// Digitalsimulation weiter (Flipflops behalten ihren Zustand).
pub fn digital_structure(doc: &Document) -> Document {
    let mut d = doc.clone();
    for p in &mut d.parts {
        if let PartKind::LogicInput { on } = &mut p.kind {
            *on = false;
        }
    }
    d.title.clear();
    d
}

impl App {
    /// Digitalsimulation aktualisieren: neu aufbauen, Eingänge setzen, Zeit voranschreiten.
    pub(crate) fn update_digital(&mut self, ctx: &egui::Context) {
        let has_digital = self.nl.as_ref().is_some_and(|nl| nl.digital.is_some());
        if !has_digital {
            self.dsim = None;
            self.dsim_key = None;
            return;
        }
        let key = digital_structure(&self.doc);
        if self.dsim_key.as_ref() != Some(&key) {
            self.dsim = self.nl.as_ref().and_then(|nl| sim::digital_sim(&self.doc, nl));
            self.dsim_key = Some(key);
            self.truth = None;
        }
        let (Some(ds), Some(nl)) = (&mut self.dsim, &self.nl) else { return };
        let d = nl.digital.as_ref().unwrap();
        // Eingänge aus Schaltplan (Schalter) und Maus (Taster)
        for p in &self.doc.parts {
            let Some(&c) = d.comp_of.get(&p.id) else { continue };
            match p.kind {
                PartKind::LogicInput { on } => ds.set_input(c, Logic::from_bool(on)),
                PartKind::Button => ds.set_input(c, Logic::from_bool(self.pressed.contains(&p.id))),
                _ => {}
            }
        }
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1) as f64;
        let step = if self.dig_paused { 0 } else { (dt * self.dig_speed * 1e9) as u64 };
        // Auch angehalten: Eingangsänderungen samt Gatterlaufzeiten durchrechnen
        let t = ds.time + step.max(if self.dig_paused { 0 } else { 1_000 });
        ds.run_until(t);
        if self.dig_paused {
            // Ereignisse bis 1 µs nach jetzt abarbeiten, ohne die Uhr der Takte weiterlaufen zu lassen
            ds.settle(1_000);
        }
        ds.trim_history(ds.time.saturating_sub(65_000_000_000));
        if d.has_clock && !self.dig_paused {
            ctx.request_repaint();
        }
    }

    pub(crate) fn bottom_ui(&mut self, ui: &mut egui::Ui) {
        let has_digital = self.nl.as_ref().is_some_and(|nl| nl.digital.is_some());
        ui.horizontal(|ui| {
            for (tab, name, topic) in [
                (BottomTab::Scope, "📈 Oszilloskop", Topic::Scope),
                (BottomTab::Analyzer, "▤ Logikanalysator", Topic::Analyzer),
                (BottomTab::Truth, "▦ Wahrheitstabelle", Topic::TruthTable),
            ] {
                let r = ui.selectable_label(self.bottom_tab == tab, RichText::new(name).size(15.0).strong());
                if r.clicked() {
                    self.bottom_tab = tab;
                    self.topic = topic;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").on_hover_text("Leiste ausblenden").clicked() {
                    self.show_scope = false;
                }
            });
        });
        ui.separator();
        match self.bottom_tab {
            BottomTab::Scope => self.scope_ui(ui),
            BottomTab::Analyzer if has_digital => self.analyzer_ui(ui),
            BottomTab::Truth if has_digital => self.truth_ui(ui),
            _ => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Die Schaltung enthält keine Digitalbausteine. Sie stehen in der Palette unter „Digital“.").color(muted()),
                    );
                });
            }
        }
    }

    fn analyzer_ui(&mut self, ui: &mut egui::Ui) {
        let Some(nl) = &self.nl else { return };
        let rows = sim::analyzer_rows(&self.doc, nl);
        ui.horizontal(|ui| {
            let label = if self.dig_paused { "▶ Weiter" } else { "⏸ Pause" };
            if ui.button(RichText::new(label).size(14.0)).on_hover_text("Digitalsimulation anhalten / fortsetzen").clicked() {
                self.dig_paused = !self.dig_paused;
            }
            ui.label(RichText::new("🐢 Zeitlupe").color(muted()));
            ui.add(egui::Slider::new(&mut self.dig_speed, 0.05..=4.0).logarithmic(true).custom_formatter(|v, _| format!("{v:.2}×")))
                .on_hover_text("1× = Echtzeit. Kleiner: langsamer, größer: schneller.");
            ui.label(RichText::new("Fenster").color(muted()));
            let cur = WINDOWS.iter().find(|w| w.0 == self.an_window).map_or("?", |w| w.1);
            egui::ComboBox::from_id_salt("anw").selected_text(cur).show_ui(ui, |ui| {
                for (w, name) in WINDOWS {
                    ui.selectable_value(&mut self.an_window, w, name);
                }
            });
            if let Some(ds) = &self.dsim {
                ui.label(RichText::new(format!("t = {}", fmt_si(ds.time as f64 * 1e-9, "s"))).color(muted()));
                if ds.overloaded {
                    ui.label(RichText::new("⚠ Dauerschwingung: Die Schaltung kommt nicht zur Ruhe").color(RED));
                }
            }
        });
        let Some(ds) = &self.dsim else { return };
        if rows.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Schalter, Taster, Takte, Logik-Anzeigen und Messpunkte erscheinen hier als Zeilen.").color(muted()));
            });
            return;
        }
        let row_h = 26.0;
        let label_w = 110.0;
        let avail = ui.available_size();
        let h = (rows.len() as f32 * row_h + 22.0).max(avail.y.min(rows.len() as f32 * row_h + 22.0));
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, CANVAS_BG);
            // Der Analysator ist dunkel wie die Zeichenfläche
            let was_dark = crate::theme::is_dark();
            crate::theme::set_dark(true);
            let now = ds.time as f64 * 1e-9;
            let t0 = now - self.an_window;
            let plot = Rect::from_min_max(rect.min + Vec2::new(label_w, 4.0), rect.max - Vec2::new(8.0, 18.0));
            let x_of = |t: f64| plot.left() + ((t - t0) / self.an_window) as f32 * plot.width();
            // Zeitachse
            let step = if self.an_window <= 5.0 {
                1.0
            } else if self.an_window <= 10.0 {
                2.0
            } else {
                5.0
            };
            let mut tick = (t0 / step).ceil() * step;
            while tick <= now + 1e-9 {
                let x = x_of(tick);
                p.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], Stroke::new(1.0, Color32::from_gray(32)));
                p.text(Pos2::new(x, plot.bottom() + 3.0), Align2::CENTER_TOP, fmt_si(tick, "s"), FontId::proportional(10.5), muted());
                tick += step;
            }
            for (k, (name, net)) in rows.iter().enumerate() {
                let top = plot.top() + k as f32 * row_h;
                let (yh, yl, ym) = (top + 5.0, top + row_h - 5.0, top + row_h * 0.5);
                let cur = ds.net(*net);
                p.text(
                    Pos2::new(rect.left() + 10.0, ym),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(13.0),
                    logic_color(cur).gamma_multiply(1.2),
                );
                p.text(Pos2::new(rect.left() + label_w - 10.0, ym), Align2::RIGHT_CENTER, cur.symbol(), FontId::monospace(12.0), logic_color(cur));
                let hist = ds.history(*net);
                let mut segs: Vec<(f64, f64, Logic)> = Vec::new();
                for (i, &(t, v)) in hist.iter().enumerate() {
                    let ta = t as f64 * 1e-9;
                    let tb = hist.get(i + 1).map_or(now, |e| e.0 as f64 * 1e-9);
                    if tb >= t0 {
                        segs.push((ta.max(t0), tb, v));
                    }
                }
                let mut last_y: Option<f32> = None;
                for (ta, tb, v) in segs {
                    let (xa, xb) = (x_of(ta), x_of(tb).max(x_of(ta) + 0.5));
                    let col = logic_color(v);
                    let y = match v {
                        Logic::L1 => yh,
                        Logic::L0 => yl,
                        _ => ym,
                    };
                    match v {
                        Logic::X => {
                            p.rect_filled(Rect::from_min_max(Pos2::new(xa, yh), Pos2::new(xb, yl)), 0.0, col.gamma_multiply(0.35));
                        }
                        Logic::L1 => {
                            p.rect_filled(Rect::from_min_max(Pos2::new(xa, yh), Pos2::new(xb, yl)), 0.0, col.gamma_multiply(0.12));
                        }
                        _ => {}
                    }
                    if let Some(ly) = last_y {
                        if (ly - y).abs() > 0.5 {
                            p.line_segment([Pos2::new(xa, ly), Pos2::new(xa, y)], Stroke::new(1.6, col));
                        }
                    }
                    p.line_segment([Pos2::new(xa, y), Pos2::new(xb, y)], Stroke::new(2.0, col));
                    last_y = Some(y);
                }
                p.line_segment([Pos2::new(plot.left(), top + row_h), Pos2::new(plot.right(), top + row_h)], Stroke::new(1.0, Color32::from_gray(28)));
            }
            crate::theme::set_dark(was_dark);
        });
    }

    fn truth_ui(&mut self, ui: &mut egui::Ui) {
        if self.truth.is_none() {
            if let Some(nl) = &self.nl {
                self.truth = Some(sim::truth_table(&self.doc, nl));
            }
        }
        let Some(tt) = &self.truth else { return };
        let tt = match tt {
            Err(e) => {
                ui.add_space(12.0);
                ui.label(RichText::new(e.as_str()).color(muted()).size(14.0));
                return;
            }
            Ok(t) => t.clone(),
        };
        // Aktuelle Schalterstellung
        let current: Vec<bool> = tt
            .inputs
            .iter()
            .map(|name| match self.doc.part_by_name(name).map(|p| (&p.kind, p.id)) {
                Some((PartKind::LogicInput { on }, _)) => *on,
                Some((PartKind::Button, id)) => self.pressed.contains(&id),
                _ => false,
            })
            .collect();
        ui.label(RichText::new("Klick auf eine Zeile stellt die Logikschalter entsprechend ein.").small().color(muted()));
        let mono = |s: &str, c: Color32| (s.to_string(), c);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let mut job = LayoutJob::default();
            let push = |job: &mut LayoutJob, (t, c): (String, Color32)| {
                job.append(&t, 0.0, TextFormat { font_id: FontId::monospace(14.0), color: c, ..Default::default() });
            };
            let width = |s: &str| s.chars().count().max(1);
            let cols: Vec<usize> = tt.inputs.iter().chain(&tt.outputs).map(|n| width(n).max(1)).collect();
            for (k, n) in tt.inputs.iter().enumerate() {
                push(&mut job, mono(&format!(" {:^w$} ", n, w = cols[k]), text()));
            }
            push(&mut job, mono(" │ ", muted()));
            for (k, n) in tt.outputs.iter().enumerate() {
                push(&mut job, mono(&format!(" {:^w$} ", n, w = cols[tt.inputs.len() + k]), table_color(Logic::L1)));
            }
            ui.label(job);
            let mut clicked: Option<Vec<bool>> = None;
            for (bits, outs, ok) in &tt.rows {
                let mut job = LayoutJob::default();
                for (k, b) in bits.iter().enumerate() {
                    push(&mut job, mono(&format!(" {:^w$} ", if *b { "1" } else { "0" }, w = cols[k]), text()));
                }
                push(&mut job, mono(" │ ", muted()));
                for (k, v) in outs.iter().enumerate() {
                    push(&mut job, mono(&format!(" {:^w$} ", v.symbol(), w = cols[tt.inputs.len() + k]), table_color(*v)));
                }
                if !ok {
                    push(&mut job, mono("  schwingt", RED));
                }
                let is_cur = *bits == current;
                if ui.selectable_label(is_cur, job).clicked() {
                    clicked = Some(bits.clone());
                }
            }
            if let Some(bits) = clicked {
                for (name, b) in tt.inputs.iter().zip(bits) {
                    let id = self.doc.part_by_name(name).map(|p| p.id);
                    if let Some(PartKind::LogicInput { on }) = id.and_then(|id| self.doc.part_mut(id)).map(|p| &mut p.kind) {
                        *on = b;
                    }
                }
            }
        });
    }
}

/// Pegelfarben der Wahrheitstabelle, auf hellem Grund kräftiger
fn table_color(v: Logic) -> Color32 {
    if crate::theme::is_dark() {
        return logic_color(v).gamma_multiply(1.3);
    }
    match v {
        Logic::L1 => Color32::from_rgb(30, 135, 50),
        Logic::L0 => Color32::from_rgb(70, 75, 88),
        Logic::X => Color32::from_rgb(200, 50, 40),
        Logic::Z => Color32::from_rgb(110, 80, 200),
    }
}
