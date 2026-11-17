//! Datenmodell des Schaltplans: Bauteile auf einem Raster und Drähte dazwischen.
//!
//! Alle Koordinaten sind ganzzahlige Rasterpunkte. Drähte sind immer
//! waagerecht oder senkrecht. Zwei Dinge sind verbunden, wenn sie einen
//! Rasterpunkt teilen oder ein Draht-/Pin-Ende auf einem anderen Draht liegt.

use elab_sim::DiodeModel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Rasterpunkt.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct P {
    pub x: i32,
    pub y: i32,
}

impl P {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl std::ops::Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        P::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        P::new(self.x - o.x, self.y - o.y)
    }
}

/// Dreht einen Punkt um `r` Vierteldrehungen im Uhrzeigersinn (Bildschirm: y nach unten).
pub fn rot_i(p: P, r: u8) -> P {
    match r % 4 {
        0 => p,
        1 => P::new(-p.y, p.x),
        2 => P::new(-p.x, -p.y),
        _ => P::new(p.y, -p.x),
    }
}

/// Wie [`rot_i`] für Gleitkommakoordinaten.
pub fn rot_f(x: f32, y: f32, r: u8) -> (f32, f32) {
    match r % 4 {
        0 => (x, y),
        1 => (-y, x),
        2 => (-x, -y),
        _ => (y, -x),
    }
}

/// Diodentypen mit Modellparametern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiodeType {
    Si1N4148,
    Si1N4007,
    Ideal,
}

impl DiodeType {
    pub const ALL: [DiodeType; 3] = [DiodeType::Si1N4148, DiodeType::Si1N4007, DiodeType::Ideal];

    pub fn label(self) -> &'static str {
        match self {
            DiodeType::Si1N4148 => "1N4148",
            DiodeType::Si1N4007 => "1N4007",
            DiodeType::Ideal => "Lehrbuch",
        }
    }

    pub fn model(self) -> DiodeModel {
        match self {
            DiodeType::Si1N4148 => DiodeModel::SI_1N4148,
            DiodeType::Si1N4007 => DiodeModel::SI_1N4007,
            DiodeType::Ideal => DiodeModel::IDEAL,
        }
    }
}

/// LED-Farben mit typischer Flussspannung bei 20 mA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedColor {
    Rot,
    Gelb,
    Gruen,
    Blau,
    Weiss,
}

impl LedColor {
    pub const ALL: [LedColor; 5] = [
        LedColor::Rot,
        LedColor::Gelb,
        LedColor::Gruen,
        LedColor::Blau,
        LedColor::Weiss,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LedColor::Rot => "rot",
            LedColor::Gelb => "gelb",
            LedColor::Gruen => "grün",
            LedColor::Blau => "blau",
            LedColor::Weiss => "weiß",
        }
    }

    /// Flussspannung bei 20 mA in V.
    pub fn vf(self) -> f64 {
        match self {
            LedColor::Rot => 1.8,
            LedColor::Gelb => 2.0,
            LedColor::Gruen => 2.2,
            LedColor::Blau => 3.0,
            LedColor::Weiss => 3.1,
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        match self {
            LedColor::Rot => [255, 60, 50],
            LedColor::Gelb => [255, 210, 40],
            LedColor::Gruen => [70, 230, 80],
            LedColor::Blau => [60, 120, 255],
            LedColor::Weiss => [240, 240, 255],
        }
    }

    /// Diodenmodell, das bei 20 mA genau die Flussspannung `vf` liefert.
    /// Der Emissionskoeffizient wird so gewählt, dass der Exponent bei 20 mA bei 30 liegt.
    pub fn model(self) -> DiodeModel {
        let n = self.vf() / (30.0 * elab_sim::diode::VT_300K);
        DiodeModel {
            is: 0.02 / (30.0f64.exp() - 1.0),
            n,
        }
    }
}

/// Bauteiltyp mit seinen Parametern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PartKind {
    Resistor { r: f64 },
    Capacitor { c: f64 },
    Inductor { l: f64 },
    Lamp { v_nom: f64, p_nom: f64 },
    DcVoltage { v: f64 },
    AcVoltage { amp: f64, freq: f64, offset: f64 },
    SquareVoltage { low: f64, high: f64, freq: f64 },
    CurrentSource { i: f64 },
    Diode { kind: DiodeType },
    Led { color: LedColor },
    Switch { closed: bool },
    Ground,
    Probe,
}

/// Grobe Einordnung für Palette und Darstellung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Zweipol waagerecht, Pins bei (−2,0) und (2,0)
    TwoPole,
    /// Zweipol senkrecht (Quellen), Pins bei (0,−2) = Plus und (0,2) = Minus
    Source,
    /// Ein Pin bei (0,0)
    OnePin,
}

impl PartKind {
    /// Alle Bauteile der Palette mit Standardwerten.
    pub fn palette() -> Vec<(&'static str, Vec<PartKind>)> {
        vec![
            (
                "Quellen",
                vec![
                    PartKind::DcVoltage { v: 9.0 },
                    PartKind::AcVoltage {
                        amp: 10.0,
                        freq: 50.0,
                        offset: 0.0,
                    },
                    PartKind::SquareVoltage {
                        low: 0.0,
                        high: 5.0,
                        freq: 1000.0,
                    },
                    PartKind::CurrentSource { i: 0.01 },
                ],
            ),
            (
                "Passiv",
                vec![
                    PartKind::Resistor { r: 1000.0 },
                    PartKind::Capacitor { c: 100e-9 },
                    PartKind::Inductor { l: 1e-3 },
                    PartKind::Lamp {
                        v_nom: 12.0,
                        p_nom: 5.0,
                    },
                ],
            ),
            (
                "Halbleiter",
                vec![
                    PartKind::Diode {
                        kind: DiodeType::Si1N4148,
                    },
                    PartKind::Led {
                        color: LedColor::Rot,
                    },
                ],
            ),
            (
                "Schalten & Messen",
                vec![
                    PartKind::Switch { closed: false },
                    PartKind::Ground,
                    PartKind::Probe,
                ],
            ),
        ]
    }

    pub fn shape(&self) -> Shape {
        use PartKind::*;
        match self {
            DcVoltage { .. } | AcVoltage { .. } | SquareVoltage { .. } | CurrentSource { .. } => {
                Shape::Source
            }
            Ground | Probe => Shape::OnePin,
            _ => Shape::TwoPole,
        }
    }

    /// Kennbuchstabe für automatische Namen.
    pub fn prefix(&self) -> &'static str {
        use PartKind::*;
        match self {
            Resistor { .. } => "R",
            Capacitor { .. } => "C",
            Inductor { .. } => "L",
            Lamp { .. } => "H",
            DcVoltage { .. } | AcVoltage { .. } | SquareVoltage { .. } => "V",
            CurrentSource { .. } => "I",
            Diode { .. } => "D",
            Led { .. } => "LED",
            Switch { .. } => "S",
            Ground => "GND",
            Probe => "M",
        }
    }

    /// Deutscher Bauteilname.
    pub fn label(&self) -> &'static str {
        use PartKind::*;
        match self {
            Resistor { .. } => "Widerstand",
            Capacitor { .. } => "Kondensator",
            Inductor { .. } => "Spule",
            Lamp { .. } => "Glühlampe",
            DcVoltage { .. } => "Gleichspannung",
            AcVoltage { .. } => "Wechselspannung",
            SquareVoltage { .. } => "Rechteckspannung",
            CurrentSource { .. } => "Stromquelle",
            Diode { .. } => "Diode",
            Led { .. } => "Leuchtdiode",
            Switch { .. } => "Schalter",
            Ground => "Masse",
            Probe => "Messpunkt",
        }
    }

    /// Pins in lokalen Rasterkoordinaten (vor der Drehung).
    pub fn local_pins(&self) -> &'static [P] {
        const TWO: [P; 2] = [P::new(-2, 0), P::new(2, 0)];
        const SRC: [P; 2] = [P::new(0, -2), P::new(0, 2)];
        const ONE: [P; 1] = [P::new(0, 0)];
        match self.shape() {
            Shape::TwoPole => &TWO,
            Shape::Source => &SRC,
            Shape::OnePin => &ONE,
        }
    }

    /// Bezeichnung der Anschlüsse für Erklärungen.
    pub fn pin_names(&self) -> &'static [&'static str] {
        use PartKind::*;
        match self {
            Diode { .. } | Led { .. } => &["Anode", "Kathode"],
            DcVoltage { .. } | AcVoltage { .. } | SquareVoltage { .. } | CurrentSource { .. } => {
                &["+", "−"]
            }
            Ground | Probe => &["Anschluss"],
            _ => &["1", "2"],
        }
    }

    /// Kurzbeschreibung des Werts, z. B. „4,7 kΩ“.
    pub fn value_text(&self) -> String {
        use elab_sim::fmt_si as si;
        use PartKind::*;
        match self {
            Resistor { r } => si(*r, "Ω"),
            Capacitor { c } => si(*c, "F"),
            Inductor { l } => si(*l, "H"),
            Lamp { v_nom, p_nom } => format!("{} / {}", si(*v_nom, "V"), si(*p_nom, "W")),
            DcVoltage { v } => si(*v, "V"),
            AcVoltage { amp, freq, offset } => {
                if *offset == 0.0 {
                    format!("{} ~ {}", si(*amp, "V"), si(*freq, "Hz"))
                } else {
                    format!(
                        "{} ~ {} (+{})",
                        si(*amp, "V"),
                        si(*freq, "Hz"),
                        si(*offset, "V")
                    )
                }
            }
            SquareVoltage { low, high, freq } => format!(
                "{}…{} {}",
                si(*low, "V").replace(" V", ""),
                si(*high, "V"),
                si(*freq, "Hz")
            ),
            CurrentSource { i } => si(*i, "A"),
            Diode { kind } => kind.label().into(),
            Led { color } => format!("LED {}", color.label()),
            Switch { closed } => (if *closed { "geschlossen" } else { "offen" }).into(),
            Ground | Probe => String::new(),
        }
    }

    /// Lokale Hüllbox (x0, y0, x1, y1) in Rastereinheiten für Treffertests.
    pub fn local_bbox(&self) -> (f32, f32, f32, f32) {
        match self {
            PartKind::Ground => (-0.8, -0.2, 0.8, 1.3),
            PartKind::Probe => (-0.2, -1.6, 2.6, 0.2),
            _ => match self.shape() {
                Shape::TwoPole => (-2.0, -0.9, 2.0, 0.9),
                Shape::Source => (-1.0, -2.0, 1.0, 2.0),
                Shape::OnePin => (-0.5, -0.5, 0.5, 0.5),
            },
        }
    }

    pub fn has_value(&self) -> bool {
        !matches!(self, PartKind::Ground | PartKind::Probe)
    }
}

/// Ein platziertes Bauteil.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: u32,
    pub name: String,
    pub kind: PartKind,
    pub pos: P,
    /// Vierteldrehungen im Uhrzeigersinn
    pub rot: u8,
}

impl Part {
    /// Pins in Rasterkoordinaten.
    pub fn pins(&self) -> Vec<P> {
        self.kind
            .local_pins()
            .iter()
            .map(|&p| self.pos + rot_i(p, self.rot))
            .collect()
    }

    /// Hüllbox in Rasterkoordinaten.
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        let (x0, y0, x1, y1) = self.kind.local_bbox();
        let (ax, ay) = rot_f(x0, y0, self.rot);
        let (bx, by) = rot_f(x1, y1, self.rot);
        let (px, py) = (self.pos.x as f32, self.pos.y as f32);
        (
            px + ax.min(bx),
            py + ay.min(by),
            px + ax.max(bx),
            py + ay.max(by),
        )
    }
}

/// Waagerechter oder senkrechter Draht.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wire {
    pub id: u32,
    pub a: P,
    pub b: P,
}

impl Wire {
    /// Liegt `p` auf dem Draht (Endpunkte eingeschlossen)?
    pub fn contains(&self, p: P) -> bool {
        let (a, b) = (self.a, self.b);
        if a.x == b.x {
            p.x == a.x && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
        } else if a.y == b.y {
            p.y == a.y && p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x)
        } else {
            false
        }
    }

    /// Liegt `p` echt innerhalb des Drahts (ohne Endpunkte)?
    pub fn interior(&self, p: P) -> bool {
        p != self.a && p != self.b && self.contains(p)
    }

    pub fn is_orthogonal(&self) -> bool {
        self.a.x == self.b.x || self.a.y == self.b.y
    }
}

/// Der gesamte Schaltplan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "Document::format_version")]
    pub format: u32,
    #[serde(default)]
    pub title: String,
    pub parts: Vec<Part>,
    pub wires: Vec<Wire>,
    pub next_id: u32,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format: Self::format_version(),
            title: String::new(),
            parts: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
        }
    }
}

impl Document {
    fn format_version() -> u32 {
        1
    }

    fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Nächster freier Name mit diesem Präfix, z. B. „R3“.
    pub fn next_name(&self, prefix: &str) -> String {
        let used: HashSet<&str> = self.parts.iter().map(|p| p.name.as_str()).collect();
        (1..)
            .map(|n| format!("{prefix}{n}"))
            .find(|n| !used.contains(n.as_str()))
            .unwrap()
    }

    /// Platziert ein Bauteil mit automatischem Namen und gibt seine ID zurück.
    pub fn add_part(&mut self, kind: PartKind, pos: P, rot: u8) -> u32 {
        let name = if matches!(kind, PartKind::Ground) {
            "GND".to_string()
        } else {
            self.next_name(kind.prefix())
        };
        let id = self.new_id();
        self.parts.push(Part {
            id,
            name,
            kind,
            pos,
            rot: rot % 4,
        });
        id
    }

    /// Fügt einen Draht hinzu; Nullängen werden ignoriert, Diagonalen als L verlegt.
    pub fn add_wire(&mut self, a: P, b: P) {
        if a == b {
            return;
        }
        if a.x != b.x && a.y != b.y {
            let c = P::new(b.x, a.y);
            self.add_wire(a, c);
            self.add_wire(c, b);
            return;
        }
        let id = self.new_id();
        self.wires.push(Wire { id, a, b });
    }

    pub fn part(&self, id: u32) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn part_mut(&mut self, id: u32) -> Option<&mut Part> {
        self.parts.iter_mut().find(|p| p.id == id)
    }

    pub fn part_by_name(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.name == name)
    }

    /// Entfernt Bauteile und Drähte.
    pub fn remove(&mut self, parts: &HashSet<u32>, wires: &HashSet<u32>) {
        self.parts.retain(|p| !parts.contains(&p.id));
        self.wires.retain(|w| !wires.contains(&w.id));
    }

    /// Alle Pins mit Eigentümer.
    pub fn pins(&self) -> Vec<(P, u32)> {
        self.parts
            .iter()
            .flat_map(|p| p.pins().into_iter().map(move |q| (q, p.id)))
            .collect()
    }

    /// Ist `p` ein Anschlusspunkt (Pin, Drahtende oder auf einem Draht)?
    pub fn is_connection(&self, p: P) -> bool {
        self.parts.iter().any(|q| q.pins().contains(&p)) || self.wires.iter().any(|w| w.contains(p))
    }

    /// Anzahl der Verbindungen an einem Punkt (für Knotenpunkte und offene Enden).
    pub fn degree(&self, p: P) -> usize {
        let pins = self
            .parts
            .iter()
            .flat_map(|q| q.pins())
            .filter(|&q| q == p)
            .count();
        let wires: usize = self
            .wires
            .iter()
            .map(|w| {
                if w.a == p || w.b == p {
                    1
                } else if w.interior(p) {
                    2
                } else {
                    0
                }
            })
            .sum();
        pins + wires
    }

    /// Räumt Drähte auf: Nullängen und Doppelte entfernen, Diagonalen als L verlegen.
    pub fn normalize(&mut self) {
        let mut out: Vec<Wire> = Vec::new();
        let mut seen: HashSet<(P, P)> = HashSet::new();
        let old = std::mem::take(&mut self.wires);
        for w in old {
            let segs = if w.is_orthogonal() {
                vec![(w.a, w.b)]
            } else {
                let c = P::new(w.b.x, w.a.y);
                vec![(w.a, c), (c, w.b)]
            };
            for (k, (a, b)) in segs.into_iter().enumerate() {
                if a == b {
                    continue;
                }
                let key = if a < b { (a, b) } else { (b, a) };
                if !seen.insert(key) {
                    continue;
                }
                let id = if k == 0 { w.id } else { self.new_id() };
                out.push(Wire { id, a, b });
            }
        }
        self.wires = out;
    }

    /// Verändert Bauteile und zieht angeschlossene Drahtenden mit (Gummiband).
    ///
    /// `moved_wires` werden komplett um `wire_delta` verschoben.
    pub fn transform_parts(
        &mut self,
        ids: &HashSet<u32>,
        moved_wires: &HashSet<u32>,
        wire_delta: P,
        f: impl Fn(&mut Part),
    ) {
        let mut map: HashMap<P, P> = HashMap::new();
        for p in self.parts.iter_mut().filter(|p| ids.contains(&p.id)) {
            let before = p.pins();
            f(p);
            for (a, b) in before.into_iter().zip(p.pins()) {
                map.insert(a, b);
            }
        }
        // Pins anderer Bauteile, die an derselben Stelle liegen, bleiben fest:
        // dort wird nicht mitgezogen, sonst würden fremde Verbindungen gerissen.
        let fixed: HashSet<P> = self
            .parts
            .iter()
            .filter(|p| !ids.contains(&p.id))
            .flat_map(|p| p.pins())
            .collect();
        for w in &mut self.wires {
            if moved_wires.contains(&w.id) {
                w.a = w.a + wire_delta;
                w.b = w.b + wire_delta;
                continue;
            }
            for end in [&mut w.a, &mut w.b] {
                if let Some(&to) = map.get(end) {
                    if !fixed.contains(end) {
                        *end = to;
                    }
                }
            }
        }
        // Schräg gewordene Drähte als L verlegen, Knick möglichst nicht durch ein Bauteil
        let diagonals: Vec<usize> = (0..self.wires.len())
            .filter(|&k| !self.wires[k].is_orthogonal())
            .collect();
        for k in diagonals.into_iter().rev() {
            let Wire { id, a, b } = self.wires[k];
            let c = self.best_corner(a, b);
            self.wires[k] = Wire { id, a, b: c };
            let nid = self.new_id();
            self.wires.push(Wire { id: nid, a: c, b });
        }
        self.normalize();
    }

    /// Wählt für eine L-Verbindung von `a` nach `b` den Knick, dessen Schenkel
    /// am wenigsten Bauteilkörper kreuzen.
    pub fn best_corner(&self, a: P, b: P) -> P {
        let hits = |p: P, q: P| -> usize {
            self.parts
                .iter()
                .filter(|part| {
                    let (x0, y0, x1, y1) = part.bbox();
                    let (x0, y0, x1, y1) = (x0 + 0.25, y0 + 0.25, x1 - 0.25, y1 - 0.25);
                    let (lx, hx) = (p.x.min(q.x) as f32, p.x.max(q.x) as f32);
                    let (ly, hy) = (p.y.min(q.y) as f32, p.y.max(q.y) as f32);
                    hx >= x0 && lx <= x1 && hy >= y0 && ly <= y1
                })
                .count()
        };
        let c1 = P::new(b.x, a.y);
        let c2 = P::new(a.x, b.y);
        if hits(a, c2) + hits(c2, b) < hits(a, c1) + hits(c1, b) {
            c2
        } else {
            c1
        }
    }

    /// Hüllbox aller Elemente in Rasterkoordinaten.
    pub fn bbox(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        let mut grow = |x0: f32, y0: f32, x1: f32, y1: f32| {
            b = Some(match b {
                None => (x0, y0, x1, y1),
                Some((a, c, d, e)) => (a.min(x0), c.min(y0), d.max(x1), e.max(y1)),
            })
        };
        for p in &self.parts {
            let (x0, y0, x1, y1) = p.bbox();
            grow(x0, y0, x1, y1);
        }
        for w in &self.wires {
            grow(
                w.a.x.min(w.b.x) as f32,
                w.a.y.min(w.b.y) as f32,
                w.a.x.max(w.b.x) as f32,
                w.a.y.max(w.b.y) as f32,
            );
        }
        b
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let mut d: Document =
            serde_json::from_str(s).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
        let max = d
            .parts
            .iter()
            .map(|p| p.id)
            .chain(d.wires.iter().map(|w| w.id))
            .max()
            .unwrap_or(0);
        d.next_id = d.next_id.max(max + 1);
        Ok(d)
    }
}

/// Undo/Redo über vollständige Schnappschüsse (Schaltpläne sind klein).
#[derive(Default)]
pub struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
}

impl History {
    const LIMIT: usize = 300;

    /// Merkt sich den Zustand *vor* einer Änderung.
    pub fn checkpoint(&mut self, doc: &Document) {
        if self.undo.last() == Some(doc) {
            return;
        }
        self.undo.push(doc.clone());
        if self.undo.len() > Self::LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(doc, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(doc, next));
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_clockwise_on_screen() {
        // (1,0) zeigt nach rechts; eine Vierteldrehung im Uhrzeigersinn zeigt nach unten (y wächst).
        assert_eq!(rot_i(P::new(1, 0), 1), P::new(0, 1));
        assert_eq!(rot_i(P::new(1, 0), 4), P::new(1, 0));
    }

    #[test]
    fn pins_follow_rotation() {
        let mut d = Document::default();
        let id = d.add_part(PartKind::Resistor { r: 1.0 }, P::new(10, 10), 1);
        assert_eq!(
            d.part(id).unwrap().pins(),
            vec![P::new(10, 8), P::new(10, 12)]
        );
    }

    #[test]
    fn automatic_names() {
        let mut d = Document::default();
        d.add_part(PartKind::Resistor { r: 1.0 }, P::new(0, 0), 0);
        d.add_part(PartKind::Resistor { r: 1.0 }, P::new(5, 0), 0);
        d.add_part(PartKind::Capacitor { c: 1.0 }, P::new(9, 0), 0);
        let names: Vec<_> = d.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["R1", "R2", "C1"]);
    }

    #[test]
    fn diagonal_wire_becomes_l() {
        let mut d = Document::default();
        d.add_wire(P::new(0, 0), P::new(3, 4));
        assert_eq!(d.wires.len(), 2);
        assert!(d.wires.iter().all(|w| w.is_orthogonal()));
    }

    #[test]
    fn moving_part_drags_wire_ends() {
        let mut d = Document::default();
        let id = d.add_part(PartKind::Resistor { r: 1.0 }, P::new(0, 0), 0);
        d.add_wire(P::new(2, 0), P::new(6, 0)); // am rechten Pin
        let ids: HashSet<u32> = [id].into();
        d.transform_parts(&ids, &HashSet::new(), P::new(0, 0), |p| {
            p.pos = p.pos + P::new(0, 3)
        });
        // Pin jetzt bei (2,3): Draht wird zu L (2,3)→(6,3)→(6,0)
        assert!(d.is_connection(P::new(2, 3)));
        assert!(d.is_connection(P::new(6, 0)));
        assert!(d.wires.iter().all(|w| w.is_orthogonal()));
    }

    #[test]
    fn rotation_routes_wires_around_part() {
        let mut d = Document::default();
        let id = d.add_part(PartKind::Resistor { r: 1.0 }, P::new(6, -4), 0); // Pins (4,-4) (8,-4)
        d.add_wire(P::new(0, -4), P::new(4, -4));
        let ids: HashSet<u32> = [id].into();
        d.transform_parts(&ids, &HashSet::new(), P::default(), |p| p.rot = 1); // Pins (6,-6) (6,-2)
                                                                               // Der Draht darf nicht durch den Widerstandskörper (x=6, y −6…−2) laufen
        for w in &d.wires {
            let through =
                w.a.x == 6 && w.b.x == 6 && w.a.y.min(w.b.y) < -2 && w.a.y.max(w.b.y) > -6;
            assert!(!through, "{w:?}");
        }
        assert!(d.is_connection(P::new(6, -6)));
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut d = Document::default();
        let mut h = History::default();
        h.checkpoint(&d);
        d.add_part(PartKind::Ground, P::new(0, 0), 0);
        assert!(h.undo(&mut d));
        assert!(d.parts.is_empty());
        assert!(h.redo(&mut d));
        assert_eq!(d.parts.len(), 1);
    }

    #[test]
    fn json_roundtrip() {
        let mut d = Document::default();
        d.add_part(
            PartKind::Led {
                color: LedColor::Blau,
            },
            P::new(1, 2),
            3,
        );
        d.add_wire(P::new(0, 0), P::new(0, 5));
        let back = Document::from_json(&d.to_json()).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn led_models_hit_forward_voltage() {
        for c in LedColor::ALL {
            let m = c.model();
            let v = m.nvt() * (0.02 / m.is + 1.0).ln();
            assert!((v - c.vf()).abs() < 1e-6, "{c:?}: {v}");
        }
    }
}
