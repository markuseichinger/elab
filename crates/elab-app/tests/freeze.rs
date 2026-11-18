//! Nachbau einer Schaltung, bei der die App beim Einsetzen eines Widerstands einfror:
//! 74HC00, dessen Versorgung über 1 kΩ läuft, mit einer LED ohne Vorwiderstand am
//! Ausgang. Zieht die LED Strom, bricht die Versorgung ein, das IC schaltet ab, die
//! Versorgung erholt sich, das IC schaltet wieder ein …

use elab_app::model::{Document, GateType, LedColor, LogicFamily, PartKind as K, P};
use elab_app::{netlist, sim};
use std::time::{Duration, Instant};

fn w(d: &mut Document, pts: &[(i32, i32)]) {
    for s in pts.windows(2) {
        d.add_wire(P::new(s[0].0, s[0].1), P::new(s[1].0, s[1].1));
    }
}

pub fn screenshot_circuit(with_r3: bool) -> Document {
    let mut d = Document::default();
    d.add_part(K::DcVoltage { v: 5.0 }, P::new(5, 12), 0);
    d.add_part(K::Ground, P::new(5, 15), 0);
    if with_r3 {
        d.add_part(K::Resistor { r: 1e3 }, P::new(5, 7), 1);
    } else {
        w(&mut d, &[(5, 5), (5, 9)]);
    }
    d.add_part(K::PushButton, P::new(8, 7), 1);
    d.add_part(K::Resistor { r: 10e3 }, P::new(8, 13), 1);
    d.add_part(K::Resistor { r: 10e3 }, P::new(10, 13), 1);
    d.add_part(K::Switch { closed: true }, P::new(14, 2), 1);
    d.add_part(K::GateIc { gate: GateType::Nand, family: LogicFamily::Hc }, P::new(15, 10), 0);
    d.add_part(K::Led { color: LedColor::Rot }, P::new(20, 14), 0);
    w(&mut d, &[(5, 9), (5, 10)]);
    w(&mut d, &[(5, 14), (5, 15)]);
    w(&mut d, &[(5, 5), (8, 5), (10, 5), (15, 5), (15, 7)]);
    w(&mut d, &[(8, 9), (12, 9)]);
    w(&mut d, &[(8, 9), (8, 11)]);
    w(&mut d, &[(10, 11), (11, 11), (12, 11)]);
    w(&mut d, &[(10, 5), (10, 0), (14, 0)]);
    w(&mut d, &[(14, 4), (11, 4), (11, 11)]);
    w(&mut d, &[(5, 15), (8, 15), (10, 15), (12, 15), (13, 15), (13, 13), (15, 13)]);
    w(&mut d, &[(12, 15), (12, 17), (22, 17), (22, 14)]);
    w(&mut d, &[(18, 10), (18, 14)]);
    d.normalize();
    d
}

#[test]
fn supply_through_resistor_with_led_load_does_not_hang() {
    for with_r3 in [false, true] {
        let d = screenshot_circuit(with_r3);
        let nl = netlist::build(&d);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        assert!(nl.excluded.is_empty(), "nicht verbunden: {:?}", nl.excluded);
        let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
        let start = Instant::now();
        for k in 1..=120 {
            let t0 = ms.time();
            let dl = Instant::now() + Duration::from_millis(30);
            if let Err(e) = ms.advance(k as f64 / 60.0, Some(dl)) {
                panic!("R3={with_r3}: Fehler bei t={}: {e}", ms.time());
            }
            let took = Instant::now().saturating_duration_since(dl);
            assert!(took < Duration::from_millis(200), "R3={with_r3}: Schritt bei t={t0} überzog die Frist um {took:?}");
        }
        assert!(start.elapsed() < Duration::from_secs(20), "R3={with_r3}: zu langsam");
        eprintln!("R3={with_r3}: t={} nach {:?}", ms.time(), start.elapsed());
    }
}

/// Mit ELAB_FREEZE_OUT=pfad wird die Schaltung als Datei geschrieben (zum Öffnen in der App).
#[test]
fn write_circuit_on_request() {
    if let Ok(p) = std::env::var("ELAB_FREEZE_OUT") {
        std::fs::write(p, serde_json::to_string(&screenshot_circuit(true)).unwrap()).unwrap();
    }
}
