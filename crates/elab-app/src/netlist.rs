//! Vom Schaltplan zur Netzliste: Welche Pins sind leitend verbunden?
//!
//! Alle Rasterpunkte, die über Drähte zusammenhängen, bilden ein *Netz*
//! (elektrisch ein Knoten). Aus den Netzen und Bauteilen entsteht die
//! `elab_sim::Circuit`, die der Simulator rechnet.

use crate::model::{Document, IcPin, Part, PartKind, PinRole, P};
use elab_sim::digital::{Comp, CompKind, GateKind, Logic};
use elab_sim::{Circuit, DriveSrc, DriverLink, ElementKind, Level, MixedSpec, NodeId, ObsKind, Observer, PowerGroup, SimError, Waveform};
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
    /// Besondere Auswertung einzelner Bauteile (Transistoren, Messgeräte, Potentiometer)
    pub roles: HashMap<u32, SimRole>,
    /// Voltmeter (kein Simulator-Element, sie messen nur)
    pub voltmeters: Vec<u32>,
    /// Digitalteil einer rein digitalen Schaltung
    pub digital: Option<DigitalNetlist>,
    /// Gemischte Schaltung: Logik mit Versorgung, an Analogteil gekoppelt
    pub mixed: Option<MixedNetlist>,
    pub error: Option<String>,
}

/// Wie ein Bauteil ausgewertet wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimRole {
    Ammeter,
    Pot,
    /// Bipolartransistor, Polarität +1 (NPN) / −1 (PNP)
    Bjt {
        pol: f64,
    },
    Mos {
        model: elab_sim::MosModel,
    },
    /// Baustein mit Versorgung: Anschlussnummern von VCC und GND
    Ic {
        vcc: usize,
        gnd: usize,
    },
    OpAmp,
}

/// Woher der Logikpegel an einem Anschluss stammt (für die Anzeige).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PinSrc {
    None,
    /// Eingang: was der Baustein liest (Beobachter, Bit)
    Obs(usize, usize),
    /// Ausgang: Digitalnetz, das die Ausgangsstufe steuert
    Net(usize),
}

/// Gemischte Schaltung: Spezifikation für [`elab_sim::MixedSim`] und Zuordnungen.
#[derive(Clone, Debug, Default)]
pub struct MixedNetlist {
    pub spec: MixedSpec,
    /// Netz wird rein digital betrieben (Index wie `nets`, ohne Spannung)
    pub digital_net: Vec<bool>,
    /// Eingabebaustein je Logikschalter/Taster
    pub comp_of: HashMap<u32, usize>,
    /// Pegelquellen der Anschlüsse von ICs und Logikbausteinen an Analognetzen
    pub pin_src: HashMap<u32, Vec<PinSrc>>,
    /// Versorgungsgruppe je IC
    pub group_of: HashMap<u32, usize>,
}

/// Baut die Kopplung von Analog- und Digitalteil auf.
struct MixBuilder {
    spec: MixedSpec,
    comp_of: HashMap<u32, usize>,
    pin_src: HashMap<u32, Vec<PinSrc>>,
    group_of: HashMap<u32, usize>,
    vdd: Option<NodeId>,
}

/// Ausgangswiderstand von 74HC-Ausgängen bei 5 V.
const R_OUT_HC: f64 = 50.0;
/// Ausgangswiderstand der idealen Logikbausteine, wenn sie Analognetze treiben.
const R_OUT_IDEAL: f64 = 20.0;

impl MixBuilder {
    fn new(n_nets: usize) -> Self {
        Self {
            spec: MixedSpec { n_nets, ..Default::default() },
            comp_of: HashMap::new(),
            pin_src: HashMap::new(),
            group_of: HashMap::new(),
            vdd: None,
        }
    }

    fn net(&mut self) -> usize {
        self.spec.n_nets += 1;
        self.spec.n_nets - 1
    }

    fn comp(&mut self, kind: CompKind, inputs: Vec<usize>, outputs: Vec<usize>, delay: u64) -> usize {
        self.spec.comps.push(Comp { kind, inputs, outputs, delay });
        self.spec.comps.len() - 1
    }

    fn group(&mut self, name: &str, vcc: NodeId, gnd: NodeId, v_min: f64, v_max: f64) -> usize {
        self.spec.groups.push(PowerGroup { name: name.into(), vcc, gnd, v_min, v_max });
        self.spec.groups.len() - 1
    }

    /// Eingang: beobachtet eine Spannung und liefert je Bit ein Digitalnetz.
    fn observe(&mut self, kind: ObsKind, bits: usize, group: Option<usize>, floating: Option<Logic>) -> (usize, Vec<usize>) {
        let nets: Vec<usize> = (0..bits).map(|_| self.net()).collect();
        let inputs: Vec<usize> = nets.iter().map(|&n| self.comp(CompKind::Input(Logic::X), vec![], vec![n], 0)).collect();
        self.spec.observers.push(Observer { kind, inputs, group, floating });
        (self.spec.observers.len() - 1, nets)
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(
        &mut self,
        c: &mut Circuit,
        name: &str,
        out: NodeId,
        hi: NodeId,
        lo: NodeId,
        r: f64,
        src: DriveSrc,
        group: Option<usize>,
        open_drain: bool,
    ) -> Result<(), SimError> {
        let elem = c.add(name, ElementKind::Driver { out, hi, lo, r })?;
        self.spec.drivers.push(DriverLink { elem, src, group, open_drain });
        Ok(())
    }

    /// Versorgungsknoten der idealen Logikbausteine (5 V).
    fn vdd(&mut self, c: &mut Circuit) -> Result<NodeId, SimError> {
        if let Some(v) = self.vdd {
            return Ok(v);
        }
        let v = c.node("~vdd");
        c.add("~VDD", ElementKind::VSource { p: v, n: 0, wave: Waveform::Dc(5.0) })?;
        self.vdd = Some(v);
        Ok(v)
    }

    /// Baut einen IC: Eingänge, Logikkern, Ausgangsstufen, Versorgung.
    fn ic(&mut self, c: &mut Circuit, part: &Part, nodes: &[NodeId], floating: &[bool]) -> Result<Option<SimRole>, SimError> {
        let name = &part.name;
        let fl = |k: usize, v: Logic| floating[k].then_some(v);
        let mut src = Vec::new();
        let role = match &part.kind {
            PartKind::GateIc { .. } | PartKind::SchmittIc { .. } | PartKind::DffIc { .. } | PartKind::CounterIc { .. } => {
                let family = part.kind.family().unwrap();
                let roles = part.kind.ic_pins();
                let n_in = roles.iter().filter(|r| **r == IcPin::In).count();
                let n_out = roles.iter().filter(|r| **r == IcPin::Out).count();
                let (vcc, gnd) = (nodes[n_in + n_out], nodes[n_in + n_out + 1]);
                let g = self.group(name, vcc, gnd, family.v_min(), 7.0);
                self.group_of.insert(part.id, g);
                let schmitt = matches!(part.kind, PartKind::SchmittIc { .. });
                let (il, ih) = if schmitt { family.schmitt() } else { family.thresholds() };
                let mut ins = Vec::new();
                for (k, &node) in nodes.iter().enumerate().take(n_in) {
                    let (oi, nets) = self.observe(ObsKind::Threshold { node, gnd, vcc, il, ih, schmitt }, 1, Some(g), fl(k, Logic::X));
                    src.push(PinSrc::Obs(oi, 0));
                    ins.push(nets[0]);
                }
                let outs: Vec<usize> = (0..n_out).map(|_| self.net()).collect();
                let slow = if family == crate::model::LogicFamily::Hct { 2 } else { 0 };
                match &part.kind {
                    PartKind::GateIc { gate, .. } => {
                        self.comp(CompKind::Gate(gate.to_sim()), ins, outs.clone(), 9 + slow);
                    }
                    PartKind::SchmittIc { .. } => {
                        self.comp(CompKind::Gate(GateKind::Not), ins, outs.clone(), 12 + slow);
                    }
                    PartKind::DffIc { .. } => {
                        self.comp(CompKind::DFlipFlop, ins, outs.clone(), 15 + slow);
                    }
                    _ => {
                        // /MR ist low-aktiv: intern invertiert auf den Reset des Zählers
                        let r = self.net();
                        self.comp(CompKind::Gate(GateKind::Not), vec![ins[1]], vec![r], 0);
                        self.comp(CompKind::Counter4, vec![ins[0], r], outs.clone(), 18 + slow);
                    }
                }
                let pin_names = part.kind.pin_names();
                for (k, &o) in outs.iter().enumerate() {
                    let pin = n_in + k;
                    self.drive(c, &format!("{name}:{}", pin_names[pin]), nodes[pin], vcc, gnd, R_OUT_HC, DriveSrc::Net(o), Some(g), false)?;
                    src.push(PinSrc::Net(o));
                }
                src.extend([PinSrc::None, PinSrc::None]);
                c.add(&format!("{name}:IQ"), ElementKind::Resistor { a: vcc, b: gnd, r: 10e6 })?;
                Some(SimRole::Ic { vcc: n_in + n_out, gnd: n_in + n_out + 1 })
            }
            PartKind::Timer555 => {
                let [trig, thr, ctrl, out, dis, vcc, res, gnd] = nodes[..] else { unreachable!() };
                let g = self.group(name, vcc, gnd, 4.5, 18.0);
                self.group_of.insert(part.id, g);
                let third = c.node(&format!("{name}:third"));
                let oh = c.node(&format!("{name}:oh"));
                c.add(&format!("{name}:R1"), ElementKind::Resistor { a: vcc, b: ctrl, r: 5e3 })?;
                c.add(&format!("{name}:R2"), ElementKind::Resistor { a: ctrl, b: third, r: 5e3 })?;
                c.add(&format!("{name}:R3"), ElementKind::Resistor { a: third, b: gnd, r: 5e3 })?;
                c.add(&format!("{name}:IQ"), ElementKind::Resistor { a: vcc, b: gnd, r: 2.2e3 })?;
                // Ausgangsstufe in Darlington-Technik: High liegt etwa 1,7 V unter VCC
                c.add(&format!("{name}:VOH"), ElementKind::VSource { p: vcc, n: oh, wave: Waveform::Dc(1.7) })?;
                let (os, s) = self.observe(ObsKind::Compare { plus: third, minus: trig }, 1, Some(g), fl(0, Logic::X));
                let (or, r) = self.observe(ObsKind::Compare { plus: thr, minus: ctrl }, 1, Some(g), fl(1, Logic::X));
                let (on, nr) = self.observe(
                    ObsKind::Threshold { node: res, gnd, vcc, il: Level::abs(0.7), ih: Level::abs(0.7), schmitt: false },
                    1,
                    Some(g),
                    fl(6, Logic::L1),
                );
                let (q1, q2) = (self.net(), self.net());
                self.comp(CompKind::Latch555, vec![s[0], r[0], nr[0]], vec![q1, q2], 100);
                self.drive(c, &format!("{name}:OUT"), out, oh, gnd, 10.0, DriveSrc::Net(q1), Some(g), false)?;
                self.drive(c, &format!("{name}:DIS"), dis, gnd, gnd, 10.0, DriveSrc::Net(q2), Some(g), true)?;
                src = vec![
                    PinSrc::Obs(os, 0),
                    PinSrc::Obs(or, 0),
                    PinSrc::None,
                    PinSrc::Net(q1),
                    PinSrc::None,
                    PinSrc::None,
                    PinSrc::Obs(on, 0),
                    PinSrc::None,
                ];
                Some(SimRole::Ic { vcc: 5, gnd: 7 })
            }
            PartKind::OpAmp { kind } => {
                let [p, n, out, vcc, vee] = nodes[..] else { unreachable!() };
                c.add(name, ElementKind::OpAmp { p, n, out, vcc, vee, model: kind.model() })?;
                Some(SimRole::OpAmp)
            }
            PartKind::Adc4 => {
                let [inp, d0, d1, d2, d3, vcc, vref, gnd] = nodes[..] else { unreachable!() };
                let g = self.group(name, vcc, gnd, 2.0, 7.0);
                self.group_of.insert(part.id, g);
                let (_, nets) = self.observe(ObsKind::Adc { node: inp, gnd, vref, bits: 4 }, 4, Some(g), floating[0].then_some(Logic::X));
                src.push(PinSrc::None);
                for (k, (&d, &net)) in [d0, d1, d2, d3].iter().zip(&nets).enumerate() {
                    self.drive(c, &format!("{name}:D{k}"), d, vcc, gnd, R_OUT_HC, DriveSrc::Net(net), Some(g), false)?;
                    src.push(PinSrc::Net(net));
                }
                src.extend([PinSrc::None; 3]);
                c.add(&format!("{name}:IQ"), ElementKind::Resistor { a: vcc, b: gnd, r: 10e6 })?;
                Some(SimRole::Ic { vcc: 5, gnd: 7 })
            }
            PartKind::Dac4 => {
                let [d0, d1, d2, d3, out, vcc, vref, gnd] = nodes[..] else { unreachable!() };
                let g = self.group(name, vcc, gnd, 2.0, 7.0);
                self.group_of.insert(part.id, g);
                let (il, ih) = crate::model::LogicFamily::Hc.thresholds();
                let mut nets = Vec::new();
                for (k, &d) in [d0, d1, d2, d3].iter().enumerate() {
                    let (oi, n) = self.observe(ObsKind::Threshold { node: d, gnd, vcc, il, ih, schmitt: false }, 1, Some(g), fl(k, Logic::X));
                    src.push(PinSrc::Obs(oi, 0));
                    nets.push(n[0]);
                }
                // R-2R-Netzwerk: Ausgangswiderstand R, Spannung = U_REF · Code / 16
                self.drive(c, &format!("{name}:OUT"), out, vref, gnd, 10e3, DriveSrc::Code { nets, full: 16.0 }, Some(g), false)?;
                src.extend([PinSrc::None; 4]);
                c.add(&format!("{name}:IQ"), ElementKind::Resistor { a: vcc, b: gnd, r: 10e6 })?;
                Some(SimRole::Ic { vcc: 5, gnd: 7 })
            }
            _ => None,
        };
        if !src.is_empty() {
            self.pin_src.insert(part.id, src);
        }
        Ok(role)
    }
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

/// Baut den Digitalteil einer rein digitalen Schaltung.
fn build_digital(doc: &Document, nl: &mut Netlist) {
    let digital_parts: Vec<_> = doc.parts.iter().filter(|p| p.kind.is_digital()).collect();
    if digital_parts.is_empty() {
        return;
    }
    let n = nl.nets.len();
    let mut is_dig = vec![false; n];
    for part in &digital_parts {
        for &net in &nl.pin_nets[&part.id] {
            is_dig[net] = true;
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
    build_with(doc, &HashSet::new())
}

/// Abstand, in dem die Innenschaltungen eigener ICs abseits des Schaltplans eingesetzt werden.
const CUSTOM_OFFSET: i32 = 100_000;
/// Größte Schachtelungstiefe eigener ICs.
const CUSTOM_DEPTH: usize = 6;

/// Setzt die Innenschaltungen eigener ICs ein: Ihre Bauteile und Drähte kommen weit
/// abseits in den Schaltplan (Namen „U1/R1“), jeder Anschluss des ICs wird mit dem
/// passenden Port-Bauteil innen verbunden (`joins`). Das IC-Symbol selbst bleibt als
/// neutrales Bauteil stehen, damit seine Anschlüsse ihr Netz kennen.
fn flatten(doc: &Document) -> (Document, Vec<(P, P)>) {
    let mut flat = doc.clone();
    let mut joins = Vec::new();
    if !doc.parts.iter().any(|p| p.kind.is_custom()) {
        return (flat, joins);
    }
    let mut next_id = doc.next_id.max(1_000_000);
    let mut slot = 0;
    let customs: Vec<Part> = doc.parts.iter().filter(|p| p.kind.is_custom()).cloned().collect();
    for part in &customs {
        expand(part, "", 0, &mut flat, &mut joins, &mut next_id, &mut slot);
    }
    (flat, joins)
}

fn expand(part: &Part, prefix: &str, depth: usize, flat: &mut Document, joins: &mut Vec<(P, P)>, next_id: &mut u32, slot: &mut i32) {
    let PartKind::Custom { def, .. } = &part.kind else { return };
    if depth >= CUSTOM_DEPTH {
        return;
    }
    *slot += 1;
    let off = P::new(CUSTOM_OFFSET * *slot, 0);
    let pre = format!("{prefix}{}/", part.name);
    let mut inner_customs = Vec::new();
    for inner in &def.parts {
        let mut q = inner.clone();
        q.pos = q.pos + off;
        q.id = *next_id;
        *next_id += 1;
        q.name = format!("{pre}{}", inner.name);
        if q.kind.is_custom() {
            inner_customs.push(q.clone());
        }
        flat.parts.push(q);
    }
    for w in &def.wires {
        let mut w2 = *w;
        w2.a = w.a + off;
        w2.b = w.b + off;
        w2.id = *next_id;
        *next_id += 1;
        flat.wires.push(w2);
    }
    let outer = part.pins();
    for (k, (pid, _, _)) in crate::model::custom_ports(def).iter().enumerate() {
        if let (Some(port), Some(&o)) = (def.part(*pid), outer.get(k)) {
            joins.push((o, port.pos + off));
        }
    }
    for q in inner_customs {
        let pre_inner = format!("{prefix}{}/", part.name);
        // Name ohne den eben vergebenen Präfix, damit er nicht doppelt erscheint
        let mut q2 = q.clone();
        q2.name = q.name.trim_start_matches(&pre_inner).to_string();
        expand(&q2, &pre_inner, depth + 1, flat, joins, next_id, slot);
    }
}

/// Wie [`build`], mit dem Zustand gedrückter Taster.
pub fn build_with(doc_in: &Document, pressed: &HashSet<u32>) -> Netlist {
    let (flat, joins) = flatten(doc_in);
    let doc = &flat;
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
    for (a, b) in &joins {
        if let (Some(&ia), Some(&ib)) = (idx.get(a), idx.get(b)) {
            dsu.union(ia, ib);
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
    let is_analog = |k: &PartKind| !k.is_digital() && !k.is_neutral();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nets.len()];
    for part in doc.parts.iter().filter(|p| is_analog(&p.kind)) {
        let pn = &pin_nets[&part.id];
        for w in pn.windows(2) {
            adj[w[0]].push(w[1]);
            adj[w[1]].push(w[0]);
        }
    }
    // Ideale Logikbausteine bringen ihre Versorgung (0 V / 5 V) selbst mit: In einer gemischten
    // Schaltung legen ihre Anschlüsse den Bezug fest, auch ohne Masse-Symbol.
    let has_digital = doc.parts.iter().any(|p| p.kind.is_digital());
    let has_ic = doc.parts.iter().any(|p| p.kind.is_ic());
    let has_plain_analog = doc.parts.iter().any(|p| !p.kind.is_digital() && !p.kind.is_ic() && !p.kind.is_neutral());
    let mixed = has_ic || (has_digital && has_plain_analog);
    let implicit_gnd = mixed && !has_gnd && has_digital;
    let mut seeds: Vec<usize> = if has_gnd { vec![0] } else { Vec::new() };
    if implicit_gnd {
        for part in doc.parts.iter().filter(|p| p.kind.is_digital()) {
            seeds.extend(pin_nets[&part.id].iter().copied());
        }
    }
    for &s in &seeds {
        nets[s].reachable = true;
    }
    {
        let mut q = VecDeque::from(seeds);
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
        roles: HashMap::new(),
        voltmeters: doc.parts.iter().filter(|p| matches!(p.kind, PartKind::Voltmeter)).map(|p| p.id).collect(),
        digital: None,
        mixed: None,
        error: None,
    };

    // 6. Rein digital oder gemischt?
    let neutral = |k: &PartKind| k.is_neutral();
    if !mixed {
        build_digital(doc, &mut nl);
    }

    // 7. Analogteil
    let electrical: Vec<_> = doc.parts.iter().filter(|p| is_analog(&p.kind)).collect();
    // Netze, deren Spannung etwas festlegt (sonst hängen IC-Eingänge in der Luft)
    let mut drivable = vec![false; nl.nets.len()];
    let mut analog_net = vec![false; nl.nets.len()];
    for part in &doc.parts {
        let pn = &nl.pin_nets[&part.id];
        if part.kind.is_ic() {
            for (k, role) in part.kind.ic_pins().iter().enumerate() {
                analog_net[pn[k]] = true;
                if !matches!(role, IcPin::In | IcPin::AnalogIn) {
                    drivable[pn[k]] = true;
                }
            }
        } else if part.kind.is_digital() {
            for (k, role) in part.kind.pin_roles().iter().enumerate() {
                if *role == PinRole::Out {
                    drivable[pn[k]] = true;
                }
            }
        } else if !neutral(&part.kind) {
            for &n in pn {
                analog_net[n] = true;
                drivable[n] = true;
            }
        }
    }
    for (k, net) in nl.nets.iter().enumerate() {
        if net.is_gnd {
            analog_net[k] = true;
            drivable[k] = true;
        }
    }
    let mut mix = MixBuilder::new(nl.nets.len());
    if !has_gnd && !implicit_gnd {
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
        if !pn.iter().any(|&n| nl.nets[n].reachable) {
            nl.excluded.push(part.id);
            continue;
        }
        let nodes: Vec<usize> = pn.iter().map(|&n| nl.circuit.node(&nl.nets[n].name)).collect();
        let (a, b) = (nodes[0], nodes[1]);
        // Durchgebrannt: Das Bauteil leitet nicht mehr, nur ein winziger Leckstrom hält die
        // Knoten definiert.
        if part.broken && !part.kind.is_ic() {
            for (k, &n) in nodes.iter().enumerate().skip(1) {
                if n != a {
                    let _ = nl.circuit.add(&format!("{}:defekt{k}", part.name), ElementKind::Resistor { a, b: n, r: 1e12 });
                }
            }
            continue;
        }
        // Mehrteilige Bauteile
        match &part.kind {
            _ if part.kind.is_ic() => {
                let floating: Vec<bool> = pn.iter().map(|&n| !drivable[n]).collect();
                match mix.ic(&mut nl.circuit, part, &nodes, &floating) {
                    Ok(role) => {
                        nl.simulated.insert(part.id, part.name.clone());
                        if let Some(role) = role {
                            nl.roles.insert(part.id, role);
                        }
                    }
                    Err(SimError::DuplicateName(_)) => {
                        nl.error.get_or_insert(format!("Der Name „{}“ ist doppelt vergeben. Benenne eines der Bauteile um.", part.name));
                    }
                    Err(e) => {
                        nl.error.get_or_insert(e.to_string());
                    }
                }
                continue;
            }
            PartKind::Potentiometer { r, pos } => {
                let pos = pos.clamp(0.0, 1.0);
                // Stellung = Anteil zwischen Schleifer und B
                let (ra, rb) = ((r * (1.0 - pos)).max(1e-3), (r * pos).max(1e-3));
                let ok = nl.circuit.add(&format!("{}:AS", part.name), ElementKind::Resistor { a, b, r: ra }).is_ok()
                    && nl.circuit.add(&format!("{}:SB", part.name), ElementKind::Resistor { a: b, b: nodes[2], r: rb }).is_ok();
                if ok {
                    nl.simulated.insert(part.id, part.name.clone());
                    nl.roles.insert(part.id, SimRole::Pot);
                } else {
                    nl.error.get_or_insert(format!("Der Name „{}“ ist doppelt vergeben. Benenne eines der Bauteile um.", part.name));
                }
                continue;
            }
            PartKind::Transistor { kind } => {
                let elem = match (kind.bjt(), kind.mos()) {
                    (Some(model), _) => ElementKind::Bjt { c: nodes[1], b: nodes[0], e: nodes[2], model },
                    (_, Some(model)) => ElementKind::Mosfet { d: nodes[1], g: nodes[0], s: nodes[2], model },
                    _ => unreachable!(),
                };
                let role = match (kind.bjt(), kind.mos()) {
                    (Some(m), _) => SimRole::Bjt { pol: m.polarity() },
                    (_, Some(m)) => SimRole::Mos { model: m },
                    _ => unreachable!(),
                };
                match nl.circuit.add(&part.name, elem) {
                    Ok(_) => {
                        nl.simulated.insert(part.id, part.name.clone());
                        nl.roles.insert(part.id, role);
                    }
                    Err(e) => {
                        nl.error.get_or_insert(e.to_string());
                    }
                }
                continue;
            }
            _ => {}
        }
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
            PartKind::PushButton => ElementKind::Switch { a, b, closed: pressed.contains(&part.id), r_on: 1e-3, r_off: 1e15 },
            PartKind::Ammeter => {
                nl.roles.insert(part.id, SimRole::Ammeter);
                ElementKind::VSource { p: a, n: b, wave: Waveform::Dc(0.0) }
            }
            other => unreachable!("{other:?} ist kein Analogbauteil"),
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
    if mixed {
        if let Err(e) = bridge_digital(doc, &mut nl, &mut mix, &analog_net, &drivable) {
            nl.error.get_or_insert(e.to_string());
        }
        let mut digital_net = vec![false; nl.nets.len()];
        for part in doc.parts.iter().filter(|p| p.kind.is_digital()) {
            for &n in &nl.pin_nets[&part.id] {
                digital_net[n] = !analog_net[n];
            }
        }
        nl.mixed = Some(MixedNetlist { spec: mix.spec, digital_net, comp_of: mix.comp_of, pin_src: mix.pin_src, group_of: mix.group_of });
    }
    nl
}

/// Gemischte Schaltung: Ideale Logikbausteine laufen im Digitalteil mit. Hängt ein
/// Anschluss an einem Analognetz, wird er gekoppelt: Ausgänge treiben mit 5 V
/// über einen kleinen Innenwiderstand, Eingänge schalten bei 2,5 V.
fn bridge_digital(doc: &Document, nl: &mut Netlist, mix: &mut MixBuilder, analog_net: &[bool], drivable: &[bool]) -> Result<(), SimError> {
    for part in doc.parts.iter().filter(|p| p.kind.is_digital()) {
        let pn = nl.pin_nets[&part.id].clone();
        let roles = part.kind.pin_roles();
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        let mut src = Vec::new();
        let mut bridged = false;
        for (k, (&n, role)) in pn.iter().zip(&roles).enumerate() {
            if !analog_net[n] {
                src.push(PinSrc::Net(n));
                match role {
                    PinRole::In => ins.push(n),
                    PinRole::Out => outs.push(n),
                    PinRole::Passive => {}
                }
                continue;
            }
            bridged = true;
            let node = nl.circuit.node(&nl.nets[n].name);
            match role {
                PinRole::In => {
                    let vdd = mix.vdd(&mut nl.circuit)?;
                    let floating = (!drivable[n]).then_some(Logic::X);
                    let (oi, nets) = mix.observe(
                        ObsKind::Threshold { node, gnd: 0, vcc: vdd, il: Level::frac(0.5), ih: Level::frac(0.5), schmitt: false },
                        1,
                        None,
                        floating,
                    );
                    ins.push(nets[0]);
                    src.push(PinSrc::Obs(oi, 0));
                }
                PinRole::Out => {
                    let vdd = mix.vdd(&mut nl.circuit)?;
                    let d = mix.net();
                    mix.drive(&mut nl.circuit, &format!("{}~{k}", part.name), node, vdd, 0, R_OUT_IDEAL, DriveSrc::Net(d), None, false)?;
                    outs.push(d);
                    src.push(PinSrc::Net(d));
                }
                PinRole::Passive => src.push(PinSrc::None),
            }
        }
        if bridged {
            mix.pin_src.insert(part.id, src);
        }
        let (kind, delay) = match &part.kind {
            PartKind::Gate { gate } => (CompKind::Gate(gate.to_sim()), 10),
            PartKind::LogicInput { on } => (CompKind::Input(Logic::from_bool(*on)), 0),
            PartKind::Button => (CompKind::Input(Logic::L0), 0),
            PartKind::Clock { freq } => (CompKind::Clock { half_period: (0.5e9 / freq.max(1e-6)).round().max(1.0) as u64 }, 0),
            PartKind::LogicHigh => (CompKind::Const(Logic::L1), 0),
            PartKind::DFlipFlop => (CompKind::DFlipFlop, 15),
            PartKind::JkFlipFlop => (CompKind::JkFlipFlop, 15),
            PartKind::Counter4 => (CompKind::Counter4, 20),
            _ => continue,
        };
        let c = mix.comp(kind, ins, outs, delay);
        mix.comp_of.insert(part.id, c);
    }
    Ok(())
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

    /// Eigene ICs geschachtelt: Volladdierer-IC aus zwei Halbaddierer-ICs.
    #[test]
    fn nested_custom_ics() {
        use crate::model::{GateType, LedColor};
        use elab_sim::digital::Logic::*;
        let ha = crate::examples::half_adder_ic();
        let ic = |name: &str, def: &Document| K::Custom { name: name.into(), def: Box::new(def.clone()) };
        // Volladdierer als IC: Ports A, B, Ci links, S, Co rechts
        let mut fa = Document::default();
        for (n, y) in [("A", -5), ("B", -1), ("Ci", -9)] {
            let id = fa.add_part(K::Port, P::new(1, y), 2);
            fa.part_mut(id).unwrap().name = n.into();
        }
        fa.add_part(ic("Halbaddierer", &ha), P::new(10, -2), 0);
        fa.add_part(ic("Halbaddierer", &ha), P::new(20, -4), 0);
        fa.add_part(K::Gate { gate: GateType::Or }, P::new(28, 2), 0);
        for (n, x, y) in [("S", 27, -5), ("Co", 35, 2)] {
            let id = fa.add_part(K::Port, P::new(x - 2, y), 0);
            fa.part_mut(id).unwrap().name = n.into();
        }
        for pts in [
            vec![(1, -5), (5, -5), (5, -3), (7, -3)],
            vec![(1, -1), (7, -1)],
            vec![(1, -9), (16, -9), (16, -5), (17, -5)],
            vec![(13, -3), (17, -3)],
            vec![(23, -5), (25, -5)],
            vec![(23, -3), (24, -3), (24, 1), (25, 1)],
            vec![(13, -1), (14, -1), (14, 3), (25, 3)],
            vec![(31, 2), (33, 2)],
        ] {
            for w in pts.windows(2) {
                fa.add_wire(P::new(w[0].0, w[0].1), P::new(w[1].0, w[1].1));
            }
        }
        let names: Vec<String> = crate::model::custom_ports(&fa).into_iter().map(|p| p.1).collect();
        assert_eq!(names, ["Ci", "A", "B", "S", "Co"]);
        // Außen: Schalter an den Eingängen, Anzeigen an den Ausgängen
        let mut d = Document::default();
        let u = d.add_part(ic("Volladdierer", &fa), P::new(0, 0), 0);
        let pins = d.part(u).unwrap().pins();
        for (k, n) in ["Ci", "A", "B"].iter().enumerate() {
            let id = d.add_part(K::LogicInput { on: false }, pins[k] + P::new(-4, 0), 0);
            d.part_mut(id).unwrap().name = n.to_string();
            d.add_wire(pins[k] + P::new(-2, 0), pins[k]);
        }
        for (k, n) in ["S", "Co"].iter().enumerate() {
            let id = d.add_part(K::LogicLed { color: LedColor::Rot }, pins[3 + k] + P::new(4, 0), 0);
            d.part_mut(id).unwrap().name = n.to_string();
            d.add_wire(pins[3 + k], pins[3 + k] + P::new(2, 0));
        }
        let nl = build(&d);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        let tt = crate::sim::truth_table(&d, &nl).unwrap();
        assert_eq!(tt.inputs.len(), 3);
        for (ins, outs, _) in &tt.rows {
            let n = ins.iter().filter(|b| **b).count();
            assert_eq!(outs, &vec![if n % 2 == 1 { L1 } else { L0 }, if n >= 2 { L1 } else { L0 }], "{ins:?}");
        }
        // Namen innen sind eindeutig und tragen den Pfad
        assert!(nl.nets.iter().all(|n| !n.name.is_empty()));
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
