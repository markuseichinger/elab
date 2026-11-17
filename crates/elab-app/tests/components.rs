//! Jedes Bauteil einzeln: in eine bestehende Leitung eingefügt, simuliert und
//! gegen die Handrechnung geprüft.
//!
//! Testschaltung (Rasterkoordinaten):
//!
//! ```text
//!   (0,-6) ───── R0 1 kΩ ───── DUT ───── (20,-6)
//!     │                                     │
//!    V1 9 V                                 │
//!     │                                     │
//!   (0, 6) ────────── ⏚ ─────────────────(20, 6)
//! ```
//!
//! R0 und das Testbauteil (DUT) werden erst *nach* dem Verdrahten auf die obere
//! Leitung gesetzt. Ohne Einfügen wären sie kurzgeschlossen.

use elab_app::model::{DiodeType, Document, LedColor, PartKind as K, P};
use elab_app::{netlist, sim};

const U0: f64 = 9.0;
const R0: f64 = 1000.0;

fn loop_with(dut: K, dut_rot: u8) -> (Document, u32) {
    let mut d = Document::default();
    d.add_part(K::DcVoltage { v: U0 }, P::new(0, 0), 0);
    d.add_part(K::Ground, P::new(10, 6), 0);
    for (a, b) in [((0, -2), (0, -6)), ((0, -6), (20, -6)), ((20, -6), (20, 6)), ((20, 6), (0, 6)), ((0, 6), (0, 2))] {
        d.add_wire(P::new(a.0, a.1), P::new(b.0, b.1));
    }
    d.normalize();
    // Jetzt in die bestehenden Leitungen einsetzen
    d.add_part(K::Resistor { r: R0 }, P::new(5, -6), 0);
    let id = d.add_part(dut, P::new(13, -6), dut_rot);
    d.normalize();
    (d, id)
}

/// (U, I) des Testbauteils im Arbeitspunkt.
fn measure(dut: K) -> (f64, f64) {
    let (d, id) = loop_with(dut.clone(), 0);
    let nl = netlist::build(&d);
    assert!(nl.error.is_none(), "{dut:?}: {:?}", nl.error);
    assert!(nl.excluded.is_empty(), "{dut:?}: nicht verbunden");
    assert!(nl.bridged.is_empty(), "{dut:?}: überbrückt – Einfügen hat nicht funktioniert");
    let op = sim::operating_point(&nl).unwrap_or_else(|e| panic!("{dut:?}: {e}"));
    let v = op.parts[&id];
    (v.u, v.i)
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: erwartet {b}, simuliert {a}");
}

/// Diodenarbeitspunkt (U0 − U)/R0 = Is·(e^(U/nVt) − 1) per Bisektion.
fn diode_op(m: elab_sim::DiodeModel) -> (f64, f64) {
    let f = |u: f64| (U0 - u) / R0 - m.is * ((u / m.nvt()).exp() - 1.0);
    let (mut lo, mut hi) = (0.0, U0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(lo) * f(mid) <= 0.0 {
            hi = mid
        } else {
            lo = mid
        }
    }
    let u = 0.5 * (lo + hi);
    (u, (U0 - u) / R0)
}

#[test]
fn widerstand() {
    let (u, i) = measure(K::Resistor { r: 2000.0 });
    close(i, U0 / 3000.0, 1e-8, "I");
    close(u, 6.0, 1e-5, "U");
}

#[test]
fn kondensator_sperrt_gleichstrom() {
    let (u, i) = measure(K::Capacitor { c: 1e-6 });
    close(i, 0.0, 1e-9, "I");
    close(u, U0, 1e-5, "U");
}

#[test]
fn spule_leitet_gleichstrom() {
    let (u, i) = measure(K::Inductor { l: 1e-3 });
    close(i, U0 / R0, 1e-8, "I");
    close(u, 0.0, 1e-6, "U");
}

#[test]
fn gluehlampe() {
    let (_, i) = measure(K::Lamp { v_nom: 12.0, p_nom: 5.0 });
    close(i, U0 / (R0 + 28.8), 1e-8, "I");
}

#[test]
fn dioden_aller_typen() {
    for t in DiodeType::ALL {
        let (u, i) = measure(K::Diode { kind: t });
        let (ue, ie) = diode_op(t.model());
        close(u, ue, 1e-4, t.label());
        close(i, ie, 1e-7, t.label());
    }
}

#[test]
fn leds_aller_farben() {
    for c in LedColor::ALL {
        let (u, i) = measure(K::Led { color: c });
        let (ue, ie) = diode_op(c.model());
        close(u, ue, 1e-4, c.label());
        close(i, ie, 1e-7, c.label());
        assert!(u < c.vf(), "{}: bei < 20 mA muss U unter der Nenn-Flussspannung liegen", c.label());
    }
}

#[test]
fn diode_in_sperrrichtung() {
    let (d, id) = loop_with(K::Diode { kind: DiodeType::Si1N4148 }, 2); // umgedreht
    let op = sim::operating_point(&netlist::build(&d)).unwrap();
    let v = op.parts[&id];
    assert!(v.i.abs() < 1e-7, "Sperrstrom {}", v.i);
    close(v.u, -U0, 1e-3, "U_Sperr");
}

#[test]
fn schalter_offen_und_geschlossen() {
    let (u, i) = measure(K::Switch { closed: false });
    assert!(i.abs() < 1e-12, "I offen = {i}");
    close(u, U0, 1e-3, "U offen");
    let (u, i) = measure(K::Switch { closed: true });
    close(i, U0 / R0, 1e-7, "I geschlossen");
    assert!(u.abs() < 1e-4);
}

#[test]
fn quellen_werden_beim_absetzen_passend_gedreht() {
    // Quellen stehen senkrecht. Auf der waagerechten Leitung dreht snap_rotation sie
    // so, dass sie eingefügt werden (Pluspol dann rechts).
    let d0 = loop_with(K::Resistor { r: 1.0 }, 0).0;
    for kind in [
        K::DcVoltage { v: 1.0 },
        K::AcVoltage { amp: 5.0, freq: 50.0, offset: 1.0 },
        K::SquareVoltage { low: 0.0, high: 5.0, freq: 1000.0 },
        K::CurrentSource { i: 1e-3 },
    ] {
        let rot = d0.snap_rotation(&kind, P::new(9, -6), 0);
        assert_eq!(rot, 1, "{kind:?}");
    }
    // Wechselspannung mit Offset 1 V in Reihe: Arbeitspunkt nutzt den Wert bei t = 0
    let (mut d, _) = loop_with(K::Resistor { r: 1.0 }, 0);
    d.parts.pop();
    d.normalize();
    let id = d.add_part(K::AcVoltage { amp: 5.0, freq: 50.0, offset: 1.0 }, P::new(13, -6), 1);
    d.normalize();
    let nl = netlist::build(&d);
    assert!(nl.bridged.is_empty() && nl.excluded.is_empty(), "eingefügt");
    let op = sim::operating_point(&nl).unwrap();
    // Pluspol rechts: Im Umlauf (oben von links nach rechts) liegt die Quelle von − nach +,
    // sie unterstützt V1. Strom = (9 + 1) V / 1 kΩ
    close(op.parts[&id].i.abs(), 10.0 / R0, 1e-6, "I");
}

#[test]
fn stromquelle() {
    // Stromquelle allein an 1 kΩ: U = I · R
    let mut d = Document::default();
    d.add_part(K::CurrentSource { i: 0.005 }, P::new(0, 0), 0);
    d.add_part(K::Resistor { r: 1000.0 }, P::new(6, 0), 1);
    d.add_part(K::Ground, P::new(3, 4), 0);
    d.add_wire(P::new(0, -2), P::new(0, -4));
    d.add_wire(P::new(0, -4), P::new(6, -4));
    d.add_wire(P::new(6, -4), P::new(6, -2));
    d.add_wire(P::new(6, 2), P::new(6, 4));
    d.add_wire(P::new(6, 4), P::new(0, 4));
    d.add_wire(P::new(0, 4), P::new(0, 2));
    let nl = netlist::build(&d);
    let op = sim::operating_point(&nl).unwrap();
    let r = d.part_by_name("R1").unwrap().id;
    close(op.parts[&r].u, 5.0, 1e-5, "U_R");
}

#[test]
fn messpunkt_zeigt_netzspannung() {
    let (mut d, _) = loop_with(K::Resistor { r: 2000.0 }, 0);
    // Messpunkt zwischen R0 und DUT, mitten auf die Leitung
    let pid = d.add_part(K::Probe, P::new(9, -6), 0);
    d.normalize();
    let nl = netlist::build(&d);
    let op = sim::operating_point(&nl).unwrap();
    let v = op.net_v[nl.pin_nets[&pid][0]].unwrap();
    close(v, 6.0, 1e-5, "U_Messpunkt");
    // und im Oszilloskop
    let sc = sim::record(&d, &nl, 1e-3).unwrap();
    assert_eq!(sc.traces.len(), 1);
    close(sc.traces[0].pts.last().unwrap()[1], 6.0, 1e-4, "Oszilloskop");
}

#[test]
fn senkrecht_eingefuegt() {
    // Bauteil senkrecht in die rechte Leitung (20,-6)…(20,6)
    let (mut d, _) = loop_with(K::Resistor { r: 1000.0 }, 0);
    let id = d.add_part(K::Resistor { r: 1000.0 }, P::new(20, 0), 1);
    d.normalize();
    let nl = netlist::build(&d);
    assert!(nl.bridged.is_empty());
    let op = sim::operating_point(&nl).unwrap();
    close(op.parts[&id].i, U0 / 3000.0, 1e-8, "I durch drei Widerstände");
}

#[test]
fn verschieben_von_der_leitung_herunter_haelt_verbindung() {
    let (mut d, id) = loop_with(K::Resistor { r: 2000.0 }, 0);
    let ids = [id].into();
    d.transform_parts(&ids, &Default::default(), P::default(), |p| p.pos = p.pos + P::new(0, 4));
    let nl = netlist::build(&d);
    assert!(nl.bridged.is_empty() && nl.excluded.is_empty());
    let op = sim::operating_point(&nl).unwrap();
    close(op.parts[&id].i, U0 / 3000.0, 1e-8, "I nach Verschieben");
}

// ---------------------------------------------------------------------------
// Phase 3: Potentiometer, Taster, Messgeräte, Transistoren
// ---------------------------------------------------------------------------

use std::collections::HashSet;

#[test]
fn amperemeter_eingefuegt_misst_strom() {
    let (d, id) = loop_with(K::Ammeter, 0);
    let nl = netlist::build(&d);
    assert!(nl.bridged.is_empty() && nl.excluded.is_empty());
    let op = sim::operating_point(&nl).unwrap();
    // Strom läuft oben von links nach rechts, also von + nach − durch das Messgerät
    close(op.parts[&id].i, U0 / R0, 1e-9, "Anzeige");
}

#[test]
fn voltmeter_parallel_belastet_nicht() {
    let (mut d, rid) = loop_with(K::Resistor { r: 2000.0 }, 0);
    // Voltmeter über das Testbauteil (Pins bei (11,-6) und (15,-6)), darüber verdrahtet
    let vm = d.add_part(K::Voltmeter, P::new(13, -10), 0);
    d.add_wire(P::new(11, -6), P::new(11, -10));
    d.add_wire(P::new(15, -6), P::new(15, -10));
    let nl = netlist::build(&d);
    assert!(nl.error.is_none() && nl.bridged.is_empty(), "{:?}", nl.error);
    let op = sim::operating_point(&nl).unwrap();
    close(op.parts[&vm].u, 6.0, 1e-5, "Anzeige");
    close(op.parts[&rid].i, U0 / 3000.0, 1e-9, "Strom unverändert");
}

#[test]
fn potentiometer_teilt_spannung() {
    // Poti direkt an 9 V (A oben, B unten), Schleifer offen: U_SB = U · Stellung
    for pos in [0.0, 0.25, 0.7, 1.0] {
        let mut d = Document::default();
        d.add_part(K::DcVoltage { v: U0 }, P::new(0, 0), 0);
        let id = d.add_part(K::Potentiometer { r: 10e3, pos }, P::new(6, 0), 1); // A (6,-2), S (8,0), B (6,2)
        d.add_part(K::Ground, P::new(3, 4), 0);
        d.add_wire(P::new(0, -2), P::new(0, -4));
        d.add_wire(P::new(0, -4), P::new(6, -4));
        d.add_wire(P::new(6, -4), P::new(6, -2));
        d.add_wire(P::new(6, 2), P::new(6, 4));
        d.add_wire(P::new(6, 4), P::new(0, 4));
        d.add_wire(P::new(0, 4), P::new(0, 2));
        let nl = netlist::build(&d);
        assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?}", nl.error);
        let op = sim::operating_point(&nl).unwrap();
        let wiper = op.net_v[nl.net_of[&P::new(8, 0)]].unwrap();
        close(wiper, U0 * pos, 1e-5, &format!("Schleifer bei {pos}"));
        close(op.parts[&id].ctrl_u, U0 * pos, 1e-5, "U_SB");
    }
}

#[test]
fn potentiometer_auf_leitung_wird_eingefuegt() {
    let (mut d, _) = loop_with(K::Resistor { r: 1.0 }, 0);
    d.parts.pop();
    d.normalize();
    let id = d.add_part(K::Potentiometer { r: 1000.0, pos: 0.5 }, P::new(13, -6), 0);
    d.normalize();
    let nl = netlist::build(&d);
    assert!(nl.excluded.is_empty() && nl.bridged.is_empty());
    let op = sim::operating_point(&nl).unwrap();
    close(op.parts[&id].i, U0 / 2000.0, 1e-9, "Strom durch das ganze Poti");
}

#[test]
fn taster_leitet_nur_gedrueckt() {
    let (d, id) = loop_with(K::PushButton, 0);
    let op = sim::operating_point(&netlist::build(&d)).unwrap();
    assert!(op.parts[&id].i.abs() < 1e-12);
    let op = sim::operating_point(&netlist::build_with(&d, &HashSet::from([id]))).unwrap();
    close(op.parts[&id].i, U0 / R0, 1e-7, "gedrückt");
}

#[test]
fn transistoren_aller_typen_schalten() {
    use elab_app::model::{TransistorFamily as F, TransistorType as T};
    // Lowside (NPN/N-Kanal) bzw. Highside (PNP/P-Kanal): Last 1 kΩ, Steuerung voll durch
    for t in T::ALL {
        let fam = t.family();
        let high_side = matches!(fam, F::Pnp | F::Pmos);
        let mut d = Document::default();
        d.add_part(K::DcVoltage { v: 9.0 }, P::new(0, 0), 0); // + (0,-2), − (0,2)
        d.add_part(K::Ground, P::new(0, 6), 0);
        d.add_wire(P::new(0, 2), P::new(0, 6));
        d.add_wire(P::new(0, -2), P::new(0, -8));
        let id = d.add_part(K::Transistor { kind: t }, P::new(10, 0), 0);
        let rl = d.add_part(K::Resistor { r: 1000.0 }, P::new(11, if high_side { 4 } else { -4 }), 1);
        let rb = d.add_part(K::Resistor { r: if t.is_bjt() { 10e3 } else { 1.0 } }, P::new(6, 0), 0); // (4,0)…(8,0) → Steueranschluss (8,0)
        if high_side {
            // E/S oben an +9 V, C/D unten über Last an Masse, Basis/Gate über R an Masse (aktiv tief)
            d.add_wire(P::new(0, -8), P::new(11, -8));
            d.add_wire(P::new(11, -8), P::new(11, -2));
            d.add_wire(P::new(11, 6), P::new(0, 6));
            d.add_wire(P::new(4, 0), P::new(4, 6));
        } else {
            // C/D oben über Last an +9 V, E/S unten an Masse, Basis/Gate über R an +9 V
            d.add_wire(P::new(0, -8), P::new(11, -8));
            d.add_wire(P::new(11, -8), P::new(11, -6));
            d.add_wire(P::new(11, 2), P::new(11, 6));
            d.add_wire(P::new(11, 6), P::new(0, 6));
            d.add_wire(P::new(4, 0), P::new(4, -8));
        }
        let nl = netlist::build(&d);
        assert!(nl.error.is_none() && nl.excluded.is_empty() && nl.bridged.is_empty(), "{t:?}: {:?} {:?}", nl.error, nl.excluded);
        let op = sim::operating_point(&nl).unwrap();
        let il = op.parts[&rl].i.abs();
        // Logik-MOSFETs (K = 0,5 mA/V²) haben durchgeschaltet ≈ 125 Ω, daher etwas weniger als 9 mA
        assert!(il > 7.5e-3 && il < 9.05e-3, "{t:?}: Laststrom {il}");
        assert_eq!(op.parts[&id].region, Some(sim::Region::On), "{t:?}: Schalter ein");
        let _ = rb;
    }
}
