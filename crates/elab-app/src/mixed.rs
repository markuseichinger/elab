//! Gemischte Schaltungen (Logik-ICs, OpAmp, 555, Wandler) laufen in Echtzeit:
//! Die Simulation schreitet mit der Uhr voran, Taster und Schalter wirken sofort,
//! Kondensatoren laden sich sichtbar auf.

use crate::theme::*;
use crate::App;
use egui::RichText;
use elab_app::model::{Document, PartKind};
use elab_app::netlist::PinSrc;
use elab_app::sim;
use elab_sim::digital::Logic;
use elab_sim::{MixedSim, PowerState};
use std::time::{Duration, Instant};

/// Größter Zeitschritt der Echtzeitsimulation.
const H_LIVE: f64 = 1e-3;

/// Schaltplan ohne Bedienzustände: Ändert sich nur ein Schalter oder ein Poti,
/// läuft die Simulation weiter (Kondensatoren und Flipflops behalten ihren Zustand).
pub fn mixed_structure(doc: &Document) -> Document {
    let mut d = doc.clone();
    for p in &mut d.parts {
        match &mut p.kind {
            PartKind::LogicInput { on } => *on = false,
            PartKind::Switch { closed } => *closed = false,
            PartKind::Potentiometer { pos, .. } => *pos = 0.5,
            _ => {}
        }
    }
    d.title.clear();
    d
}

impl App {
    /// Echtzeitsimulation gemischter Schaltungen: aufbauen, bedienen, voranschreiten.
    pub(crate) fn update_mixed(&mut self, ctx: &egui::Context) {
        let active = self.sim_on && self.nl.as_ref().is_some_and(|nl| sim::realtime(nl) && nl.error.is_none() && !nl.circuit.elements().is_empty());
        if !active {
            self.msim = None;
            self.msim_key = None;
            self.pin_logic.clear();
            return;
        }
        let nl = self.nl.as_ref().unwrap();
        let empty = elab_app::netlist::MixedNetlist::default();
        let mx = nl.mixed.as_ref().unwrap_or(&empty);
        let key = mixed_structure(&self.doc);
        if self.msim_key.as_ref() != Some(&key) || self.msim.is_none() {
            self.msim_key = Some(key);
            self.msim_gen = self.nl_gen;
            match MixedSim::new(nl.circuit.clone(), mx.spec.clone(), H_LIVE) {
                Ok(m) => self.msim = Some(m),
                Err(e) => {
                    self.msim = None;
                    self.op = Err(e.to_string());
                    return;
                }
            }
        }
        let ms = self.msim.as_mut().unwrap();
        // Bedienung übernehmen: Schalter, Taster, Potis
        if self.msim_gen != self.nl_gen {
            self.msim_gen = self.nl_gen;
            for e in nl.circuit.elements() {
                let _ = ms.update_element(&e.name, e.kind.clone());
            }
        }
        for p in &self.doc.parts {
            let Some(&c) = mx.comp_of.get(&p.id) else { continue };
            match p.kind {
                PartKind::LogicInput { on } => ms.set_input(c, Logic::from_bool(on)),
                PartKind::Button => ms.set_input(c, Logic::from_bool(self.pressed.contains(&p.id))),
                _ => {}
            }
        }
        // Voranschreiten
        if !self.dig_paused {
            let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1) as f64;
            let target = ms.time() + dt * self.dig_speed;
            match ms.advance(target, Some(Instant::now() + Duration::from_millis(14))) {
                Ok(done) => self.msim_lag = !done,
                Err(e) => {
                    crate::log::write(&format!("Echtzeitsimulation abgebrochen bei t = {}: {e}", ms.time()));
                    self.op = Err(e.to_string());
                    self.msim = None;
                    return;
                }
            }
            ctx.request_repaint();
        }
        // Mitschrift für das Oszilloskop
        let nodes = sim::probe_nodes(&self.doc, nl).into_iter().map(|p| p.2).collect();
        ms.set_watch(nodes, 3.0 * self.scope_t);
        // Anzeige
        let mut v = sim::view(nl, &ms.snapshot());
        v.newton = ms.stats().newton_iterations;
        self.op = Ok(v);
        let d = ms.digital();
        self.mixed_logic = (0..nl.nets.len()).map(|n| d.net(n)).collect();
        self.pin_logic.clear();
        for (&id, src) in &mx.pin_src {
            let vals = src
                .iter()
                .map(|s| match *s {
                    PinSrc::None => None,
                    PinSrc::Obs(o, b) => ms.observer_values(o).get(b).copied(),
                    PinSrc::Net(n) => Some(d.net(n)),
                })
                .collect();
            self.pin_logic.insert(id, vals);
        }
    }

    /// Versorgungszustand eines ICs.
    pub(crate) fn ic_power(&self, id: u32) -> Option<PowerState> {
        let g = *self.nl.as_ref()?.mixed.as_ref()?.group_of.get(&id)?;
        Some(self.msim.as_ref()?.power(g))
    }

    /// Statuszeile: Zeit, Tempo, Pause, Warnungen.
    pub(crate) fn mixed_status(&mut self, ui: &mut egui::Ui) {
        let Some(ms) = &self.msim else { return };
        let t = ms.time();
        let now = ui.input(|i| i.time);
        ui.label(RichText::new("✔ Echtzeit").color(GREEN).strong());
        // Uhr und Bedienung in einem festen Kasten: nichts verrutscht, wenn die Zeit wächst
        // Uhr mit fester Breite, Bedienung daneben: nichts verrutscht, wenn die Zeit wächst
        let txt = if t < 1.0 { format!("t = {:>7.2} ms", t * 1e3) } else { format!("t = {:>7.3} s ", t) };
        ui.add_sized([118.0, 18.0], egui::Label::new(RichText::new(txt).monospace()));
        let label = if self.dig_paused { "▶" } else { "⏸" };
        let tip = if self.dig_paused { "Weiter" } else { "Anhalten" };
        if ui.add_sized([26.0, 18.0], egui::Button::new(label).small()).on_hover_text(tip).clicked() {
            self.dig_paused = !self.dig_paused;
        }
        let restart = ui
            .add_sized([26.0, 18.0], egui::Button::new("↺").small())
            .on_hover_text("Neu einschalten (Kondensatoren leer, Flipflops auf 0)")
            .clicked();
        ui.label(RichText::new("Tempo").color(muted()));
        ui.add(egui::Slider::new(&mut self.dig_speed, 0.001..=4.0).logarithmic(true).show_value(false))
            .on_hover_text("Tempo: 1× ist Echtzeit. Kleiner: Zeitlupe, um schnelle Vorgänge zu sehen.");
        ui.add_sized([52.0, 18.0], egui::Label::new(RichText::new(format!("{:.3}×", self.dig_speed)).monospace()));
        // „Langsamer als Echtzeit“ nicht bei jedem einzelnen Bild an- und ausschalten
        if self.msim_lag && !self.dig_paused {
            self.lag_seen = now;
        }
        let lag = now - self.lag_seen < 1.0;
        ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(if lag { "kommt nicht hinterher" } else { "" }).color(ACCENT)))
            .on_hover_text("Die Schaltung ist so aufwendig zu rechnen, dass die Simulation langsamer läuft als die Uhr.");
        // Zerstörte oder unversorgte Bausteine
        let (Some(nl), Some(ms)) = (&self.nl, &self.msim) else { return };
        let Some(mx) = &nl.mixed else {
            if restart {
                self.msim = None;
            }
            return;
        };
        let mut warn = Vec::new();
        for (&id, &g) in &mx.group_of {
            let name = self.doc.part(id).map(|p| p.name.clone()).unwrap_or_default();
            match ms.power(g) {
                PowerState::Ok => {}
                PowerState::Low => warn.push(format!("{name} ohne Versorgung")),
                PowerState::Overvoltage => warn.push(format!("{name} durch Überspannung zerstört")),
                PowerState::Reversed => warn.push(format!("{name} verpolt und zerstört")),
            }
        }
        // Warnungen bleiben kurz stehen, auch wenn der Zustand schnell wechselt
        for w in warn {
            self.warn_seen.insert(w, now);
        }
        self.warn_seen.retain(|_, t| now - *t < 0.8);
        let mut shown: Vec<&String> = self.warn_seen.keys().collect();
        shown.sort();
        if !shown.is_empty() {
            let txt: Vec<&str> = shown.iter().map(|s| s.as_str()).collect();
            ui.label(RichText::new(format!("⚠ {}", txt.join(" · "))).color(RED));
        }
        if restart {
            self.msim = None;
        }
    }
}
