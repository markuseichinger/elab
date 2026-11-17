//! Schaltungsbeschreibung: Knoten und Bauteile.

use crate::diode::DiodeModel;
use crate::waveform::Waveform;
use std::collections::HashMap;

/// Knotennummer. `0` ist immer Masse (GND).
pub type NodeId = usize;

/// Masseknoten.
pub const GND: NodeId = 0;

/// Ein Bauteil der Schaltung.
#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub kind: ElementKind,
}

/// Bauteiltypen.
///
/// Stromrichtung: Bei passiven Bauteilen ist der Strom positiv, wenn er im
/// Bauteil von `a` nach `b` fließt (bei der Diode von Anode `a` zur Kathode `k`).
/// Bei Quellen ist der Strom positiv, wenn er am Pluspol `p` aus der Quelle
/// heraus in die Schaltung fließt, die Quelle also Leistung abgibt.
#[derive(Clone, Debug)]
pub enum ElementKind {
    Resistor {
        a: NodeId,
        b: NodeId,
        r: f64,
    },
    /// `ic`: Anfangsspannung für [`TranOptions::use_ic`](crate::TranOptions).
    Capacitor {
        a: NodeId,
        b: NodeId,
        c: f64,
        ic: Option<f64>,
    },
    /// `ic`: Anfangsstrom für [`TranOptions::use_ic`](crate::TranOptions).
    Inductor {
        a: NodeId,
        b: NodeId,
        l: f64,
        ic: Option<f64>,
    },
    VSource {
        p: NodeId,
        n: NodeId,
        wave: Waveform,
    },
    /// Stromquelle: treibt den Strom am Knoten `p` in die Schaltung und nimmt ihn an `n` wieder auf.
    ISource {
        p: NodeId,
        n: NodeId,
        wave: Waveform,
    },
    Diode {
        a: NodeId,
        k: NodeId,
        model: DiodeModel,
    },
    /// Statischer Schalter (Phase 0: nur offen/geschlossen ohne Zeitsteuerung).
    Switch {
        a: NodeId,
        b: NodeId,
        closed: bool,
        r_on: f64,
        r_off: f64,
    },
}

impl ElementKind {
    /// Anschlussknoten des Bauteils.
    pub fn terminals(&self) -> [NodeId; 2] {
        use ElementKind::*;
        match *self {
            Resistor { a, b, .. }
            | Capacitor { a, b, .. }
            | Inductor { a, b, .. }
            | Switch { a, b, .. } => [a, b],
            VSource { p, n, .. } | ISource { p, n, .. } => [p, n],
            Diode { a, k, .. } => [a, k],
        }
    }

    /// Benötigt das Bauteil eine eigene Stromvariable im Gleichungssystem?
    pub(crate) fn needs_branch(&self) -> bool {
        matches!(
            self,
            ElementKind::VSource { .. } | ElementKind::Inductor { .. }
        )
    }

    pub(crate) fn is_nonlinear(&self) -> bool {
        matches!(self, ElementKind::Diode { .. })
    }

    /// Kurzname des Bauteiltyps für Meldungen.
    pub fn type_name(&self) -> &'static str {
        use ElementKind::*;
        match self {
            Resistor { .. } => "Widerstand",
            Capacitor { .. } => "Kondensator",
            Inductor { .. } => "Spule",
            VSource { .. } => "Spannungsquelle",
            ISource { .. } => "Stromquelle",
            Diode { .. } => "Diode",
            Switch { .. } => "Schalter",
        }
    }
}

/// Fehler beim Aufbau oder bei der Simulation einer Schaltung.
#[derive(Clone, Debug, PartialEq)]
pub enum SimError {
    /// Ein Bauteilwert ist ungültig (z. B. negativer Widerstand).
    InvalidValue { element: String, msg: String },
    /// Doppelter Bauteilname.
    DuplicateName(String),
    /// Unbekannter Knoten- oder Bauteilname bei einer Abfrage.
    UnknownName(String),
    /// Knoten ohne jede Verbindung zur Masse.
    FloatingNodes(Vec<String>),
    /// Gleichungssystem nicht lösbar, z. B. Masche aus Spannungsquellen.
    Singular { hint: String },
    /// Newton-Raphson konvergiert nicht.
    NoConvergence { time: Option<f64> },
    /// Zeitschritt unter das Minimum gefallen.
    TimestepTooSmall { time: f64 },
}

impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimError::InvalidValue { element, msg } => write!(f, "{element}: {msg}"),
            SimError::DuplicateName(n) => write!(f, "Der Bauteilname „{n}“ ist doppelt vergeben."),
            SimError::UnknownName(n) => write!(f, "„{n}“ gibt es in der Schaltung nicht."),
            SimError::FloatingNodes(ns) => write!(
                f,
                "Diese Knoten haben keine Verbindung zur Masse: {}. Jeder Schaltungsteil braucht einen Bezugspunkt.",
                ns.join(", ")
            ),
            SimError::Singular { hint } => write!(f, "Die Schaltung ist nicht eindeutig lösbar. {hint}"),
            SimError::NoConvergence { time: None } => {
                write!(f, "Der Arbeitspunkt konnte nicht berechnet werden (keine Konvergenz).")
            }
            SimError::NoConvergence { time: Some(t) } => {
                write!(f, "Die Simulation konvergiert bei t = {} nicht.", crate::fmt_si(*t, "s"))
            }
            SimError::TimestepTooSmall { time } => write!(
                f,
                "Bei t = {} wurde der Zeitschritt zu klein. Die Schaltung ändert sich dort sprunghaft.",
                crate::fmt_si(*time, "s")
            ),
        }
    }
}

impl std::error::Error for SimError {}

/// Eine Schaltung aus benannten Knoten und Bauteilen.
#[derive(Clone, Debug)]
pub struct Circuit {
    node_names: Vec<String>,
    node_map: HashMap<String, NodeId>,
    elements: Vec<Element>,
    elem_map: HashMap<String, usize>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

impl Circuit {
    pub fn new() -> Self {
        let mut node_map = HashMap::new();
        for alias in ["0", "gnd", "GND", "masse"] {
            node_map.insert(alias.to_string(), GND);
        }
        Self {
            node_names: vec!["GND".into()],
            node_map,
            elements: Vec::new(),
            elem_map: HashMap::new(),
        }
    }

    /// Liefert den Knoten mit diesem Namen und legt ihn bei Bedarf an.
    /// `"0"`, `"gnd"`, `"GND"` und `"masse"` bezeichnen die Masse.
    pub fn node(&mut self, name: &str) -> NodeId {
        if let Some(&id) = self.node_map.get(name) {
            return id;
        }
        let id = self.node_names.len();
        self.node_names.push(name.to_string());
        self.node_map.insert(name.to_string(), id);
        id
    }

    pub fn find_node(&self, name: &str) -> Option<NodeId> {
        self.node_map.get(name).copied()
    }

    pub fn node_name(&self, id: NodeId) -> &str {
        &self.node_names[id]
    }

    /// Anzahl Knoten einschließlich Masse.
    pub fn node_count(&self) -> usize {
        self.node_names.len()
    }

    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn find_element(&self, name: &str) -> Option<usize> {
        self.elem_map.get(name).copied()
    }

    /// Fügt ein Bauteil hinzu und prüft dabei seine Werte.
    pub fn add(&mut self, name: &str, kind: ElementKind) -> Result<usize, SimError> {
        if self.elem_map.contains_key(name) {
            return Err(SimError::DuplicateName(name.into()));
        }
        validate(name, &kind)?;
        for t in kind.terminals() {
            assert!(t < self.node_names.len(), "Knoten {t} existiert nicht");
        }
        let idx = self.elements.len();
        self.elements.push(Element {
            name: name.into(),
            kind,
        });
        self.elem_map.insert(name.into(), idx);
        Ok(idx)
    }

    /// Ersetzt die Daten eines vorhandenen Bauteils (z. B. Poti verstellt, Schalter umgelegt).
    pub fn update(&mut self, name: &str, kind: ElementKind) -> Result<(), SimError> {
        let idx = self
            .find_element(name)
            .ok_or_else(|| SimError::UnknownName(name.into()))?;
        validate(name, &kind)?;
        self.elements[idx].kind = kind;
        Ok(())
    }

    // ---- Komfortfunktionen: Knoten per Name -------------------------------

    pub fn resistor(&mut self, name: &str, a: &str, b: &str, r: f64) -> Result<usize, SimError> {
        let (a, b) = (self.node(a), self.node(b));
        self.add(name, ElementKind::Resistor { a, b, r })
    }

    pub fn capacitor(&mut self, name: &str, a: &str, b: &str, c: f64) -> Result<usize, SimError> {
        let (a, b) = (self.node(a), self.node(b));
        self.add(name, ElementKind::Capacitor { a, b, c, ic: None })
    }

    pub fn inductor(&mut self, name: &str, a: &str, b: &str, l: f64) -> Result<usize, SimError> {
        let (a, b) = (self.node(a), self.node(b));
        self.add(name, ElementKind::Inductor { a, b, l, ic: None })
    }

    pub fn vsource(
        &mut self,
        name: &str,
        p: &str,
        n: &str,
        wave: impl Into<Waveform>,
    ) -> Result<usize, SimError> {
        let (p, n) = (self.node(p), self.node(n));
        self.add(
            name,
            ElementKind::VSource {
                p,
                n,
                wave: wave.into(),
            },
        )
    }

    pub fn isource(
        &mut self,
        name: &str,
        p: &str,
        n: &str,
        wave: impl Into<Waveform>,
    ) -> Result<usize, SimError> {
        let (p, n) = (self.node(p), self.node(n));
        self.add(
            name,
            ElementKind::ISource {
                p,
                n,
                wave: wave.into(),
            },
        )
    }

    pub fn diode(
        &mut self,
        name: &str,
        anode: &str,
        kathode: &str,
        model: DiodeModel,
    ) -> Result<usize, SimError> {
        let (a, k) = (self.node(anode), self.node(kathode));
        self.add(name, ElementKind::Diode { a, k, model })
    }

    pub fn switch(
        &mut self,
        name: &str,
        a: &str,
        b: &str,
        closed: bool,
    ) -> Result<usize, SimError> {
        let (a, b) = (self.node(a), self.node(b));
        self.add(
            name,
            ElementKind::Switch {
                a,
                b,
                closed,
                r_on: 1e-3,
                r_off: 1e12,
            },
        )
    }

    /// Prüft, ob jeder Knoten über irgendeinen Pfad mit Masse verbunden ist.
    pub fn check_connectivity(&self) -> Result<(), SimError> {
        let n = self.node_names.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for e in &self.elements {
            let [a, b] = e.kind.terminals();
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        let root = find(&mut parent, GND);
        let used: Vec<bool> = {
            let mut u = vec![false; n];
            for e in &self.elements {
                for t in e.kind.terminals() {
                    u[t] = true;
                }
            }
            u
        };
        let floating: Vec<String> = (1..n)
            .filter(|&i| used[i] && find(&mut parent, i) != root)
            .map(|i| self.node_names[i].clone())
            .collect();
        if floating.is_empty() {
            Ok(())
        } else {
            Err(SimError::FloatingNodes(floating))
        }
    }
}

fn validate(name: &str, kind: &ElementKind) -> Result<(), SimError> {
    use ElementKind::*;
    let bad = |msg: &str| {
        Err(SimError::InvalidValue {
            element: name.into(),
            msg: msg.into(),
        })
    };
    let pos = |v: f64| v.is_finite() && v > 0.0;
    match kind {
        Resistor { r, .. } if !pos(*r) => bad("Der Widerstand muss größer als 0 Ω sein."),
        Capacitor { c, .. } if !pos(*c) => bad("Die Kapazität muss größer als 0 F sein."),
        Inductor { l, .. } if !pos(*l) => bad("Die Induktivität muss größer als 0 H sein."),
        Diode { model, .. } if !pos(model.is) || !pos(model.n) => {
            bad("Is und n der Diode müssen positiv sein.")
        }
        Switch { r_on, r_off, .. } if !pos(*r_on) || !pos(*r_off) => {
            bad("Schalterwiderstände müssen positiv sein.")
        }
        _ => {
            let [a, b] = kind.terminals();
            if a == b && kind.needs_branch() {
                return bad(
                    "Beide Anschlüsse liegen am selben Knoten, das Bauteil ist kurzgeschlossen.",
                );
            }
            Ok(())
        }
    }
}
