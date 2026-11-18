//! Aufgaben mit automatischer Prüfung.
//!
//! Eine Aufgabe beginnt auf einem leeren Blatt mit einem vorgegebenen Bauteil (Batterie,
//! Anzeige oder Timer). Die Schritte sind Ziele, keine Bauanleitung. Geprüft wird möglichst
//! das Verhalten: Beim UND aus Schaltern werden alle vier Schalterstellungen durchgerechnet,
//! beim XOR die Wahrheitstabelle. Jede Schaltung, die das Ziel erfüllt, zählt.

use super::*;
use crate::{netlist, sim};

/// Alle Aufgaben in der Reihenfolge des Menüs.
pub fn tasks() -> Vec<Lesson> {
    vec![divider(), led_current(), switch_logic(true), switch_logic(false), xor_from_nand(), blinker_1hz()]
}

fn task(title: &str, intro: &str, outro: &str, start_kind: PartKind, steps: Vec<Step>) -> Lesson {
    Lesson { title: title.into(), intro: intro.into(), outro: outro.into(), steps, start_kind, task: true }
}

/// Minuspol der vorgegebenen Batterie an Masse.
fn ground_step() -> Step {
    Step::new("Masse", "Setz eine Masse an den Minuspol der Batterie.", "Ohne Bezugspunkt gibt es keine Spannungen. Die Masse ist 0 V.", &[1], |c| {
        c.is_gnd(c.pin(1))
    })
}

// ---------------------------------------------------------------------------
// Spannungsteiler
// ---------------------------------------------------------------------------

const DIV_LO: f64 = 3.2;
const DIV_HI: f64 = 3.4;

/// Mittelpunkte von Spannungsteilern: Netze mit einem Widerstand nach Plus und einem
/// nach Masse, mit ihrer Spannung. Der Mittelpunkt, der 3,3 V am nächsten liegt, zuerst.
fn divider_taps(c: &Ctx) -> Vec<(usize, Option<f64>)> {
    let (plus, gnd) = (c.plus(), c.gnd_nets());
    let mut taps: Vec<(usize, Option<f64>)> = (0..c.nl.nets.len())
        .filter(|n| !plus.contains(n) && !gnd.contains(n))
        .filter(|&n| {
            plus.iter().any(|&p| c.between(Some(n), Some(p), is_resistor).is_some())
                && gnd.iter().any(|&g| c.between(Some(n), Some(g), is_resistor).is_some())
        })
        .map(|n| (n, c.net_voltage(Some(n))))
        .collect();
    taps.sort_by(|a, b| {
        let d = |x: Option<f64>| x.map_or(f64::MAX, |v| (v - 3.3).abs());
        d(a.1).total_cmp(&d(b.1))
    });
    taps
}

/// Ein Voltmeter oder Messpunkt am Netz.
fn measured(c: &Ctx, n: usize) -> bool {
    c.doc.parts.iter().any(|p| {
        let pn = c.nl.pin_nets.get(&p.id);
        match p.kind {
            PartKind::Probe => pn.is_some_and(|pn| pn[0] == n),
            PartKind::Voltmeter => pn.is_some_and(|pn| pn[0] == n && c.is_gnd(Some(pn[1]))),
            _ => false,
        }
    })
}

fn divider() -> Lesson {
    let tap_text = |c: &Ctx| {
        divider_taps(c)
            .first()
            .and_then(|t| t.1)
            .map(|v| format!("U_Mitte = {}   (Ziel {} … {})", fmt_si(v, "V"), fmt_si(DIV_LO, "V"), fmt_si(DIV_HI, "V")))
    };
    let steps = vec![
        ground_step(),
        Step::new(
            "Spannungsteiler",
            "Schalte zwei Widerstände in Reihe von Plus nach Masse.",
            "Durch beide Widerstände fließt derselbe Strom. Die Spannung teilt sich im Verhältnis der Widerstände auf.",
            &[0],
            |c| !divider_taps(c).is_empty(),
        ),
        Step::new(
            "3,3 V in der Mitte",
            "Wähle die Widerstände so, dass in der Mitte 3,3 V (± 0,1 V) anliegen.",
            "U_Mitte = 9 V · R₂ / (R₁ + R₂). Für 3,3 V muss R₂ gut ein Drittel der Summe sein: R₂ / (R₁ + R₂) = 0,367. \
             Aus der E12-Reihe passen zum Beispiel R₁ = 3,3 kΩ und R₂ = 1,8 kΩ (3,18 V, knapp daneben) – probier weiter!",
            &[],
            |c| divider_taps(c).first().and_then(|t| t.1).is_some_and(|v| (DIV_LO..=DIV_HI).contains(&v)),
        )
        .live(tap_text),
        Step::new(
            "Nachmessen",
            "Miss die Spannung in der Mitte mit einem Voltmeter (Minus an Masse) oder einem Messpunkt.",
            "Ein Voltmeter hat einen sehr großen Innenwiderstand und verändert den Teiler kaum.",
            &[],
            |c| divider_taps(c).first().is_some_and(|t| measured(c, t.0)),
        ),
        Step::new(
            "Bonus: sparsam",
            "Der Teiler soll weniger als 1 mA aus der Batterie ziehen.",
            "Ein Spannungsteiler verheizt ständig Strom. Größere Widerstände sparen Energie, machen den Teiler aber empfindlicher \
             gegen Belastung: Der Laststrom sollte klein gegen den Querstrom sein.",
            &[],
            |c| c.anchor_values().is_some_and(|v| v.i.abs() < 1e-3 && v.i.abs() > 1e-7),
        )
        .live(|c| c.anchor_values().map(|v| format!("I_Batterie = {}", fmt_si(v.i.abs(), "A")))),
    ];
    task(
        "Spannungsteiler für 3,3 V",
        "Ein Sensor braucht 3,3 V, du hast nur eine 9-V-Batterie. Baue einen Spannungsteiler, der 3,3 V liefert.",
        "Geschafft! U_aus = U · R₂ / (R₁ + R₂). Für echte Schaltungen mit Last nimmt man statt eines Teilers einen Spannungsregler.",
        PartKind::DcVoltage { v: 9.0 },
        steps,
    )
}

// ---------------------------------------------------------------------------
// LED mit genau 10 mA
// ---------------------------------------------------------------------------

fn led_current() -> Lesson {
    let led_i = |c: &Ctx| -> Option<f64> {
        c.doc.parts.iter().filter(|p| is_led(&p.kind)).filter_map(|p| c.values(p.id)).map(|v| v.i).max_by(|a, b| a.total_cmp(b))
    };
    let steps = vec![
        ground_step(),
        Step::new(
            "LED leuchtet",
            "Bring eine LED mit Vorwiderstand zum Leuchten.",
            "Kathode (Strich) Richtung Minus, und nie ohne Vorwiderstand: Sonst brennt die LED durch.",
            &[],
            move |c| led_i(c).is_some_and(|i| i > 1e-3),
        )
        .hint(|c| {
            let broken = c.doc.parts.iter().any(|p| p.broken);
            broken.then(|| "Da ist etwas durchgebrannt. Ersetze es im Inspektor und überleg, welcher Widerstand nötig ist.".into())
        }),
        Step::new(
            "Genau 10 mA",
            "Stell den Vorwiderstand so ein, dass durch die LED 10 mA (± 0,5 mA) fließen.",
            "R = (U − U_F) / I. Eine rote LED hat U_F ≈ 1,8 V: R = (5 V − 1,8 V) / 10 mA = 320 Ω. Den genauen Wert findest du \
             durch Probieren, weil U_F etwas vom Strom abhängt.",
            &[],
            move |c| led_i(c).is_some_and(|i| (9.5e-3..=10.5e-3).contains(&i)),
        )
        .live(move |c| led_i(c).map(|i| format!("I_LED = {}   (Ziel 9,5 … 10,5 mA)", fmt_si(i, "A")))),
        Step::new(
            "Strom messen",
            "Miss den LED-Strom mit einem Amperemeter in Reihe.",
            "Ein Amperemeter wird in den Stromkreis eingefügt, nicht parallel geschaltet: Der Strom muss durch das Messgerät fließen.",
            &[],
            |c| {
                c.doc
                    .parts
                    .iter()
                    .any(|p| matches!(p.kind, PartKind::Ammeter) && c.values(p.id).is_some_and(|v| (9.5e-3..=10.5e-3).contains(&v.i.abs())))
            },
        ),
    ];
    task(
        "LED mit genau 10 mA",
        "An 5 V soll eine rote LED mit genau 10 mA leuchten. Berechne den Vorwiderstand und miss nach.",
        "Geschafft! Mit dem Ohmschen Gesetz R = (U − U_F) / I triffst du jeden gewünschten Strom.",
        PartKind::DcVoltage { v: 5.0 },
        steps,
    )
}

// ---------------------------------------------------------------------------
// UND und ODER aus Schaltern
// ---------------------------------------------------------------------------

/// Schalter und Taster der Schaltung, nach Namen sortiert.
fn switches(doc: &Document) -> Vec<&Part> {
    let mut v: Vec<&Part> = doc.parts.iter().filter(|p| is_switch(&p.kind)).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// Rechnet die Schaltung für alle vier Stellungen der zwei Schalter durch. Ergebnis je
/// Stellung (Bit 0 = erster, Bit 1 = zweiter Schalter): leuchtet eine LED oder Lampe?
fn switch_table(doc: &Document) -> Option<[bool; 4]> {
    let sw: Vec<u32> = switches(doc).iter().map(|p| p.id).collect();
    if sw.len() != 2 {
        return None;
    }
    let mut out = [false; 4];
    for (k, lit) in out.iter_mut().enumerate() {
        let mut d = doc.clone();
        let mut pressed = HashSet::new();
        for (j, &id) in sw.iter().enumerate() {
            let on = (k >> j) & 1 == 1;
            match &mut d.part_mut(id)?.kind {
                PartKind::Switch { closed } => *closed = on,
                _ if on => {
                    pressed.insert(id);
                }
                _ => {}
            }
        }
        let nl = netlist::build_with(&d, &pressed);
        if nl.error.is_some() {
            return None;
        }
        let v = sim::operating_point(&nl).ok()?;
        *lit = d.parts.iter().filter(|p| is_light(&p.kind) && !p.broken).any(|p| v.parts.get(&p.id).is_some_and(|x| x.i.abs() > 1e-3));
    }
    Some(out)
}

fn switch_logic(and: bool) -> Lesson {
    let want = move |k: usize| if and { k == 3 } else { k != 0 };
    let table_text = |c: &Ctx| -> Option<String> {
        let t = switch_table(c.doc)?;
        let names: Vec<String> = switches(c.doc).iter().map(|p| p.name.clone()).collect();
        let row = |k: usize| format!("{}={} {}={} → {}", names[0], k & 1, names[1], (k >> 1) & 1, if t[k] { "an" } else { "aus" });
        Some((0..4).map(row).collect::<Vec<_>>().join("\n"))
    };
    let (name, rule, how) = if and {
        ("UND", "nur leuchten, wenn beide Schalter geschlossen sind", "Schalter in Reihe: Der Strom muss durch beide hindurch.")
    } else {
        ("ODER", "leuchten, sobald mindestens ein Schalter geschlossen ist", "Schalter parallel: Jeder der beiden kann den Stromkreis schließen.")
    };
    let steps = vec![
        ground_step(),
        Step::new(
            "Zwei Schalter und eine Lampe",
            "Setz zwei Schalter (oder Taster) und eine Lampe oder LED mit Vorwiderstand.",
            "Schalter sind die einfachste Logik: geschlossen = 1, offen = 0. Die Lampe zeigt das Ergebnis.",
            &[],
            |c| switches(c.doc).len() == 2 && c.doc.parts.iter().any(|p| is_light(&p.kind)),
        )
        .hint(|c| (switches(c.doc).len() > 2).then(|| "Nimm genau zwei Schalter, sonst ist die Aufgabe nicht eindeutig.".into())),
        Step::new(
            format!("{name}-Verhalten"),
            format!("Verdrahte alles so, dass die Lampe {rule}. Geprüft werden alle vier Schalterstellungen."),
            how,
            &[],
            move |c| switch_table(c.doc).is_some_and(|t| (0..4).all(|k| t[k] == want(k))),
        )
        .live(table_text),
    ];
    task(
        &format!("{name} mit zwei Schaltern"),
        &format!("Baue eine {name}-Verknüpfung aus zwei Schaltern: Die Lampe soll {rule}."),
        if and {
            "Geschafft! Reihenschaltung = UND. Genau so sind Sicherheitsschaltungen aufgebaut: Die Maschine läuft nur, wenn alle Türen zu sind."
        } else {
            "Geschafft! Parallelschaltung = ODER. Eine Treppenhausbeleuchtung mit mehreren Tastern funktioniert so."
        },
        PartKind::DcVoltage { v: 9.0 },
        steps,
    )
}

// ---------------------------------------------------------------------------
// XOR nur aus NAND
// ---------------------------------------------------------------------------

fn nand_count(doc: &Document) -> usize {
    doc.parts.iter().filter(|p| matches!(p.kind, PartKind::Gate { gate: GateType::Nand })).count()
}

fn other_logic(doc: &Document) -> Option<&Part> {
    doc.parts.iter().find(|p| match p.kind {
        PartKind::Gate { gate } => gate != GateType::Nand,
        PartKind::GateIc { .. } | PartKind::SchmittIc { .. } | PartKind::DffIc { .. } | PartKind::CounterIc { .. } => true,
        PartKind::DFlipFlop | PartKind::JkFlipFlop | PartKind::Counter4 | PartKind::Custom { .. } => true,
        _ => false,
    })
}

fn inputs(doc: &Document) -> usize {
    doc.parts.iter().filter(|p| matches!(p.kind, PartKind::LogicInput { .. } | PartKind::Button)).count()
}

/// Wahrheitstabelle für den Ausgang am Übungsbauteil (die Logik-Anzeige): Ist es XOR?
fn is_xor(c: &Ctx) -> Option<bool> {
    let tt = sim::truth_table(c.doc, c.nl).ok()?;
    let name = &c.anchor_part()?.name;
    let col = tt.outputs.iter().position(|o| o == name)?;
    if tt.inputs.len() != 2 {
        return None;
    }
    Some(tt.rows.iter().all(|(bits, out, settled)| *settled && out[col] == Logic::from_bool(bits[0] ^ bits[1])))
}

fn xor_from_nand() -> Lesson {
    let steps = vec![
        Step::new(
            "Zwei Eingänge",
            "Setz zwei Logikschalter A und B.",
            "XOR hat zwei Eingänge: Der Ausgang ist 1, wenn genau einer der beiden 1 ist.",
            &[],
            |c| inputs(c.doc) == 2,
        ),
        Step::new(
            "Nur NAND-Gatter",
            "Verwende ausschließlich NAND-Gatter (Reiter Digital).",
            "NAND ist universell: Aus NAND-Gattern lässt sich jede andere Logikfunktion bauen. Ein NOT ist ein NAND mit \
             verbundenen Eingängen.",
            &[],
            |c| nand_count(c.doc) > 0 && other_logic(c.doc).is_none(),
        )
        .hint(|c| other_logic(c.doc).map(|p| format!("{} ist kein NAND-Gatter. Nimm es heraus.", p.name))),
        Step::new(
            "XOR-Verhalten",
            "Verdrahte die NAND-Gatter so, dass die Anzeige A ⊕ B zeigt. Die Wahrheitstabelle unten hilft beim Prüfen.",
            "Mit vier NANDs: N1 = NAND(A, B), dann N2 = NAND(A, N1) und N3 = NAND(B, N1), am Ende Y = NAND(N2, N3).",
            &[0],
            |c| is_xor(c) == Some(true),
        )
        .hint(|c| (is_xor(c) == Some(false)).then(|| "Die Wahrheitstabelle stimmt noch nicht: Bei A = B muss die Anzeige aus sein.".into())),
        Step::new(
            "Bonus: mit vier Gattern",
            "Schaffst du es mit nur vier NAND-Gattern?",
            "Der Trick: Das erste NAND(A, B) wird zweimal verwendet.",
            &[],
            |c| nand_count(c.doc) <= 4 && is_xor(c) == Some(true),
        )
        .live(|c| Some(format!("{} NAND-Gatter", nand_count(c.doc)))),
    ];
    task(
        "XOR nur aus NAND",
        "Baue ein Exklusiv-ODER (XOR) und verwende dabei nur NAND-Gatter. Die Anzeige ist schon da.",
        "Geschafft! NAND ist universell. Darum bestanden frühe Computer (und der Apollo-Bordcomputer) fast nur aus NAND-Gattern.",
        PartKind::LogicLed { color: LedColor::Gruen },
        steps,
    )
}

// ---------------------------------------------------------------------------
// 555 mit 1 Hz
// ---------------------------------------------------------------------------

/// R_A, R_B und C der astabilen 555-Schaltung am Übungsbauteil.
fn astable(c: &Ctx) -> Option<(f64, f64, f64)> {
    // Pins: TRIG 0, THR 1, CTRL 2, OUT 3, DIS 4, VCC 5, /RES 6, GND 7
    if c.pin(0) != c.pin(1) {
        return None;
    }
    let ra = c.plus().iter().find_map(|&p| c.between(Some(p), c.pin(4), is_resistor))?;
    let rb = c.between(c.pin(4), c.pin(1), is_resistor)?;
    let cap = c.gnd_nets().iter().find_map(|&g| c.between(c.pin(1), Some(g), is_capacitor))?;
    match (&ra.kind, &rb.kind, &cap.kind) {
        (PartKind::Resistor { r: a }, PartKind::Resistor { r: b }, PartKind::Capacitor { c }) => Some((*a, *b, *c)),
        _ => None,
    }
}

fn freq(c: &Ctx) -> Option<f64> {
    astable(c).map(|(a, b, cap)| 1.44 / ((a + 2.0 * b) * cap))
}

fn blinker_1hz() -> Lesson {
    let steps = vec![
        supply_step(4.5, 16.0, 9.0),
        Step::new(
            "Astabile Schaltung",
            "Verdrahte den 555 als Taktgeber: VCC und /RES an Plus, GND an Masse, TRIG und THR am Kondensator, R_A von Plus nach DIS, \
             R_B von DIS zum Kondensator.",
            "Das ist die Schaltung aus der geführten Übung „Timer 555 als Blinker“. Schau dort nach, wenn du nicht weiterkommst.",
            &[0, 1, 4, 5, 6, 7],
            |c| c.is_plus(c.pin(5)) && c.is_plus(c.pin(6)) && c.is_gnd(c.pin(7)) && astable(c).is_some(),
        ),
        Step::new(
            "LED blinkt",
            "Schließe am Ausgang eine LED mit Vorwiderstand an und bring sie zum Blinken.",
            "OUT kann etwa 200 mA treiben. Ein Vorwiderstand von 1 kΩ reicht für die LED.",
            &[3],
            |c| c.light_current() > 1e-3,
        )
        .sticky(),
        Step::new(
            "Genau 1 Hz",
            "Wähle R_A, R_B und C so, dass die LED einmal pro Sekunde blinkt (0,9 … 1,1 Hz).",
            "f = 1,44 / ((R_A + 2 · R_B) · C). Mit C = 10 µF muss R_A + 2 · R_B = 144 kΩ sein, zum Beispiel R_A = 10 kΩ und R_B = 68 kΩ.",
            &[],
            |c| freq(c).is_some_and(|f| (0.9..=1.1).contains(&f)),
        )
        .live(|c| freq(c).map(|f| format!("f = {}   (Ziel 0,9 … 1,1 Hz)", fmt_si(f, "Hz")))),
    ];
    task(
        "555-Blinker mit 1 Hz",
        "Baue mit dem Timer 555 einen Blinker, der genau einmal pro Sekunde blinkt.",
        "Geschafft! Zähl mit der Stoppuhr nach: 10 Blinker in 10 Sekunden. Die Formel gilt, weil der Kondensator zwischen ⅓ und ⅔ der \
         Versorgung pendelt, unabhängig von der Versorgungsspannung.",
        PartKind::Timer555,
        steps,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::P;

    fn state(task: &Lesson, doc: &Document, anchor: u32) -> (Vec<bool>, Option<usize>) {
        let nl = netlist::build(doc);
        let op = if nl.mixed.is_some() || sim::realtime(&nl) {
            let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
            // Blinker: einen Moment mit leuchtender LED suchen
            let mut v = sim::mixed_view(&nl, &ms);
            for _ in 0..150 {
                if doc.parts.iter().any(|p| is_light(&p.kind) && v.parts.get(&p.id).is_some_and(|x| x.i.abs() > 2e-3)) {
                    break;
                }
                ms.advance(ms.time() + 0.02, None).unwrap();
                v = sim::mixed_view(&nl, &ms);
            }
            Some(v)
        } else {
            sim::operating_point(&nl).ok()
        };
        let ctx = Ctx { doc, nl: &nl, op: op.as_ref(), anchor, scope_open: false };
        Progress::default().update(task, &ctx)
    }

    /// Baut auf dem Startblatt: Batterie bei (0,0), Pins (0,-2) und (0,2).
    fn start(t: &Lesson) -> (Document, u32) {
        let mut d = Document::default();
        let id = d.add_part(t.start_kind.clone(), P::new(0, 0), 0);
        (d, id)
    }

    fn find(title: &str) -> Lesson {
        tasks().into_iter().find(|t| t.title.starts_with(title)).unwrap()
    }

    #[test]
    fn empty_tasks_start_at_first_step() {
        for t in tasks() {
            assert!(t.task);
            let (d, id) = start(&t);
            let (_, cur) = state(&t, &d, id);
            assert_eq!(cur, Some(0), "{}", t.title);
        }
    }

    #[test]
    fn divider_task() {
        let t = find("Spannungsteiler");
        let (mut d, b) = start(&t);
        d.add_part(PartKind::Ground, P::new(0, 4), 0);
        d.add_wire(P::new(0, 2), P::new(0, 4));
        // R1 waagerecht von (0,-2) nach (4,-2)? Widerstände liegen bei pos ± 2
        let r1 = d.add_part(PartKind::Resistor { r: 5.6e3 }, P::new(4, -4), 1);
        let r2 = d.add_part(PartKind::Resistor { r: 3.3e3 }, P::new(4, 2), 1);
        let pins = |d: &Document, id: u32| d.part(id).unwrap().pins();
        let (a1, b1) = (pins(&d, r1)[0], pins(&d, r1)[1]);
        let (a2, b2) = (pins(&d, r2)[0], pins(&d, r2)[1]);
        d.add_wire(P::new(0, -2), P::new(0, -6));
        d.add_wire(P::new(0, -6), a1);
        d.add_wire(b1, a2);
        d.add_wire(b2, P::new(0, 4));
        let (done, cur) = state(&t, &d, b);
        // 9 V · 3,3 / 8,9 = 3,34 V: Teiler steht, Messen fehlt
        assert_eq!(&done[..3], &[true, true, true], "{done:?} {cur:?}");
        assert_eq!(cur, Some(3));
        d.add_part(PartKind::Probe, b1, 0);
        let (done, cur) = state(&t, &d, b);
        assert!(done[3], "{done:?}");
        // 8,9 kΩ: ca. 1 mA → Bonus knapp verfehlt
        assert_eq!(cur, Some(4));
    }

    #[test]
    fn and_or_by_behaviour() {
        for and in [true, false] {
            let t = find(if and { "UND" } else { "ODER" });
            let (mut d, b) = start(&t);
            d.add_part(PartKind::Ground, P::new(0, 4), 0);
            d.add_wire(P::new(0, 2), P::new(0, 4));
            // Lampe senkrecht bei x = 12 zwischen (12,-2) und (12,2)
            let lamp = d.add_part(PartKind::Lamp { v_nom: 9.0, p_nom: 1.0 }, P::new(12, 0), 1);
            let lp = d.part(lamp).unwrap().pins();
            d.add_wire(lp[1], P::new(12, 4));
            d.add_wire(P::new(12, 4), P::new(0, 4));
            let s1 = d.add_part(PartKind::Switch { closed: false }, P::new(4, -6), 0);
            let s2 = d.add_part(PartKind::Switch { closed: false }, P::new(8, -6), 0);
            let (p1, p2) = (d.part(s1).unwrap().pins(), d.part(s2).unwrap().pins());
            d.add_wire(P::new(0, -2), P::new(0, -6));
            d.add_wire(P::new(0, -6), p1[0]);
            // Reihe: S1 → S2 → Lampe. Parallel stimmt dann nicht.
            d.add_wire(p1[1], p2[0]);
            d.add_wire(p2[1], P::new(12, -6));
            d.add_wire(P::new(12, -6), lp[0]);
            let (done, _) = state(&t, &d, b);
            assert_eq!(done[2], and, "Reihenschaltung als {}: {done:?} {:?}", t.title, switch_table(&d));
        }
    }

    #[test]
    fn xor_from_four_nands() {
        let t = find("XOR");
        let mut d = Document::default();
        let y = d.add_part(PartKind::LogicLed { color: LedColor::Gruen }, P::new(20, 0), 0);
        let a = d.add_part(PartKind::LogicInput { on: false }, P::new(0, -4), 0);
        let bb = d.add_part(PartKind::LogicInput { on: false }, P::new(0, 4), 0);
        let g: Vec<u32> = [(6, 0), (11, -4), (11, 4), (16, 0)]
            .iter()
            .map(|&(x, yy)| d.add_part(PartKind::Gate { gate: GateType::Nand }, P::new(x, yy), 0))
            .collect();
        let pin = |d: &Document, id: u32, k: usize| d.part(id).unwrap().pins()[k];
        let wires = [
            (pin(&d, a, 0), pin(&d, g[0], 0)),
            (pin(&d, bb, 0), pin(&d, g[0], 1)),
            (pin(&d, a, 0), pin(&d, g[1], 0)),
            (pin(&d, g[0], 2), pin(&d, g[1], 1)),
            (pin(&d, g[0], 2), pin(&d, g[2], 0)),
            (pin(&d, bb, 0), pin(&d, g[2], 1)),
            (pin(&d, g[1], 2), pin(&d, g[3], 0)),
            (pin(&d, g[2], 2), pin(&d, g[3], 1)),
            (pin(&d, g[3], 2), pin(&d, y, 0)),
        ];
        for (p, q) in wires {
            d.add_wire(p, q);
        }
        let (done, cur) = state(&t, &d, y);
        assert_eq!(done, vec![true; 4], "{cur:?}");
    }

    #[test]
    fn blinker_task_on_example() {
        let t = find("555");
        let mut d = crate::examples::all().into_iter().find(|e| e.title == "555-Blinker").unwrap().doc;
        let u1 = d.part_by_name("U1").unwrap().id;
        // Beispiel: 1,44 / (104 kΩ · 10 µF) ≈ 1,4 Hz → letzter Schritt offen
        let (done, cur) = state(&t, &d, u1);
        assert_eq!(cur, Some(3), "{done:?}");
        let rb = d.part_by_name("R_B").unwrap().id;
        d.part_mut(rb).unwrap().kind = PartKind::Resistor { r: 68e3 };
        let (done, cur) = state(&t, &d, u1);
        assert_eq!(cur, None, "{done:?}");
    }
}
