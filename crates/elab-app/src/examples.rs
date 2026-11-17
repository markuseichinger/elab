//! Mitgelieferte Beispielschaltungen.

use crate::model::{DiodeType, Document, LedColor, PartKind as K, P};

/// Eine Beispielschaltung mit Beschreibung und passender Oszilloskop-Zeitbasis.
pub struct Example {
    pub title: &'static str,
    pub summary: &'static str,
    pub scope_t: f64,
    pub doc: Document,
}

struct B(Document);

impl B {
    fn new(title: &str) -> Self {
        B(Document {
            title: title.into(),
            ..Default::default()
        })
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
            self.0
                .add_wire(P::new(w[0].0, w[0].1), P::new(w[1].0, w[1].1));
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
            assert!(
                nl.excluded.is_empty(),
                "{}: nicht verbunden: {:?}",
                ex.title,
                nl.excluded
            );
            let op = sim::operating_point(&nl).unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            let has_dc = ex
                .doc
                .parts
                .iter()
                .any(|p| matches!(p.kind, crate::model::PartKind::DcVoltage { .. }));
            assert!(!has_dc || op.max_abs_v() > 0.0, "{}", ex.title);
            let scope = sim::record(&ex.doc, &nl, ex.scope_t)
                .unwrap_or_else(|e| panic!("{}: {e}", ex.title));
            assert_eq!(
                scope.traces.len(),
                sim::probes(&ex.doc).len(),
                "{}",
                ex.title
            );
        }
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
