//! Stromfluss auf den Leitungen.
//!
//! Die Simulation kennt nur Ströme in Bauteile hinein. Wie sie sich auf die einzelnen
//! Leitungsstücke verteilen, folgt aus der Knotenregel: Je Netz entsteht ein Graph aus
//! Rasterpunkten (Drahtenden, Abzweige, Anschlüsse) und Leitungsstücken. Auf einem
//! aufspannenden Baum ist der Strom durch jedes Stück die Summe der Einspeisungen auf
//! seiner einen Seite. Ideale Drahtschleifen tragen dabei keinen Strom (Masche aus
//! Drähten ohne Widerstand ist ohnehin unbestimmt).

use crate::model::{Document, PartKind, P};
use crate::netlist::Netlist;
use crate::sim::OpView;
use std::collections::{HashMap, HashSet};

/// Ein Leitungsstück mit Strom: positiv heißt Fluss von `.0` nach `.1` (technische Stromrichtung).
pub type FlowSeg = (P, P, f64);

/// Ströme auf allen Leitungsstücken des Schaltplans.
pub fn wire_flow(doc: &Document, nl: &Netlist, op: &OpView) -> Vec<FlowSeg> {
    let mut out = Vec::new();
    // Einspeisung ins Netz je Rasterpunkt; Senken nehmen den Rest auf (Masse, unbekannte Bauteile)
    let mut inj: HashMap<P, f64> = HashMap::new();
    let mut sink: HashSet<P> = HashSet::new();
    for part in &doc.parts {
        let pins = part.pins();
        match (op.pin_i.get(&part.id), &part.kind) {
            (Some(ii), _) => {
                for (k, p) in pins.iter().enumerate() {
                    *inj.entry(*p).or_default() -= ii.get(k).copied().unwrap_or(0.0);
                }
            }
            (None, PartKind::Probe | PartKind::Voltmeter) => {}
            _ => sink.extend(pins),
        }
    }
    // Netze einzeln
    let mut by_net: HashMap<usize, Vec<usize>> = HashMap::new();
    for (wi, w) in doc.wires.iter().enumerate() {
        if let Some(&n) = nl.wire_net.get(&w.id) {
            by_net.entry(n).or_default().push(wi);
        }
    }
    for wires in by_net.values() {
        let mut pts: HashSet<P> = HashSet::new();
        for &wi in wires {
            pts.insert(doc.wires[wi].a);
            pts.insert(doc.wires[wi].b);
        }
        for p in inj.keys().chain(sink.iter()) {
            if wires.iter().any(|&wi| doc.wires[wi].contains(*p)) {
                pts.insert(*p);
            }
        }
        // Leitungsstücke zwischen benachbarten Punkten
        let mut adj: HashMap<P, Vec<P>> = HashMap::new();
        for &wi in wires {
            let w = &doc.wires[wi];
            let mut on: Vec<P> = pts.iter().copied().filter(|p| w.contains(*p)).collect();
            on.sort_by_key(|p| (p.x - w.a.x).abs() + (p.y - w.a.y).abs());
            for pair in on.windows(2) {
                if pair[0] != pair[1] {
                    adj.entry(pair[0]).or_default().push(pair[1]);
                    adj.entry(pair[1]).or_default().push(pair[0]);
                }
            }
        }
        // Inseln: Baum per Breitensuche, dann Teilsummen von den Blättern her
        let mut seen: HashSet<P> = HashSet::new();
        let mut starts: Vec<P> = adj.keys().copied().collect();
        starts.sort_by_key(|p| (p.x, p.y));
        for root in starts {
            if seen.contains(&root) {
                continue;
            }
            let mut order = vec![root];
            let mut parent: HashMap<P, P> = HashMap::new();
            seen.insert(root);
            let mut i = 0;
            while i < order.len() {
                let p = order[i];
                i += 1;
                for &q in adj.get(&p).into_iter().flatten() {
                    if seen.insert(q) {
                        parent.insert(q, p);
                        order.push(q);
                    }
                }
            }
            let mut sum: HashMap<P, f64> = order.iter().map(|p| (*p, inj.get(p).copied().unwrap_or(0.0))).collect();
            let residual: f64 = sum.values().sum();
            let sinks: Vec<P> = order.iter().copied().filter(|p| sink.contains(p)).collect();
            let scale = sum.values().fold(0.0_f64, |m, v| m.max(v.abs()));
            if !sinks.is_empty() {
                for s in &sinks {
                    *sum.get_mut(s).unwrap() -= residual / sinks.len() as f64;
                }
            } else if residual.abs() > 1e-9 + 1e-3 * scale {
                continue; // Bilanz geht nicht auf: lieber nichts zeigen als Falsches
            }
            for &p in order.iter().rev() {
                if let Some(&par) = parent.get(&p) {
                    let s = sum[&p];
                    out.push((p, par, s));
                    *sum.get_mut(&par).unwrap() += s;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{examples, netlist, sim};

    fn flows(title: &str) -> (Document, Vec<FlowSeg>, sim::OpView) {
        let ex = examples::all().into_iter().find(|e| e.title.starts_with(title)).unwrap();
        let nl = netlist::build(&ex.doc);
        let op = sim::operating_point(&nl).unwrap();
        let f = wire_flow(&ex.doc, &nl, &op);
        (ex.doc, f, op)
    }

    #[test]
    fn series_loop_carries_led_current_everywhere() {
        let (doc, f, op) = flows("LED mit Vorwiderstand");
        let i = op.parts[&doc.part_by_name("LED1").unwrap().id].i;
        assert!(i > 1e-3);
        assert!(!f.is_empty());
        for (a, b, x) in &f {
            assert!((x.abs() - i).abs() < 1e-6, "{a:?}→{b:?}: {x} statt {i}");
        }
    }

    #[test]
    fn parallel_branches_add_up() {
        let (doc, f, op) = flows("Glühlampen parallel");
        let total: f64 = doc.parts.iter().filter(|p| matches!(p.kind, PartKind::Lamp { .. })).map(|p| op.parts[&p.id].i.abs()).sum();
        let max = f.iter().map(|s| s.2.abs()).fold(0.0, f64::max);
        // Zuleitung trägt die Summe der Lampenströme
        assert!((max - total).abs() < 1e-6, "{max} vs {total}");
        // Knotenregel an jedem Punkt: Zu- und Abflüsse plus Einspeisung sind null
        let mut bal: HashMap<P, f64> = HashMap::new();
        for (a, b, x) in &f {
            *bal.entry(*a).or_default() -= x;
            *bal.entry(*b).or_default() += x;
        }
        let pins: HashMap<P, f64> = doc
            .parts
            .iter()
            .filter_map(|p| op.pin_i.get(&p.id).map(|ii| (p, ii)))
            .flat_map(|(p, ii)| p.pins().into_iter().zip(ii.iter().copied()).collect::<Vec<_>>())
            .fold(HashMap::new(), |mut m, (q, i)| {
                *m.entry(q).or_default() += i;
                m
            });
        for (p, b) in bal {
            let into_parts = pins.get(&p).copied().unwrap_or(0.0);
            let gnd = doc.parts.iter().any(|q| matches!(q.kind, PartKind::Ground) && q.pos == p);
            if !gnd {
                assert!((b - into_parts).abs() < 1e-6, "{p:?}: {b} vs {into_parts}");
            }
        }
    }

    #[test]
    fn transistor_terminals() {
        let (doc, f, op) = flows("Transistor als Schalter");
        let t = doc.part_by_name("T1").unwrap();
        let ii = &op.pin_i[&t.id];
        // B, C, E: Summe null
        assert!((ii[0] + ii[1] + ii[2]).abs() < 1e-9, "{ii:?}");
        assert!(!f.is_empty());
    }
}
