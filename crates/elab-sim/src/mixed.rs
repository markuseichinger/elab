//! Mixed-Signal-Simulation: Analog- und Digitalteil in einem Zeitverlauf.
//!
//! Logikbausteine mit Versorgungsanschlüssen (74HC…, 555, ADC/DAC) bestehen aus
//! drei Teilen:
//!
//! - **Eingänge** ([`Observer`]) lesen Spannungen und machen daraus Logikpegel –
//!   mit den Schaltschwellen der Logikfamilie bezogen auf die eigene Versorgung.
//!   Zwischen den Schwellen ist der Pegel undefiniert (X), beim Schmitt-Trigger
//!   bleibt der alte Wert (Hysterese).
//! - **Kern**: ereignisgesteuerte Logik mit Laufzeiten ([`DigitalSim`]).
//! - **Ausgänge** ([`DriverLink`]): Ausgangsstufen im Analogteil
//!   ([`ElementKind::Driver`]), die über ihren Innenwiderstand an VCC bzw. GND
//!   schalten. Der Strom fließt also wirklich aus dem VCC-Pin.
//!
//! Ohne ausreichende Versorgung schalten die Ausgänge ab (hochohmig), bei
//! Überspannung oder Verpolung gilt der Baustein als zerstört.
//!
//! Der Zeitverlauf läuft im Analogsimulator; jeder Schritt endet spätestens am
//! nächsten Digitalereignis. Kippt ein Eingang innerhalb eines Schritts, wird der
//! Schritt verkürzt, bis der Schaltzeitpunkt genau getroffen ist.

use crate::analysis::{node_voltage, OpResult, Simulator, StepCheck, TranOptions, TranResult, TranState};
use crate::circuit::{Circuit, ElementKind, NodeId, SimError};
use crate::digital::{Comp, CompKind, DigitalSim, Logic};
use std::collections::VecDeque;
use std::time::Instant;

/// Schaltschwelle U = U(gnd) + `frac` · (U(vcc) − U(gnd)) + `abs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub frac: f64,
    pub abs: f64,
}

impl Level {
    /// Anteil der Versorgungsspannung (CMOS, z. B. 74HC: 30 % / 70 %)
    pub const fn frac(frac: f64) -> Self {
        Self { frac, abs: 0.0 }
    }
    /// Feste Spannung über GND (TTL-kompatibel, z. B. 74HCT: 0,8 V / 2,0 V)
    pub const fn abs(abs: f64) -> Self {
        Self { frac: 0.0, abs }
    }
    fn volts(&self, x: &[f64], gnd: NodeId, vcc: NodeId) -> f64 {
        let g = node_voltage(x, gnd);
        g + self.frac * (node_voltage(x, vcc) - g) + self.abs
    }
}

/// Wie ein Eingang Spannungen in Logikpegel übersetzt.
#[derive(Clone, Debug, PartialEq)]
pub enum ObsKind {
    /// Logikeingang: unter `il` → 0, über `ih` → 1, dazwischen X
    /// (Schmitt-Trigger: alter Wert bleibt).
    Threshold { node: NodeId, gnd: NodeId, vcc: NodeId, il: Level, ih: Level, schmitt: bool },
    /// Komparator: 1, wenn U(plus) > U(minus)
    Compare { plus: NodeId, minus: NodeId },
    /// Analog-Digital-Umsetzer: Code = ⌊2^bits · U_ein / U_ref⌋, Bits LSB zuerst
    Adc { node: NodeId, gnd: NodeId, vref: NodeId, bits: usize },
}

/// Ein Eingang eines Bausteins (oder ein ganzer ADC).
#[derive(Clone, Debug, PartialEq)]
pub struct Observer {
    pub kind: ObsKind,
    /// Eingabebausteine ([`CompKind::Input`]) im Digitalteil, je Bit einer
    pub inputs: Vec<usize>,
    /// Versorgung des Bausteins
    pub group: Option<usize>,
    /// Pin hängt in der Luft und liest diesen Wert
    pub floating: Option<Logic>,
}

/// Quelle eines Ausgangspegels.
#[derive(Clone, Debug, PartialEq)]
pub enum DriveSrc {
    /// Logikpegel eines Digitalnetzes
    Net(usize),
    /// DAC: Anteil = Code / `full`, Bits LSB zuerst
    Code { nets: Vec<usize>, full: f64 },
}

/// Ausgangsstufe: Element im Analogteil, gesteuert vom Digitalteil.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverLink {
    /// Index des [`ElementKind::Driver`] in der Schaltung
    pub elem: usize,
    pub src: DriveSrc,
    pub group: Option<usize>,
    /// Offener Kollektor/Drain: High = hochohmig
    pub open_drain: bool,
}

/// Versorgung eines Bausteins.
#[derive(Clone, Debug, PartialEq)]
pub struct PowerGroup {
    pub name: String,
    pub vcc: NodeId,
    pub gnd: NodeId,
    /// Unterhalb arbeitet der Baustein nicht
    pub v_min: f64,
    /// Oberhalb wird er zerstört
    pub v_max: f64,
}

/// Zustand der Versorgung eines Bausteins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    Ok,
    /// Versorgungsspannung zu klein (oder gar keine)
    Low,
    /// Überspannung
    Overvoltage,
    /// Verpolt
    Reversed,
}

impl PowerState {
    pub fn destroyed(self) -> bool {
        matches!(self, PowerState::Overvoltage | PowerState::Reversed)
    }
}

/// Beschreibung des Digitalteils und seiner Kopplung an die Schaltung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MixedSpec {
    pub n_nets: usize,
    pub comps: Vec<Comp>,
    pub observers: Vec<Observer>,
    pub drivers: Vec<DriverLink>,
    pub groups: Vec<PowerGroup>,
}

/// Spannung, ab der ein verpolter Baustein Schaden nimmt.
const REVERSE_LIMIT: f64 = -0.7;

/// Einschalthysterese der Versorgung in V
const POWER_HYST: f64 = 0.3;

fn power_state(g: &PowerGroup, x: &[f64], prev: PowerState) -> PowerState {
    if prev.destroyed() {
        return prev;
    }
    let u = node_voltage(x, g.vcc) - node_voltage(x, g.gnd);
    if u > g.v_max {
        PowerState::Overvoltage
    } else if u < REVERSE_LIMIT {
        PowerState::Reversed
    } else if u < g.v_min || (prev == PowerState::Low && u < g.v_min + POWER_HYST) {
        // Mit Hysterese: Ein Baustein, der wegen Unterspannung aus war, startet erst wieder,
        // wenn die Versorgung deutlich über der Mindestspannung liegt.
        PowerState::Low
    } else {
        PowerState::Ok
    }
}

/// Beobachtete Größe und ihre Schaltschwellen (für die Suche nach dem Schaltzeitpunkt).
fn observed(o: &Observer, x: &[f64]) -> (f64, Vec<f64>) {
    match &o.kind {
        ObsKind::Threshold { node, gnd, vcc, il, ih, .. } => (node_voltage(x, *node), vec![il.volts(x, *gnd, *vcc), ih.volts(x, *gnd, *vcc)]),
        ObsKind::Compare { plus, minus } => (node_voltage(x, *plus) - node_voltage(x, *minus), vec![0.0]),
        ObsKind::Adc { node, gnd, vref, bits } => {
            let g = node_voltage(x, *gnd);
            let r = node_voltage(x, *vref) - g;
            let n = 1usize << bits;
            (node_voltage(x, *node) - g, (1..n).map(|k| r * k as f64 / n as f64).collect())
        }
    }
}

/// Knoten, deren Spannung ein Eingang liest.
fn sensed(o: &Observer) -> Vec<NodeId> {
    match &o.kind {
        ObsKind::Threshold { node, .. } | ObsKind::Adc { node, .. } => vec![*node],
        ObsKind::Compare { plus, minus } => vec![*plus, *minus],
    }
}

/// Hängt ein Knoten gerade in der Luft (offener Schalter, abgeschalteter Ausgang)?
/// Gezählt wird, was die Spannung festlegen kann: Leitwerte über 1 nS, Quellen,
/// Kondensatoren (halten ihre Ladung), Halbleiter und treibende Ausgänge.
fn node_floating(sim: &Simulator, node: NodeId) -> bool {
    if node == 0 {
        return false;
    }
    let mut g = 0.0;
    for (ei, e) in sim.circuit().elements().iter().enumerate() {
        let t = e.kind.terminals();
        let Some(k) = t.iter().position(|&n| n == node) else { continue };
        g += match &e.kind {
            ElementKind::Resistor { r, .. } => 1.0 / r,
            ElementKind::Switch { closed, r_on, r_off, .. } => 1.0 / if *closed { r_on } else { r_off },
            ElementKind::Driver { .. } => {
                if sim.drive(ei).is_some() || k > 0 {
                    1.0
                } else {
                    0.0
                }
            }
            // OpAmp-Eingänge sind hochohmig
            ElementKind::OpAmp { .. } if k < 2 => 0.0,
            _ => 1.0,
        };
    }
    g < 1e-9
}

fn sample(o: &Observer, x: &[f64], prev: &[Logic], power: &[PowerState], floating_now: bool) -> Vec<Logic> {
    let n = o.inputs.len();
    if let Some(v) = o.floating {
        return vec![v; n];
    }
    if floating_now {
        return vec![Logic::X; n];
    }
    if o.group.is_some_and(|g| power[g] != PowerState::Ok) {
        return vec![Logic::X; n];
    }
    match &o.kind {
        ObsKind::Threshold { node, gnd, vcc, il, ih, schmitt } => {
            let v = node_voltage(x, *node);
            let (lo, hi) = (il.volts(x, *gnd, *vcc), ih.volts(x, *gnd, *vcc));
            let val = if v < lo {
                Logic::L0
            } else if v > hi {
                Logic::L1
            } else if *schmitt && matches!(prev.first(), Some(Logic::L0 | Logic::L1)) {
                prev[0]
            } else if *schmitt {
                // Hysterese ohne Vorgeschichte: zur näheren Schwelle
                Logic::from_bool(v - lo > hi - v)
            } else {
                Logic::X
            };
            vec![val]
        }
        ObsKind::Compare { plus, minus } => vec![Logic::from_bool(node_voltage(x, *plus) > node_voltage(x, *minus))],
        ObsKind::Adc { node, gnd, vref, bits } => {
            let g = node_voltage(x, *gnd);
            let r = node_voltage(x, *vref) - g;
            let full = (1usize << bits) as f64;
            let code = if r > 1e-6 { ((node_voltage(x, *node) - g) / r * full).floor().clamp(0.0, full - 1.0) as usize } else { 0 };
            (0..*bits).map(|b| Logic::from_bool(code >> b & 1 == 1)).collect()
        }
    }
}

fn drive_value(l: &DriverLink, d: &DigitalSim, power: &[PowerState]) -> Option<f64> {
    if l.group.is_some_and(|g| power[g] != PowerState::Ok) {
        return None;
    }
    match &l.src {
        DriveSrc::Net(n) => match d.net(*n) {
            Logic::L0 => Some(0.0),
            Logic::L1 if l.open_drain => None,
            Logic::L1 => Some(1.0),
            Logic::X => Some(0.5),
            Logic::Z => None,
        },
        DriveSrc::Code { nets, full } => {
            let mut code = 0usize;
            for (b, &n) in nets.iter().enumerate() {
                match d.net(n) {
                    Logic::L1 => code |= 1 << b,
                    Logic::L0 => {}
                    _ => return Some(0.5),
                }
            }
            Some(code as f64 / full)
        }
    }
}

fn ns(t: f64) -> u64 {
    (t * 1e9).round().max(0.0) as u64
}

/// Laufende Mixed-Signal-Simulation (Einschalten bei t = 0, dann schrittweise).
pub struct MixedSim {
    sim: Simulator<'static>,
    spec: MixedSpec,
    dsim: DigitalSim,
    st: TranState,
    obs_val: Vec<Vec<Logic>>,
    power: Vec<PowerState>,
    /// Eingang hängt gerade in der Luft (je Beobachter)
    floating: Vec<bool>,
    pub opts: TranOptions,
    /// Genauigkeit, mit der Schaltzeitpunkte gesucht werden, in s
    pub ev_tol: f64,
    bps: Vec<f64>,
    bp_until: f64,
    pub steps: usize,
    /// Mitschrift für das Oszilloskop: beobachtete Knoten, Verlauf, aufbewahrte Dauer
    watch: Vec<NodeId>,
    trace: VecDeque<(f64, Vec<f64>)>,
    trace_span: f64,
}

/// Höchstzahl gespeicherter Zeitpunkte der Mitschrift
const TRACE_MAX: usize = 200_000;

impl MixedSim {
    /// Baut die Simulation auf und schaltet ein: Kondensatoren leer, Spulen stromlos,
    /// Flipflops auf 0. Digital- und Analogteil werden gegeneinander eingeschwungen.
    pub fn new(circuit: Circuit, spec: MixedSpec, h_max: f64) -> Result<Self, SimError> {
        let mut sim = Simulator::owned(circuit)?;
        let mut obs_val: Vec<Vec<Logic>> = spec.observers.iter().map(|o| vec![Logic::X; o.inputs.len()]).collect();
        let mut power = vec![PowerState::Low; spec.groups.len()];
        let mut comps = spec.comps.clone();
        let mut dsim = DigitalSim::new(spec.n_nets, comps.clone());
        let mut x;
        let mut k = 0;
        let floating_of =
            |sim: &Simulator| -> Vec<bool> { spec.observers.iter().map(|o| sensed(o).iter().any(|&n| node_floating(sim, n))).collect() };
        let mut floating;
        loop {
            for l in &spec.drivers {
                sim.set_drive(l.elem, drive_value(l, &dsim, &power));
            }
            floating = floating_of(&sim);
            x = sim.initial_solution(true)?;
            let new_power: Vec<PowerState> =
                spec.groups.iter().zip(&power).map(|(g, p)| power_state(g, &x, if k == 0 { PowerState::Low } else { *p })).collect();
            let new_obs: Vec<Vec<Logic>> =
                spec.observers.iter().zip(&obs_val).zip(&floating).map(|((o, prev), &f)| sample(o, &x, prev, &new_power, f)).collect();
            if k > 0 && new_obs == obs_val && new_power == power {
                break;
            }
            k += 1;
            obs_val = new_obs;
            power = new_power;
            for (o, vals) in spec.observers.iter().zip(&obs_val) {
                for (&c, v) in o.inputs.iter().zip(vals) {
                    comps[c].kind = CompKind::Input(*v);
                }
            }
            dsim = DigitalSim::new(spec.n_nets, comps.clone());
            if k > 25 {
                // schwingt beim Einschalten (z. B. Oszillator): mit diesem Stand weiter
                for l in &spec.drivers {
                    sim.set_drive(l.elem, drive_value(l, &dsim, &power));
                }
                floating = floating_of(&sim);
                x = sim.initial_solution(true)?;
                break;
            }
        }
        let opts = TranOptions::new(f64::MAX, h_max);
        let mut st = sim.tran_state(x, true);
        st.h = h_max * 0.02;
        let mut opts = opts;
        opts.h_min = (h_max * 1e-12).max(1e-18);
        Ok(Self {
            sim,
            spec,
            dsim,
            st,
            obs_val,
            power,
            floating,
            opts,
            ev_tol: (h_max * 1e-4).max(1e-9),
            bps: Vec::new(),
            bp_until: 0.0,
            steps: 0,
            watch: Vec::new(),
            trace: VecDeque::new(),
            trace_span: 0.0,
        })
    }

    /// Legt fest, welche Knoten mitgeschrieben werden und wie lange (in s Simulationszeit)
    /// der Verlauf aufbewahrt wird. Andere Knoten löschen die bisherige Mitschrift.
    pub fn set_watch(&mut self, nodes: Vec<NodeId>, span: f64) {
        if nodes != self.watch {
            self.watch = nodes;
            self.trace.clear();
            self.push_trace();
        }
        self.trace_span = span;
    }

    /// Mitschrift: (Zeit, Spannung je beobachtetem Knoten), älteste zuerst.
    pub fn trace(&self) -> &VecDeque<(f64, Vec<f64>)> {
        &self.trace
    }

    fn push_trace(&mut self) {
        if self.watch.is_empty() {
            return;
        }
        let x = self.st.x();
        let t = self.st.t;
        self.trace.push_back((t, self.watch.iter().map(|&n| node_voltage(x, n)).collect()));
        while self.trace.len() > TRACE_MAX || self.trace.front().is_some_and(|f| f.0 < t - self.trace_span) {
            self.trace.pop_front();
        }
    }

    /// Aktuelle Simulationszeit in s.
    pub fn time(&self) -> f64 {
        self.st.t
    }

    pub fn circuit(&self) -> &Circuit {
        self.sim.circuit()
    }

    pub fn spec(&self) -> &MixedSpec {
        &self.spec
    }

    pub fn digital(&self) -> &DigitalSim {
        &self.dsim
    }

    /// Logikpegel, die ein Eingang gerade liest.
    pub fn observer_values(&self, i: usize) -> &[Logic] {
        &self.obs_val[i]
    }

    pub fn power(&self, g: usize) -> PowerState {
        self.power[g]
    }

    /// Spannung eines Knotens jetzt.
    pub fn voltage(&self, node: NodeId) -> f64 {
        node_voltage(self.st.x(), node)
    }

    /// Momentaufnahme, auswertbar wie ein Arbeitspunkt.
    pub fn snapshot(&self) -> OpResult {
        self.sim.snapshot(&self.st)
    }

    pub fn stats(&self) -> &crate::Stats {
        &self.sim.stats
    }

    fn update_drives(&mut self) -> bool {
        let mut changed = false;
        for l in &self.spec.drivers {
            changed |= self.sim.set_drive(l.elem, drive_value(l, &self.dsim, &self.power));
        }
        if changed {
            self.update_floating();
        }
        changed
    }

    fn update_floating(&mut self) {
        self.floating = self.spec.observers.iter().map(|o| sensed(o).iter().any(|&n| node_floating(&self.sim, n))).collect();
    }

    fn discontinuity(&mut self) {
        let gap = self.dsim.next_event_time().map_or(self.opts.h_max, |te| (te as f64 * 1e-9 - self.st.t).max(self.ev_tol));
        self.sim.mark_discontinuity(&mut self.st, gap, &self.opts);
    }

    /// Setzt einen Eingabebaustein (Schalter, Taster) im Digitalteil.
    pub fn set_input(&mut self, comp: usize, v: Logic) {
        self.dsim.set_input(comp, v);
        self.dsim.run_until(ns(self.st.t));
        if self.update_drives() {
            self.discontinuity();
        }
    }

    /// Ändert die Werte eines Bauteils (Schalter, Poti) bei laufender Simulation.
    pub fn update_element(&mut self, name: &str, kind: ElementKind) -> Result<(), SimError> {
        let idx = self.sim.circuit().find_element(name).ok_or_else(|| SimError::UnknownName(name.into()))?;
        let old = format!("{:?}", self.sim.circuit().elements()[idx].kind);
        if old != format!("{kind:?}") {
            self.sim.update_element(name, kind)?;
            self.update_floating();
            self.discontinuity();
        }
        Ok(())
    }

    /// Ein angenommener Zeitschritt, höchstens bis `t_target`.
    pub fn step(&mut self, t_target: f64) -> Result<(), SimError> {
        let t = self.st.t;
        // Knickstellen der Quellen vorhalten
        if self.bp_until < t_target {
            let until = t_target.max(t + self.opts.h_max * 50.0);
            let mut more = self.sim.source_breakpoints(self.bp_until.max(t), until);
            self.bps.append(&mut more);
            self.bps.sort_by(|a, b| a.partial_cmp(b).unwrap());
            self.bps.dedup_by(|a, b| (*a - *b).abs() < 1e-15);
            self.bp_until = until;
        }
        let eps = self.ev_tol * 1e-3;
        while self.bps.first().is_some_and(|&b| b <= t + eps) {
            self.bps.remove(0);
        }
        // Grenze: Ziel, nächstes Digitalereignis, nächste Knickstelle
        let mut limit = t_target;
        let mut at_bp = false;
        if let Some(te) = self.dsim.next_event_time() {
            let te = te as f64 * 1e-9;
            if te <= t + eps {
                // Ereignis fällig: sofort verarbeiten
                self.dsim.run_until(ns(t));
                if self.update_drives() {
                    self.discontinuity();
                }
                return Ok(());
            }
            limit = limit.min(te);
        }
        if let Some(&b) = self.bps.first() {
            if b <= limit {
                limit = b;
                at_bp = true;
            }
        }

        let (spec, obs_val, power, floating, ev_tol) = (&self.spec, &self.obs_val, &self.power, &self.floating, self.ev_tol);
        let mut check = |t0: f64, x0: &[f64], t1: f64, x1: &[f64]| locate(spec, obs_val, power, floating, ev_tol, t0, x0, t1, x1);
        let hit = self.sim.tran_step(&mut self.st, limit, &self.opts, &mut check)?;
        self.steps += 1;
        self.push_trace();

        // Digitalteil nachziehen
        let tn = ns(self.st.t);
        self.dsim.run_until(tn);
        let x = self.st.x().to_vec();
        for (g, grp) in self.spec.groups.iter().enumerate() {
            self.power[g] = power_state(grp, &x, self.power[g]);
        }
        for (i, o) in self.spec.observers.iter().enumerate() {
            let vals = sample(o, &x, &self.obs_val[i], &self.power, self.floating[i]);
            if vals != self.obs_val[i] {
                for (&c, &v) in o.inputs.iter().zip(&vals) {
                    self.dsim.set_input(c, v);
                }
                self.obs_val[i] = vals;
            }
        }
        self.dsim.run_until(tn);
        let changed = self.update_drives();
        if hit && at_bp {
            self.bps.remove(0);
        }
        if changed || (hit && at_bp) {
            self.discontinuity();
        }
        Ok(())
    }

    /// Rechnet bis `t_target`, höchstens bis `deadline` (Echtzeitbetrieb).
    /// Gibt zurück, ob das Ziel erreicht wurde.
    pub fn advance(&mut self, t_target: f64, deadline: Option<Instant>) -> Result<bool, SimError> {
        while self.st.t < t_target - self.ev_tol * 1e-3 {
            self.step(t_target)?;
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Zeitverlauf von jetzt bis `t_stop` mit allen Zeitpunkten.
    pub fn run(&mut self, t_stop: f64) -> Result<TranResult, SimError> {
        let mut res = self.sim.tran_result(&self.st);
        while self.st.t < t_stop - self.ev_tol * 1e-3 {
            self.step(t_stop)?;
            self.sim.record(&mut res, &self.st);
            if self.dsim.overloaded {
                break;
            }
        }
        Ok(res)
    }
}

/// Prüft einen vorläufigen Schritt: Kippt ein Eingang oder eine Versorgung,
/// wird der Schritt bis kurz hinter den (linear geschätzten) Schaltpunkt verkürzt.
#[allow(clippy::too_many_arguments)]
fn locate(
    spec: &MixedSpec,
    obs_val: &[Vec<Logic>],
    power: &[PowerState],
    floating: &[bool],
    ev_tol: f64,
    t0: f64,
    x0: &[f64],
    t1: f64,
    x1: &[f64],
) -> StepCheck {
    let h = t1 - t0;
    if h <= ev_tol {
        return StepCheck::Accept;
    }
    let mut frac = f64::INFINITY;
    let mut consider = |s0: f64, s1: f64, thr: &[f64]| {
        if s1 == s0 {
            frac = frac.min(0.5);
            return;
        }
        let mut best = f64::INFINITY;
        for &th in thr {
            let f = (th - s0) / (s1 - s0);
            if f > 0.0 && f <= 1.0 {
                best = best.min(f);
            }
        }
        frac = frac.min(if best.is_finite() { best } else { 0.5 });
    };
    let new_power: Vec<PowerState> = spec.groups.iter().zip(power).map(|(g, p)| power_state(g, x1, *p)).collect();
    for (g, grp) in spec.groups.iter().enumerate() {
        if new_power[g] != power[g] {
            let s0 = node_voltage(x0, grp.vcc) - node_voltage(x0, grp.gnd);
            let s1 = node_voltage(x1, grp.vcc) - node_voltage(x1, grp.gnd);
            consider(s0, s1, &[grp.v_min, grp.v_max, REVERSE_LIMIT]);
        }
    }
    for (i, o) in spec.observers.iter().enumerate() {
        if o.floating.is_some() || floating[i] {
            continue;
        }
        if sample(o, x1, &obs_val[i], &new_power, false) != obs_val[i] {
            let (s0, _) = observed(o, x0);
            let (s1, thr) = observed(o, x1);
            consider(s0, s1, &thr);
        }
    }
    if !frac.is_finite() || h * (1.0 - frac) <= ev_tol {
        return StepCheck::Accept;
    }
    StepCheck::Retry(t0 + h * frac + ev_tol * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digital::GateKind;

    /// 74HC04-artiger Inverter an 5 V: Eingang über Spannungsquelle, Ausgang mit Last.
    fn inverter(vin: f64, vcc: f64) -> (Circuit, MixedSpec) {
        let mut c = Circuit::new();
        c.vsource("VCC", "vcc", "gnd", vcc).unwrap();
        c.vsource("VIN", "in", "gnd", vin).unwrap();
        let (out, vccn) = (c.node("out"), c.node("vcc"));
        let drv = c.add("U1:Y", ElementKind::Driver { out, hi: vccn, lo: 0, r: 50.0 }).unwrap();
        c.resistor("RL", "out", "gnd", 1000.0).unwrap();
        let spec = MixedSpec {
            n_nets: 2,
            comps: vec![Comp { kind: CompKind::Input(Logic::X), inputs: vec![], outputs: vec![0], delay: 0 }, Comp::gate(GateKind::Not, vec![0], 1)],
            observers: vec![Observer {
                kind: ObsKind::Threshold { node: c.node("in"), gnd: 0, vcc: vccn, il: Level::frac(0.3), ih: Level::frac(0.7), schmitt: false },
                inputs: vec![0],
                group: Some(0),
                floating: None,
            }],
            drivers: vec![DriverLink { elem: drv, src: DriveSrc::Net(1), group: Some(0), open_drain: false }],
            groups: vec![PowerGroup { name: "U1".into(), vcc: vccn, gnd: 0, v_min: 2.0, v_max: 7.0 }],
        };
        (c, spec)
    }

    #[test]
    fn inverter_levels_and_load() {
        let (c, s) = inverter(0.0, 5.0);
        let m = MixedSim::new(c.clone(), s.clone(), 1e-6).unwrap();
        let out = c.find_node("out").unwrap();
        // High mit 1 kΩ Last: 5 V · 1000/1050
        assert!((m.voltage(out) - 5.0 * 1000.0 / 1050.0).abs() < 1e-3, "{}", m.voltage(out));
        // Strom kommt aus dem VCC-Anschluss der Versorgung
        let op = m.snapshot();
        assert!((op.current("VCC").unwrap() - 5.0 / 1050.0).abs() < 1e-6);

        let (c, s) = inverter(5.0, 5.0);
        let m = MixedSim::new(c, s, 1e-6).unwrap();
        assert!(m.voltage(out).abs() < 1e-6);
        // 2,5 V liegt zwischen den Schwellen: undefiniert, Ausgang halb
        let (c, s) = inverter(2.5, 5.0);
        let m = MixedSim::new(c, s, 1e-6).unwrap();
        assert_eq!(m.observer_values(0), &[Logic::X]);
        assert!((m.voltage(out) - 2.5 * 1000.0 / 1050.0).abs() < 0.05, "{}", m.voltage(out));
    }

    #[test]
    fn supply_matters() {
        let out = 3;
        // ohne ausreichende Versorgung: Ausgang hochohmig
        let (c, s) = inverter(0.0, 1.0);
        let m = MixedSim::new(c, s, 1e-6).unwrap();
        assert_eq!(m.power(0), PowerState::Low);
        assert!(m.voltage(out).abs() < 1e-6);
        // 9 V an einem HC-Baustein: zerstört
        let (c, s) = inverter(0.0, 9.0);
        let m = MixedSim::new(c, s, 1e-6).unwrap();
        assert_eq!(m.power(0), PowerState::Overvoltage);
    }

    #[test]
    fn schmitt_trigger_rc_oscillator() {
        // 74HC14-Oszillator: Ausgang über R auf C, C an den Eingang
        let mut c = Circuit::new();
        c.vsource("VCC", "vcc", "gnd", 5.0).unwrap();
        let (vccn, out, inp) = (c.node("vcc"), c.node("out"), c.node("in"));
        let drv = c.add("U1:Y", ElementKind::Driver { out, hi: vccn, lo: 0, r: 50.0 }).unwrap();
        c.resistor("R", "out", "in", 10e3).unwrap();
        c.capacitor("C", "in", "gnd", 100e-9).unwrap();
        let spec = MixedSpec {
            n_nets: 2,
            comps: vec![Comp { kind: CompKind::Input(Logic::X), inputs: vec![], outputs: vec![0], delay: 0 }, Comp::gate(GateKind::Not, vec![0], 1)],
            observers: vec![Observer {
                kind: ObsKind::Threshold { node: inp, gnd: 0, vcc: vccn, il: Level::frac(0.36), ih: Level::frac(0.56), schmitt: true },
                inputs: vec![0],
                group: Some(0),
                floating: None,
            }],
            drivers: vec![DriverLink { elem: drv, src: DriveSrc::Net(1), group: Some(0), open_drain: false }],
            groups: vec![PowerGroup { name: "U1".into(), vcc: vccn, gnd: 0, v_min: 2.0, v_max: 7.0 }],
        };
        let mut m = MixedSim::new(c, spec, 2e-5).unwrap();
        let res = m.run(10e-3).unwrap();
        let v = res.voltage("out").unwrap();
        // Flanken zählen
        let mut edges = Vec::new();
        for k in 1..v.len() {
            if v[k - 1] < 2.5 && v[k] >= 2.5 {
                edges.push(res.time[k]);
            }
        }
        // T = RC·(ln((V−Vn)/(V−Vp)) + ln(Vp/Vn)) mit Vp = 2,8 V, Vn = 1,8 V
        let (vp, vn) = (2.8, 1.8);
        let t_exp = 10e3 * 100e-9 * ((5.0_f64 - vn) / (5.0 - vp)).ln() + 10e3 * 100e-9 * (vp / vn).ln();
        assert!(edges.len() >= 5, "{edges:?}");
        let period = (edges[edges.len() - 1] - edges[1]) / (edges.len() - 2) as f64;
        // R_out 50 Ω verlängert leicht
        assert!((period / t_exp - 1.0).abs() < 0.02, "T = {period}, erwartet {t_exp}");
        // Kondensatorspannung pendelt zwischen den Schwellen
        let vin = res.voltage("in").unwrap();
        let late: Vec<f64> = vin.iter().zip(&res.time).filter(|(_, &t)| t > 3e-3).map(|(v, _)| *v).collect();
        let (mn, mx) = late.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!((mn - vn).abs() < 0.02 && (mx - vp).abs() < 0.02, "{mn} {mx}");
    }
}
