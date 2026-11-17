//! Vom Schaltplan zur Netzliste: Welche Pins sind leitend verbunden?
//!
//! Alle Rasterpunkte, die über Drähte zusammenhängen, bilden ein *Netz*
//! (elektrisch ein Knoten). Aus den Netzen und Bauteilen entsteht die
//! `elab_sim::Circuit`, die der Simulator rechnet.

use crate::model::{Document, PartKind, PinRole, P};
use elab_sim::digital::{Comp, CompKind, Logic};
use elab_sim::{Circuit, ElementKind, SimError, Waveform};
use std::collections::{HashMap, HashSet, VecDeque};

/// Informationen zu einem Netz.
#[derive(Clone, Debug)]
pub struct NetInfo {
    pub name: String,
    pub is_gnd: bool,
    /// Über Bauteile mit der Masse verbunden (nur solche Netze werden simuliert).
    pub reachable: bool,
}

/// Ergebnis der Netzlisten-Erzeugung.
pub struct Netlist {
    pub circuit: Circuit,
    pub nets: Vec<NetInfo>,
    /// Netz jedes Anschlusspunkts (Pins und Drahtenden).
    pub net_of: HashMap<P, usize>,
    /// Netz jedes Drahts.
    pub wire_net: HashMap<u32, usize>,
    /// Netze der Pins jedes Bauteils (in Pin-Reihenfolge).
    pub pin_nets: HashMap<u32, Vec<usize>>,
    /// Bauteile, die simuliert werden (ID → Elementname im Simulator).
    pub simulated: HashMap<u32, String>,
    /// Bauteile ohne Verbindung zur Masse.
    pub excluded: Vec<u32>,
    /// Zweipole, deren beide Anschlüsse am selben Netz hängen (überbrückt, stromlos).
    pub bridged: Vec<u32>,
    /// Digitalteil der Schaltung (falls Digitalbausteine vorhanden)
    pub digital: Option<DigitalNetlist>,
    pub error: Option<String>,
}

/// Digitalteil: Bausteine für [`elab_sim::digital::DigitalSim`].
#[derive(Clone, Debug)]
pub struct DigitalNetlist {
    pub comps: Vec<Comp>,
    /// Baustein je Bauteil (Schalter, Taster, Takt, Gatter, Flipflops, Zähler)
    pub comp_of: HashMap<u32, usize>,
    /// Netz wird digital betrieben
    pub is_digital_net: Vec<bool>,
    /// Enthält Takt, Flipflops oder Zähler (also Gedächtnis)
    pub sequential: bool,
    pub has_clock: bool,
}

/// Baut den Digitalteil und prüft, dass Analog- und Digitalnetze getrennt sind.
fn build_digital(doc: &Document, nl: &mut Netlist) {
    let digital_parts: Vec<_> = doc.parts.iter().filter(|p| p.kind.is_digital()).collect();
    if digital_parts.is_empty() {
        return;
    }
    let n = nl.nets.len();
    let mut is_dig = vec![false; n];
    let mut analog_part: Vec<Option<&str>> = vec![None; n];
    let mut digital_part: Vec<Option<&str>> = vec![None; n];
    for part in &doc.parts {
        let digital = part.kind.is_digital();
        let neutral = matches!(part.kind, PartKind::Ground | PartKind::Probe);
        for &net in &nl.pin_nets[&part.id] {
            if digital {
                is_dig[net] = true;
                digital_part[net].get_or_insert(&part.name);
            } else if !neutral {
                analog_part[net].get_or_insert(&part.name);
            }
        }
    }
    for k in 0..n {
        if let (Some(a), Some(d), false) = (analog_part[k], digital_part[k], nl.nets[k].is_gnd) {
            nl.error.get_or_insert(format!(
                "{d} (digital) und {a} (analog) hängen am selben Netz {}. Die Kopplung von Analog- und Digitaltechnik folgt in Phase 4.",
                nl.nets[k].name
            ));
        }
    }
    let mut comps = Vec::new();
    let mut comp_of = HashMap::new();
    let (mut sequential, mut has_clock) = (false, false);
    for part in &digital_parts {
        let pn = &nl.pin_nets[&part.id];
        let roles = part.kind.pin_roles();
        let ins: Vec<usize> = pn.iter().zip(&roles).filter(|(_, r)| **r == PinRole::In).map(|(n, _)| *n).collect();
        let outs: Vec<usize> = pn.iter().zip(&roles).filter(|(_, r)| **r == PinRole::Out).map(|(n, _)| *n).collect();
        let (kind, delay) = match &part.kind {
            PartKind::Gate { gate } => (CompKind::Gate(gate.to_sim()), 10),
            PartKind::LogicInput { on } => (CompKind::Input(Logic::from_bool(*on)), 0),
            PartKind::Button => (CompKind::Input(Logic::L0), 0),
            PartKind::Clock { freq } => {
                has_clock = true;
                sequential = true;
                (CompKind::Clock { half_period: (0.5e9 / freq.max(1e-6)).round().max(1.0) as u64 }, 0)
            }
            PartKind::LogicHigh => (CompKind::Const(Logic::L1), 0),
            PartKind::DFlipFlop => {
                sequential = true;
                (CompKind::DFlipFlop, 15)
            }
            PartKind::JkFlipFlop => {
                sequential = true;
                (CompKind::JkFlipFlop, 15)
            }
            PartKind::Counter4 => {
                sequential = true;
                (CompKind::Counter4, 20)
            }
            PartKind::LogicLed { .. } | PartKind::SevenSeg => continue,
            _ => continue,
        };
        comp_of.insert(part.id, comps.len());
        comps.push(Comp { kind, inputs: ins, outputs: outs, delay });
    }
    // Masse an einem Digitalnetz: fester Pegel 0
    if let Some(g) = nl.nets.iter().position(|x| x.is_gnd) {
        if is_dig[g] {
            comps.push(Comp { kind: CompKind::Const(Logic::L0), inputs: vec![], outputs: vec![g], delay: 0 });
        }
    }
    nl.digital = Some(DigitalNetlist { comps, comp_of, is_digital_net: is_dig, sequential, has_clock });
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra] = rb;
        }
    }
}

/// Erzeugt die Netzliste. Fehler stehen in `error`, die übrigen Felder sind trotzdem gefüllt.
pub fn build(doc: &Document) -> Netlist {
    // 1. Alle Anschlusspunkte einsammeln
    let mut idx: HashMap<P, usize> = HashMap::new();
    let mut pts: Vec<P> = Vec::new();
    let add = |p: P, idx: &mut HashMap<P, usize>, pts: &mut Vec<P>| {
        *idx.entry(p).or_insert_with(|| {
            pts.push(p);
            pts.len() - 1
        })
    };
    for part in &doc.parts {
        for p in part.pins() {
            add(p, &mut idx, &mut pts);
        }
    }
    for w in &doc.wires {
        add(w.a, &mut idx, &mut pts);
        add(w.b, &mut idx, &mut pts);
    }

    // 2. Verbindungen: Drahtenden, Punkte auf Drähten (T-Abzweig), Masse-Symbole
    let gnd_slot = pts.len();
    let mut dsu = Dsu((0..=gnd_slot).collect());
    for w in &doc.wires {
        dsu.union(idx[&w.a], idx[&w.b]);
        for (k, p) in pts.iter().enumerate() {
            if w.interior(*p) {
                dsu.union(k, idx[&w.a]);
            }
        }
    }
    let mut has_gnd = false;
    for part in &doc.parts {
        if matches!(part.kind, PartKind::Ground) {
            has_gnd = true;
            dsu.union(idx[&part.pins()[0]], gnd_slot);
        }
    }

    // 3. Netze nummerieren und benennen
    let mut root_net: HashMap<usize, usize> = HashMap::new();
    let mut nets: Vec<NetInfo> = Vec::new();
    let gnd_root = dsu.find(gnd_slot);
    if has_gnd {
        root_net.insert(gnd_root, 0);
        nets.push(NetInfo { name: "gnd".into(), is_gnd: true, reachable: true });
    }
    let mut net_of: HashMap<P, usize> = HashMap::new();
    for (k, p) in pts.iter().enumerate() {
        let r = dsu.find(k);
        let n = *root_net.entry(r).or_insert_with(|| {
            nets.push(NetInfo { name: String::new(), is_gnd: false, reachable: false });
            nets.len() - 1
        });
        net_of.insert(*p, n);
    }
    // Messpunkte geben ihrem Netz den Namen
    let mut used_names: HashSet<String> = ["gnd".to_string()].into();
    for part in &doc.parts {
        if matches!(part.kind, PartKind::Probe) {
            let n = net_of[&part.pins()[0]];
            if nets[n].name.is_empty() && !nets[n].is_gnd && used_names.insert(part.name.clone()) {
                nets[n].name = part.name.clone();
            }
        }
    }
    let mut k = 1;
    for net in nets.iter_mut().filter(|n| n.name.is_empty()) {
        while used_names.contains(&format!("N{k}")) {
            k += 1;
        }
        net.name = format!("N{k}");
        used_names.insert(net.name.clone());
        k += 1;
    }

    let pin_nets: HashMap<u32, Vec<usize>> = doc.parts.iter().map(|p| (p.id, p.pins().iter().map(|q| net_of[q]).collect())).collect();
    let wire_net: HashMap<u32, usize> = doc.wires.iter().map(|w| (w.id, net_of[&w.a])).collect();

    // 4. Erreichbarkeit ab Masse über analoge Zweipole
    let is_analog = |k: &PartKind| !k.is_digital() && !matches!(k, PartKind::Ground | PartKind::Probe);
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nets.len()];
    for part in doc.parts.iter().filter(|p| is_analog(&p.kind)) {
        let pn = &pin_nets[&part.id];
        if pn.len() == 2 {
            adj[pn[0]].push(pn[1]);
            adj[pn[1]].push(pn[0]);
        }
    }
    if has_gnd {
        let mut q = VecDeque::from([0usize]);
        while let Some(n) = q.pop_front() {
            for &m in &adj[n] {
                if !nets[m].reachable {
                    nets[m].reachable = true;
                    q.push_back(m);
                }
            }
        }
    }

    // 5. Netlisten-Struktur
    let mut nl = Netlist {
        circuit: Circuit::new(),
        nets,
        net_of,
        wire_net,
        pin_nets,
        simulated: HashMap::new(),
        excluded: Vec::new(),
        bridged: Vec::new(),
        digital: None,
        error: None,
    };

    // 6. Digitalteil
    build_digital(doc, &mut nl);

    // 7. Analogteil
    let electrical: Vec<_> = doc.parts.iter().filter(|p| is_analog(&p.kind)).collect();
    if !has_gnd {
        if !electrical.is_empty() {
            nl.error
                .get_or_insert("Es fehlt die Masse (⏚). Ohne Bezugspunkt gibt es keine Spannungen, setze ein Masse-Symbol an den Minuspol.".into());
        }
        nl.excluded = electrical.iter().map(|p| p.id).collect();
        return nl;
    }
    for part in &electrical {
        let pn = &nl.pin_nets[&part.id];
        if pn.len() == 2 && pn[0] == pn[1] && part.kind.shape() == crate::model::Shape::TwoPole {
            nl.bridged.push(part.id);
        }
    }
    for part in electrical {
        let pn = nl.pin_nets[&part.id].clone();
        if !nl.nets[pn[0]].reachable {
            nl.excluded.push(part.id);
            continue;
        }
        let a = nl.circuit.node(&nl.nets[pn[0]].name);
        let b = nl.circuit.node(&nl.nets[pn[1]].name);
        let kind = match &part.kind {
            PartKind::Resistor { r } => ElementKind::Resistor { a, b, r: *r },
            PartKind::Lamp { v_nom, p_nom } => ElementKind::Resistor { a, b, r: v_nom * v_nom / p_nom },
            PartKind::Capacitor { c } => ElementKind::Capacitor { a, b, c: *c, ic: None },
            PartKind::Inductor { l } => ElementKind::Inductor { a, b, l: *l, ic: None },
            PartKind::DcVoltage { v } => ElementKind::VSource { p: a, n: b, wave: Waveform::Dc(*v) },
            PartKind::AcVoltage { amp, freq, offset } => ElementKind::VSource {
                p: a,
                n: b,
                wave: Waveform::Sine { offset: *offset, amplitude: *amp, freq: *freq, phase_deg: 0.0, delay: 0.0 },
            },
            PartKind::SquareVoltage { low, high, freq } => ElementKind::VSource { p: a, n: b, wave: Waveform::square(*low, *high, *freq) },
            PartKind::CurrentSource { i } => ElementKind::ISource { p: a, n: b, wave: Waveform::Dc(*i) },
            PartKind::Diode { kind } => ElementKind::Diode { a, k: b, model: kind.model() },
            PartKind::Led { color } => ElementKind::Diode { a, k: b, model: color.model() },
            PartKind::Switch { closed } => ElementKind::Switch { a, b, closed: *closed, r_on: 1e-3, r_off: 1e15 },
            _ => unreachable!("nur analoge Bauteile"),
        };
        match nl.circuit.add(&part.name, kind) {
            Ok(_) => {
                nl.simulated.insert(part.id, part.name.clone());
            }
            Err(e) => {
                let msg = match e {
                    SimError::InvalidValue { element, msg } if msg.contains("kurzgeschlossen") => {
                        format!("{element} ist kurzgeschlossen: Beide Anschlüsse hängen am selben Netz.")
                    }
                    SimError::DuplicateName(n) => format!("Der Name „{n}“ ist doppelt vergeben. Benenne eines der Bauteile um."),
                    other => other.to_string(),
                };
                nl.error.get_or_insert(msg);
            }
        }
    }
    nl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PartKind as K;

    /// Batterie 9 V, R1 1k, R2 2k, Masse unten – klassisch verdrahtet.
    fn divider() -> Document {
        let mut d = Document::default();
        d.add_part(K::DcVoltage { v: 9.0 }, P::new(0, 0), 0); // Pins (0,-2) und (0,2)
        d.add_part(K::Resistor { r: 1000.0 }, P::new(4, -4), 0); // Pins (2,-4) (6,-4)
        d.add_part(K::Resistor { r: 2000.0 }, P::new(8, 0), 1); // Pins (8,-2) (8,2)
        d.add_part(K::Ground, P::new(0, 4), 0);
        d.add_wire(P::new(0, -2), P::new(0, -4));
        d.add_wire(P::new(0, -4), P::new(2, -4));
        d.add_wire(P::new(6, -4), P::new(8, -4));
        d.add_wire(P::new(8, -4), P::new(8, -2));
        d.add_wire(P::new(8, 2), P::new(8, 4));
        d.add_wire(P::new(8, 4), P::new(0, 4));
        d.add_wire(P::new(0, 2), P::new(0, 4));
        d
    }

    #[test]
    fn divider_has_three_nets() {
        let nl = build(&divider());
        assert!(nl.error.is_none(), "{:?}", nl.error);
        assert_eq!(nl.nets.len(), 3);
        assert!(nl.excluded.is_empty());
        let op = elab_sim::Simulator::new(&nl.circuit).unwrap().operating_point().unwrap();
        let out = &nl.nets[nl.net_of[&P::new(8, -2)]].name;
        assert!((op.voltage(out).unwrap() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn t_junction_connects() {
        let mut d = divider();
        // Messpunkt mitten auf den unteren Draht gesetzt: muss Masse sein
        d.add_part(K::Probe, P::new(4, 4), 0);
        let nl = build(&d);
        assert!(nl.nets[nl.net_of[&P::new(4, 4)]].is_gnd);
    }

    #[test]
    fn probe_names_net() {
        let mut d = divider();
        let id = d.add_part(K::Probe, P::new(8, -4), 0);
        d.part_mut(id).unwrap().name = "out".into();
        let nl = build(&d);
        assert_eq!(nl.nets[nl.net_of[&P::new(8, -2)]].name, "out");
    }

    #[test]
    fn missing_ground_is_reported() {
        let mut d = divider();
        d.parts.retain(|p| !matches!(p.kind, K::Ground));
        let nl = build(&d);
        assert!(nl.error.as_deref().unwrap().contains("Masse"));
    }

    #[test]
    fn unconnected_part_is_excluded_not_fatal() {
        let mut d = divider();
        let lone = d.add_part(K::Resistor { r: 5.0 }, P::new(20, 20), 0);
        let nl = build(&d);
        assert!(nl.error.is_none());
        assert_eq!(nl.excluded, vec![lone]);
        assert!(elab_sim::Simulator::new(&nl.circuit).is_ok());
    }

    #[test]
    fn crossing_wires_do_not_connect() {
        let mut d = Document::default();
        d.add_wire(P::new(0, 5), P::new(10, 5));
        d.add_wire(P::new(5, 0), P::new(5, 10));
        let nl = build(&d);
        assert_ne!(nl.net_of[&P::new(0, 5)], nl.net_of[&P::new(5, 0)]);
    }

    #[test]
    fn bridged_resistor_is_detected() {
        let mut d = divider();
        // Draht parallel zu R2 (8,-2)…(8,2) außen herum: R2 ist überbrückt
        d.add_wire(P::new(8, -2), P::new(10, -2));
        d.add_wire(P::new(10, -2), P::new(10, 2));
        d.add_wire(P::new(10, 2), P::new(8, 2));
        let nl = build(&d);
        let r2 = d.part_by_name("R2").unwrap().id;
        assert_eq!(nl.bridged, vec![r2]);
    }

    #[test]
    fn shorted_source_gives_clear_error() {
        let mut d = Document::default();
        d.add_part(K::DcVoltage { v: 5.0 }, P::new(0, 0), 0);
        d.add_part(K::Ground, P::new(0, 2), 0);
        d.add_wire(P::new(0, -2), P::new(3, -2));
        d.add_wire(P::new(3, -2), P::new(3, 2));
        d.add_wire(P::new(3, 2), P::new(0, 2));
        let nl = build(&d);
        assert!(nl.error.as_deref().unwrap().contains("kurzgeschlossen"), "{:?}", nl.error);
    }
}
