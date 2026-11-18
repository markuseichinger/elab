//! Anbindung an den Simulationskern: Arbeitspunkt live, Zeitverlauf fürs Oszilloskop.

use crate::model::{Document, PartKind};
use crate::netlist::{Netlist, SimRole};
use elab_sim::digital::{CompKind, DigitalSim, Logic};
use elab_sim::{MixedSim, OpResult, Simulator, TranOptions};
use std::collections::HashMap;
use std::time::Instant;

/// Messwerte eines Bauteils.
///
/// Bei Transistoren sind die Werte auf die Durchlassrichtung bezogen (bei PNP und
/// P-Kanal also mit umgekehrtem Vorzeichen), damit im Normalbetrieb positive Zahlen stehen:
/// `u` = U_CE bzw. U_DS, `i` = I_C bzw. I_D, `ctrl_u` = U_BE bzw. U_GS, `ctrl_i` = I_B.
#[derive(Clone, Copy, Debug, Default)]
pub struct PartValues {
    /// Spannung erster minus zweiter Anschluss in V
    pub u: f64,
    /// Strom (Vorzeichen wie im Simulator) in A
    pub i: f64,
    /// Leistung in W: aufgenommen (passiv) bzw. abgegeben (Quelle)
    pub p: f64,
    /// Steuerspannung (Transistor) bzw. U_SB (Potentiometer)
    pub ctrl_u: f64,
    /// Steuerstrom (Transistor) bzw. Schleiferstrom (Potentiometer)
    pub ctrl_i: f64,
    /// Betriebsbereich eines Transistors
    pub region: Option<Region>,
}

/// Betriebsbereich eines Transistors in Lernbegriffen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// gesperrt: Schalter aus
    Off,
    /// aktiv / Sättigung beim MOSFET: Verstärker, Strom wird gesteuert
    Active,
    /// gesättigt / linearer Bereich beim MOSFET: Schalter ein
    On,
}

impl Region {
    pub fn text(self, bjt: bool) -> &'static str {
        match (self, bjt) {
            (Region::Off, _) => "gesperrt (Schalter aus)",
            (Region::Active, true) => "aktiv: verstärkt (I_C = β · I_B)",
            (Region::Active, false) => "Sättigung: Strom durch U_GS gesteuert",
            (Region::On, true) => "gesättigt (Schalter ein)",
            (Region::On, false) => "linearer Bereich (Schalter ein)",
        }
    }
}

/// Ergebnis des DC-Arbeitspunkts, aufbereitet für die Anzeige.
#[derive(Clone, Debug, Default)]
pub struct OpView {
    /// Spannung je Netz (None = nicht simuliert)
    pub net_v: Vec<Option<f64>>,
    pub parts: HashMap<u32, PartValues>,
    /// Strom in jedes Bauteil hinein, je Anschluss (für die Stromfluss-Anzeige)
    pub pin_i: HashMap<u32, Vec<f64>>,
    pub millis: f64,
    pub newton: usize,
}

impl OpView {
    pub fn max_abs_v(&self) -> f64 {
        self.net_v.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()))
    }
}

/// Berechnet den Arbeitspunkt der Netzliste.
pub fn operating_point(nl: &Netlist) -> Result<OpView, String> {
    if let Some(e) = &nl.error {
        return Err(e.clone());
    }
    let t0 = Instant::now();
    let mut sim = Simulator::new(&nl.circuit).map_err(|e| e.to_string())?;
    let op = sim.operating_point().map_err(|e| e.to_string())?;
    let mut v = view(nl, &op);
    v.millis = t0.elapsed().as_secs_f64() * 1e3;
    v.newton = sim.stats.newton_iterations;
    Ok(v)
}

/// Bereitet einen berechneten Zustand (Arbeitspunkt oder Momentaufnahme) für die Anzeige auf.
pub fn view(nl: &Netlist, op: &OpResult) -> OpView {
    let net_v: Vec<Option<f64>> = nl
        .nets
        .iter()
        .map(|n| {
            if n.is_gnd {
                Some(0.0)
            } else if n.reachable {
                // Rechenrauschen (pV über offene Schalter) als 0 anzeigen
                op.voltage(&n.name).ok().map(|v| if v.abs() < 1e-7 { 0.0 } else { v })
            } else {
                None
            }
        })
        .collect();
    let mut parts: HashMap<u32, PartValues> = HashMap::new();
    for (&id, name) in &nl.simulated {
        let v = match nl.roles.get(&id) {
            None => {
                let u = op.element_voltage(name).unwrap_or(0.0);
                let i = op.current(name).unwrap_or(0.0);
                PartValues { u, i, p: u * i, ..Default::default() }
            }
            Some(SimRole::Ammeter) => {
                // Strom, der am Pluspol in das Messgerät hinein fließt
                let i = -op.current(name).unwrap_or(0.0);
                PartValues { i, ..Default::default() }
            }
            Some(SimRole::Pot) => {
                let (a, b) = (format!("{name}:AS"), format!("{name}:SB"));
                let (uas, usb) = (op.element_voltage(&a).unwrap_or(0.0), op.element_voltage(&b).unwrap_or(0.0));
                let (ia, ib) = (op.current(&a).unwrap_or(0.0), op.current(&b).unwrap_or(0.0));
                PartValues { u: uas + usb, i: ia, p: uas * ia + usb * ib, ctrl_u: usb, ctrl_i: ia - ib, region: None }
            }
            Some(SimRole::Bjt { pol }) => {
                let tv = op.terminal_voltages(name).unwrap_or_default(); // C, B, E
                let tc = op.terminal_currents(name).unwrap_or_default();
                let (vce, vbe, vbc) = (pol * (tv[0] - tv[2]), pol * (tv[1] - tv[2]), pol * (tv[1] - tv[0]));
                let region = if vbe < 0.45 {
                    Region::Off
                } else if vbc > 0.4 {
                    Region::On
                } else {
                    Region::Active
                };
                PartValues { u: vce, i: pol * tc[0], p: op.power(name).unwrap_or(0.0), ctrl_u: vbe, ctrl_i: pol * tc[1], region: Some(region) }
            }
            Some(SimRole::Mos { model }) => {
                let tv = op.terminal_voltages(name).unwrap_or_default(); // D, G, S
                let tc = op.terminal_currents(name).unwrap_or_default();
                let pol = model.polarity();
                let (vds, vgs) = (pol * (tv[0] - tv[2]), pol * (tv[1] - tv[2]));
                let region = match model.eval(vgs, vds).region {
                    elab_sim::MosRegion::Off => Region::Off,
                    elab_sim::MosRegion::Linear => Region::On,
                    elab_sim::MosRegion::Saturation => Region::Active,
                };
                PartValues { u: vds, i: pol * tc[0], p: op.power(name).unwrap_or(0.0), ctrl_u: vgs, ctrl_i: 0.0, region: Some(region) }
            }
            Some(SimRole::OpAmp) => {
                let tv = op.terminal_voltages(name).unwrap_or_default(); // +, −, Ausgang, V+, V−
                let tc = op.terminal_currents(name).unwrap_or_default();
                let p: f64 = tv.iter().zip(&tc).map(|(u, i)| u * i).sum();
                PartValues { u: tv[2], i: op.current(name).unwrap_or(0.0), p, ctrl_u: tv[0] - tv[1], ctrl_i: tc[3], region: None }
            }
            Some(SimRole::Ic { vcc, gnd }) => {
                // Alle Innenteile heißen „Name:…“: Versorgungsstrom und Verlustleistung aufsummieren
                let pn = &nl.pin_nets[&id];
                let node = |k: usize| nl.circuit.find_node(&nl.nets[pn[k]].name);
                let (nv, ng) = (node(*vcc), node(*gnd));
                let prefix = format!("{name}:");
                let (mut i, mut p) = (0.0, 0.0);
                for e in op.element_names().filter(|e| e.starts_with(&prefix)) {
                    let (tn, tc, tv) = (
                        op.terminal_nodes(e).unwrap_or_default(),
                        op.terminal_currents(e).unwrap_or_default(),
                        op.terminal_voltages(e).unwrap_or_default(),
                    );
                    for k in 0..tn.len() {
                        if Some(tn[k]) == nv {
                            i += tc[k];
                        }
                        p += tv[k] * tc[k];
                    }
                }
                let u = nv.map_or(0.0, |n| op.node_voltage(n)) - ng.map_or(0.0, |n| op.node_voltage(n));
                PartValues { u, i, p, ..Default::default() }
            }
        };
        parts.insert(id, v);
    }
    // Voltmeter: Spannung zwischen ihren Anschlüssen
    for &id in &nl.voltmeters {
        if let Some(pn) = nl.pin_nets.get(&id) {
            if let (Some(Some(a)), Some(Some(b))) = (net_v.get(pn[0]), net_v.get(pn[1])) {
                parts.insert(id, PartValues { u: a - b, ..Default::default() });
            }
        }
    }
    let pin_i = pin_currents(nl, op);
    OpView { net_v, parts, pin_i, millis: 0.0, newton: 0 }
}

/// Ströme in die Anschlüsse aller simulierten Bauteile. Jedes Simulationselement gehört
/// zu dem Bauteil, dessen Namen es trägt („R1“, „U1:Y“, „A~0“). Ein Anschlussstrom an
/// einem Knoten, an dem mehrere Pins desselben Bauteils liegen, wird aufgeteilt.
fn pin_currents(nl: &Netlist, op: &OpResult) -> HashMap<u32, Vec<f64>> {
    let by_name: HashMap<&str, u32> = nl.simulated.iter().map(|(id, n)| (n.as_str(), *id)).collect();
    let mut out: HashMap<u32, Vec<f64>> = HashMap::new();
    let pin_nodes = |id: u32| -> Vec<Option<usize>> {
        nl.pin_nets.get(&id).map(|pn| pn.iter().map(|&n| nl.circuit.find_node(&nl.nets[n].name)).collect()).unwrap_or_default()
    };
    let mut nodes_of: HashMap<u32, Vec<Option<usize>>> = HashMap::new();
    for e in op.element_names() {
        let base = e.split([':', '~']).next().unwrap_or(e);
        let Some(&id) = by_name.get(base) else { continue };
        let pins = nodes_of.entry(id).or_insert_with(|| pin_nodes(id));
        let acc = out.entry(id).or_insert_with(|| vec![0.0; pins.len()]);
        let (Ok(tn), Ok(tc)) = (op.terminal_nodes(e), op.terminal_currents(e)) else { continue };
        for (node, i) in tn.iter().zip(&tc) {
            let hits: Vec<usize> = (0..pins.len()).filter(|&k| pins[k] == Some(*node)).collect();
            for &k in &hits {
                acc[k] += i / hits.len() as f64;
            }
        }
    }
    out
}

/// Läuft die Schaltung in Echtzeit? Gemischte Schaltungen immer, analoge, sobald etwas
/// zeitabhängig ist (Kondensator, Spule, Wechsel- oder Rechteckquelle): Der Arbeitspunkt
/// allein würde dort das Wesentliche verstecken.
pub fn realtime(nl: &Netlist) -> bool {
    nl.mixed.is_some()
        || nl.circuit.elements().iter().any(|e| match &e.kind {
            elab_sim::ElementKind::Capacitor { .. } | elab_sim::ElementKind::Inductor { .. } => true,
            elab_sim::ElementKind::VSource { wave, .. } | elab_sim::ElementKind::ISource { wave, .. } => !matches!(wave, elab_sim::Waveform::Dc(_)),
            _ => false,
        })
}

/// Startet die Simulation einer gemischten Schaltung (Einschalten bei t = 0).
pub fn mixed_sim(nl: &Netlist, h_max: f64) -> Result<MixedSim, String> {
    if let Some(e) = &nl.error {
        return Err(e.clone());
    }
    let spec = nl.mixed.as_ref().map(|m| m.spec.clone()).unwrap_or_default();
    MixedSim::new(nl.circuit.clone(), spec, h_max).map_err(|e| e.to_string())
}

/// Momentaufnahme einer laufenden gemischten Simulation für die Anzeige.
pub fn mixed_view(nl: &Netlist, ms: &MixedSim) -> OpView {
    view(nl, &ms.snapshot())
}

/// Eine Kurve im Oszilloskop.
#[derive(Clone, Debug)]
pub struct Trace {
    pub name: String,
    pub color_idx: usize,
    /// Punkte (t in s, Wert)
    pub pts: Vec<[f64; 2]>,
}

/// Aufzeichnung für das Oszilloskop.
#[derive(Clone, Debug, Default)]
pub struct ScopeData {
    pub traces: Vec<Trace>,
    pub t_stop: f64,
    pub millis: f64,
    pub steps: usize,
    /// Echtzeit: Anzeige steht auf einer Triggerflanke (sonst läuft sie durch)
    pub triggered: bool,
}

/// Messpunkte mit ihrem Simulatorknoten: (Name, Farbindex, Knoten).
pub fn probe_nodes(doc: &Document, nl: &Netlist) -> Vec<(String, usize, elab_sim::NodeId)> {
    probes(doc)
        .into_iter()
        .filter_map(|(id, name, k)| {
            let net = *nl.pin_nets.get(&id)?.first()?;
            let info = &nl.nets[net];
            let node = if info.is_gnd { 0 } else { nl.circuit.find_node(&info.name)? };
            Some((name, k, node))
        })
        .collect()
}

/// Oszilloskop im Echtzeitbetrieb: zeigt das zuletzt aufgezeichnete Fenster der Länge
/// `t_win`. Mit Trigger beginnt das Fenster kurz vor der letzten steigenden Flanke des
/// ersten Messpunkts durch die Mitte seines Bereichs. Periodische Signale stehen dann
/// still. Ohne Flanke läuft die Anzeige durch (Rollmodus).
pub fn live_scope(doc: &Document, nl: &Netlist, ms: &MixedSim, t_win: f64, trigger: bool) -> ScopeData {
    let probes = probe_nodes(doc, nl);
    let tr = ms.trace();
    let now = ms.time();
    let pre = 0.1 * t_win;
    let mut start = (now - t_win).max(0.0);
    let mut triggered = false;
    if trigger && !tr.is_empty() && !probes.is_empty() {
        let recent = tr.iter().filter(|p| p.0 >= now - 2.0 * t_win);
        let (lo, hi) = recent.fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1[0]), hi.max(p.1[0])));
        if hi - lo > 1e-3 {
            let level = 0.5 * (lo + hi);
            // jüngste steigende Flanke, nach der noch ein ganzes Fenster aufgezeichnet ist
            for k in (1..tr.len()).rev() {
                let (t1, v1) = (tr[k].0, tr[k].1[0]);
                let (t0, v0) = (tr[k - 1].0, tr[k - 1].1[0]);
                if t1 < now - 2.0 * t_win {
                    break;
                }
                if v0 < level && v1 >= level {
                    let tc = t0 + (level - v0) / (v1 - v0) * (t1 - t0);
                    if tc + t_win - pre <= now && tc - pre >= 0.0 {
                        start = tc - pre;
                        triggered = true;
                        break;
                    }
                }
            }
        }
    }
    let end = start + t_win;
    let traces = probes
        .iter()
        .enumerate()
        .map(|(j, (name, k, _))| Trace {
            name: name.clone(),
            color_idx: *k,
            pts: tr.iter().filter(|p| p.0 >= start && p.0 <= end).map(|p| [p.0 - start, p.1[j]]).collect(),
        })
        .collect();
    ScopeData { traces, t_stop: t_win, millis: 0.0, steps: 0, triggered }
}

/// Messpunkte in Dokumentreihenfolge: (Teile-ID, Name, Farbindex).
pub fn probes(doc: &Document) -> Vec<(u32, String, usize)> {
    doc.parts.iter().filter(|p| matches!(p.kind, PartKind::Probe)).enumerate().map(|(k, p)| (p.id, p.name.clone(), k)).collect()
}

/// Simuliert den Zeitverlauf und liefert die Spannung an jedem Messpunkt.
pub fn record(doc: &Document, nl: &Netlist, t_stop: f64) -> Result<ScopeData, String> {
    if let Some(e) = &nl.error {
        return Err(e.clone());
    }
    let t0 = Instant::now();
    let (res, steps) = if let Some(m) = &nl.mixed {
        // Gemischte Schaltung: ab dem Einschalten
        let mut ms = MixedSim::new(nl.circuit.clone(), m.spec.clone(), t_stop / 1000.0).map_err(|e| e.to_string())?;
        let res = ms.run(t_stop).map_err(|e| e.to_string())?;
        (res, ms.stats().accepted_steps)
    } else {
        let mut sim = Simulator::new(&nl.circuit).map_err(|e| e.to_string())?;
        let res = sim.transient(&TranOptions::new(t_stop, t_stop / 1000.0)).map_err(|e| e.to_string())?;
        (res, sim.stats.accepted_steps)
    };
    let mut traces = Vec::new();
    for (id, name, color_idx) in probes(doc) {
        let Some(&net) = nl.pin_nets.get(&id).and_then(|v| v.first()) else { continue };
        let info = &nl.nets[net];
        let v: Vec<f64> = if info.is_gnd {
            vec![0.0; res.len()]
        } else if info.reachable {
            match res.voltage(&info.name) {
                Ok(v) => v,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        traces.push(Trace { name, color_idx, pts: res.time.iter().zip(v).map(|(&t, v)| [t, v]).collect() });
    }
    Ok(ScopeData { traces, t_stop, millis: t0.elapsed().as_secs_f64() * 1e3, steps, triggered: false })
}

// ---------------------------------------------------------------------------
// Digitalteil
// ---------------------------------------------------------------------------

/// Zeilen des Logikanalysators in Dokumentreihenfolge: (Name, Netz).
/// Schalter, Taster und Takte zeigen ihren Ausgang, Anzeigen und Messpunkte ihr Netz.
pub fn analyzer_rows(doc: &Document, nl: &Netlist) -> Vec<(String, usize)> {
    let Some(d) = &nl.digital else { return Vec::new() };
    doc.parts
        .iter()
        .filter_map(|p| {
            let pn = nl.pin_nets.get(&p.id)?;
            match p.kind {
                PartKind::LogicInput { .. } | PartKind::Button | PartKind::Clock { .. } | PartKind::LogicLed { .. } => Some((p.name.clone(), pn[0])),
                PartKind::Probe if d.is_digital_net[pn[0]] => Some((p.name.clone(), pn[0])),
                _ => None,
            }
        })
        .collect()
}

/// Erzeugt den Digitalsimulator für die Netzliste (beobachtet die Analysator-Netze).
pub fn digital_sim(doc: &Document, nl: &Netlist) -> Option<DigitalSim> {
    let d = nl.digital.as_ref()?;
    let mut s = DigitalSim::new(nl.nets.len(), d.comps.clone());
    for (_, net) in analyzer_rows(doc, nl) {
        s.watch(net);
    }
    Some(s)
}

/// Wahrheitstabelle einer kombinatorischen Schaltung.
#[derive(Clone, Debug)]
pub struct TruthTable {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Eingangsbelegung und Ausgangspegel je Zeile; `false` im dritten Feld: schwingt
    pub rows: Vec<(Vec<bool>, Vec<Logic>, bool)>,
}

/// Berechnet die Wahrheitstabelle: Jede Eingangskombination wird in einem frischen
/// Simulator eingestellt, bis die Schaltung zur Ruhe kommt.
pub fn truth_table(doc: &Document, nl: &Netlist) -> Result<TruthTable, String> {
    let d = nl.digital.as_ref().ok_or("Die Schaltung enthält keine Digitalbausteine.")?;
    if d.sequential {
        return Err("Die Schaltung hat ein Gedächtnis (Takt, Flipflop oder Zähler). Eine Wahrheitstabelle gibt es nur für                     Schaltnetze ohne Speicher. Den Ablauf zeigt der Logikanalysator."
            .into());
    }
    let mut inputs: Vec<(&str, usize)> = doc
        .parts
        .iter()
        .filter(|p| matches!(p.kind, PartKind::LogicInput { .. } | PartKind::Button))
        .filter_map(|p| Some((p.name.as_str(), *d.comp_of.get(&p.id)?)))
        .collect();
    inputs.sort();
    if inputs.is_empty() {
        return Err("Setze Logikschalter als Eingänge (A, B, …).".into());
    }
    if inputs.len() > 6 {
        return Err("Höchstens 6 Eingänge (64 Zeilen) werden tabelliert.".into());
    }
    let outputs: Vec<(String, usize)> = doc
        .parts
        .iter()
        .filter_map(|p| {
            let pn = nl.pin_nets.get(&p.id)?;
            match p.kind {
                PartKind::LogicLed { .. } => Some((p.name.clone(), pn[0])),
                PartKind::Probe if d.is_digital_net[pn[0]] => Some((p.name.clone(), pn[0])),
                _ => None,
            }
        })
        .collect();
    if outputs.is_empty() {
        return Err("Setze Logik-Anzeigen oder Messpunkte als Ausgänge.".into());
    }
    let n = inputs.len();
    let mut rows = Vec::new();
    for mask in 0..(1u32 << n) {
        let bits: Vec<bool> = (0..n).map(|k| mask >> (n - 1 - k) & 1 == 1).collect();
        let mut comps = d.comps.clone();
        for (k, &(_, c)) in inputs.iter().enumerate() {
            comps[c].kind = CompKind::Input(Logic::from_bool(bits[k]));
        }
        let mut s = DigitalSim::new(nl.nets.len(), comps);
        let ok = s.settle(1_000_000);
        rows.push((bits, outputs.iter().map(|&(_, net)| s.net(net)).collect(), ok));
    }
    Ok(TruthTable { inputs: inputs.iter().map(|i| i.0.to_string()).collect(), outputs: outputs.into_iter().map(|o| o.0).collect(), rows })
}

/// Liest einen Wert mit SI-Präfix: „4k7“, „4,7 k“, „100n“, „10µF“, „1M“, „2.2 mA“.
///
/// Groß-M ist Mega, klein-m ist Milli. Eine angehängte Einheit wird ignoriert.
pub fn parse_si(s: &str) -> Option<f64> {
    let s: String = s.trim().replace(',', ".").chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    let prefix = |c: char| -> Option<f64> {
        Some(match c {
            'T' => 1e12,
            'G' => 1e9,
            'M' => 1e6,
            'k' | 'K' => 1e3,
            'm' => 1e-3,
            'u' | 'µ' | 'μ' => 1e-6,
            'n' => 1e-9,
            'p' => 1e-12,
            'f' => 1e-15,
            'R' => 1.0, // „2R2“ = 2,2 Ω
            _ => return None,
        })
    };
    // Zahlanteil (inkl. Exponent) abtrennen
    let chars: Vec<char> = s.chars().collect();
    let mut end = 0;
    while end < chars.len() && (chars[end].is_ascii_digit() || chars[end] == '.' || chars[end] == '-' || chars[end] == '+') {
        end += 1;
    }
    if end < chars.len() && (chars[end] == 'e' || chars[end] == 'E') {
        let mut j = end + 1;
        if j < chars.len() && (chars[j] == '-' || chars[j] == '+') {
            j += 1;
        }
        if j < chars.len() && chars[j].is_ascii_digit() {
            end = j;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
        }
    }
    let num: String = chars[..end].iter().collect();
    let rest: String = chars[end..].iter().collect();
    let base: f64 = if num.is_empty() || num == "-" {
        return None;
    } else {
        num.parse().ok()?
    };
    let mut it = rest.chars();
    let Some(c) = it.next() else {
        return Some(base);
    };
    // „meg“ wie in SPICE
    if rest.to_lowercase().starts_with("meg") {
        return Some(base * 1e6);
    }
    match prefix(c) {
        Some(mult) => {
            // „4k7“: Ziffern nach dem Präfix sind Nachkommastellen
            let tail: String = it.clone().take_while(|c| c.is_ascii_digit()).collect();
            if !tail.is_empty() && !num.contains('.') {
                let v: f64 = format!("{num}.{tail}").parse().ok()?;
                return Some(v * mult);
            }
            Some(base * mult)
        }
        // Nur eine Einheit ohne Präfix (Ω, V, A, F, H, W, Hz, Ohm, s)
        None if c.is_alphabetic() || "Ωµ".contains(c) => Some(base),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_si;

    #[test]
    fn si_parsing() {
        let cases = [
            ("1000", 1000.0),
            ("4k7", 4700.0),
            ("4,7k", 4700.0),
            ("4.7 kΩ", 4700.0),
            ("100n", 100e-9),
            ("10µF", 10e-6),
            ("10uF", 10e-6),
            ("1M", 1e6),
            ("1m", 1e-3),
            ("2,2 mA", 2.2e-3),
            ("1meg", 1e6),
            ("12 V", 12.0),
            ("50Hz", 50.0),
            ("1e-3", 1e-3),
            ("-5", -5.0),
            ("2R2", 2.2),
        ];
        for (s, v) in cases {
            let got = parse_si(s).unwrap_or_else(|| panic!("{s} nicht lesbar"));
            assert!((got - v).abs() <= v.abs() * 1e-12, "{s}: {got} statt {v}");
        }
        assert!(parse_si("").is_none());
        assert!(parse_si("abc").is_none());
    }
}
