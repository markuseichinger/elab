//! Grenzwerte: Bauteile, die zu viel Strom oder Leistung abbekommen, brennen durch.
//!
//! Wie bei echten Bauteilen zählt nicht ein winziger Spitzenwert, sondern die Erwärmung:
//! Die Belastung läuft durch einen Tiefpass mit τ = 0,1 s (Simulationszeit). Erst wenn
//! die so geglättete Belastung den Grenzwert übersteigt, ist das Bauteil hinüber. Im
//! reinen Arbeitspunkt (keine Zeit) zählt der Wert sofort.

use std::collections::HashMap;

use crate::model::{Document, Limit, PartKind};
use crate::sim::OpView;
use elab_sim::fmt_si as si;

/// Thermische Zeitkonstante in s
pub const TAU: f64 = 0.1;

/// Belastung eines Bauteils im Verhältnis zum Grenzwert (1 = genau an der Grenze) und
/// eine Erklärung, welcher Wert zu groß ist.
pub fn stress(kind: &PartKind, i: f64, p: f64) -> (f64, String) {
    let mut worst = (0.0, String::new());
    for l in kind.limits() {
        let (r, txt) = match l {
            Limit::Current(max) => (i.abs() / max, format!("{} statt höchstens {}", si(i.abs(), "A"), si(max, "A"))),
            Limit::Power(max) => (p.abs() / max, format!("{} statt höchstens {}", si(p.abs(), "W"), si(max, "W"))),
        };
        if r > worst.0 {
            worst = (r, txt);
        }
    }
    worst
}

/// Merkt sich die Erwärmung aller Bauteile.
#[derive(Default)]
pub struct Thermal {
    heat: HashMap<u32, f64>,
    last_t: Option<f64>,
}

impl Thermal {
    pub fn reset(&mut self) {
        self.heat.clear();
        self.last_t = None;
    }

    /// Erwärmung aller Bauteile (1 = Grenze erreicht).
    pub fn map(&self) -> &HashMap<u32, f64> {
        &self.heat
    }

    /// Erwärmung eines Bauteils (1 = Grenze erreicht).
    pub fn heat(&self, id: u32) -> f64 {
        self.heat.get(&id).copied().unwrap_or(0.0)
    }

    /// Nimmt den aktuellen Zustand auf. `t` ist die Simulationszeit (Echtzeit) oder `None`
    /// (Arbeitspunkt). Liefert die Bauteile, die jetzt durchbrennen, mit Begründung.
    pub fn update(&mut self, doc: &Document, view: &OpView, t: Option<f64>) -> Vec<(u32, String)> {
        let alpha = match (t, self.last_t) {
            (Some(t), Some(t0)) if t >= t0 => 1.0 - (-(t - t0) / TAU).exp(),
            // Neustart der Simulation oder erster Schritt: noch keine Erwärmung
            (Some(_), _) => 0.0,
            (None, _) => 1.0,
        };
        self.last_t = t;
        let mut burned = Vec::new();
        for part in &doc.parts {
            if part.broken {
                self.heat.remove(&part.id);
                continue;
            }
            let Some(v) = view.parts.get(&part.id) else { continue };
            let (s, why) = stress(&part.kind, v.i, v.p);
            let h = self.heat.entry(part.id).or_insert(0.0);
            *h += (s - *h) * alpha;
            if *h > 1.0 {
                burned.push((part.id, why));
            }
        }
        burned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, PartKind};
    use crate::{examples, netlist, sim};

    /// Beispiel „LED mit Vorwiderstand“ mit anderem Vorwiderstand
    fn led_circuit(r: f64) -> Document {
        let mut d = examples::all().into_iter().find(|e| e.title == "LED mit Vorwiderstand").unwrap().doc;
        for p in &mut d.parts {
            if let PartKind::Resistor { r: rr } = &mut p.kind {
                *rr = r;
            }
        }
        d
    }

    fn view(doc: &Document) -> OpView {
        let nl = netlist::build(doc);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        sim::operating_point(&nl).unwrap()
    }

    #[test]
    fn led_without_enough_resistance_burns() {
        let mut th = Thermal::default();
        let ok = led_circuit(470.0);
        assert!(th.update(&ok, &view(&ok), None).is_empty());
        let bad = led_circuit(47.0);
        let burned = th.update(&bad, &view(&bad), None);
        assert_eq!(burned.len(), 1, "{burned:?}");
        assert!(burned[0].1.contains("30 mA"), "{}", burned[0].1);
    }

    #[test]
    fn short_spikes_do_not_burn() {
        let bad = led_circuit(47.0);
        let v = view(&bad);
        let mut th = Thermal::default();
        // 10 ms Überlast bei τ = 100 ms: Erwärmung bleibt unter der Grenze
        assert!(th.update(&bad, &v, Some(0.0)).is_empty());
        assert!(th.update(&bad, &v, Some(0.01)).is_empty());
        // Dauerhaft: brennt durch
        let mut gone = false;
        for k in 2..100 {
            gone |= !th.update(&bad, &v, Some(k as f64 * 0.01)).is_empty();
        }
        assert!(gone);
    }

    #[test]
    fn broken_part_conducts_nothing() {
        let mut d = led_circuit(47.0);
        let led = d.parts.iter().find(|p| matches!(p.kind, PartKind::Led { .. })).unwrap().id;
        d.part_mut(led).unwrap().broken = true;
        let nl = netlist::build(&d);
        assert!(nl.error.is_none(), "{:?}", nl.error);
        let v = sim::operating_point(&nl).unwrap();
        let r = d.parts.iter().find(|p| matches!(p.kind, PartKind::Resistor { .. })).unwrap().id;
        assert!(v.parts[&r].i.abs() < 1e-9, "{}", v.parts[&r].i);
        // Gespeichert und wieder geladen bleibt es defekt
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert!(back.part(led).unwrap().broken);
    }
}
