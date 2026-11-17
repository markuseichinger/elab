//! Datenmodell des Schaltplans: Bauteile auf einem Raster und Drähte dazwischen.
//!
//! Alle Koordinaten sind ganzzahlige Rasterpunkte. Drähte sind immer
//! waagerecht oder senkrecht. Zwei Dinge sind verbunden, wenn sie einen
//! Rasterpunkt teilen oder ein Draht-/Pin-Ende auf einem anderen Draht liegt.

use elab_sim::DiodeModel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Drahtstück auf einer Linie: (von, bis, Draht-ID).
type Span = (i32, i32, u32);

/// Rasterpunkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
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
    pub const ALL: [LedColor; 5] = [LedColor::Rot, LedColor::Gelb, LedColor::Gruen, LedColor::Blau, LedColor::Weiss];

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
        DiodeModel { is: 0.02 / (30.0f64.exp() - 1.0), n }
    }
}

/// Logikgatter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateType {
    And,
    Or,
    Not,
    Nand,
    Nor,
    Xor,
    Xnor,
}

impl GateType {
    pub const ALL: [GateType; 7] = [GateType::And, GateType::Or, GateType::Not, GateType::Nand, GateType::Nor, GateType::Xor, GateType::Xnor];

    pub fn label(self) -> &'static str {
        match self {
            GateType::And => "UND",
            GateType::Or => "ODER",
            GateType::Not => "NICHT",
            GateType::Nand => "NAND",
            GateType::Nor => "NOR",
            GateType::Xor => "XOR",
            GateType::Xnor => "XNOR",
        }
    }

    /// Kennzeichen im IEC-Rechtecksymbol.
    pub fn symbol(self) -> &'static str {
        match self {
            GateType::And | GateType::Nand => "&",
            GateType::Or | GateType::Nor => "≥1",
            GateType::Not => "1",
            GateType::Xor | GateType::Xnor => "=1",
        }
    }

    /// Negierter Ausgang (Kreis am Ausgang)?
    pub fn inverted(self) -> bool {
        matches!(self, GateType::Not | GateType::Nand | GateType::Nor | GateType::Xnor)
    }

    pub fn inputs(self) -> usize {
        if self == GateType::Not {
            1
        } else {
            2
        }
    }

    pub fn to_sim(self) -> elab_sim::digital::GateKind {
        use elab_sim::digital::GateKind as G;
        match self {
            GateType::And => G::And,
            GateType::Or => G::Or,
            GateType::Not => G::Not,
            GateType::Nand => G::Nand,
            GateType::Nor => G::Nor,
            GateType::Xor => G::Xor,
            GateType::Xnor => G::Xnor,
        }
    }
}

/// Transistortypen mit Modellparametern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransistorType {
    Bc547,
    N2222,
    Bc557,
    N2n7000,
    CmosN,
    Bs250,
    CmosP,
}

/// Grundart eines Transistors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransistorFamily {
    Npn,
    Pnp,
    Nmos,
    Pmos,
}

impl TransistorType {
    pub const ALL: [TransistorType; 7] = [
        TransistorType::Bc547,
        TransistorType::N2222,
        TransistorType::Bc557,
        TransistorType::N2n7000,
        TransistorType::CmosN,
        TransistorType::Bs250,
        TransistorType::CmosP,
    ];

    pub fn family(self) -> TransistorFamily {
        use TransistorType::*;
        match self {
            Bc547 | N2222 => TransistorFamily::Npn,
            Bc557 => TransistorFamily::Pnp,
            N2n7000 | CmosN => TransistorFamily::Nmos,
            Bs250 | CmosP => TransistorFamily::Pmos,
        }
    }

    pub fn label(self) -> &'static str {
        use TransistorType::*;
        match self {
            Bc547 => "BC547",
            N2222 => "2N2222",
            Bc557 => "BC557",
            N2n7000 => "2N7000",
            CmosN => "CMOS-N",
            Bs250 => "BS250",
            CmosP => "CMOS-P",
        }
    }

    pub fn is_bjt(self) -> bool {
        matches!(self.family(), TransistorFamily::Npn | TransistorFamily::Pnp)
    }

    pub fn bjt(self) -> Option<elab_sim::BjtModel> {
        use elab_sim::BjtModel as B;
        match self {
            TransistorType::Bc547 => Some(B::BC547),
            TransistorType::N2222 => Some(B::N2222),
            TransistorType::Bc557 => Some(B::BC557),
            _ => None,
        }
    }

    pub fn mos(self) -> Option<elab_sim::MosModel> {
        use elab_sim::MosModel as M;
        match self {
            TransistorType::N2n7000 => Some(M::N2N7000),
            TransistorType::CmosN => Some(M::CMOS_N),
            TransistorType::Bs250 => Some(M::BS250),
            TransistorType::CmosP => Some(M::CMOS_P),
            _ => None,
        }
    }
}

/// Rolle eines Anschlusses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinRole {
    /// Analog (oder neutral wie Masse und Messpunkt)
    Passive,
    /// Digitaler Eingang
    In,
    /// Digitaler Ausgang
    Out,
}

/// Bauteiltyp mit seinen Parametern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PartKind {
    Resistor {
        r: f64,
    },
    Capacitor {
        c: f64,
    },
    Inductor {
        l: f64,
    },
    Lamp {
        v_nom: f64,
        p_nom: f64,
    },
    DcVoltage {
        v: f64,
    },
    AcVoltage {
        amp: f64,
        freq: f64,
        offset: f64,
    },
    SquareVoltage {
        low: f64,
        high: f64,
        freq: f64,
    },
    CurrentSource {
        i: f64,
    },
    Diode {
        kind: DiodeType,
    },
    Led {
        color: LedColor,
    },
    Switch {
        closed: bool,
    },
    Ground,
    Probe,
    // --- Phase 3 ---
    /// Bipolar- oder Feldeffekttransistor
    Transistor {
        kind: TransistorType,
    },
    /// Potentiometer: Gesamtwiderstand und Stellung 0 … 1 = Anteil des Widerstands zwischen Schleifer und B
    Potentiometer {
        r: f64,
        pos: f64,
    },
    /// Taster (Schließer): leitet, solange gedrückt
    PushButton,
    Voltmeter,
    Ammeter,
    // --- Digital ---
    Gate {
        gate: GateType,
    },
    /// Logikschalter (rastend)
    LogicInput {
        on: bool,
    },
    /// Taster: 1, solange gedrückt
    Button,
    /// Taktgeber
    Clock {
        freq: f64,
    },
    /// Fester Pegel 1
    LogicHigh,
    /// Leuchtanzeige für einen Logikpegel
    LogicLed {
        color: LedColor,
    },
    DFlipFlop,
    JkFlipFlop,
    Counter4,
    SevenSeg,
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
    /// Digitalbaustein: Eingänge links, Ausgänge rechts
    Logic,
    /// Dreipol (Transistor, Potentiometer)
    ThreePole,
}

const fn p(x: i32, y: i32) -> P {
    P::new(x, y)
}

impl PartKind {
    /// Bauteile der Palette mit Standardwerten: (Bereich, Gruppe, Bauteile).
    pub fn palette() -> Vec<(&'static str, Vec<PartKind>)> {
        let mut v = Self::palette_analog();
        v.extend(Self::palette_digital());
        v
    }

    pub fn palette_analog() -> Vec<(&'static str, Vec<PartKind>)> {
        vec![
            (
                "Quellen",
                vec![
                    PartKind::DcVoltage { v: 9.0 },
                    PartKind::AcVoltage { amp: 10.0, freq: 50.0, offset: 0.0 },
                    PartKind::SquareVoltage { low: 0.0, high: 5.0, freq: 1000.0 },
                    PartKind::CurrentSource { i: 0.01 },
                ],
            ),
            (
                "Passiv",
                vec![
                    PartKind::Resistor { r: 1000.0 },
                    PartKind::Potentiometer { r: 10e3, pos: 0.5 },
                    PartKind::Capacitor { c: 100e-9 },
                    PartKind::Inductor { l: 1e-3 },
                    PartKind::Lamp { v_nom: 12.0, p_nom: 5.0 },
                ],
            ),
            (
                "Halbleiter",
                vec![
                    PartKind::Diode { kind: DiodeType::Si1N4148 },
                    PartKind::Led { color: LedColor::Rot },
                    PartKind::Transistor { kind: TransistorType::Bc547 },
                    PartKind::Transistor { kind: TransistorType::Bc557 },
                    PartKind::Transistor { kind: TransistorType::N2n7000 },
                    PartKind::Transistor { kind: TransistorType::Bs250 },
                ],
            ),
            ("Schalten", vec![PartKind::Switch { closed: false }, PartKind::PushButton]),
            ("Messen", vec![PartKind::Ground, PartKind::Probe, PartKind::Voltmeter, PartKind::Ammeter]),
        ]
    }

    pub fn palette_digital() -> Vec<(&'static str, Vec<PartKind>)> {
        vec![
            ("Gatter", GateType::ALL.iter().map(|&g| PartKind::Gate { gate: g }).collect()),
            (
                "Eingänge",
                vec![PartKind::LogicInput { on: false }, PartKind::Button, PartKind::Clock { freq: 1.0 }, PartKind::LogicHigh, PartKind::Ground],
            ),
            ("Ausgänge", vec![PartKind::LogicLed { color: LedColor::Rot }, PartKind::SevenSeg, PartKind::Probe]),
            ("Speicher & Zähler", vec![PartKind::DFlipFlop, PartKind::JkFlipFlop, PartKind::Counter4]),
        ]
    }

    pub fn shape(&self) -> Shape {
        use PartKind::*;
        match self {
            DcVoltage { .. } | AcVoltage { .. } | SquareVoltage { .. } | CurrentSource { .. } => Shape::Source,
            Ground | Probe => Shape::OnePin,
            Transistor { .. } | Potentiometer { .. } => Shape::ThreePole,
            Gate { .. } | LogicInput { .. } | Button | Clock { .. } | LogicHigh | LogicLed { .. } | DFlipFlop | JkFlipFlop | Counter4 | SevenSeg => {
                Shape::Logic
            }
            _ => Shape::TwoPole,
        }
    }

    /// Ist das ein Digitalbaustein?
    pub fn is_digital(&self) -> bool {
        self.shape() == Shape::Logic
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
            Switch { .. } | PushButton => "S",
            Ground => "GND",
            Probe => "M",
            Transistor { .. } => "T",
            Potentiometer { .. } => "P",
            Voltmeter => "VM",
            Ammeter => "AM",
            Gate { gate } => gate.label(),
            LogicInput { .. } | Button => "",
            Clock { .. } => "CLK",
            LogicHigh => "VCC",
            LogicLed { .. } => "Y",
            DFlipFlop | JkFlipFlop => "FF",
            Counter4 => "CTR",
            SevenSeg => "ANZ",
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
            Transistor { kind } => match kind.family() {
                TransistorFamily::Npn => "NPN-Transistor",
                TransistorFamily::Pnp => "PNP-Transistor",
                TransistorFamily::Nmos => "N-Kanal-MOSFET",
                TransistorFamily::Pmos => "P-Kanal-MOSFET",
            },
            Potentiometer { .. } => "Potentiometer",
            PushButton => "Taster",
            Voltmeter => "Voltmeter",
            Ammeter => "Amperemeter",
            Gate { gate } => match gate {
                GateType::And => "UND-Gatter",
                GateType::Or => "ODER-Gatter",
                GateType::Not => "NICHT-Gatter",
                GateType::Nand => "NAND-Gatter",
                GateType::Nor => "NOR-Gatter",
                GateType::Xor => "XOR-Gatter",
                GateType::Xnor => "XNOR-Gatter",
            },
            LogicInput { .. } => "Logikschalter",
            Button => "Logiktaster",
            Clock { .. } => "Taktgeber",
            LogicHigh => "Pegel 1",
            LogicLed { .. } => "Logik-Anzeige",
            DFlipFlop => "D-Flipflop",
            JkFlipFlop => "JK-Flipflop",
            Counter4 => "4-Bit-Zähler",
            SevenSeg => "7-Segment-Anzeige",
        }
    }

    /// Pins in lokalen Rasterkoordinaten (vor der Drehung).
    pub fn local_pins(&self) -> &'static [P] {
        const TWO: [P; 2] = [p(-2, 0), p(2, 0)];
        const SRC: [P; 2] = [p(0, -2), p(0, 2)];
        const ONE: [P; 1] = [p(0, 0)];
        const GATE2: [P; 3] = [p(-3, -1), p(-3, 1), p(3, 0)];
        const GATE1: [P; 2] = [p(-3, 0), p(3, 0)];
        const OUT: [P; 1] = [p(2, 0)];
        const IN: [P; 1] = [p(-2, 0)];
        const DFF: [P; 4] = [p(-3, -2), p(-3, 2), p(3, -2), p(3, 2)];
        const JK: [P; 5] = [p(-3, -2), p(-3, 0), p(-3, 2), p(3, -2), p(3, 2)];
        const CTR: [P; 6] = [p(-3, -2), p(-3, 2), p(3, -3), p(3, -1), p(3, 1), p(3, 3)];
        const SEG: [P; 4] = [p(-3, -3), p(-3, -1), p(-3, 1), p(-3, 3)];
        // Transistoren: Steueranschluss links; Emitter bzw. Source zeigt zur Versorgung
        const T_TOP_C: [P; 3] = [p(-2, 0), p(1, -2), p(1, 2)]; // NPN: B, C oben, E unten · NMOS: G, D oben, S unten
        const T_TOP_E: [P; 3] = [p(-2, 0), p(1, 2), p(1, -2)]; // PNP: B, C unten, E oben · PMOS: G, D unten, S oben
        const POT: [P; 3] = [p(-2, 0), p(0, -2), p(2, 0)];
        use PartKind::*;
        match self {
            Transistor { kind } => match kind.family() {
                TransistorFamily::Npn | TransistorFamily::Nmos => &T_TOP_C,
                TransistorFamily::Pnp | TransistorFamily::Pmos => &T_TOP_E,
            },
            Potentiometer { .. } => &POT,
            Gate { gate } if gate.inputs() == 1 => &GATE1,
            Gate { .. } => &GATE2,
            LogicInput { .. } | Button | Clock { .. } => &OUT,
            LogicHigh => &ONE,
            LogicLed { .. } => &IN,
            DFlipFlop => &DFF,
            JkFlipFlop => &JK,
            Counter4 => &CTR,
            SevenSeg => &SEG,
            _ => match self.shape() {
                Shape::Source => &SRC,
                Shape::OnePin => &ONE,
                _ => &TWO,
            },
        }
    }

    /// Rolle jedes Anschlusses (gleiche Reihenfolge wie [`Self::local_pins`]).
    pub fn pin_roles(&self) -> Vec<PinRole> {
        use PartKind::*;
        use PinRole::*;
        let n = self.local_pins().len();
        match self {
            Gate { .. } => (0..n).map(|k| if k + 1 == n { Out } else { In }).collect(),
            LogicInput { .. } | Button | Clock { .. } | LogicHigh => vec![Out],
            LogicLed { .. } | SevenSeg => vec![In; n],
            DFlipFlop => vec![In, In, Out, Out],
            JkFlipFlop => vec![In, In, In, Out, Out],
            Counter4 => vec![In, In, Out, Out, Out, Out],
            _ => vec![Passive; n],
        }
    }

    /// Bezeichnung der Anschlüsse für Erklärungen.
    pub fn pin_names(&self) -> &'static [&'static str] {
        use PartKind::*;
        match self {
            Diode { .. } | Led { .. } => &["Anode", "Kathode"],
            DcVoltage { .. } | AcVoltage { .. } | SquareVoltage { .. } | CurrentSource { .. } => &["+", "−"],
            Ground | Probe | LogicHigh => &["Anschluss"],
            Transistor { kind } if kind.is_bjt() => &["B (Basis)", "C (Kollektor)", "E (Emitter)"],
            Transistor { .. } => &["G (Gate)", "D (Drain)", "S (Source)"],
            Potentiometer { .. } => &["A", "S (Schleifer)", "B"],
            Voltmeter | Ammeter => &["+", "−"],
            Gate { gate } if gate.inputs() == 1 => &["A", "Y"],
            Gate { .. } => &["A", "B", "Y"],
            LogicInput { .. } | Button | Clock { .. } => &["Ausgang"],
            LogicLed { .. } => &["Eingang"],
            DFlipFlop => &["D", "C", "Q", "/Q"],
            JkFlipFlop => &["J", "C", "K", "Q", "/Q"],
            Counter4 => &["C", "R", "Q0", "Q1", "Q2", "Q3"],
            SevenSeg => &["1", "2", "4", "8"],
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
                    format!("{} ~ {} (+{})", si(*amp, "V"), si(*freq, "Hz"), si(*offset, "V"))
                }
            }
            SquareVoltage { low, high, freq } => format!("{}…{} {}", si(*low, "V").replace(" V", ""), si(*high, "V"), si(*freq, "Hz")),
            CurrentSource { i } => si(*i, "A"),
            Diode { kind } => kind.label().into(),
            Led { color } => format!("LED {}", color.label()),
            Switch { closed } => (if *closed { "ein" } else { "aus" }).into(),
            Clock { freq } => si(*freq, "Hz"),
            Transistor { kind } => kind.label().into(),
            Potentiometer { r, pos } => format!("{} · {:.0} %", si(*r, "Ω"), pos * 100.0),
            _ => String::new(),
        }
    }

    /// Lokale Hüllbox (x0, y0, x1, y1) in Rastereinheiten für Treffertests.
    pub fn local_bbox(&self) -> (f32, f32, f32, f32) {
        use PartKind::*;
        match self {
            Ground => (-0.8, -0.2, 0.8, 1.3),
            Probe => (-0.2, -1.6, 2.6, 0.2),
            Transistor { .. } => (-2.0, -2.0, 1.6, 2.0),
            Potentiometer { .. } => (-2.0, -2.0, 2.0, 0.9),
            Voltmeter | Ammeter => (-2.0, -1.1, 2.0, 1.1),
            Gate { gate } if gate.inputs() == 1 => (-3.0, -1.5, 3.0, 1.5),
            Gate { .. } => (-3.0, -2.0, 3.0, 2.0),
            LogicInput { .. } | Button | Clock { .. } => (-1.8, -1.0, 2.0, 1.0),
            LogicHigh => (-0.8, -1.6, 0.8, 0.2),
            LogicLed { .. } => (-2.0, -1.0, 1.0, 1.0),
            DFlipFlop | JkFlipFlop => (-3.0, -3.2, 3.0, 3.2),
            Counter4 => (-3.0, -4.4, 3.0, 4.4),
            SevenSeg => (-3.0, -4.4, 2.8, 4.4),
            _ => match self.shape() {
                Shape::Source => (-1.0, -2.0, 1.0, 2.0),
                _ => (-2.0, -0.9, 2.0, 0.9),
            },
        }
    }

    /// Anschlusspaar, zwischen dem der Bauteilkörper liegt (für das Einfügen in Leitungen).
    pub fn body_span(&self) -> Option<(usize, usize)> {
        match self.shape() {
            Shape::TwoPole | Shape::Source => Some((0, 1)),
            _ if matches!(self, PartKind::Potentiometer { .. }) => Some((0, 2)),
            Shape::Logic if self.local_pins().len() == 2 => Some((0, 1)),
            _ => None,
        }
    }

    pub fn has_value(&self) -> bool {
        !self.value_text().is_empty()
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
        self.kind.local_pins().iter().map(|&p| self.pos + rot_i(p, self.rot)).collect()
    }

    /// Hüllbox in Rasterkoordinaten.
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        let (x0, y0, x1, y1) = self.kind.local_bbox();
        let (ax, ay) = rot_f(x0, y0, self.rot);
        let (bx, by) = rot_f(x1, y1, self.rot);
        let (px, py) = (self.pos.x as f32, self.pos.y as f32);
        (px + ax.min(bx), py + ay.min(by), px + ax.max(bx), py + ay.max(by))
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
        Self { format: Self::format_version(), title: String::new(), parts: Vec::new(), wires: Vec::new(), next_id: 1 }
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
        (1..).map(|n| format!("{prefix}{n}")).find(|n| !used.contains(n.as_str())).unwrap()
    }

    /// Platziert ein Bauteil mit automatischem Namen und gibt seine ID zurück.
    /// Nächster freier Buchstabe A, B, C … (für Logikeingänge, wie in Wahrheitstabellen).
    pub fn next_letter(&self) -> String {
        let used: HashSet<&str> = self.parts.iter().map(|p| p.name.as_str()).collect();
        ('A'..='Z').map(|c| c.to_string()).find(|n| !used.contains(n.as_str())).unwrap_or_else(|| self.next_name("E"))
    }

    pub fn add_part(&mut self, kind: PartKind, pos: P, rot: u8) -> u32 {
        let name = match kind {
            PartKind::Ground => "GND".to_string(),
            PartKind::LogicInput { .. } | PartKind::Button => self.next_letter(),
            _ => self.next_name(kind.prefix()),
        };
        let id = self.new_id();
        self.parts.push(Part { id, name, kind, pos, rot: rot % 4 });
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
        self.parts.iter().flat_map(|p| p.pins().into_iter().map(move |q| (q, p.id))).collect()
    }

    /// Ist `p` ein Anschlusspunkt (Pin, Drahtende oder auf einem Draht)?
    pub fn is_connection(&self, p: P) -> bool {
        self.parts.iter().any(|q| q.pins().contains(&p)) || self.wires.iter().any(|w| w.contains(p))
    }

    /// Anzahl der Verbindungen an einem Punkt (für Knotenpunkte und offene Enden).
    pub fn degree(&self, p: P) -> usize {
        let pins = self.parts.iter().flat_map(|q| q.pins()).filter(|&q| q == p).count();
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

    /// Räumt Drähte auf:
    /// 1. Diagonalen als L verlegen, Nullängen entfernen
    /// 2. Drähte auf derselben Linie zusammenfassen, wenn sie sich überlappen oder
    ///    an einem Punkt ohne Bauteilanschluss aneinanderstoßen
    /// 3. Leitungsstücke unter dem Körper eines Zweipols entfernen: Ein Bauteil, das auf
    ///    eine Leitung gesetzt wird, ist damit *eingefügt* statt kurzgeschlossen.
    pub fn normalize(&mut self) {
        // 1. orthogonal machen
        let mut segs: Vec<(u32, P, P)> = Vec::new();
        for w in std::mem::take(&mut self.wires) {
            if w.is_orthogonal() {
                segs.push((w.id, w.a, w.b));
            } else {
                let c = P::new(w.b.x, w.a.y);
                segs.push((w.id, w.a, c));
                let id = self.new_id();
                segs.push((id, c, w.b));
            }
        }
        segs.retain(|s| s.1 != s.2);

        // 2. je Linie sortieren und zusammenfassen
        let pins: HashSet<P> = self.parts.iter().flat_map(|p| p.pins()).collect();
        let mut lines: HashMap<(bool, i32), Vec<Span>> = HashMap::new();
        for (id, a, b) in segs {
            if a.y == b.y {
                lines.entry((true, a.y)).or_default().push((a.x.min(b.x), a.x.max(b.x), id));
            } else {
                lines.entry((false, a.x)).or_default().push((a.y.min(b.y), a.y.max(b.y), id));
            }
        }
        let at = |h: bool, c: i32, v: i32| if h { P::new(v, c) } else { P::new(c, v) };
        let mut merged: Vec<(bool, i32, i32, i32, u32)> = Vec::new();
        let mut keys: Vec<_> = lines.keys().copied().collect();
        keys.sort();
        for key in keys {
            let mut iv = lines.remove(&key).unwrap();
            iv.sort();
            let (h, c) = key;
            let mut cur = iv[0];
            for &n in &iv[1..] {
                let touching = n.0 == cur.1 && !pins.contains(&at(h, c, n.0));
                if n.0 < cur.1 || touching {
                    cur.1 = cur.1.max(n.1);
                } else {
                    merged.push((h, c, cur.0, cur.1, cur.2));
                    cur = n;
                }
            }
            merged.push((h, c, cur.0, cur.1, cur.2));
        }

        // 3. unter Zweipolen ausschneiden
        let bodies: Vec<(bool, i32, i32, i32)> = self
            .parts
            .iter()
            .filter_map(|p| {
                let pins = p.pins();
                let (i, j) = p.kind.body_span()?;
                let (a, b) = (pins[i], pins[j]);
                if a.y == b.y {
                    Some((true, a.y, a.x.min(b.x), a.x.max(b.x)))
                } else if a.x == b.x {
                    Some((false, a.x, a.y.min(b.y), a.y.max(b.y)))
                } else {
                    None
                }
            })
            .collect();
        for (h, c, lo, hi) in bodies {
            let mut next = Vec::new();
            for m in merged {
                if m.0 == h && m.1 == c && m.2 < hi && m.3 > lo {
                    if m.2 < lo {
                        next.push((h, c, m.2, lo, m.4));
                    }
                    if m.3 > hi {
                        let id = if m.2 < lo { self.new_id() } else { m.4 };
                        next.push((h, c, hi, m.3, id));
                    }
                } else {
                    next.push(m);
                }
            }
            merged = next;
        }
        self.wires = merged.into_iter().map(|(h, c, lo, hi, id)| Wire { id, a: at(h, c, lo), b: at(h, c, hi) }).collect();
    }

    /// Würde ein Bauteil mit diesen Pins in eine bestehende Leitung eingefügt?
    pub fn would_insert(&self, kind: &PartKind, pins: &[P]) -> bool {
        let Some((i, j)) = kind.body_span() else { return false };
        self.wires.iter().any(|w| w.contains(pins[i]) && w.contains(pins[j]))
    }

    /// Drehung beim Absetzen: Liegt ein Zweipol quer über einer Leitung, wird er
    /// um 90° gedreht, damit er eingefügt wird.
    pub fn snap_rotation(&self, kind: &PartKind, pos: P, rot: u8) -> u8 {
        let pins = |r: u8| kind.local_pins().iter().map(|&q| pos + rot_i(q, r)).collect::<Vec<_>>();
        if !self.would_insert(kind, &pins(rot)) && self.would_insert(kind, &pins(rot + 1)) {
            (rot + 1) % 4
        } else {
            rot
        }
    }

    /// Verändert Bauteile und zieht angeschlossene Drahtenden mit (Gummiband).
    ///
    /// `moved_wires` werden komplett um `wire_delta` verschoben.
    pub fn transform_parts(&mut self, ids: &HashSet<u32>, moved_wires: &HashSet<u32>, wire_delta: P, f: impl Fn(&mut Part)) {
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
        let fixed: HashSet<P> = self.parts.iter().filter(|p| !ids.contains(&p.id)).flat_map(|p| p.pins()).collect();
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
        let diagonals: Vec<usize> = (0..self.wires.len()).filter(|&k| !self.wires[k].is_orthogonal()).collect();
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
            grow(w.a.x.min(w.b.x) as f32, w.a.y.min(w.b.y) as f32, w.a.x.max(w.b.x) as f32, w.a.y.max(w.b.y) as f32);
        }
        b
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let mut d: Document = serde_json::from_str(s).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
        let max = d.parts.iter().map(|p| p.id).chain(d.wires.iter().map(|w| w.id)).max().unwrap_or(0);
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
        assert_eq!(d.part(id).unwrap().pins(), vec![P::new(10, 8), P::new(10, 12)]);
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
        d.transform_parts(&ids, &HashSet::new(), P::new(0, 0), |p| p.pos = p.pos + P::new(0, 3));
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
            let through = w.a.x == 6 && w.b.x == 6 && w.a.y.min(w.b.y) < -2 && w.a.y.max(w.b.y) > -6;
            assert!(!through, "{w:?}");
        }
        assert!(d.is_connection(P::new(6, -6)));
    }

    #[test]
    fn part_dropped_on_wire_is_inserted() {
        let mut d = Document::default();
        d.add_wire(P::new(0, 0), P::new(10, 0));
        d.add_part(PartKind::Resistor { r: 1.0 }, P::new(5, 0), 0); // Pins (3,0) (7,0)
        d.normalize();
        // Kein Draht darf mehr zwischen den Pins verlaufen (sonst Kurzschluss)
        assert!(d.wires.iter().all(|w| !(w.contains(P::new(4, 0)) || w.contains(P::new(6, 0)))));
        assert!(d.is_connection(P::new(0, 0)) && d.is_connection(P::new(10, 0)));
        assert_eq!(d.wires.len(), 2);
        // Elektrisch: zwei Netze, R dazwischen
        let nl = crate::netlist::build(&d);
        let pn = &nl.pin_nets[&d.parts[0].id];
        assert_ne!(pn[0], pn[1]);
    }

    #[test]
    fn vertical_source_inserted_into_vertical_wire() {
        let mut d = Document::default();
        d.add_wire(P::new(2, -5), P::new(2, 5));
        d.add_part(PartKind::DcVoltage { v: 1.0 }, P::new(2, 0), 0);
        d.normalize();
        assert_eq!(d.wires.len(), 2);
        assert!(!d.wires.iter().any(|w| w.contains(P::new(2, 0))));
    }

    #[test]
    fn collinear_wires_are_merged_but_not_across_pins() {
        let mut d = Document::default();
        d.add_wire(P::new(0, 0), P::new(3, 0));
        d.add_wire(P::new(3, 0), P::new(6, 0));
        d.add_wire(P::new(4, 0), P::new(9, 0)); // überlappend
        d.normalize();
        assert_eq!(d.wires.len(), 1);
        assert_eq!((d.wires[0].a, d.wires[0].b), (P::new(0, 0), P::new(9, 0)));
        // An einem Bauteilanschluss bleiben die Stücke getrennt (damit sie beim Verschieben mitgehen)
        let mut e = Document::default();
        e.add_part(PartKind::Ground, P::new(3, 0), 0);
        e.add_wire(P::new(0, 0), P::new(3, 0));
        e.add_wire(P::new(3, 0), P::new(6, 0));
        e.normalize();
        assert_eq!(e.wires.len(), 2);
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
        d.add_part(PartKind::Led { color: LedColor::Blau }, P::new(1, 2), 3);
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
