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
use elab_sim::{fmt_si, MixedSim, PowerState};
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
        let active = self.sim_on && self.nl.as_ref().is_some_and(|nl| nl.mixed.is_some() && nl.error.is_none());
        if !active {
            self.msim = None;
            self.msim_key = None;
            self.pin_logic.clear();
            return;
        }
        let nl = self.nl.as_ref().unwrap();
        let mx = nl.mixed.as_ref().unwrap();
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
                    self.op = Err(e.to_string());
                    self.msim = None;
                    return;
                }
            }
            ctx.request_repaint();
        }
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
        ui.label(RichText::new("✔ Echtzeitsimulation").color(GREEN).strong());
        ui.label(RichText::new(format!("t = {}", fmt_si(t, "s"))).color(MUTED).monospace());
        let label = if self.dig_paused { "▶ Weiter" } else { "⏸ Pause" };
        if ui.small_button(label).clicked() {
            self.dig_paused = !self.dig_paused;
        }
        ui.add(egui::Slider::new(&mut self.dig_speed, 0.001..=4.0).logarithmic(true).custom_formatter(|v, _| format!("{v:.3}×")))
            .on_hover_text("Tempo: 1× ist Echtzeit. Kleiner: Zeitlupe, um schnelle Vorgänge zu sehen.");
        let restart = ui.small_button("↺").on_hover_text("Neu einschalten (Kondensatoren leer, Flipflops auf 0)").clicked();
        if self.msim_lag && !self.dig_paused {
            ui.label(RichText::new("· langsamer als Echtzeit").color(ACCENT));
        }
        // Zerstörte oder unversorgte Bausteine
        let (Some(nl), Some(ms)) = (&self.nl, &self.msim) else { return };
        let Some(mx) = &nl.mixed else { return };
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
        warn.sort();
        if !warn.is_empty() {
            ui.label(RichText::new(format!("⚠ {}", warn.join(" · "))).color(RED));
        }
        if restart {
            self.msim = None;
        }
    }
}
