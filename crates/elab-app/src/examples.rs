//! Mitgelieferte Beispielschaltungen.

use crate::model::{DiodeType, Document, GateType as G, LedColor, PartKind as K, P};

/// Eine Beispielschaltung mit Beschreibung und passender Oszilloskop-Zeitbasis.
pub struct Example {
    pub title: &'static str,
    pub summary: &'static str,
    pub scope_t: f64,
    pub doc: Document,
    /// Digitalschaltung (Palette und Messgeräte passend umschalten)
    pub digital: bool,
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

pub fn all() -> Vec<Example> {
    vec![
        Example {
            title: "Spannungsteiler",
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
        // ---------------- Digitaltechnik ----------------
        Example {
            title: "Logikgatter UND, ODER, XOR",
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
    ]
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
            let op = sim::operating_point(&nl).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            let has_dc = ex.doc.parts.iter().any(|p| matches!(p.kind, crate::model::PartKind::DcVoltage { .. }));
            assert!(!has_dc || op.max_abs_v() > 0.0, "{}", ex.title);
            let scope = sim::record(&ex.doc, &nl, ex.scope_t).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            assert_eq!(scope.traces.len(), sim::probes(&ex.doc).len(), "{}", ex.title);
        }
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
