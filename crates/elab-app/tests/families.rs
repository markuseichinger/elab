//! Bauteiltypen: Logikfamilien, Zener-Diode, Elko.

use elab_app::model::{CapKind, DiodeType, Document, GateType, LogicFamily, PartKind as K, P};
use elab_app::{limits, netlist, sim};

fn w(d: &mut Document, pts: &[(i32, i32)]) {
    for s in pts.windows(2) {
        d.add_wire(P::new(s[0].0, s[0].1), P::new(s[1].0, s[1].1));
    }
}

/// NAND mit beiden Eingängen an Masse (Ausgang 1), 10 kΩ Last am Ausgang.
fn nand_high(family: LogicFamily, vcc: f64) -> f64 {
    let mut d = Document::default();
    d.add_part(K::DcVoltage { v: vcc }, P::new(0, 0), 0);
    d.add_part(K::Ground, P::new(0, 6), 0);
    let u = d.add_part(K::GateIc { gate: GateType::Nand, family }, P::new(10, 0), 0);
    d.add_part(K::Resistor { r: 10e3 }, P::new(16, 3), 1);
    w(&mut d, &[(0, -2), (0, -6), (10, -6), (10, -3)]);
    w(&mut d, &[(0, 2), (0, 6), (5, 6), (10, 6), (16, 6), (16, 5)]);
    w(&mut d, &[(10, 3), (10, 6)]);
    w(&mut d, &[(7, -1), (5, -1), (5, 1), (5, 6)]);
    w(&mut d, &[(7, 1), (5, 1)]);
    w(&mut d, &[(13, 0), (16, 0), (16, 1)]);
    d.normalize();
    let nl = netlist::build(&d);
    assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?} {:?}", nl.error, nl.excluded);
    let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
    ms.advance(2e-3, None).unwrap();
    let v = sim::mixed_view(&nl, &ms);
    v.net_v[nl.pin_nets[&u][2]].unwrap()
}

#[test]
fn logic_families_have_their_output_levels() {
    let hc = nand_high(LogicFamily::Hc, 5.0);
    let ls = nand_high(LogicFamily::Ls, 5.0);
    let cd = nand_high(LogicFamily::Cd4000, 5.0);
    let cd12 = nand_high(LogicFamily::Cd4000, 12.0);
    assert!(hc > 4.9, "74HC: {hc}");
    assert!(ls > 3.0 && ls < 3.6, "74LS: {ls}");
    assert!(cd > 4.3 && cd < 4.8, "CD4000 schwacher Ausgang: {cd}");
    assert!(cd12 > 11.0, "CD4000 an 12 V: {cd12}");
    // 74HC an 12 V: zerstört, Ausgang tot
    let dead = nand_high(LogicFamily::Hc, 12.0);
    assert!(dead < 1.0, "74HC an 12 V lebt noch: {dead}");
}

/// Zener-Stabilisierung im Schaltplan: 9 V, 470 Ω, ZPD 5,1 mit der Kathode an Plus.
#[test]
fn zener_regulates_in_the_app() {
    let mut d = Document::default();
    d.add_part(K::DcVoltage { v: 9.0 }, P::new(0, 0), 0);
    d.add_part(K::Ground, P::new(0, 6), 0);
    d.add_part(K::Resistor { r: 470.0 }, P::new(4, -4), 0);
    // senkrecht, Anode unten (rot 3: Anschluss 1 oben)
    let z = d.add_part(K::Diode { kind: DiodeType::Zener5V1 }, P::new(8, 1), 3);
    let pins = d.part(z).unwrap().pins();
    assert!(pins[1].y < pins[0].y, "Kathode soll oben liegen: {pins:?}");
    w(&mut d, &[(0, -2), (0, -4), (2, -4)]);
    w(&mut d, &[(6, -4), (8, -4), (8, pins[1].y)]);
    w(&mut d, &[(8, pins[0].y), (8, 6), (0, 6), (0, 2)]);
    d.normalize();
    let nl = netlist::build(&d);
    assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?} {:?}", nl.error, nl.excluded);
    let v = sim::operating_point(&nl).unwrap();
    let u = -v.parts[&z].u;
    assert!((u - 5.0).abs() < 0.25, "U_Z = {u}");
}

#[test]
fn reversed_or_overvolted_elko_is_destroyed() {
    let elko = K::Capacitor { c: 100e-6, kind: CapKind::Elko16 };
    let (s, why) = limits::stress(&elko, -5.0, 0.0, 0.0);
    assert!(s > 1.0 && why.contains("verpolt"), "{s} {why}");
    let (s, _) = limits::stress(&elko, 20.0, 0.0, 0.0);
    assert!(s > 1.0, "20 V an 16-V-Elko: {s}");
    let (s, _) = limits::stress(&elko, 12.0, 0.0, 0.0);
    assert!(s < 1.0);
    let keramik = K::Capacitor { c: 100e-9, kind: CapKind::Keramik };
    assert!(limits::stress(&keramik, -12.0, 0.0, 0.0).0 < 1.0, "Keramik ist ungepolt");
}
