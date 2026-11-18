//! Bearbeiten: Drähte verschieben, Kopieren und Einfügen.

use elab_app::model::{Document, PartKind, P};
use elab_app::{examples, netlist, sim};
use std::collections::HashSet;

fn example(title: &str) -> Document {
    examples::all().into_iter().find(|e| e.title.starts_with(title)).unwrap().doc
}

fn led_current(d: &Document) -> f64 {
    let nl = netlist::build(d);
    assert!(nl.error.is_none(), "{:?}", nl.error);
    assert!(nl.excluded.is_empty(), "abgehängt: {:?}", nl.excluded);
    let v = sim::operating_point(&nl).unwrap();
    let led = d.parts.iter().find(|p| matches!(p.kind, PartKind::Led { .. })).unwrap().id;
    v.parts[&led].i
}

/// Alle Drähte, die ganz auf der Zeile y liegen.
fn wires_on_row(d: &Document, y: i32) -> HashSet<u32> {
    d.wires.iter().filter(|w| w.a.y == y && w.b.y == y).map(|w| w.id).collect()
}

#[test]
fn moving_a_wire_keeps_its_connections() {
    let d0 = example("LED mit Vorwiderstand");
    let i0 = led_current(&d0);
    // Untere Leitung (y = 6) nach unten verschieben: Enden an LED, Batterie und Masse bleiben verbunden
    for delta in [P::new(0, 2), P::new(0, -1), P::new(3, 2)] {
        let mut d = d0.clone();
        let wires = wires_on_row(&d, 6);
        assert!(!wires.is_empty());
        d.transform_parts(&HashSet::new(), &wires, delta, |_| {});
        let i = led_current(&d);
        assert!((i - i0).abs() < 1e-9, "Verschiebung {delta:?}: I_LED {i} statt {i0}");
    }
}

#[test]
fn moving_a_rail_with_branches_keeps_all_branches() {
    // Spannungsteiler: obere Schiene mit Abzweigen
    let d0 = example("Spannungsteiler");
    let nl0 = netlist::build(&d0);
    let n0 = nl0.nets.len();
    let ys: HashSet<i32> = d0.wires.iter().filter(|w| w.a.y == w.b.y).map(|w| w.a.y).collect();
    for y in ys {
        let mut d = d0.clone();
        let wires = wires_on_row(&d, y);
        d.transform_parts(&HashSet::new(), &wires, P::new(0, -2), |_| {});
        let nl = netlist::build(&d);
        assert!(nl.error.is_none() && nl.excluded.is_empty(), "Zeile {y}: {:?} {:?}", nl.error, nl.excluded);
        assert_eq!(nl.nets.len(), n0, "Zeile {y}: Netze geändert");
    }
}

#[test]
fn copy_paste_duplicates_with_new_names() {
    let mut d = example("LED mit Vorwiderstand");
    let parts: HashSet<u32> = d.parts.iter().map(|p| p.id).collect();
    let wires: HashSet<u32> = d.wires.iter().map(|w| w.id).collect();
    let clip = d.copy(&parts, &wires);
    let text = clip.to_text();
    let back = elab_app::model::Clip::from_text(&text).unwrap();
    let n = d.parts.len();
    let (np, nw) = d.paste(&back, P::new(30, 0));
    assert_eq!(np.len(), n);
    assert!(!nw.is_empty());
    let names: HashSet<&str> = d.parts.iter().map(|p| p.name.as_str()).collect();
    // Masse heißt immer GND, alle anderen Namen sind eindeutig
    assert_eq!(names.len(), d.parts.len() - d.parts.iter().filter(|p| matches!(p.kind, PartKind::Ground)).count() + 1);
    // Beide Kopien funktionieren gleich
    let nl = netlist::build(&d);
    assert!(nl.error.is_none() && nl.excluded.is_empty(), "{:?} {:?}", nl.error, nl.excluded);
    let v = sim::operating_point(&nl).unwrap();
    let leds: Vec<f64> = d.parts.iter().filter(|p| matches!(p.kind, PartKind::Led { .. })).map(|p| v.parts[&p.id].i).collect();
    assert_eq!(leds.len(), 2);
    assert!((leds[0] - leds[1]).abs() < 1e-9 && leds[0] > 1e-3, "{leds:?}");
}

#[test]
fn ideal_counter_replaced_by_real_ic_keeps_its_connections() {
    let mut d = example("Zähler mit 7");
    let ctr = d.parts.iter().find(|p| matches!(p.kind, PartKind::Counter4)).unwrap().id;
    let nl0 = netlist::build(&d);
    // Netze der Ausgänge Q0..Q3 und des Takts: Wer hängt daran?
    let peers = |d: &Document, nl: &netlist::Netlist, k: usize| -> Vec<String> {
        let n = nl.pin_nets[&ctr][k];
        let mut v: Vec<String> =
            d.parts.iter().filter(|p| p.id != ctr && nl.pin_nets.get(&p.id).is_some_and(|pn| pn.contains(&n))).map(|p| p.name.clone()).collect();
        v.sort();
        v
    };
    let before: Vec<Vec<String>> = [0, 2, 3, 4, 5].iter().map(|&k| peers(&d, &nl0, k)).collect();
    let real = elab_app::datasheet::real_counterpart(&d.part(ctr).unwrap().kind).unwrap();
    d.replace_kind(ctr, real);
    assert!(d.part(ctr).unwrap().name.starts_with('U'));
    let nl = netlist::build(&d);
    let after: Vec<Vec<String>> = [0, 2, 3, 4, 5].iter().map(|&k| peers(&d, &nl, k)).collect();
    assert_eq!(before, after);
}
