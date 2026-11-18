//! Mitgelieferte Beispielschaltungen.

use crate::model::{DiodeType, Document, GateType as G, LedColor, LogicFamily as F, OpAmpType, PartKind as K, TransistorType as T, P};

/// Eine Beispielschaltung mit Beschreibung und passender Oszilloskop-Zeitbasis.
pub struct Example {
    pub title: &'static str,
    /// Themengruppe im Menü (siehe [`CATEGORIES`])
    pub category: &'static str,
    pub summary: &'static str,
    pub scope_t: f64,
    pub doc: Document,
    /// Digitalschaltung (Palette und Messgeräte passend umschalten)
    pub digital: bool,
}

/// Themengruppen der Beispiele in Menü-Reihenfolge: (Name, Beschreibung).
pub const CATEGORIES: [(&str, &str); 8] = [
    ("Grundlagen", "Stromkreis, Ohmsches Gesetz, Reihen- und Parallelschaltung"),
    ("Kondensator, Spule, Diode", "Zeitverhalten, Schwingkreis, Gleichrichter"),
    ("Transistoren", "Bipolartransistor und MOSFET als Schalter und Verstärker"),
    ("Logik aus Transistoren", "Wie aus Transistoren Logikgatter werden"),
    ("Logikgatter", "Ideale Gatter, Rechnen mit Bits, eigene ICs"),
    ("Speicher & Zähler", "Flipflops, Takt, Zähler"),
    ("Logik-ICs mit Versorgung", "Echte 74HC/74HCT-Bausteine: Pegel, Schwellen, Hysterese"),
    ("Timer, OpAmp & Wandler", "NE555, Operationsverstärker, ADC und DAC"),
];

impl Example {
    /// Enthält das Beispiel echte Bausteine mit Versorgung?
    pub fn doc_has_ic(&self) -> bool {
        self.doc.parts.iter().any(|p| p.kind.is_ic())
    }
}

struct B(Document);

impl B {
    fn new(title: &str) -> Self {
        B(Document { title: title.into(), ..Default::default() })
    }
    fn part(&mut self, kind: K, x: i32, y: i32, rot: u8) -> &mut Self {
        self.0.add_part(kind, P::new(x, y), rot);
        self
    }
    fn named(&mut self, kind: K, x: i32, y: i32, rot: u8, name: &str) -> &mut Self {
        let id = self.0.add_part(kind, P::new(x, y), rot);
        self.0.part_mut(id).unwrap().name = name.into();
        self
    }
    /// Drahtzug durch mehrere Punkte.
    fn wire(&mut self, pts: &[(i32, i32)]) -> &mut Self {
        for w in pts.windows(2) {
            self.0.add_wire(P::new(w[0].0, w[0].1), P::new(w[1].0, w[1].1));
        }
        self
    }
    fn done(&mut self) -> Document {
        std::mem::take(&mut self.0)
    }
}

/// Innenschaltung des eigenen ICs „Halbaddierer“: S = A ⊕ B, C = A · B.
pub fn half_adder_ic() -> Document {
    B::new("Halbaddierer")
        .named(K::Port, 0, -4, 2, "A")
        .named(K::Port, 0, 4, 2, "B")
        .part(K::Gate { gate: G::Xor }, 8, -3, 0)
        .part(K::Gate { gate: G::And }, 8, 3, 0)
        .named(K::Port, 14, -3, 0, "S")
        .named(K::Port, 14, 3, 0, "C")
        .wire(&[(0, -4), (5, -4)])
        .wire(&[(2, -4), (2, 2), (5, 2)])
        .wire(&[(0, 4), (5, 4)])
        .wire(&[(3, 4), (3, -2), (5, -2)])
        .wire(&[(11, -3), (14, -3)])
        .wire(&[(11, 3), (14, 3)])
        .done()
}

pub fn all() -> Vec<Example> {
    let mut v = vec![
        Example {
            title: "Spannungsteiler",
            category: "Grundlagen",
            summary: "Zwei Widerstände teilen 9 V im Verhältnis 1 : 2. Ändere R2 und beobachte U_aus.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("Spannungsteiler")
                .part(K::DcVoltage { v: 9.0 }, 0, 0, 0)
                .part(K::Resistor { r: 1000.0 }, 8, -2, 1)
                .part(K::Resistor { r: 2000.0 }, 8, 4, 1)
                .part(K::Ground, 0, 8, 0)
                .named(K::Probe, 4, -6, 0, "U_ein")
                .named(K::Probe, 8, 1, 0, "U_aus")
                .wire(&[(0, -2), (0, -6), (8, -6), (8, -4)])
                .wire(&[(8, 0), (8, 2)])
                .wire(&[(8, 6), (8, 8), (0, 8), (0, 2)])
                .done(),
        },
        Example {
            title: "LED mit Vorwiderstand",
            category: "Grundlagen",
            summary: "Der Vorwiderstand begrenzt den Strom auf etwa 15 mA. Öffne den Schalter per Doppelklick.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("LED mit Vorwiderstand")
                .part(K::DcVoltage { v: 5.0 }, 0, 0, 0)
                .part(K::Switch { closed: true }, 4, -6, 0)
                .part(K::Resistor { r: 220.0 }, 10, -6, 0)
                .part(K::Led { color: LedColor::Rot }, 14, -1, 1)
                .part(K::Ground, 7, 6, 0)
                .named(K::Probe, 14, -5, 0, "U_LED")
                .wire(&[(0, -2), (0, -6), (2, -6)])
                .wire(&[(6, -6), (8, -6)])
                .wire(&[(12, -6), (14, -6), (14, -3)])
                .wire(&[(14, 1), (14, 6), (0, 6), (0, 2)])
                .done(),
        },
        Example {
            title: "RC-Tiefpass am Rechteck",
            category: "Kondensator, Spule, Diode",
            summary: "Der Kondensator lädt und entlädt sich mit τ = R·C = 0,1 ms. Im Oszilloskop siehst du die e-Funktion.",
            scope_t: 3e-3,
            digital: false,
            doc: B::new("RC-Tiefpass")
                .part(K::SquareVoltage { low: 0.0, high: 5.0, freq: 1000.0 }, 0, 0, 0)
                .part(K::Resistor { r: 1000.0 }, 6, -6, 0)
                .part(K::Capacitor { c: 100e-9 }, 12, 0, 1)
                .part(K::Ground, 6, 6, 0)
                .named(K::Probe, 0, -4, 0, "Eingang")
                .named(K::Probe, 12, -4, 0, "Ausgang")
                .wire(&[(0, -2), (0, -6), (4, -6)])
                .wire(&[(8, -6), (12, -6), (12, -2)])
                .wire(&[(12, 2), (12, 6), (0, 6), (0, 2)])
                .done(),
        },
        Example {
            title: "Einweggleichrichter",
            category: "Kondensator, Spule, Diode",
            summary: "Die Diode lässt nur die positive Halbwelle durch, der Kondensator glättet. Vergleiche Eingang und Ausgang.",
            scope_t: 0.1,
            digital: false,
            doc: B::new("Einweggleichrichter")
                .part(K::AcVoltage { amp: 10.0, freq: 50.0, offset: 0.0 }, 0, 0, 0)
                .part(K::Diode { kind: DiodeType::Si1N4007 }, 5, -6, 0)
                .part(K::Capacitor { c: 470e-6 }, 10, 0, 1)
                .part(K::Resistor { r: 1000.0 }, 16, 0, 1)
                .part(K::Ground, 5, 6, 0)
                .named(K::Probe, 0, -4, 0, "Eingang")
                .named(K::Probe, 13, -6, 0, "Ausgang")
                .wire(&[(0, -2), (0, -6), (3, -6)])
                .wire(&[(7, -6), (16, -6), (16, -2)])
                .wire(&[(10, -6), (10, -2)])
                .wire(&[(0, 2), (0, 6), (16, 6), (16, 2)])
                .wire(&[(10, 2), (10, 6)])
                .done(),
        },
        Example {
            title: "Reihenschwingkreis",
            category: "Kondensator, Spule, Diode",
            summary: "Bei jedem Sprung schwingt der Kreis mit f₀ = 1/(2π√LC) ≈ 5 kHz und klingt durch R ab.",
            scope_t: 3e-3,
            digital: false,
            doc: B::new("Reihenschwingkreis")
                .part(K::SquareVoltage { low: 0.0, high: 1.0, freq: 500.0 }, 0, 0, 0)
                .part(K::Resistor { r: 10.0 }, 4, -6, 0)
                .part(K::Inductor { l: 1e-3 }, 10, -6, 0)
                .part(K::Capacitor { c: 1e-6 }, 16, 0, 1)
                .part(K::Ground, 8, 6, 0)
                .named(K::Probe, 0, -4, 0, "Eingang")
                .named(K::Probe, 16, -4, 0, "U_C")
                .wire(&[(0, -2), (0, -6), (2, -6)])
                .wire(&[(6, -6), (8, -6)])
                .wire(&[(12, -6), (16, -6), (16, -2)])
                .wire(&[(16, 2), (16, 6), (0, 6), (0, 2)])
                .done(),
        },
        Example {
            title: "Glühlampen parallel",
            category: "Grundlagen",
            summary: "Zwei 12-V-Lampen hängen parallel an der Batterie. Jede bekommt die volle Spannung; Ströme addieren sich.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("Glühlampen parallel")
                .part(K::DcVoltage { v: 12.0 }, 0, 0, 0)
                .part(K::Switch { closed: true }, 4, -6, 0)
                .part(K::Lamp { v_nom: 12.0, p_nom: 5.0 }, 10, 0, 1)
                .part(K::Lamp { v_nom: 12.0, p_nom: 5.0 }, 16, 0, 1)
                .part(K::Ground, 5, 6, 0)
                .wire(&[(0, -2), (0, -6), (2, -6)])
                .wire(&[(6, -6), (16, -6), (16, -2)])
                .wire(&[(10, -6), (10, -2)])
                .wire(&[(0, 2), (0, 6), (16, 6), (16, 2)])
                .wire(&[(10, 2), (10, 6)])
                .done(),
        },
        // ---------------- Transistoren und Messen ----------------
        Example {
            title: "Transistor als Schalter",
            category: "Transistoren",
            summary: "Halte den Taster gedrückt: Ein kleiner Basisstrom (I_B) schaltet den 25-mal größeren LED-Strom (I_C).",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("Transistor als Schalter")
                .part(K::DcVoltage { v: 9.0 }, 0, 0, 0)
                .part(K::Ground, 8, 8, 0)
                .named(K::PushButton, 4, -5, 1, "S1")
                .named(K::Resistor { r: 10e3 }, 4, -1, 1, "R_B")
                .named(K::Ammeter, 8, 6, 0, "I_B")
                .named(K::Resistor { r: 330.0 }, 16, -6, 1, "R_C")
                .part(K::Led { color: LedColor::Rot }, 16, -2, 1)
                .named(K::Ammeter, 16, 2, 1, "I_C")
                .named(K::Transistor { kind: T::Bc547 }, 15, 6, 0, "T1")
                .wire(&[(0, -2), (0, -8), (16, -8)])
                .wire(&[(0, 2), (0, 8), (16, 8)])
                .wire(&[(4, -8), (4, -7)])
                .wire(&[(4, 1), (4, 6), (6, 6)])
                .wire(&[(10, 6), (13, 6)])
                .done(),
        },
        Example {
            title: "CMOS-Inverter: echte Logikpegel",
            category: "Logik aus Transistoren",
            summary: "Ein NICHT-Gatter aus zwei MOSFETs. Stell das Poti von 0 bis 100 %: Wo kippt der Ausgang, wann fließt Querstrom?",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("CMOS-Inverter")
                .part(K::DcVoltage { v: 5.0 }, -4, 0, 0)
                .part(K::Ground, 2, 8, 0)
                .named(K::Potentiometer { r: 10e3, pos: 0.2 }, 6, 0, 1, "P1")
                .named(K::Transistor { kind: T::CmosP }, 14, -3, 0, "TP")
                .named(K::Transistor { kind: T::CmosN }, 14, 3, 0, "TN")
                .named(K::Ammeter, 15, -7, 1, "I_quer")
                .named(K::Voltmeter, 8, 5, 1, "U_ein")
                .named(K::Voltmeter, 23, 4, 1, "U_aus")
                .wire(&[(-4, -2), (-4, -10), (15, -10), (15, -9)])
                .wire(&[(-4, 2), (-4, 8), (23, 8)])
                .wire(&[(6, -10), (6, -2)])
                .wire(&[(6, 2), (6, 8)])
                .wire(&[(8, 0), (10, 0)])
                .wire(&[(10, -3), (10, 3)])
                .wire(&[(10, -3), (12, -3)])
                .wire(&[(10, 3), (12, 3)])
                .wire(&[(15, -1), (15, 1)])
                .wire(&[(15, 5), (15, 8)])
                .wire(&[(8, 0), (8, 3)])
                .wire(&[(8, 7), (8, 8)])
                .wire(&[(15, 0), (23, 0), (23, 2)])
                .wire(&[(23, 6), (23, 8)])
                .done(),
        },
        Example {
            title: "NAND-Gatter aus Transistoren",
            category: "Logik aus Transistoren",
            summary: "Vier MOSFETs bilden ein NAND: Nur wenn A und B auf 5 V liegen, zieht die Reihenschaltung unten den Ausgang auf 0 V. Schalter per Doppelklick.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("NAND aus Transistoren")
                .part(K::DcVoltage { v: 5.0 }, 0, 0, 0)
                .part(K::Ground, 2, 10, 0)
                .named(K::Switch { closed: false }, 4, -7, 1, "A")
                .named(K::Switch { closed: false }, 10, -7, 1, "B")
                .named(K::Resistor { r: 100e3 }, 4, 5, 1, "R_A")
                .named(K::Resistor { r: 100e3 }, 10, 5, 1, "R_B")
                .named(K::Transistor { kind: T::CmosP }, 16, -6, 0, "P1")
                .named(K::Transistor { kind: T::CmosP }, 24, -6, 0, "P2")
                .named(K::Transistor { kind: T::CmosN }, 24, 1, 0, "N1")
                .named(K::Transistor { kind: T::CmosN }, 24, 6, 0, "N2")
                .named(K::Voltmeter, 29, 2, 1, "U_Y")
                .wire(&[(0, -2), (0, -10), (25, -10)])
                .wire(&[(0, 2), (0, 10), (29, 10)])
                .wire(&[(4, -10), (4, -9)])
                .wire(&[(10, -10), (10, -9)])
                .wire(&[(4, -5), (4, 3)])
                .wire(&[(10, -5), (10, 3)])
                .wire(&[(4, 7), (4, 10)])
                .wire(&[(10, 7), (10, 10)])
                .wire(&[(17, -8), (17, -10)])
                .wire(&[(25, -8), (25, -10)])
                .wire(&[(17, -4), (17, -2), (25, -2)])
                .wire(&[(25, -4), (25, -2)])
                .wire(&[(25, -2), (25, -1)])
                .wire(&[(25, 3), (25, 4)])
                .wire(&[(25, 8), (25, 10)])
                .wire(&[(4, -3), (12, -3)])
                .wire(&[(12, -6), (12, 1)])
                .wire(&[(12, -6), (14, -6)])
                .wire(&[(12, 1), (22, 1)])
                .wire(&[(10, 2), (21, 2)])
                .wire(&[(21, -6), (21, 6)])
                .wire(&[(21, -6), (22, -6)])
                .wire(&[(21, 6), (22, 6)])
                .wire(&[(25, -2), (29, -2), (29, 0)])
                .wire(&[(29, 4), (29, 10)])
                .done(),
        },
        Example {
            title: "Emitterschaltung (Verstärker)",
            category: "Transistoren",
            summary: "Ein Transistor verstärkt 0,5 V Wechselspannung etwa 4,6-fach und dreht die Phase um. Verstärkung ≈ R_C / R_E.",
            scope_t: 3e-3,
            digital: false,
            doc: B::new("Emitterschaltung")
                .part(K::DcVoltage { v: 12.0 }, 0, 0, 0)
                .part(K::Ground, 8, 10, 0)
                .named(K::AcVoltage { amp: 0.5, freq: 1000.0, offset: 0.0 }, 4, 4, 0, "Signal")
                .named(K::Capacitor { c: 10e-6 }, 8, -3, 0, "C1")
                .named(K::Resistor { r: 47e3 }, 12, -7, 1, "R1")
                .named(K::Resistor { r: 10e3 }, 12, 5, 1, "R2")
                .named(K::Resistor { r: 4.7e3 }, 17, -6, 1, "R_C")
                .named(K::Resistor { r: 1e3 }, 17, 6, 1, "R_E")
                .named(K::Transistor { kind: T::Bc547 }, 16, 0, 0, "T1")
                .named(K::Probe, 4, 1, 0, "Eingang")
                .named(K::Probe, 17, -3, 0, "Ausgang")
                .wire(&[(0, -2), (0, -10), (17, -10)])
                .wire(&[(0, 2), (0, 10), (17, 10)])
                .wire(&[(4, 6), (4, 10)])
                .wire(&[(4, 2), (4, -3), (6, -3)])
                .wire(&[(10, -3), (12, -3)])
                .wire(&[(12, 0), (14, 0)])
                .wire(&[(12, -10), (12, -9)])
                .wire(&[(12, -5), (12, 3)])
                .wire(&[(12, 7), (12, 10)])
                .wire(&[(17, -10), (17, -8)])
                .wire(&[(17, -4), (17, -2)])
                .wire(&[(17, 2), (17, 4)])
                .wire(&[(17, 8), (17, 10)])
                .done(),
        },
        // ---------------- Digitaltechnik ----------------
        Example {
            title: "Logikgatter UND, ODER, XOR",
            category: "Logikgatter",
            summary: "Zwei Schalter A und B steuern drei Gatter. Klicke auf die Schalter und vergleiche mit der Wahrheitstabelle.",
            scope_t: 10e-3,
            digital: true,
            doc: B::new("Logikgatter")
                .named(K::LogicInput { on: false }, 0, -4, 0, "A")
                .named(K::LogicInput { on: false }, 0, 4, 0, "B")
                .part(K::Gate { gate: G::And }, 12, -8, 0)
                .part(K::Gate { gate: G::Or }, 12, 0, 0)
                .part(K::Gate { gate: G::Xor }, 12, 8, 0)
                .named(K::LogicLed { color: LedColor::Gruen }, 19, -8, 0, "UND")
                .named(K::LogicLed { color: LedColor::Gelb }, 19, 0, 0, "ODER")
                .named(K::LogicLed { color: LedColor::Rot }, 19, 8, 0, "XOR")
                .wire(&[(2, -4), (5, -4)])
                .wire(&[(5, -9), (5, 7)])
                .wire(&[(5, -9), (9, -9)])
                .wire(&[(5, -1), (9, -1)])
                .wire(&[(5, 7), (9, 7)])
                .wire(&[(2, 4), (7, 4)])
                .wire(&[(7, -7), (7, 9)])
                .wire(&[(7, -7), (9, -7)])
                .wire(&[(7, 1), (9, 1)])
                .wire(&[(7, 9), (9, 9)])
                .wire(&[(15, -8), (17, -8)])
                .wire(&[(15, 0), (17, 0)])
                .wire(&[(15, 8), (17, 8)])
                .done(),
        },
        Example {
            title: "Halbaddierer",
            category: "Logikgatter",
            summary: "Addiert zwei Bits: Summe = A XOR B, Übertrag = A UND B. Die Wahrheitstabelle zeigt 1 + 1 = 10 (binär).",
            scope_t: 10e-3,
            digital: true,
            doc: B::new("Halbaddierer")
                .named(K::LogicInput { on: false }, 0, -4, 0, "A")
                .named(K::LogicInput { on: false }, 0, 4, 0, "B")
                .part(K::Gate { gate: G::Xor }, 12, -6, 0)
                .part(K::Gate { gate: G::And }, 12, 4, 0)
                .named(K::LogicLed { color: LedColor::Gruen }, 19, -6, 0, "Summe")
                .named(K::LogicLed { color: LedColor::Rot }, 19, 4, 0, "Übertrag")
                .wire(&[(2, -4), (5, -4)])
                .wire(&[(5, -7), (5, 3)])
                .wire(&[(5, -7), (9, -7)])
                .wire(&[(5, 3), (9, 3)])
                .wire(&[(2, 4), (7, 4)])
                .wire(&[(7, -5), (7, 5)])
                .wire(&[(7, -5), (9, -5)])
                .wire(&[(7, 5), (9, 5)])
                .wire(&[(15, -6), (17, -6)])
                .wire(&[(15, 4), (17, 4)])
                .done(),
        },
        Example {
            title: "RS-Flipflop aus NOR-Gattern",
            category: "Speicher & Zähler",
            summary: "Zwei rückgekoppelte NOR-Gatter speichern ein Bit. Taster S setzt, Taster R setzt zurück, der Zustand bleibt.",
            scope_t: 10e-3,
            digital: true,
            doc: B::new("RS-Flipflop")
                .named(K::Button, 0, -4, 0, "S")
                .named(K::Button, 0, 6, 0, "R")
                .part(K::Gate { gate: G::Nor }, 10, -3, 0)
                .part(K::Gate { gate: G::Nor }, 10, 5, 0)
                .named(K::LogicLed { color: LedColor::Gelb }, 22, -3, 0, "/Q")
                .named(K::LogicLed { color: LedColor::Gruen }, 22, 5, 0, "Q")
                .wire(&[(2, -4), (7, -4)])
                .wire(&[(2, 6), (7, 6)])
                .wire(&[(13, -3), (16, -3), (20, -3)])
                .wire(&[(16, -3), (16, 1), (5, 1), (5, 4), (7, 4)])
                .wire(&[(13, 5), (18, 5), (20, 5)])
                .wire(&[(18, 5), (18, -1), (4, -1), (4, -2), (7, -2)])
                .done(),
        },
        Example {
            title: "Frequenzteiler mit D-Flipflops",
            category: "Speicher & Zähler",
            summary: "Jedes D-Flipflop mit /Q → D halbiert die Frequenz. Im Logikanalysator: Takt, Q1 halb so schnell, Q2 ein Viertel.",
            scope_t: 10e-3,
            digital: true,
            doc: B::new("Frequenzteiler")
                .named(K::Clock { freq: 2.0 }, 0, 0, 0, "Takt")
                .part(K::DFlipFlop, 8, 0, 0)
                .part(K::DFlipFlop, 20, 0, 0)
                .named(K::LogicLed { color: LedColor::Gelb }, 13, -6, 3, "Q1")
                .named(K::LogicLed { color: LedColor::Gruen }, 27, -2, 0, "Q2")
                .wire(&[(2, 0), (3, 0), (3, 2), (5, 2)])
                .wire(&[(11, 2), (13, 2), (13, 5), (4, 5), (4, -2), (5, -2)])
                .wire(&[(11, -2), (15, -2), (15, 2), (17, 2)])
                .wire(&[(13, -4), (13, -2)])
                .wire(&[(23, 2), (25, 2), (25, 5), (16, 5), (16, -2), (17, -2)])
                .wire(&[(23, -2), (25, -2)])
                .done(),
        },
        Example {
            title: "Zähler mit 7-Segment-Anzeige",
            category: "Speicher & Zähler",
            summary: "Ein 4-Bit-Zähler zählt die Takte von 0 bis F, die Anzeige zeigt den Stand hexadezimal. Reset-Taster setzt auf 0.",
            scope_t: 10e-3,
            digital: true,
            doc: B::new("Zähler")
                .named(K::Clock { freq: 2.0 }, 0, -2, 0, "Takt")
                .named(K::Button, 0, 2, 0, "Reset")
                .part(K::Counter4, 8, 0, 0)
                .part(K::SevenSeg, 25, 0, 0)
                .named(K::LogicLed { color: LedColor::Rot }, 12, -7, 3, "Q0")
                .named(K::LogicLed { color: LedColor::Rot }, 15, -7, 3, "Q1")
                .named(K::LogicLed { color: LedColor::Rot }, 18, -7, 3, "Q2")
                .named(K::LogicLed { color: LedColor::Rot }, 21, -7, 3, "Q3")
                .wire(&[(2, -2), (5, -2)])
                .wire(&[(2, 2), (5, 2)])
                .wire(&[(11, -3), (22, -3)])
                .wire(&[(11, -1), (22, -1)])
                .wire(&[(11, 1), (22, 1)])
                .wire(&[(11, 3), (22, 3)])
                .wire(&[(12, -5), (12, -3)])
                .wire(&[(15, -5), (15, -1)])
                .wire(&[(18, -5), (18, 1)])
                .wire(&[(21, -5), (21, 3)])
                .done(),
        },
        Example {
            title: "Volladdierer aus Halbaddierer-ICs",
            category: "Logikgatter",
            summary: "Ein eigenes IC „Halbaddierer“ (XOR und UND) wird zweimal eingesetzt, ein ODER verknüpft die Überträge. \
                      Schalte A, B und C_ein. „Innenschaltung öffnen“ im Inspektor zeigt den Inhalt des ICs.",
            scope_t: 10e-3,
            digital: true,
            doc: {
                let ha = half_adder_ic();
                let ic = || K::Custom { name: "Halbaddierer".into(), def: Box::new(ha.clone()) };
                B::new("Volladdierer")
                    .named(K::LogicInput { on: true }, 1, -5, 0, "A")
                    .named(K::LogicInput { on: false }, 1, -1, 0, "B")
                    .named(K::LogicInput { on: true }, 1, -9, 0, "C_ein")
                    .named(ic(), 10, -2, 0, "HA1")
                    .named(ic(), 20, -4, 0, "HA2")
                    .part(K::Gate { gate: G::Or }, 28, 2, 0)
                    .named(K::LogicLed { color: LedColor::Gruen }, 27, -5, 0, "S")
                    .named(K::LogicLed { color: LedColor::Rot }, 35, 2, 0, "C_aus")
                    .wire(&[(3, -5), (5, -5), (5, -3), (7, -3)])
                    .wire(&[(3, -1), (7, -1)])
                    .wire(&[(3, -9), (16, -9), (16, -5), (17, -5)])
                    .wire(&[(13, -3), (17, -3)])
                    .wire(&[(23, -5), (25, -5)])
                    .wire(&[(23, -3), (24, -3), (24, 1), (25, 1)])
                    .wire(&[(13, -1), (14, -1), (14, 3), (25, 3)])
                    .wire(&[(31, 2), (33, 2)])
                    .done()
            },
        },
        Example {
            title: "Brückengleichrichter mit Glättung",
            category: "Kondensator, Spule, Diode",
            summary: "Vier Dioden machen aus Wechselspannung eine pulsierende Gleichspannung, der Kondensator glättet sie. \
                      Im Oszilloskop: kleine Restwelligkeit am Ausgang. Verkleinere C1 und sieh, wie sie wächst.",
            scope_t: 60e-3,
            digital: false,
            doc: B::new("Brückengleichrichter")
                .part(K::AcVoltage { amp: 12.0, freq: 50.0, offset: 0.0 }, 11, -1, 3)
                .part(K::Diode { kind: DiodeType::Si1N4007 }, 8, -5, 3)
                .part(K::Diode { kind: DiodeType::Si1N4007 }, 8, 3, 3)
                .part(K::Diode { kind: DiodeType::Si1N4007 }, 14, -5, 3)
                .part(K::Diode { kind: DiodeType::Si1N4007 }, 14, 3, 3)
                .named(K::Capacitor { c: 470e-6 }, 20, 0, 1, "C1")
                .named(K::Resistor { r: 1e3 }, 25, 0, 1, "R_Last")
                .part(K::Ground, 17, 8, 0)
                .named(K::Probe, 25, -8, 0, "U_aus")
                .wire(&[(8, -3), (8, 1)])
                .wire(&[(14, -3), (14, 1)])
                .wire(&[(9, -1), (8, -1)])
                .wire(&[(13, -1), (14, -1)])
                .wire(&[(8, -7), (8, -8), (25, -8), (25, -2)])
                .wire(&[(14, -7), (14, -8)])
                .wire(&[(8, 5), (8, 8), (25, 8), (25, 2)])
                .wire(&[(14, 5), (14, 8)])
                .wire(&[(20, -8), (20, -2)])
                .wire(&[(20, 2), (20, 8)])
                .done(),
        },
        Example {
            title: "MOSFET schaltet eine Lampe",
            category: "Transistoren",
            summary: "Der 2N7000 schaltet eine 12-V-Lampe. Das Gate braucht nur Spannung, keinen Strom. Der Pull-down R_PD \
                      hält es aus, solange der Taster offen ist. Halte S1 gedrückt.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("MOSFET schaltet eine Lampe")
                .part(K::DcVoltage { v: 12.0 }, 0, 0, 0)
                .named(K::PushButton, 6, -5, 1, "S1")
                .named(K::Resistor { r: 1e3 }, 6, 0, 1, "R_G")
                .named(K::Resistor { r: 100e3 }, 10, 5, 1, "R_PD")
                .part(K::Lamp { v_nom: 12.0, p_nom: 1.2 }, 16, -4, 1)
                .named(K::Transistor { kind: T::N2n7000 }, 15, 2, 0, "T1")
                .part(K::Ground, 4, 8, 0)
                .named(K::Probe, 12, 2, 0, "U_GS")
                .wire(&[(0, -2), (0, -8), (16, -8), (16, -6)])
                .wire(&[(6, -8), (6, -7)])
                .wire(&[(6, -3), (6, -2)])
                .wire(&[(6, 2), (13, 2)])
                .wire(&[(10, 2), (10, 3)])
                .wire(&[(10, 7), (10, 8)])
                .wire(&[(16, -2), (16, 0)])
                .wire(&[(16, 4), (16, 8)])
                .wire(&[(0, 2), (0, 8), (16, 8)])
                .done(),
        },
        // ---------------- Echte Bausteine (Mixed-Signal) ----------------
        Example {
            title: "NAND-Baustein (74HC00) mit Tastern",
            category: "Logik-ICs mit Versorgung",
            summary: "Ein echter Logik-IC an 5 V. Taster mit Pull-down-Widerständen liefern die Eingangsspannungen, \
                      die LED zeigt den Ausgang. Halte beide Taster gedrückt: Erst dann geht die LED aus.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("NAND-Baustein")
                .part(K::DcVoltage { v: 5.0 }, 0, 0, 0)
                .named(K::PushButton, 6, -6, 1, "S_A")
                .named(K::PushButton, 12, -6, 1, "S_B")
                .part(K::Resistor { r: 10e3 }, 6, 5, 1)
                .part(K::Resistor { r: 10e3 }, 12, 5, 1)
                .named(K::GateIc { gate: G::Nand, family: F::Hc }, 22, 0, 0, "U1")
                .part(K::Resistor { r: 470.0 }, 31, 0, 0)
                .part(K::Led { color: LedColor::Rot }, 33, 5, 1)
                .part(K::Ground, 16, 10, 0)
                .named(K::Probe, 9, -1, 0, "A")
                .named(K::Probe, 15, 1, 0, "B")
                .named(K::Probe, 25, 0, 0, "Y")
                .wire(&[(0, -2), (0, -10), (22, -10), (22, -3)])
                .wire(&[(6, -10), (6, -8)])
                .wire(&[(12, -10), (12, -8)])
                .wire(&[(6, -4), (6, 3)])
                .wire(&[(12, -4), (12, 3)])
                .wire(&[(6, -1), (19, -1)])
                .wire(&[(12, 1), (19, 1)])
                .wire(&[(25, 0), (29, 0)])
                .wire(&[(33, 0), (33, 3)])
                .wire(&[(0, 2), (0, 10), (33, 10), (33, 7)])
                .wire(&[(6, 7), (6, 10)])
                .wire(&[(12, 7), (12, 10)])
                .wire(&[(22, 3), (22, 10)])
                .done(),
        },
        Example {
            title: "3,3 V trifft 5 V: 74HC und 74HCT",
            category: "Logik-ICs mit Versorgung",
            summary: "Ein 3,3-V-Signal (wie von einem ESP32) an zwei UND-Bausteinen mit 5 V. Der 74HC braucht für eine 1 \
                      mindestens 3,5 V und sieht X, der 74HCT erkennt schon 2 V als 1. Öffne den Schalter per Doppelklick.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("3,3 V trifft 5 V")
                .named(K::DcVoltage { v: 5.0 }, 0, 0, 0, "V_5V")
                .named(K::DcVoltage { v: 3.3 }, 8, 0, 0, "V_3V3")
                .part(K::Switch { closed: true }, 12, -4, 0)
                .part(K::Resistor { r: 10e3 }, 18, 10, 1)
                .named(K::GateIc { gate: G::And, family: F::Hc }, 24, 0, 0, "U1")
                .named(K::GateIc { gate: G::And, family: F::Hct }, 40, 0, 0, "U2")
                .part(K::Resistor { r: 470.0 }, 29, 0, 0)
                .part(K::Led { color: LedColor::Gruen }, 31, 2, 1)
                .part(K::Resistor { r: 470.0 }, 45, 0, 0)
                .part(K::Led { color: LedColor::Gruen }, 47, 2, 1)
                .part(K::Ground, 0, 3, 0)
                .part(K::Ground, 8, 3, 0)
                .part(K::Ground, 18, 12, 0)
                .part(K::Ground, 24, 4, 0)
                .part(K::Ground, 31, 5, 0)
                .part(K::Ground, 40, 4, 0)
                .part(K::Ground, 47, 5, 0)
                .named(K::Probe, 18, 2, 0, "Signal")
                .named(K::Probe, 27, 0, 0, "Y_HC")
                .named(K::Probe, 43, 0, 0, "Y_HCT")
                .wire(&[(0, -2), (0, -10), (40, -10), (40, -3)])
                .wire(&[(24, -10), (24, -3)])
                .wire(&[(0, 2), (0, 3)])
                .wire(&[(8, 2), (8, 3)])
                .wire(&[(8, -2), (8, -4), (10, -4)])
                .wire(&[(14, -4), (18, -4), (18, 8), (37, 8), (37, 1)])
                .wire(&[(21, 8), (21, 1)])
                .wire(&[(21, -1), (21, 1)])
                .wire(&[(37, -1), (37, 1)])
                .wire(&[(24, 3), (24, 4)])
                .wire(&[(40, 3), (40, 4)])
                .wire(&[(31, 4), (31, 5)])
                .wire(&[(47, 4), (47, 5)])
                .done(),
        },
        Example {
            title: "Schmitt-Trigger-Oszillator (74HC14)",
            category: "Logik-ICs mit Versorgung",
            summary: "Der Kondensator lädt über R1 bis zur oberen Schaltschwelle, dann kippt der Ausgang und entlädt ihn bis zur \
                      unteren. Die LED blinkt etwa 2,6-mal pro Sekunde. Der Messpunkt U_C zeigt die Hysterese.",
            scope_t: 2.0,
            digital: false,
            doc: B::new("Schmitt-Trigger-Oszillator")
                .part(K::DcVoltage { v: 5.0 }, 0, 0, 0)
                .named(K::SchmittIc { family: F::Hc }, 14, 0, 0, "U1")
                .part(K::Resistor { r: 47e3 }, 14, 9, 0)
                .part(K::Capacitor { c: 10e-6 }, 5, 3, 1)
                .part(K::Resistor { r: 1e3 }, 25, 0, 0)
                .part(K::Led { color: LedColor::Rot }, 29, 2, 1)
                .part(K::Ground, 0, 3, 0)
                .part(K::Ground, 5, 6, 0)
                .part(K::Ground, 14, 4, 0)
                .part(K::Ground, 29, 5, 0)
                .named(K::Probe, 7, 0, 0, "U_C")
                .named(K::Probe, 18, 0, 0, "U_aus")
                .wire(&[(0, -2), (0, -6), (14, -6), (14, -3)])
                .wire(&[(0, 2), (0, 3)])
                .wire(&[(5, 0), (11, 0)])
                .wire(&[(5, 0), (5, 1)])
                .wire(&[(5, 5), (5, 6)])
                .wire(&[(14, 3), (14, 4)])
                .wire(&[(17, 0), (23, 0)])
                .wire(&[(20, 0), (20, 9), (16, 9)])
                .wire(&[(12, 9), (8, 9), (8, 0)])
                .wire(&[(27, 0), (29, 0)])
                .wire(&[(29, 4), (29, 5)])
                .done(),
        },
        Example {
            title: "555-Blinker",
            category: "Timer, OpAmp & Wandler",
            summary: "Der Timer NE555 lädt C1 über R_A und R_B bis ⅔ der Versorgung und entlädt ihn über R_B bis ⅓. \
                      Die LED hängt zwischen +9 V und dem Ausgang: Sie leuchtet, während der Ausgang low ist (etwa 1,4 Hz).",
            scope_t: 3.0,
            digital: false,
            doc: B::new("555-Blinker")
                .part(K::DcVoltage { v: 9.0 }, 0, 0, 0)
                .named(K::Timer555, 20, 0, 0, "U1")
                .named(K::Resistor { r: 10e3 }, 30, -4, 1, "R_A")
                .named(K::Resistor { r: 47e3 }, 30, 5, 1, "R_B")
                .named(K::Capacitor { c: 10e-6 }, 10, 5, 1, "C1")
                .part(K::Resistor { r: 1e3 }, 26, -4, 1)
                .part(K::Led { color: LedColor::Rot }, 26, -8, 1)
                .part(K::Ground, 0, 3, 0)
                .part(K::Ground, 10, 8, 0)
                .part(K::Ground, 20, 6, 0)
                .named(K::Probe, 13, 5, 0, "U_C")
                .named(K::Probe, 25, -1, 0, "OUT")
                .wire(&[(0, -2), (0, -12), (30, -12), (30, -6)])
                .wire(&[(19, -5), (19, -12)])
                .wire(&[(21, -5), (21, -12)])
                .wire(&[(26, -12), (26, -10)])
                .wire(&[(24, -1), (26, -1), (26, -2)])
                .wire(&[(30, -2), (30, 3)])
                .wire(&[(24, 1), (30, 1)])
                .wire(&[(30, 7), (30, 11), (13, 11), (13, -2), (16, -2)])
                .wire(&[(13, 0), (16, 0)])
                .wire(&[(10, 3), (13, 3)])
                .wire(&[(10, 7), (10, 8)])
                .wire(&[(20, 5), (20, 6)])
                .wire(&[(0, 2), (0, 3)])
                .done(),
        },
        Example {
            title: "OpAmp-Verstärker (LM358)",
            category: "Timer, OpAmp & Wandler",
            summary: "Nichtinvertierender Verstärker mit V = 1 + R2/R1 = 3 an einfacher 12-V-Versorgung. Dreh das Poti hoch: \
                      Ab etwa 3,5 V am Eingang bleibt der Ausgang bei 10,5 V stehen, der LM358 ist übersteuert.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("OpAmp-Verstärker")
                .part(K::DcVoltage { v: 12.0 }, -8, 0, 0)
                .named(K::Potentiometer { r: 10e3, pos: 0.25 }, -2, 8, 1, "P1")
                .named(K::OpAmp { kind: OpAmpType::Lm358 }, 16, 2, 0, "U1")
                .named(K::Resistor { r: 10e3 }, 8, 3, 1, "R1")
                .named(K::Resistor { r: 20e3 }, 13, -4, 0, "R2")
                .named(K::Voltmeter, 4, 11, 1, "U_ein")
                .named(K::Voltmeter, 24, 5, 1, "U_aus")
                .part(K::Ground, -8, 3, 0)
                .part(K::Ground, -2, 11, 0)
                .part(K::Ground, 8, 6, 0)
                .part(K::Ground, 16, 6, 0)
                .part(K::Ground, 4, 14, 0)
                .part(K::Ground, 24, 8, 0)
                .wire(&[(-8, -2), (-8, -12), (16, -12), (16, -1)])
                .wire(&[(-2, 6), (-2, -12)])
                .wire(&[(-8, 2), (-8, 3)])
                .wire(&[(-2, 10), (-2, 11)])
                .wire(&[(0, 8), (11, 8), (11, 3), (13, 3)])
                .wire(&[(13, 1), (8, 1)])
                .wire(&[(8, 5), (8, 6)])
                .wire(&[(10, 1), (10, -4), (11, -4)])
                .wire(&[(15, -4), (21, -4), (21, 2), (19, 2)])
                .wire(&[(21, 2), (24, 2), (24, 3)])
                .wire(&[(16, 5), (16, 6)])
                .wire(&[(4, 8), (4, 9)])
                .wire(&[(4, 13), (4, 14)])
                .wire(&[(24, 7), (24, 8)])
                .done(),
        },
        Example {
            title: "ADC und DAC (4 Bit)",
            category: "Timer, OpAmp & Wandler",
            summary: "Der A/D-Umsetzer macht aus der Poti-Spannung eine 4-Bit-Zahl, der D/A-Umsetzer daraus wieder eine Spannung. \
                      Dreh das Poti: Die Bits zählen binär, U_DAC springt in Stufen von 5 V / 16 = 0,31 V.",
            scope_t: 10e-3,
            digital: false,
            doc: B::new("ADC und DAC")
                .part(K::DcVoltage { v: 5.0 }, -6, 0, 0)
                .named(K::Potentiometer { r: 10e3, pos: 0.4 }, 1, 0, 1, "P1")
                .named(K::Adc4, 14, 0, 0, "U1")
                .named(K::Dac4, 26, 0, 0, "U2")
                .named(K::Voltmeter, 6, 4, 1, "U_ein")
                .named(K::Voltmeter, 34, 3, 1, "U_DAC")
                .part(K::Ground, -6, 3, 0)
                .part(K::Ground, 1, 3, 0)
                .part(K::Ground, 6, 7, 0)
                .part(K::Ground, 14, 7, 0)
                .part(K::Ground, 26, 7, 0)
                .part(K::Ground, 34, 6, 0)
                .wire(&[(-6, -2), (-6, -9), (27, -9), (27, -6)])
                .wire(&[(1, -2), (1, -9)])
                .wire(&[(13, -6), (13, -9)])
                .wire(&[(15, -6), (15, -9)])
                .wire(&[(25, -6), (25, -9)])
                .wire(&[(-6, 2), (-6, 3)])
                .wire(&[(1, 2), (1, 3)])
                .wire(&[(3, 0), (11, 0)])
                .wire(&[(6, 0), (6, 2)])
                .wire(&[(6, 6), (6, 7)])
                .wire(&[(17, -3), (23, -3)])
                .wire(&[(17, -1), (23, -1)])
                .wire(&[(17, 1), (23, 1)])
                .wire(&[(17, 3), (23, 3)])
                .wire(&[(14, 6), (14, 7)])
                .wire(&[(26, 6), (26, 7)])
                .wire(&[(29, 0), (34, 0), (34, 1)])
                .wire(&[(34, 5), (34, 6)])
                .done(),
        },
    ];
    // nach Themen ordnen (stabil: innerhalb eines Themas bleibt die Reihenfolge)
    v.sort_by_key(|e| CATEGORIES.iter().position(|c| c.0 == e.category).unwrap_or(usize::MAX));
    v
}

#[cfg(test)]
mod tests {
    use crate::{netlist, sim};

    #[test]
    fn all_examples_simulate_without_warnings() {
        for ex in super::all() {
            let nl = netlist::build(&ex.doc);
            assert!(nl.error.is_none(), "{}: {:?}", ex.title, nl.error);
            assert!(nl.excluded.is_empty(), "{}: nicht verbunden: {:?}", ex.title, nl.excluded);
            let op = if nl.mixed.is_some() {
                let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
                ms.advance(0.05, None).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
                sim::mixed_view(&nl, &ms)
            } else {
                sim::operating_point(&nl).unwrap_or_else(|e| panic!("{}: {e}", ex.title))
            };
            let has_dc = ex.doc.parts.iter().any(|p| matches!(p.kind, crate::model::PartKind::DcVoltage { .. }));
            assert!(!has_dc || op.max_abs_v() > 0.0, "{}", ex.title);
            let scope = sim::record(&ex.doc, &nl, ex.scope_t).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            assert_eq!(scope.traces.len(), sim::probes(&ex.doc).len(), "{}", ex.title);
        }
    }

    /// Echtzeitbetrieb mit unregelmäßigen Bildwechseln (wie in der App): Die Zeit muss
    /// immer weiterlaufen, kein Schritt darf hängen bleiben.
    #[test]
    fn realtime_examples_advance_with_irregular_frames() {
        use std::time::{Duration, Instant};
        for ex in super::all() {
            let nl = netlist::build(&ex.doc);
            if !sim::realtime(&nl) || nl.error.is_some() {
                continue;
            }
            let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            let mut seed: u64 = 12345;
            let start = Instant::now();
            for _ in 0..300 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let dt = 0.001 + (seed >> 33) as f64 / (1u64 << 31) as f64 * 0.05;
                let t0 = ms.time();
                ms.advance(t0 + dt, Some(Instant::now() + Duration::from_millis(50))).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
                assert!(ms.time() > t0, "{}: Zeit steht bei {t0}", ex.title);
            }
            assert!(start.elapsed() < Duration::from_secs(60), "{}: zu langsam", ex.title);
        }
    }

    /// Kein Beispiel darf von selbst Bauteile zerstören, auch nicht mit gedrückten Tastern.
    #[test]
    fn examples_stay_within_limits() {
        use crate::limits::Thermal;
        use crate::model::PartKind;
        use std::collections::HashSet;
        for ex in super::all() {
            let buttons: HashSet<u32> = ex.doc.parts.iter().filter(|p| matches!(p.kind, PartKind::PushButton)).map(|p| p.id).collect();
            for pressed in [HashSet::new(), buttons] {
                let nl = netlist::build_with(&ex.doc, &pressed);
                if nl.error.is_some() {
                    continue;
                }
                let mut th = Thermal::default();
                if sim::realtime(&nl) {
                    let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
                    for k in 1..=120 {
                        ms.run(k as f64 / 60.0).unwrap();
                        let burned = th.update(&ex.doc, &sim::mixed_view(&nl, &ms), Some(ms.time()));
                        assert!(burned.is_empty(), "{}: {burned:?}", ex.title);
                    }
                } else if let Ok(v) = sim::operating_point(&nl) {
                    let burned = th.update(&ex.doc, &v, None);
                    assert!(burned.is_empty(), "{}: {burned:?}", ex.title);
                }
            }
        }
    }

    #[test]
    fn mixed_examples_behave() {
        use crate::model::PartKind;
        use elab_sim::digital::Logic;
        use std::collections::HashSet;
        let all = super::all();
        let find = |t: &str| all.iter().find(|e| e.title.starts_with(t)).unwrap();
        let id = |doc: &crate::model::Document, name: &str| doc.part_by_name(name).unwrap().id;
        let start = |doc: &crate::model::Document, pressed: &[&str]| {
            let pressed: HashSet<u32> = pressed.iter().map(|n| id(doc, n)).collect();
            let nl = netlist::build_with(doc, &pressed);
            assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?} {:?}", nl.error, nl.excluded);
            let ms = sim::mixed_sim(&nl, 1e-3).unwrap();
            (nl, ms)
        };
        let probe_v = |nl: &netlist::Netlist, doc: &crate::model::Document, ms: &elab_sim::MixedSim, name: &str| {
            let v = sim::mixed_view(nl, ms);
            v.net_v[nl.pin_nets[&id(doc, name)][0]].unwrap()
        };

        // NAND: LED an, solange nicht beide Taster gedrückt sind
        let ex = find("NAND-Baustein");
        let led = id(&ex.doc, "LED1");
        for (pressed, on) in [(&[][..], true), (&["S_A"][..], true), (&["S_B"][..], true), (&["S_A", "S_B"][..], false)] {
            let (nl, ms) = start(&ex.doc, pressed);
            let i = sim::mixed_view(&nl, &ms).parts[&led].i;
            assert_eq!(i > 3e-3, on, "{pressed:?}: I_LED = {i}");
            let y = probe_v(&nl, &ex.doc, &ms, "Y");
            assert!(if on { y > 4.5 } else { y < 0.05 }, "{pressed:?}: Y = {y}");
        }
        // Versorgungsstrom = LED-Strom (plus Ruhestrom)
        let (nl, ms) = start(&ex.doc, &[]);
        let v = sim::mixed_view(&nl, &ms);
        let u1 = v.parts[&id(&ex.doc, "U1")];
        assert!((u1.u - 5.0).abs() < 1e-6 && (u1.i - v.parts[&led].i).abs() < 1e-5, "{u1:?}");
        // offener Eingang: X
        let mut d = ex.doc.clone();
        let r1 = id(&d, "R1");
        d.remove(&[r1].into(), &Default::default());
        let nl = netlist::build(&d);
        let ms = sim::mixed_sim(&nl, 1e-3).unwrap();
        assert_eq!(ms.observer_values(0), &[Logic::X], "offener Eingang");
        // 9 V: zerstört
        let mut d = ex.doc.clone();
        d.part_mut(id(&d, "V1")).unwrap().kind = PartKind::DcVoltage { v: 9.0 };
        let nl = netlist::build(&d);
        let ms = sim::mixed_sim(&nl, 1e-3).unwrap();
        assert_eq!(ms.power(0), elab_sim::PowerState::Overvoltage);

        // 3,3 V an HC und HCT
        let ex = find("3,3 V trifft 5 V");
        let (nl, ms) = start(&ex.doc, &[]);
        let (hc, hct) = (probe_v(&nl, &ex.doc, &ms, "Y_HC"), probe_v(&nl, &ex.doc, &ms, "Y_HCT"));
        assert!(hc > 1.5 && hc < 3.0, "HC sieht X: {hc}");
        assert!(hct > 4.5, "HCT sieht 1: {hct}");

        // Schmitt-Trigger-Oszillator: T ≈ 0,817 · R · C
        let ex = find("Schmitt-Trigger");
        let (nl, mut ms) = start(&ex.doc, &[]);
        let res = ms.run(3.0).unwrap();
        let net = &nl.nets[nl.pin_nets[&id(&ex.doc, "U_aus")][0]].name;
        let v = res.voltage(net).unwrap();
        let rises: Vec<f64> = (1..v.len()).filter(|&k| v[k - 1] < 2.5 && v[k] >= 2.5).map(|k| res.time[k]).collect();
        let t = (rises[rises.len() - 1] - rises[1]) / (rises.len() - 2) as f64;
        assert!((t - 0.817 * 47e3 * 10e-6).abs() < 0.03, "T = {t}");

        // 555: Kondensator pendelt zwischen 3 V und 6 V, f ≈ 1,4 Hz
        let ex = find("555-Blinker");
        let (nl, mut ms) = start(&ex.doc, &[]);
        let res = ms.run(5.0).unwrap();
        let net = &nl.nets[nl.pin_nets[&id(&ex.doc, "U_C")][0]].name;
        let vc = res.voltage(net).unwrap();
        let late: Vec<f64> = vc.iter().zip(&res.time).filter(|(_, &t)| t > 2.0).map(|(v, _)| *v).collect();
        let (mn, mx) = late.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!((mn - 3.0).abs() < 0.1 && (mx - 6.0).abs() < 0.1, "{mn} … {mx}");
        let out = &nl.nets[nl.pin_nets[&id(&ex.doc, "OUT")][0]].name;
        let vo = res.voltage(out).unwrap();
        let rises: Vec<f64> = (1..vo.len()).filter(|&k| vo[k - 1] < 4.5 && vo[k] >= 4.5).map(|k| res.time[k]).collect();
        let t = (rises[rises.len() - 1] - rises[1]) / (rises.len() - 2) as f64;
        let t_exp = 0.693 * (10e3 + 2.0 * 47e3) * 10e-6;
        assert!((t / t_exp - 1.0).abs() < 0.03, "T = {t} statt {t_exp}");

        // OpAmp: V = 3, dann Übersteuerung bei etwa 10,5 V
        let ex = find("OpAmp-Verstärker");
        let (nl, ms) = start(&ex.doc, &[]);
        let v = sim::mixed_view(&nl, &ms);
        let (ue, ua) = (v.parts[&id(&ex.doc, "U_ein")].u, v.parts[&id(&ex.doc, "U_aus")].u);
        assert!((ue - 3.0).abs() < 0.01 && (ua - 9.0).abs() < 0.01, "{ue} → {ua}");
        let mut d = ex.doc.clone();
        d.part_mut(id(&d, "P1")).unwrap().kind = PartKind::Potentiometer { r: 10e3, pos: 0.6 };
        let (nl, ms) = start(&d, &[]);
        let ua = sim::mixed_view(&nl, &ms).parts[&id(&d, "U_aus")].u;
        assert!((ua - 10.48).abs() < 0.05, "übersteuert: {ua}");

        // ADC → DAC
        let ex = find("ADC und DAC");
        let (nl, ms) = start(&ex.doc, &[]);
        let v = sim::mixed_view(&nl, &ms);
        let (ue, ud) = (v.parts[&id(&ex.doc, "U_ein")].u, v.parts[&id(&ex.doc, "U_DAC")].u);
        assert!((ue - 2.0).abs() < 1e-3 && (ud - 5.0 * 6.0 / 16.0).abs() < 1e-3, "{ue} → {ud}");
    }

    #[test]
    fn transistor_examples_behave() {
        use crate::model::PartKind;
        use std::collections::HashSet;
        let all = super::all();
        let find = |t: &str| all.iter().find(|e| e.title.starts_with(t)).unwrap();
        let val = |doc: &crate::model::Document, op: &sim::OpView, name: &str| op.parts[&doc.part_by_name(name).unwrap().id];

        // Transistor als Schalter: aus ohne Taster, gesättigt mit Taster
        let ex = find("Transistor als Schalter");
        let nl = netlist::build(&ex.doc);
        assert!(nl.error.is_none() && nl.excluded.is_empty() && nl.bridged.is_empty(), "{:?} {:?}", nl.error, nl.excluded);
        let op = sim::operating_point(&nl).unwrap();
        assert!(val(&ex.doc, &op, "I_C").i.abs() < 1e-6, "aus: I_C = {}", val(&ex.doc, &op, "I_C").i);
        let s1 = ex.doc.part_by_name("S1").unwrap().id;
        let nl = netlist::build_with(&ex.doc, &HashSet::from([s1]));
        let op = sim::operating_point(&nl).unwrap();
        let (ib, ic) = (val(&ex.doc, &op, "I_B").i, val(&ex.doc, &op, "I_C").i);
        assert!(ib > 0.7e-3 && ib < 0.9e-3, "I_B = {ib}");
        assert!(ic > 0.018 && ic < 0.024, "I_C = {ic}");
        assert_eq!(val(&ex.doc, &op, "T1").region, Some(sim::Region::On), "gesättigt");

        // CMOS-Inverter: Kennlinie über das Poti
        let ex = find("CMOS-Inverter");
        let at = |pos: f64| {
            let mut d = ex.doc.clone();
            let id = d.part_by_name("P1").unwrap().id;
            d.part_mut(id).unwrap().kind = PartKind::Potentiometer { r: 10e3, pos };
            let nl = netlist::build(&d);
            assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?}", nl.error);
            let op = sim::operating_point(&nl).unwrap();
            (val(&d, &op, "U_ein").u, val(&d, &op, "U_aus").u, val(&d, &op, "I_quer").i)
        };
        let (ue, ua, iq) = at(0.1);
        assert!((ue - 0.5).abs() < 0.01 && ua > 4.99 && iq.abs() < 1e-6, "Eingang 0: {ue} → {ua}, {iq}");
        let (ue, ua, _) = at(0.9);
        assert!((ue - 4.5).abs() < 0.01 && ua < 0.01, "Eingang 1: {ue} → {ua}");
        let (_, ua, iq) = at(0.5);
        assert!((ua - 2.5).abs() < 0.3 && iq > 1e-4, "Schwelle: {ua}, Querstrom {iq}");

        // NAND: nur A = B = 1 gibt 0
        let ex = find("NAND-Gatter aus Transistoren");
        for (a, b, high) in [(false, false, true), (true, false, true), (false, true, true), (true, true, false)] {
            let mut d = ex.doc.clone();
            for (n, on) in [("A", a), ("B", b)] {
                let id = d.part_by_name(n).unwrap().id;
                d.part_mut(id).unwrap().kind = PartKind::Switch { closed: on };
            }
            let nl = netlist::build(&d);
            assert!(nl.error.is_none() && nl.excluded.is_empty() && nl.bridged.is_empty(), "{:?}", nl.error);
            let y = val(&d, &sim::operating_point(&nl).unwrap(), "U_Y").u;
            assert_eq!(y > 4.5, high, "A={a} B={b}: U_Y = {y}");
            assert!(!(0.5..=4.5).contains(&y), "sauberer Pegel: {y}");
        }

        // Emitterschaltung: Arbeitspunkt und Verstärkung
        let ex = find("Emitterschaltung");
        let nl = netlist::build(&ex.doc);
        let op = sim::operating_point(&nl).unwrap();
        let t1 = val(&ex.doc, &op, "T1");
        assert_eq!(t1.region, Some(sim::Region::Active));
        assert!(t1.i > 1.2e-3 && t1.i < 1.6e-3, "I_C = {}", t1.i);
        let sc = sim::record(&ex.doc, &nl, 3e-3).unwrap();
        let swing = |name: &str| {
            let tr = sc.traces.iter().find(|t| t.name == name).unwrap();
            let v: Vec<f64> = tr.pts.iter().filter(|p| p[0] > 1e-3).map(|p| p[1]).collect();
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        let gain = swing("Ausgang") / swing("Eingang");
        assert!(gain > 4.0 && gain < 5.0, "Verstärkung {gain}");
    }

    #[test]
    fn digital_examples_behave() {
        use elab_sim::digital::Logic::*;
        let all = super::all();
        let find = |t: &str| all.iter().find(|e| e.title.starts_with(t)).unwrap();
        // Halbaddierer: Wahrheitstabelle
        let ex = find("Halbaddierer");
        let nl = netlist::build(&ex.doc);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        let tt = sim::truth_table(&ex.doc, &nl).unwrap();
        assert_eq!(tt.inputs, ["A", "B"]);
        let outs: Vec<Vec<_>> = tt.rows.iter().map(|r| r.1.clone()).collect();
        assert_eq!(outs, vec![vec![L0, L0], vec![L1, L0], vec![L1, L0], vec![L0, L1]]);
        // Volladdierer aus zwei eigenen ICs
        // Jedes Beispiel hat ein bekanntes Thema
        for e in &all {
            assert!(super::CATEGORIES.iter().any(|c| c.0 == e.category), "{}", e.title);
        }
        let ex = find("Volladdierer");
        let nl = netlist::build(&ex.doc);
        assert!(nl.error.is_none() && nl.mixed.is_none(), "{:?}", nl.error);
        let tt = sim::truth_table(&ex.doc, &nl).unwrap();
        assert_eq!(tt.inputs, ["A", "B", "C_ein"]);
        assert_eq!(tt.outputs, ["S", "C_aus"]);
        for (ins, outs, ok) in &tt.rows {
            let n = ins.iter().filter(|b| **b).count();
            assert!(ok);
            assert_eq!(outs, &vec![if n % 2 == 1 { L1 } else { L0 }, if n >= 2 { L1 } else { L0 }], "{ins:?}");
        }
        // Logikgatter: UND, ODER, XOR
        let ex = find("Logikgatter");
        let nl = netlist::build(&ex.doc);
        let tt = sim::truth_table(&ex.doc, &nl).unwrap();
        let col = |k: usize| tt.rows.iter().map(|r| r.1[k]).collect::<Vec<_>>();
        assert_eq!(col(0), [L0, L0, L0, L1], "UND");
        assert_eq!(col(1), [L0, L1, L1, L1], "ODER");
        assert_eq!(col(2), [L0, L1, L1, L0], "XOR");
        // Frequenzteiler: Q2 hat ein Viertel der Taktfrequenz
        let ex = find("Frequenzteiler");
        let nl = netlist::build(&ex.doc);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        let mut ds = sim::digital_sim(&ex.doc, &nl).unwrap();
        ds.run_until(4_000_000_000); // 4 s bei 2 Hz = 8 Takte
        let rows = sim::analyzer_rows(&ex.doc, &nl);
        let rises = |name: &str| {
            let net = rows.iter().find(|r| r.0 == name).unwrap().1;
            ds.history(net).windows(2).filter(|w| w[0].1 == L0 && w[1].1 == L1).count()
        };
        assert_eq!(rises("Takt"), 8);
        assert_eq!(rises("Q1"), 4);
        assert_eq!(rises("Q2"), 2);
        // Zähler: nach 5 Takten steht 5 an
        let ex = find("Zähler");
        let nl = netlist::build(&ex.doc);
        let mut ds = sim::digital_sim(&ex.doc, &nl).unwrap();
        ds.run_until(2_600_000_000); // steigende Flanken bei 0,25 s, 0,75 s, … → 5 bis 2,6 s
        let bits: Vec<_> = ["Q0", "Q1", "Q2", "Q3"].iter().map(|n| ds.net(nl.pin_nets[&ex.doc.part_by_name(n).unwrap().id][0])).collect();
        assert_eq!(bits, [L1, L0, L1, L0], "5 = 0101");
        // RS-Flipflop: kein X im Ruhezustand
        let ex = find("RS-Flipflop");
        let nl = netlist::build(&ex.doc);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        let ds = sim::digital_sim(&ex.doc, &nl).unwrap();
        let q = ds.net(nl.pin_nets[&ex.doc.part_by_name("Q").unwrap().id][0]);
        let qn = ds.net(nl.pin_nets[&ex.doc.part_by_name("/Q").unwrap().id][0]);
        assert!(q != X && qn != X && q != qn, "Q = {q:?}, /Q = {qn:?}");
    }

    #[test]
    fn divider_example_values() {
        let ex = &super::all()[0];
        let nl = netlist::build(&ex.doc);
        let op = sim::operating_point(&nl).unwrap();
        let p = ex.doc.part_by_name("U_aus").unwrap();
        let v = op.net_v[nl.pin_nets[&p.id][0]].unwrap();
        assert!((v - 6.0).abs() < 1e-5, "{v}");
    }

    #[test]
    fn led_example_current() {
        let ex = &super::all()[1];
        let nl = netlist::build(&ex.doc);
        let op = sim::operating_point(&nl).unwrap();
        let led = ex.doc.part_by_name("LED1").unwrap();
        let i = op.parts[&led.id].i;
        assert!(i > 0.012 && i < 0.016, "I_LED = {i}");
    }
}
