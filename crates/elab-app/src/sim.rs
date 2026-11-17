//! Anbindung an den Simulationskern: Arbeitspunkt live, Zeitverlauf fürs Oszilloskop.

use crate::model::{Document, PartKind};
use crate::netlist::Netlist;
use elab_sim::digital::{CompKind, DigitalSim, Logic};
use elab_sim::{Simulator, TranOptions};
use std::collections::HashMap;
use std::time::Instant;

/// Messwerte eines Bauteils.
#[derive(Clone, Copy, Debug, Default)]
pub struct PartValues {
    /// Spannung erster minus zweiter Anschluss in V
    pub u: f64,
    /// Strom (Vorzeichen wie im Simulator) in A
    pub i: f64,
    /// Leistung in W: aufgenommen (passiv) bzw. abgegeben (Quelle)
    pub p: f64,
}

/// Ergebnis des DC-Arbeitspunkts, aufbereitet für die Anzeige.
#[derive(Clone, Debug, Default)]
pub struct OpView {
    /// Spannung je Netz (None = nicht simuliert)
    pub net_v: Vec<Option<f64>>,
    pub parts: HashMap<u32, PartValues>,
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
    let net_v = nl
        .nets
        .iter()
        .map(|n| {
            if n.is_gnd {
                Some(0.0)
            } else if n.reachable {
                op.voltage(&n.name).ok()
            } else {
                None
            }
        })
        .collect();
    let parts = nl
        .simulated
        .iter()
        .map(|(&id, name)| {
            let u = op.element_voltage(name).unwrap_or(0.0);
            let i = op.current(name).unwrap_or(0.0);
            (id, PartValues { u, i, p: u * i })
        })
        .collect();
    Ok(OpView { net_v, parts, millis: t0.elapsed().as_secs_f64() * 1e3, newton: sim.stats.newton_iterations })
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
    let mut sim = Simulator::new(&nl.circuit).map_err(|e| e.to_string())?;
    let res = sim.transient(&TranOptions::new(t_stop, t_stop / 1000.0)).map_err(|e| e.to_string())?;
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
    Ok(ScopeData { traces, t_stop, millis: t0.elapsed().as_secs_f64() * 1e3, steps: sim.stats.accepted_steps })
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
