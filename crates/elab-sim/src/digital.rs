//! Ereignisgesteuerte Digitalsimulation mit vierwertiger Logik.
//!
//! - **0 / 1**: Logikpegel
//! - **X**: unbestimmt, z. B. bei einem Buskonflikt (zwei Ausgänge treiben
//!   gegeneinander) oder einem Eingang, der nirgends angeschlossen ist
//! - **Z**: hochohmig, kein Treiber am Netz
//!
//! Jede Änderung eines Ausgangs wird als Ereignis mit der Gatterlaufzeit in die
//! Warteschlange gestellt. So entstehen Laufzeiteffekte (Glitches) und
//! Schwingungen (Ringoszillator) wie in echter Hardware. Zeiteinheit: 1 ns.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Not;

/// Logikwert.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Logic {
    L0,
    L1,
    X,
    Z,
}

impl Logic {
    pub fn from_bool(b: bool) -> Self {
        if b {
            Logic::L1
        } else {
            Logic::L0
        }
    }

    /// Wie ein Eingang den Wert liest: Z (offen) wird zu X.
    fn input(self) -> Logic {
        if self == Logic::Z {
            Logic::X
        } else {
            self
        }
    }

    pub fn is_high(self) -> bool {
        self == Logic::L1
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Logic::L0 => "0",
            Logic::L1 => "1",
            Logic::X => "X",
            Logic::Z => "Z",
        }
    }
}

impl std::ops::Not for Logic {
    type Output = Logic;
    fn not(self) -> Logic {
        match self.input() {
            Logic::L0 => Logic::L1,
            Logic::L1 => Logic::L0,
            _ => Logic::X,
        }
    }
}

/// Gattertyp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateKind {
    Buf,
    Not,
    And,
    Or,
    Nand,
    Nor,
    Xor,
    Xnor,
}

impl GateKind {
    pub fn eval(self, ins: &[Logic]) -> Logic {
        let ins: Vec<Logic> = ins.iter().map(|v| v.input()).collect();
        let and = || {
            if ins.contains(&Logic::L0) {
                Logic::L0
            } else if ins.iter().all(|v| *v == Logic::L1) {
                Logic::L1
            } else {
                Logic::X
            }
        };
        let or = || {
            if ins.contains(&Logic::L1) {
                Logic::L1
            } else if ins.iter().all(|v| *v == Logic::L0) {
                Logic::L0
            } else {
                Logic::X
            }
        };
        let xor = || {
            if ins.contains(&Logic::X) {
                Logic::X
            } else {
                Logic::from_bool(ins.iter().filter(|v| **v == Logic::L1).count() % 2 == 1)
            }
        };
        match self {
            GateKind::Buf => ins[0],
            GateKind::Not => ins[0].not(),
            GateKind::And => and(),
            GateKind::Or => or(),
            GateKind::Nand => and().not(),
            GateKind::Nor => or().not(),
            GateKind::Xor => xor(),
            GateKind::Xnor => xor().not(),
        }
    }
}

/// Bausteinverhalten.
#[derive(Clone, Debug, PartialEq)]
pub enum CompKind {
    /// Gatter: Eingänge beliebig viele, ein Ausgang
    Gate(GateKind),
    /// Fester Pegel (z. B. Masse = 0), ein Ausgang
    Const(Logic),
    /// Von außen gesetzter Pegel (Schalter, Taster), ein Ausgang
    Input(Logic),
    /// Taktgeber, ein Ausgang. Halbe Periodendauer in ns.
    Clock { half_period: u64 },
    /// D-Flipflop, positiv flankengesteuert. Eingänge D, C; Ausgänge Q, /Q
    DFlipFlop,
    /// JK-Flipflop, positiv flankengesteuert. Eingänge J, C, K; Ausgänge Q, /Q
    JkFlipFlop,
    /// 4-Bit-Binärzähler. Eingänge C (Takt), R (Reset, asynchron); Ausgänge Q0…Q3
    Counter4,
}

/// Ein Baustein mit seinen Anschlüssen (Netznummern).
#[derive(Clone, Debug)]
pub struct Comp {
    pub kind: CompKind,
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    /// Laufzeit in ns
    pub delay: u64,
}

impl Comp {
    pub fn gate(kind: GateKind, inputs: Vec<usize>, output: usize) -> Self {
        Self { kind: CompKind::Gate(kind), inputs, outputs: vec![output], delay: 10 }
    }
}

#[derive(Clone, Debug, Default)]
struct State {
    last_clk: Option<Logic>,
    q: Option<Logic>,
    count: u8,
    clock_level: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ev {
    /// Ausgang `out` von Baustein `comp` nimmt `val` an
    Out { comp: usize, out: usize, val: Logic },
    /// Takt kippt
    Tick { comp: usize },
}

/// Simulator für ein Digitalnetz.
pub struct DigitalSim {
    comps: Vec<Comp>,
    drivers: Vec<Vec<(usize, usize)>>,
    readers: Vec<Vec<usize>>,
    out_val: Vec<Vec<Logic>>,
    pending: Vec<Vec<Logic>>,
    net: Vec<Logic>,
    state: Vec<State>,
    queue: BinaryHeap<Reverse<(u64, u64, Ev)>>,
    seq: u64,
    /// Aktuelle Simulationszeit in ns
    pub time: u64,
    /// Aufgezeichnete Pegelwechsel je Netz (nur für Netze in `watch`)
    history: Vec<Vec<(u64, Logic)>>,
    watch: Vec<bool>,
    /// Zahl verarbeiteter Ereignisse (gesamt)
    pub events: u64,
    /// Wurde das Ereignisbudget bei `run_until` überschritten (Dauerschwingung)?
    pub overloaded: bool,
}

/// Maximale Ereignisse je `run_until`-Aufruf, schützt vor Endlosschwingungen.
const EVENT_BUDGET: u64 = 400_000;

impl DigitalSim {
    pub fn new(n_nets: usize, comps: Vec<Comp>) -> Self {
        let mut drivers = vec![Vec::new(); n_nets];
        let mut readers = vec![Vec::new(); n_nets];
        for (c, comp) in comps.iter().enumerate() {
            for (o, &n) in comp.outputs.iter().enumerate() {
                drivers[n].push((c, o));
            }
            for &n in &comp.inputs {
                if !readers[n].contains(&c) {
                    readers[n].push(c);
                }
            }
        }
        let out_val: Vec<Vec<Logic>> = comps.iter().map(|c| vec![Logic::L0; c.outputs.len()]).collect();
        let mut sim = Self {
            pending: out_val.clone(),
            out_val,
            net: vec![Logic::Z; n_nets],
            state: vec![State::default(); comps.len()],
            comps,
            drivers,
            readers,
            queue: BinaryHeap::new(),
            seq: 0,
            time: 0,
            history: vec![Vec::new(); n_nets],
            watch: vec![false; n_nets],
            events: 0,
            overloaded: false,
        };
        sim.init();
        sim
    }

    /// Einschwingen bei t = 0 ohne Laufzeiten (Gauß-Seidel): Flipflops starten mit Q = 0,
    /// rückgekoppelte Schaltungen finden so einen stabilen Anfangszustand.
    fn init(&mut self) {
        for n in 0..self.net.len() {
            self.net[n] = self.resolve(n);
        }
        for _ in 0..64 {
            let mut changed = false;
            for c in 0..self.comps.len() {
                let outs = self.evaluate(c, true);
                for (o, v) in outs.into_iter().enumerate() {
                    if self.out_val[c][o] != v {
                        self.out_val[c][o] = v;
                        self.pending[c][o] = v;
                        let n = self.comps[c].outputs[o];
                        let nv = self.resolve(n);
                        if nv != self.net[n] {
                            self.net[n] = nv;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for c in 0..self.comps.len() {
            // Flankenerkennung ab jetzt mit dem eingeschwungenen Taktpegel
            let clk = match self.comps[c].kind {
                CompKind::DFlipFlop | CompKind::Counter4 => Some(self.read(c, if self.comps[c].kind == CompKind::Counter4 { 0 } else { 1 })),
                CompKind::JkFlipFlop => Some(self.read(c, 1)),
                _ => None,
            };
            self.state[c].last_clk = clk;
            if let CompKind::Clock { half_period } = self.comps[c].kind {
                self.push(half_period.max(1), Ev::Tick { comp: c });
            }
        }
        for n in 0..self.net.len() {
            self.history[n].push((0, self.net[n]));
        }
        // Was beim Einschwingen nicht zur Ruhe kam (z. B. Ringoszillator), läuft als Ereignis weiter
        for c in 0..self.comps.len() {
            self.schedule(c);
        }
    }

    fn push(&mut self, t: u64, ev: Ev) {
        self.seq += 1;
        self.queue.push(Reverse((t, self.seq, ev)));
    }

    fn read(&self, c: usize, k: usize) -> Logic {
        self.net[self.comps[c].inputs[k]].input()
    }

    /// Pegel eines Netzes aus allen Treibern: keiner → Z, widersprüchlich → X.
    fn resolve(&self, n: usize) -> Logic {
        let mut v = Logic::Z;
        for &(c, o) in &self.drivers[n] {
            let d = self.out_val[c][o];
            if d == Logic::Z {
                continue;
            }
            v = match v {
                Logic::Z => d,
                cur if cur == d => cur,
                _ => Logic::X,
            };
        }
        v
    }

    /// Neue Ausgangswerte eines Bausteins. `init`: keine Flanken auswerten.
    fn evaluate(&mut self, c: usize, init: bool) -> Vec<Logic> {
        let kind = self.comps[c].kind.clone();
        match kind {
            CompKind::Gate(g) => {
                let ins: Vec<Logic> = (0..self.comps[c].inputs.len()).map(|k| self.read(c, k)).collect();
                vec![g.eval(&ins)]
            }
            CompKind::Const(v) | CompKind::Input(v) => vec![v],
            CompKind::Clock { .. } => vec![Logic::from_bool(self.state[c].clock_level)],
            CompKind::DFlipFlop => {
                let (d, clk) = (self.read(c, 0), self.read(c, 1));
                let mut q = self.state[c].q.unwrap_or(Logic::L0);
                if !init && self.rising(c, clk) {
                    q = d;
                }
                self.state[c].last_clk = Some(clk);
                self.state[c].q = Some(q);
                vec![q, q.not()]
            }
            CompKind::JkFlipFlop => {
                let (j, clk, k) = (self.read(c, 0), self.read(c, 1), self.read(c, 2));
                let mut q = self.state[c].q.unwrap_or(Logic::L0);
                if !init && self.rising(c, clk) {
                    q = match (j, k) {
                        (Logic::L0, Logic::L0) => q,
                        (Logic::L0, Logic::L1) => Logic::L0,
                        (Logic::L1, Logic::L0) => Logic::L1,
                        (Logic::L1, Logic::L1) => q.not(),
                        _ => Logic::X,
                    };
                }
                self.state[c].last_clk = Some(clk);
                self.state[c].q = Some(q);
                vec![q, q.not()]
            }
            CompKind::Counter4 => {
                let (clk, reset) = (self.read(c, 0), self.read(c, 1));
                if reset == Logic::L1 {
                    self.state[c].count = 0;
                } else if !init && self.rising(c, clk) {
                    self.state[c].count = (self.state[c].count + 1) & 15;
                }
                self.state[c].last_clk = Some(clk);
                let n = self.state[c].count;
                (0..4).map(|b| Logic::from_bool(n >> b & 1 == 1)).collect()
            }
        }
    }

    fn rising(&self, c: usize, clk: Logic) -> bool {
        self.state[c].last_clk == Some(Logic::L0) && clk == Logic::L1
    }

    /// Plant die Ausgänge eines Bausteins neu (nach einer Eingangsänderung).
    fn schedule(&mut self, c: usize) {
        let outs = self.evaluate(c, false);
        let t = self.time + self.comps[c].delay;
        for (o, v) in outs.into_iter().enumerate() {
            if self.pending[c][o] != v {
                self.pending[c][o] = v;
                self.push(t, Ev::Out { comp: c, out: o, val: v });
            }
        }
    }

    /// Setzt einen Eingang (Schalter/Taster) auf einen neuen Pegel.
    pub fn set_input(&mut self, comp: usize, v: Logic) {
        if let CompKind::Input(cur) = &mut self.comps[comp].kind {
            if *cur != v {
                *cur = v;
                self.pending[comp][0] = v;
                self.push(self.time, Ev::Out { comp, out: 0, val: v });
            }
        }
    }

    /// Simuliert bis zur Zeit `t_end` (ns). Bei Dauerschwingung bricht sie nach
    /// einem Ereignisbudget ab und setzt `overloaded`.
    pub fn run_until(&mut self, t_end: u64) {
        let mut budget = EVENT_BUDGET;
        self.overloaded = false;
        while let Some(Reverse((t, _, ev))) = self.queue.peek().copied() {
            if t > t_end {
                break;
            }
            if budget == 0 {
                self.overloaded = true;
                return;
            }
            budget -= 1;
            self.queue.pop();
            self.time = t;
            self.events += 1;
            match ev {
                Ev::Tick { comp } => {
                    self.state[comp].clock_level = !self.state[comp].clock_level;
                    let v = Logic::from_bool(self.state[comp].clock_level);
                    self.apply(comp, 0, v);
                    if let CompKind::Clock { half_period } = self.comps[comp].kind {
                        self.push(t + half_period.max(1), Ev::Tick { comp });
                    }
                }
                Ev::Out { comp, out, val } => self.apply(comp, out, val),
            }
        }
        self.time = self.time.max(t_end);
    }

    fn apply(&mut self, c: usize, o: usize, v: Logic) {
        if self.out_val[c][o] == v {
            return;
        }
        self.out_val[c][o] = v;
        self.pending[c][o] = v;
        let n = self.comps[c].outputs[o];
        let nv = self.resolve(n);
        if nv == self.net[n] {
            return;
        }
        self.net[n] = nv;
        self.history[n].push((self.time, nv));
        if !self.watch[n] && self.history[n].len() > 2 {
            // Nur Endstand merken, wenn das Netz nicht beobachtet wird
            let last = *self.history[n].last().unwrap();
            self.history[n].clear();
            self.history[n].push(last);
        }
        for r in self.readers[n].clone() {
            self.schedule(r);
        }
    }

    /// Pegel eines Netzes.
    pub fn net(&self, n: usize) -> Logic {
        self.net.get(n).copied().unwrap_or(Logic::Z)
    }

    pub fn nets(&self) -> &[Logic] {
        &self.net
    }

    /// Pegelwechsel eines Netzes beobachten (für den Logikanalysator).
    pub fn watch(&mut self, n: usize) {
        self.watch[n] = true;
    }

    /// Aufgezeichnete Pegelwechsel eines beobachteten Netzes.
    pub fn history(&self, n: usize) -> &[(u64, Logic)] {
        &self.history[n]
    }

    /// Verwirft Aufzeichnungen vor `t` (den letzten Stand davor behält es).
    pub fn trim_history(&mut self, t: u64) {
        for h in &mut self.history {
            let k = h.partition_point(|e| e.0 < t);
            if k > 1 {
                h.drain(..k - 1);
            }
        }
    }

    /// Netze mit Buskonflikt: mehrere Treiber mit unterschiedlichen Pegeln.
    pub fn conflicts(&self) -> Vec<usize> {
        (0..self.net.len())
            .filter(|&n| {
                let vals: Vec<Logic> =
                    self.drivers[n].iter().map(|&(c, o)| self.out_val[c][o]).filter(|v| *v == Logic::L0 || *v == Logic::L1).collect();
                vals.contains(&Logic::L0) && vals.contains(&Logic::L1)
            })
            .collect()
    }

    /// Ausgangszustand eines Bausteins (z. B. Q eines Flipflops).
    pub fn output(&self, comp: usize, out: usize) -> Logic {
        self.out_val[comp][out]
    }

    /// Simuliert, bis keine Ereignisse mehr anstehen (oder höchstens `max_ns`).
    /// Gibt `false` zurück, wenn die Schaltung nicht zur Ruhe kommt.
    pub fn settle(&mut self, max_ns: u64) -> bool {
        let t_end = self.time + max_ns;
        while let Some(Reverse((t, _, ev))) = self.queue.peek().copied() {
            if t > t_end {
                return false;
            }
            if matches!(ev, Ev::Tick { .. }) && self.queue.iter().all(|e| matches!(e.0 .2, Ev::Tick { .. })) {
                return true; // nur noch Takte: kombinatorisch eingeschwungen
            }
            self.run_until(t);
            if self.overloaded {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Logic::*;

    fn truth(g: GateKind) -> Vec<Logic> {
        [(L0, L0), (L0, L1), (L1, L0), (L1, L1)].iter().map(|&(a, b)| g.eval(&[a, b])).collect()
    }

    #[test]
    fn gate_truth_tables() {
        assert_eq!(truth(GateKind::And), [L0, L0, L0, L1]);
        assert_eq!(truth(GateKind::Or), [L0, L1, L1, L1]);
        assert_eq!(truth(GateKind::Nand), [L1, L1, L1, L0]);
        assert_eq!(truth(GateKind::Nor), [L1, L0, L0, L0]);
        assert_eq!(truth(GateKind::Xor), [L0, L1, L1, L0]);
        assert_eq!(truth(GateKind::Xnor), [L1, L0, L0, L1]);
    }

    #[test]
    fn x_propagation_is_pessimistic_but_correct() {
        // 0 UND X = 0 (dominierender Wert), 1 UND X = X
        assert_eq!(GateKind::And.eval(&[L0, X]), L0);
        assert_eq!(GateKind::And.eval(&[L1, X]), X);
        assert_eq!(GateKind::Or.eval(&[L1, Z]), L1);
        assert_eq!(GateKind::Not.eval(&[Z]), X);
    }

    /// Netze: 0 = A, 1 = B, 2 = Y
    fn and_circuit() -> DigitalSim {
        DigitalSim::new(
            3,
            vec![
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![1], delay: 0 },
                Comp::gate(GateKind::And, vec![0, 1], 2),
            ],
        )
    }

    #[test]
    fn gate_delay_is_respected() {
        let mut s = and_circuit();
        assert_eq!(s.net(2), L0);
        s.set_input(0, L1);
        s.set_input(1, L1);
        s.run_until(5);
        assert_eq!(s.net(2), L0, "nach 5 ns noch alt");
        s.run_until(10);
        assert_eq!(s.net(2), L1, "nach 10 ns Laufzeit neu");
    }

    #[test]
    fn bus_conflict_gives_x() {
        let mut s = DigitalSim::new(
            1,
            vec![
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L1), inputs: vec![], outputs: vec![0], delay: 0 },
            ],
        );
        s.run_until(1);
        assert_eq!(s.net(0), X);
        assert_eq!(s.conflicts(), vec![0]);
    }

    #[test]
    fn floating_input_reads_x() {
        // NICHT-Gatter mit offenem Eingang (Netz 0 ohne Treiber)
        let s = DigitalSim::new(2, vec![Comp::gate(GateKind::Not, vec![0], 1)]);
        assert_eq!(s.net(0), Z);
        assert_eq!(s.net(1), X);
    }

    #[test]
    fn d_flipflop_samples_on_rising_edge() {
        // Netze: 0 = D, 1 = C, 2 = Q, 3 = /Q
        let mut s = DigitalSim::new(
            4,
            vec![
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![1], delay: 0 },
                Comp { kind: CompKind::DFlipFlop, inputs: vec![0, 1], outputs: vec![2, 3], delay: 15 },
            ],
        );
        assert_eq!((s.net(2), s.net(3)), (L0, L1));
        s.set_input(0, L1);
        s.run_until(100);
        assert_eq!(s.net(2), L0, "ohne Flanke keine Übernahme");
        s.set_input(1, L1);
        s.run_until(200);
        assert_eq!((s.net(2), s.net(3)), (L1, L0), "steigende Flanke übernimmt D");
        s.set_input(0, L0);
        s.set_input(1, L0);
        s.run_until(300);
        assert_eq!(s.net(2), L1, "fallende Flanke ändert nichts");
    }

    #[test]
    fn toggle_ff_divides_clock_by_two() {
        // D-Flipflop mit /Q → D: Frequenzteiler. Netze: 0 = Takt, 1 = Q, 2 = /Q
        let mut s = DigitalSim::new(
            3,
            vec![
                Comp { kind: CompKind::Clock { half_period: 500 }, inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::DFlipFlop, inputs: vec![2, 0], outputs: vec![1, 2], delay: 15 },
            ],
        );
        s.watch(1);
        s.run_until(10_000); // 10 Takte à 1000 ns
        let rises = s.history(1).windows(2).filter(|w| w[0].1 == L0 && w[1].1 == L1).count();
        assert_eq!(rises, 5, "halbe Taktfrequenz");
    }

    #[test]
    fn jk_flipflop_modes() {
        // Netze: 0 = J, 1 = C, 2 = K, 3 = Q, 4 = /Q
        let mut s = DigitalSim::new(
            5,
            vec![
                Comp { kind: CompKind::Input(L1), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![1], delay: 0 },
                Comp { kind: CompKind::Input(L1), inputs: vec![], outputs: vec![2], delay: 0 },
                Comp { kind: CompKind::JkFlipFlop, inputs: vec![0, 1, 2], outputs: vec![3, 4], delay: 15 },
            ],
        );
        let clock = |s: &mut DigitalSim| {
            s.set_input(1, L1);
            let t = s.time + 100;
            s.run_until(t);
            s.set_input(1, L0);
            let t = s.time + 100;
            s.run_until(t);
            s.net(3)
        };
        assert_eq!(clock(&mut s), L1, "J=K=1: kippen");
        assert_eq!(clock(&mut s), L0, "J=K=1: kippen");
        s.set_input(2, L0);
        assert_eq!(clock(&mut s), L1, "J=1, K=0: setzen");
        assert_eq!(clock(&mut s), L1, "bleibt gesetzt");
        s.set_input(0, L0);
        s.set_input(2, L1);
        assert_eq!(clock(&mut s), L0, "J=0, K=1: rücksetzen");
    }

    #[test]
    fn counter_counts_and_resets() {
        // Netze: 0 = C, 1 = R, 2..5 = Q0..Q3
        let mut s = DigitalSim::new(
            6,
            vec![
                Comp { kind: CompKind::Clock { half_period: 50 }, inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![1], delay: 0 },
                Comp { kind: CompKind::Counter4, inputs: vec![0, 1], outputs: vec![2, 3, 4, 5], delay: 20 },
            ],
        );
        let value = |s: &DigitalSim| (0..4).map(|b| (s.net(2 + b) == L1) as u8 * (1 << b)).sum::<u8>();
        s.run_until(1000 + 40); // 10 steigende Flanken (bei 50, 150, …, 950)
        assert_eq!(value(&s), 10);
        s.run_until(1700 + 40); // insgesamt 17 → 17 mod 16 = 1
        assert_eq!(value(&s), 1, "Überlauf nach 15");
        s.set_input(1, L1);
        let t = s.time + 30;
        s.run_until(t);
        assert_eq!(value(&s), 0, "Reset");
    }

    #[test]
    fn nor_latch_holds_state() {
        // RS-Flipflop aus zwei NOR: Q = NOR(R, /Q), /Q = NOR(S, Q)
        // Netze: 0 = S, 1 = R, 2 = Q, 3 = /Q
        let mut s = DigitalSim::new(
            4,
            vec![
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![1], delay: 0 },
                Comp::gate(GateKind::Nor, vec![1, 3], 2),
                Comp::gate(GateKind::Nor, vec![0, 2], 3),
            ],
        );
        // Stabiler Anfangszustand, kein X
        assert!(s.net(2) != X && s.net(3) != X && s.net(2) != s.net(3));
        s.set_input(0, L1);
        s.run_until(s.time + 100);
        s.set_input(0, L0);
        s.run_until(s.time + 100);
        assert_eq!((s.net(2), s.net(3)), (L1, L0), "gesetzt und gehalten");
        s.set_input(1, L1);
        s.run_until(s.time + 100);
        s.set_input(1, L0);
        s.run_until(s.time + 100);
        assert_eq!((s.net(2), s.net(3)), (L0, L1), "zurückgesetzt und gehalten");
    }

    #[test]
    fn ring_oscillator_runs_and_is_bounded() {
        // Drei NICHT-Gatter im Ring schwingen mit Periode 2·3·10 ns = 60 ns
        let mut s = DigitalSim::new(
            3,
            vec![Comp::gate(GateKind::Not, vec![0], 1), Comp::gate(GateKind::Not, vec![1], 2), Comp::gate(GateKind::Not, vec![2], 0)],
        );
        s.watch(0);
        s.run_until(600);
        let changes = s.history(0).len();
        assert!((15..=25).contains(&changes), "{changes} Wechsel in 600 ns");
        // Sehr lange Laufzeit: Budget greift statt Endlosschleife
        s.run_until(1_000_000_000);
        assert!(s.overloaded);
    }

    #[test]
    fn glitch_from_unequal_paths() {
        // Y = A UND NICHT A: statisch 0, aber beim Wechsel 0→1 entsteht ein kurzer Impuls
        // Netze: 0 = A, 1 = Ā, 2 = Y
        let mut s = DigitalSim::new(
            3,
            vec![
                Comp { kind: CompKind::Input(L0), inputs: vec![], outputs: vec![0], delay: 0 },
                Comp::gate(GateKind::Not, vec![0], 1),
                Comp::gate(GateKind::And, vec![0, 1], 2),
            ],
        );
        s.watch(2);
        s.set_input(0, L1);
        s.run_until(100);
        assert_eq!(s.net(2), L0);
        let pulses = s.history(2).iter().filter(|e| e.1 == L1).count();
        assert_eq!(pulses, 1, "Glitch von einer Gatterlaufzeit");
    }
}
