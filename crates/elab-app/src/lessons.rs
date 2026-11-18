//! Geführte Übungen („Bauteil-Lehrgänge“).
//!
//! Zu komplexeren Bauteilen gibt es eine Übung, die Schritt für Schritt zeigt, wie das
//! Bauteil beschaltet wird. Jeder Schritt prüft selbst, ob er erledigt ist: Die Prüfungen
//! schauen auf die Netzliste (was ist womit verbunden?) und auf die Simulation (leuchtet
//! die LED, schaltet der Transistor durch?). Typische Fehler (LED verdreht, Basis ohne
//! Widerstand, VCC und GND vertauscht) werden als Hinweis erkannt.
//!
//! Strukturschritte gelten, solange die Schaltung sie erfüllt. Löscht man später eine
//! Leitung, springt die Übung dorthin zurück. Aktionsschritte („Taster drücken“) bleiben
//! erledigt, sobald sie einmal geklappt haben.

use crate::model::{Document, GateType, LedColor, OpAmpType, Part, PartKind, TransistorFamily, TransistorType};
use crate::netlist::Netlist;
use crate::sim::{OpView, PartValues, Region};
use elab_sim::digital::Logic;
use elab_sim::fmt_si;
use std::collections::HashSet;

/// Alles, was eine Prüfung über den aktuellen Stand wissen darf.
pub struct Ctx<'a> {
    pub doc: &'a Document,
    pub nl: &'a Netlist,
    pub op: Option<&'a OpView>,
    /// Das Bauteil, um das es in der Übung geht
    pub anchor: u32,
    /// Oszilloskop ist sichtbar
    pub scope_open: bool,
}

type Pred = fn(&PartKind) -> bool;

fn is_resistor(k: &PartKind) -> bool {
    matches!(k, PartKind::Resistor { .. })
}
fn is_light(k: &PartKind) -> bool {
    matches!(k, PartKind::Led { .. } | PartKind::Lamp { .. })
}
fn is_led(k: &PartKind) -> bool {
    matches!(k, PartKind::Led { .. })
}
fn is_switch(k: &PartKind) -> bool {
    matches!(k, PartKind::Switch { .. } | PartKind::PushButton)
}
fn is_capacitor(k: &PartKind) -> bool {
    matches!(k, PartKind::Capacitor { .. })
}
fn load_or_resistor(k: &PartKind) -> bool {
    is_resistor(k) || is_light(k)
}
fn resistor_or_switch(k: &PartKind) -> bool {
    is_resistor(k) || is_switch(k)
}

impl Ctx<'_> {
    pub fn anchor_part(&self) -> Option<&Part> {
        self.doc.part(self.anchor)
    }

    fn net(&self, id: u32, k: usize) -> Option<usize> {
        self.nl.pin_nets.get(&id)?.get(k).copied()
    }

    /// Netz eines Anschlusses des Übungsbauteils.
    pub fn pin(&self, k: usize) -> Option<usize> {
        self.net(self.anchor, k)
    }

    pub fn is_gnd(&self, n: Option<usize>) -> bool {
        n.is_some_and(|n| self.nl.nets.get(n).is_some_and(|x| x.is_gnd))
    }

    /// Quellen mit Minuspol an Masse: (Bauteil, Netz des Pluspols, Spannung).
    pub fn supplies(&self) -> Vec<(&Part, usize, f64)> {
        self.doc
            .parts
            .iter()
            .filter_map(|p| {
                let v = match p.kind {
                    PartKind::DcVoltage { v } => v,
                    PartKind::SquareVoltage { high, .. } => high,
                    _ => return None,
                };
                let (plus, minus) = (self.net(p.id, 0)?, self.net(p.id, 1));
                if !self.is_gnd(minus) {
                    return None;
                }
                Some((p, plus, v))
            })
            .collect()
    }

    /// Netze, die am Pluspol einer Gleichspannungsquelle hängen.
    pub fn plus(&self) -> Vec<usize> {
        self.supplies().iter().filter(|s| matches!(s.0.kind, PartKind::DcVoltage { .. }) && s.2 > 0.0).map(|s| s.1).collect()
    }

    pub fn is_plus(&self, n: Option<usize>) -> bool {
        n.is_some_and(|n| self.plus().contains(&n))
    }

    fn gnd_nets(&self) -> Vec<usize> {
        self.nl.nets.iter().enumerate().filter(|(_, n)| n.is_gnd).map(|(i, _)| i).collect()
    }

    /// Ein Zweipol der Art `pred` direkt zwischen zwei Netzen.
    pub fn between(&self, a: Option<usize>, b: Option<usize>, pred: Pred) -> Option<&Part> {
        let (a, b) = (a?, b?);
        self.doc.parts.iter().filter(|p| p.id != self.anchor && pred(&p.kind)).find(|p| {
            let pn = self.nl.pin_nets.get(&p.id);
            pn.is_some_and(|pn| pn.len() == 2 && ((pn[0] == a && pn[1] == b) || (pn[0] == b && pn[1] == a)))
        })
    }

    /// Hängt an einem Netz ein Bauteil der Art `pred` (außer dem Übungsbauteil)?
    pub fn on_net(&self, n: Option<usize>, pred: Pred) -> bool {
        let Some(n) = n else { return false };
        self.doc.parts.iter().any(|p| p.id != self.anchor && pred(&p.kind) && self.nl.pin_nets.get(&p.id).is_some_and(|pn| pn.contains(&n)))
    }

    /// Weg von `from` zu einem der Netze `to` über Zweipole, die `pass` erfüllen
    /// (Amperemeter zählen wie Draht), mit mindestens einem Bauteil, das `must` erfüllt.
    pub fn path(&self, from: Option<usize>, to: &[usize], pass: Pred, must: Pred) -> bool {
        let Some(from) = from else { return false };
        let edges: Vec<(usize, usize, bool)> = self
            .doc
            .parts
            .iter()
            .filter(|p| p.id != self.anchor)
            .filter(|p| pass(&p.kind) || matches!(p.kind, PartKind::Ammeter))
            .filter_map(|p| {
                let pn = self.nl.pin_nets.get(&p.id)?;
                (pn.len() == 2 && pn[0] != pn[1]).then(|| (pn[0], pn[1], must(&p.kind)))
            })
            .collect();
        // Tiefensuche über einfache Wege (kein Netz zweimal): Hin und zurück über
        // dasselbe Bauteil zählt nicht als Verbindung.
        fn dfs(n: usize, ok: bool, to: &[usize], edges: &[(usize, usize, bool)], seen: &mut Vec<usize>, budget: &mut usize) -> bool {
            if ok && to.contains(&n) {
                return true;
            }
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            for &(a, b, m) in edges {
                let other = if a == n {
                    b
                } else if b == n {
                    a
                } else {
                    continue;
                };
                if seen.contains(&other) {
                    continue;
                }
                seen.push(other);
                if dfs(other, ok || m, to, edges, seen, budget) {
                    return true;
                }
                seen.pop();
            }
            false
        }
        let mut budget = 20_000;
        dfs(from, false, to, &edges, &mut vec![from], &mut budget)
    }

    pub fn values(&self, id: u32) -> Option<&PartValues> {
        self.op?.parts.get(&id)
    }

    pub fn anchor_values(&self) -> Option<&PartValues> {
        self.values(self.anchor)
    }

    /// Größter LED-/Lampenstrom in der Schaltung.
    pub fn light_current(&self) -> f64 {
        self.doc.parts.iter().filter(|p| is_light(&p.kind)).filter_map(|p| self.values(p.id)).map(|v| v.i.abs()).fold(0.0, f64::max)
    }

    fn net_voltage(&self, n: Option<usize>) -> Option<f64> {
        self.op?.net_v.get(n?).copied().flatten()
    }
}

/// Liefert zum aktuellen Stand einen Text (Hinweis oder Messwerte).
pub type TextFn = Box<dyn Fn(&Ctx) -> Option<String>>;

/// Ein Schritt einer Übung.
pub struct Step {
    pub title: String,
    pub text: String,
    /// Warum man das so macht
    pub why: String,
    /// Anschlüsse des Übungsbauteils, die im Schaltplan aufleuchten
    pub pins: Vec<usize>,
    pub check: Box<dyn Fn(&Ctx) -> bool>,
    /// Erkennt typische Fehler
    pub hint: TextFn,
    /// Messwerte zum Schritt
    pub live: TextFn,
    /// Einmal erreicht, bleibt der Schritt erledigt (Aktionen wie „Taster drücken“)
    pub sticky: bool,
}

impl Step {
    fn new(
        title: impl Into<String>,
        text: impl Into<String>,
        why: impl Into<String>,
        pins: &[usize],
        check: impl Fn(&Ctx) -> bool + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            why: why.into(),
            pins: pins.to_vec(),
            check: Box::new(check),
            hint: Box::new(|_| None),
            live: Box::new(|_| None),
            sticky: false,
        }
    }
    fn hint(mut self, f: impl Fn(&Ctx) -> Option<String> + 'static) -> Self {
        self.hint = Box::new(f);
        self
    }
    fn live(mut self, f: impl Fn(&Ctx) -> Option<String> + 'static) -> Self {
        self.live = Box::new(f);
        self
    }
    fn sticky(mut self) -> Self {
        self.sticky = true;
        self
    }
}

/// Eine Übung zu einem Bauteil.
pub struct Lesson {
    pub title: String,
    pub intro: String,
    pub outro: String,
    pub steps: Vec<Step>,
    /// Bauteil, das beim Start auf leerem Blatt gesetzt wird
    pub start_kind: PartKind,
    /// Aufgabe mit Prüfung statt geführter Übung: Startet immer auf leerem Blatt, die
    /// Schritte sind Ziele, Erklärungen gibt es erst auf Nachfrage als Tipp.
    pub task: bool,
}

/// Fortschritt einer laufenden Übung.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// Aktionsschritte bis hierhin sind erledigt
    pub reached: usize,
    /// Übersprungene Schritte
    pub skipped: HashSet<usize>,
}

impl Progress {
    /// Prüft alle Schritte. Ergebnis: erledigt je Schritt, aktueller Schritt (None = fertig).
    pub fn update(&mut self, lesson: &Lesson, ctx: &Ctx) -> (Vec<bool>, Option<usize>) {
        let mut done = Vec::with_capacity(lesson.steps.len());
        let mut current = None;
        for (i, s) in lesson.steps.iter().enumerate() {
            let ok = if self.skipped.contains(&i) {
                true
            } else if s.sticky {
                if i < self.reached {
                    true
                } else if current.is_none() && (s.check)(ctx) {
                    self.reached = i + 1;
                    true
                } else {
                    false
                }
            } else {
                (s.check)(ctx)
            };
            if !ok && current.is_none() {
                current = Some(i);
            }
            done.push(ok);
        }
        (done, current)
    }
}

/// Gibt es zu diesem Bauteil eine Übung?
pub fn has_lesson(kind: &PartKind) -> bool {
    lesson_for(kind).is_some()
}

/// Übung für ein Bauteil (Varianten wie NPN/PNP ergeben angepasste Texte und Prüfungen).
pub fn lesson_for(kind: &PartKind) -> Option<Lesson> {
    Some(match kind {
        PartKind::Led { .. } => led(),
        PartKind::Capacitor { .. } => capacitor(),
        PartKind::Transistor { kind } if kind.is_bjt() => bjt(*kind),
        PartKind::Transistor { kind } => mosfet(*kind),
        PartKind::GateIc { gate, .. } => gate_ic(*gate),
        PartKind::Timer555 => timer555(),
        PartKind::OpAmp { kind } => opamp(*kind),
        _ => return None,
    })
}

/// Alle Übungen (für das Menü), je mit dem Bauteil, mit dem sie beginnen.
pub fn all() -> Vec<Lesson> {
    [
        PartKind::Led { color: LedColor::Rot },
        PartKind::Capacitor { c: 100e-9 },
        PartKind::Transistor { kind: TransistorType::Bc547 },
        PartKind::Transistor { kind: TransistorType::Bc557 },
        PartKind::Transistor { kind: TransistorType::N2n7000 },
        PartKind::GateIc { gate: GateType::Nand, family: crate::model::LogicFamily::Hc },
        PartKind::Timer555,
        PartKind::OpAmp { kind: OpAmpType::Lm358 },
    ]
    .iter()
    .filter_map(lesson_for)
    .collect()
}

mod tasks;
pub use tasks::tasks;

// ---------------------------------------------------------------------------
// Bausteine für Schritte
// ---------------------------------------------------------------------------

/// Gleichspannungsquelle im Bereich `lo … hi` mit Masse am Minuspol.
fn supply_step(lo: f64, hi: f64, suggest: f64) -> Step {
    Step::new(
        "Spannungsquelle und Masse",
        format!(
            "Zieh eine Batterie (Gleichspannung) in den Schaltplan, stell sie auf {} (Inspektor) und setz eine Masse an ihren Minuspol.",
            fmt_si(suggest, "V")
        ),
        "Die Masse ist der Bezugspunkt 0 V. Alle Spannungen in elab werden gegen sie gemessen.",
        &[],
        move |c| c.supplies().iter().any(|s| matches!(s.0.kind, PartKind::DcVoltage { .. }) && s.2 >= lo && s.2 <= hi),
    )
    .hint(move |c| {
        let any = c.doc.parts.iter().any(|p| matches!(p.kind, PartKind::DcVoltage { .. }));
        if any && c.supplies().is_empty() {
            return Some("Der Minuspol der Batterie hängt noch nicht an der Masse.".into());
        }
        c.supplies()
            .iter()
            .find(|s| s.2 < lo || s.2 > hi)
            .map(|s| format!("{} hat {}. Für diese Übung passen {} … {}.", s.0.name, fmt_si(s.2, "V"), fmt_si(lo, "V"), fmt_si(hi, "V")))
    })
}

fn pct(x: f64) -> String {
    format!("{:.0} %", x * 100.0)
}

// ---------------------------------------------------------------------------
// LED
// ---------------------------------------------------------------------------

fn led() -> Lesson {
    let steps = vec![
        supply_step(3.0, 24.0, 9.0),
        Step::new(
            "Kathode an Masse",
            "Die Kathode ist die Seite mit dem Strich im Symbol. Sie zeigt Richtung Minus: Verbinde sie mit der Masse.",
            "Eine LED leitet nur in einer Richtung, von der Anode zur Kathode. Verkehrt herum bleibt sie dunkel.",
            &[1],
            |c| c.is_gnd(c.pin(1)),
        )
        .hint(|c| c.is_gnd(c.pin(0)).then(|| "Die LED steckt verkehrt herum: Die Anode hängt an Masse. Dreh sie mit R zweimal.".into())),
        Step::new(
            "Vorwiderstand zur Anode",
            "Verbinde die Anode über einen Widerstand mit dem Pluspol. Für 9 V und etwa 20 mA: R = (9 V − 2 V) / 20 mA ≈ 350 Ω, nimm 330 Ω oder 470 Ω.",
            "Ohne Widerstand würde der Strom nur von den Leitungen begrenzt – die LED brennt durch. Der Widerstand nimmt die überschüssige Spannung auf.",
            &[0],
            |c| c.path(c.pin(0), &c.plus(), |k| is_resistor(k) || is_switch(k), is_resistor),
        )
        .hint(|c| c.is_plus(c.pin(0)).then(|| "Die Anode hängt direkt am Pluspol: Hier fehlt der Vorwiderstand!".into())),
        Step::new(
            "Leuchten lassen",
            "Ist der Stromkreis geschlossen, leuchtet die LED. Schließe einen Schalter, falls einer im Weg ist (Doppelklick).",
            "Die LED nimmt sich ihre Flussspannung (rot ≈ 1,8 V, blau ≈ 3 V). Der Rest fällt am Widerstand ab: I = (U − U_F) / R.",
            &[],
            |c| c.anchor_values().is_some_and(|v| v.i > 1e-3),
        )
        .live(|c| c.anchor_values().map(|v| format!("I_LED = {}   U_LED = {}", fmt_si(v.i, "A"), fmt_si(v.u, "V"))))
        .sticky(),
        Step::new(
            "Strom ändern",
            "Stell den Vorwiderstand im Inspektor auf 1 kΩ oder mehr. Die LED wird dunkler, der Strom sinkt.",
            "Rechne nach: I = (U − U_F) / R. Moderne LEDs leuchten schon bei 2 … 5 mA deutlich.",
            &[],
            |c| {
                c.anchor_values().is_some_and(|v| v.i > 0.3e-3 && v.i < 12e-3)
                    && c.doc.parts.iter().any(|p| matches!(p.kind, PartKind::Resistor { r } if r >= 999.0) && c.values(p.id).is_some_and(|v| v.i.abs() > 0.3e-3))
            },
        )
        .live(|c| c.anchor_values().map(|v| format!("I_LED = {}", fmt_si(v.i, "A"))))
        .sticky(),
    ];
    Lesson {
        title: "LED mit Vorwiderstand".into(),
        intro: "Eine LED braucht immer einen Vorwiderstand, sonst fließt viel zu viel Strom. Du baust den klassischen LED-Stromkreis.".into(),
        outro: "Merke: LED immer mit Vorwiderstand, Kathode (Strich) Richtung Minus. R = (U − U_F) / I.".into(),
        steps,
        start_kind: PartKind::Led { color: LedColor::Rot },
        task: false,
    }
}

// ---------------------------------------------------------------------------
// Kondensator
// ---------------------------------------------------------------------------

fn capacitor() -> Lesson {
    let tau = |c: &Ctx| -> Option<(f64, f64)> {
        let cap = match c.anchor_part()?.kind {
            PartKind::Capacitor { c } => c,
            _ => return None,
        };
        let hot = if c.is_gnd(c.pin(0)) { c.pin(1) } else { c.pin(0) };
        let src: Vec<usize> = c.supplies().iter().map(|s| s.1).collect();
        // Widerstand am Kondensator, der zur Quelle führt
        let r = c
            .doc
            .parts
            .iter()
            .filter(|p| is_resistor(&p.kind))
            .find(|p| c.nl.pin_nets.get(&p.id).is_some_and(|pn| pn.contains(&hot.unwrap_or(usize::MAX))))?;
        let PartKind::Resistor { r } = r.kind else { return None };
        let freq = c.supplies().iter().find_map(|s| match s.0.kind {
            PartKind::SquareVoltage { freq, .. } if src.contains(&s.1) => Some(freq),
            _ => None,
        })?;
        Some((r * cap, 0.5 / freq))
    };
    let steps = vec![
        Step::new(
            "Rechteckspannung und Masse",
            "Zieh eine Rechteckspannung in den Schaltplan und setz eine Masse an ihren Minuspol. Sie schaltet immer wieder zwischen 0 V und 5 V um.",
            "So lädt und entlädt sich der Kondensator ständig, und du siehst beides im Oszilloskop.",
            &[],
            |c| c.supplies().iter().any(|s| matches!(s.0.kind, PartKind::SquareVoltage { .. })),
        ),
        Step::new(
            "Kondensator an Masse",
            "Verbinde einen Anschluss des Kondensators mit der Masse.",
            "Der Kondensator speichert Ladung zwischen seinen Platten. Die Spannung über ihm kann nicht springen, sie ändert sich nur, während Strom fließt.",
            &[1],
            |c| c.is_gnd(c.pin(0)) || c.is_gnd(c.pin(1)),
        ),
        Step::new(
            "Ladewiderstand",
            "Verbinde den anderen Anschluss über einen Widerstand mit dem Pluspol der Rechteckspannung.",
            "Der Widerstand begrenzt den Ladestrom. Je größer R, desto langsamer lädt der Kondensator.",
            &[0],
            |c| {
                let hot = if c.is_gnd(c.pin(0)) { c.pin(1) } else { c.pin(0) };
                let src: Vec<usize> = c.supplies().iter().map(|s| s.1).collect();
                c.path(hot, &src, is_resistor, is_resistor)
            },
        )
        .hint(|c| {
            let src: Vec<usize> = c.supplies().iter().map(|s| s.1).collect();
            [c.pin(0), c.pin(1)]
                .iter()
                .any(|n| n.is_some_and(|n| src.contains(&n)))
                .then(|| "Der Kondensator hängt direkt an der Quelle. Ohne Widerstand gibt es keine Ladekurve, nur Stromspitzen.".into())
        }),
        Step::new(
            "Messpunkt setzen",
            "Setz einen Messpunkt an die Verbindung zwischen Widerstand und Kondensator. Das Oszilloskop zeichnet alle Messpunkte auf.",
            "Ein Messpunkt misst die Spannung gegen Masse, ohne die Schaltung zu belasten.",
            &[0],
            |c| {
                let hot = if c.is_gnd(c.pin(0)) { c.pin(1) } else { c.pin(0) };
                c.on_net(hot, |k| matches!(k, PartKind::Probe))
            },
        ),
        Step::new(
            "Zeitkonstante passend wählen",
            "τ = R · C bestimmt, wie schnell geladen wird. Wähle R und C so, dass τ ein Zehntel bis ein Drittel einer halben Periode ist. Bei 1 kHz zum Beispiel R = 1 kΩ und C = 100 nF.",
            "Nach τ hat der Kondensator 63 % erreicht, nach 5 τ ist er praktisch voll. Ist τ viel größer als die halbe Periode, bleibt nur ein kleines Dreieck übrig.",
            &[],
            move |c| tau(c).is_some_and(|(t, h)| t / h >= 0.08 && t / h <= 0.4),
        )
        .live(move |c| tau(c).map(|(t, h)| format!("τ = {}   halbe Periode = {}   τ / halbe Periode = {}", fmt_si(t, "s"), fmt_si(h, "s"), pct(t / h)))),
        Step::new(
            "Im Oszilloskop ansehen",
            "Öffne das Oszilloskop (Knopf „Messgeräte“ oben). Die Kurve am Kondensator steigt und fällt als e-Funktion.",
            "Lies ab: Nach einer Zeitkonstante sind 63 % des Sprungs erreicht. Ändere C und sieh, wie sich die Kurve streckt.",
            &[],
            |c| c.scope_open,
        )
        .sticky(),
    ];
    Lesson {
        title: "Kondensator laden und entladen".into(),
        intro: "Du baust ein RC-Glied und siehst im Oszilloskop, wie der Kondensator sich mit der Zeitkonstante τ = R · C lädt und entlädt.".into(),
        outro: "Merke: τ = R · C. Nach τ 63 %, nach 5 τ voll. Die Kondensatorspannung springt nie.".into(),
        steps,
        start_kind: PartKind::Capacitor { c: 100e-9 },
        task: false,
    }
}

// ---------------------------------------------------------------------------
// Bipolartransistor als Schalter
// ---------------------------------------------------------------------------

fn bjt(t: TransistorType) -> Lesson {
    let npn = t.family() == TransistorFamily::Npn;
    let (e_target, e_word, other_word) = if npn { ("Masse", "an die Masse", "Plus") } else { ("Plus", "an den Pluspol", "Masse") };
    let to_e = move |c: &Ctx, n: Option<usize>| if npn { c.is_gnd(n) } else { c.is_plus(n) };
    let others = move |c: &Ctx| if npn { c.plus() } else { c.gnd_nets() };
    let steps = vec![
        supply_step(3.0, 24.0, 9.0),
        Step::new(
            format!("Emitter an {e_target}"),
            format!("Verbinde den Emitter (E, mit dem Pfeil) {e_word}."),
            if npn {
                "Beim NPN-Transistor fließt der Strom von oben (Kollektor) nach unten zum Emitter. Der Emitter ist der Bezugspunkt für die Steuerspannung U_BE."
            } else {
                "Der PNP-Transistor ist das Spiegelbild des NPN: Der Emitter liegt an Plus, der Strom fließt vom Emitter zum Kollektor nach unten."
            },
            &[2],
            move |c| to_e(c, c.pin(2)),
        )
        .hint(move |c| {
            (if npn { c.is_plus(c.pin(2)) } else { c.is_gnd(c.pin(2)) })
                .then(|| format!("Der Emitter hängt an {other_word}. Beim {} gehört er an {e_target}.", if npn { "NPN" } else { "PNP" }))
        }),
        Step::new(
            "Last am Kollektor",
            format!("Verbinde den Kollektor (C) über eine LED mit Vorwiderstand (z. B. 470 Ω) mit {other_word}."),
            "Der Transistor schaltet die Last wie ein Schalter. Den Strom begrenzt die Last selbst, nicht der Transistor.",
            &[1],
            move |c| c.path(c.pin(1), &others(c), load_or_resistor, is_light),
        )
        .hint(move |c| {
            (if npn { c.is_plus(c.pin(1)) } else { c.is_gnd(c.pin(1)) }).then(|| "Der Kollektor hängt direkt an der Versorgung. Dann fehlt die Last, die geschaltet werden soll.".into())
        }),
        Step::new(
            "Basiswiderstand und Taster",
            format!("Verbinde die Basis (B) über einen Widerstand (z. B. 10 kΩ) und einen Taster mit {other_word}."),
            "Zwischen Basis und Emitter liegt eine Diode. Ohne Widerstand würde sie den Basisstrom nicht begrenzen. Schon wenige zehntel Milliampere Basisstrom steuern viele Milliampere Kollektorstrom.",
            &[0],
            move |c| c.path(c.pin(0), &others(c), resistor_or_switch, is_resistor) && c.path(c.pin(0), &others(c), resistor_or_switch, is_switch),
        )
        .hint(move |c| {
            (if npn { c.is_plus(c.pin(0)) } else { c.is_gnd(c.pin(0)) }).then(|| "Die Basis hängt ohne Widerstand an der Versorgung: Der Basisstrom wäre viel zu groß. Immer einen Basiswiderstand einsetzen.".into())
        }),
        Step::new(
            "Einschalten",
            "Halte den Taster gedrückt (oder schließe den Schalter per Doppelklick). Die LED leuchtet.",
            "Der Basisstrom schaltet den Transistor durch. Er ist jetzt gesättigt: U_CE fällt auf etwa 0,1 V, fast die ganze Spannung liegt an der Last.",
            &[0],
            |c| c.anchor_values().is_some_and(|v| v.region == Some(Region::On) || v.i.abs() > 2e-3) && c.light_current() > 1e-3,
        )
        .live(|c| {
            c.anchor_values().map(|v| {
                let b = if v.ctrl_i.abs() > 1e-6 { format!("   I_C / I_B = {:.0}", v.i / v.ctrl_i) } else { String::new() };
                format!("I_B = {}   I_C = {}   U_CE = {}{b}", fmt_si(v.ctrl_i, "A"), fmt_si(v.i, "A"), fmt_si(v.u, "V"))
            })
        })
        .sticky(),
        Step::new(
            "Ausschalten",
            "Lass den Taster los. Ohne Basisstrom sperrt der Transistor, die LED geht aus.",
            "Gesperrt fließt praktisch kein Kollektorstrom, U_CE ist so groß wie die Versorgung. Fahr mit der Maus über den Transistor und vergleiche.",
            &[],
            |c| c.anchor_values().is_some_and(|v| v.region == Some(Region::Off)),
        )
        .live(|c| c.anchor_values().map(|v| format!("I_C = {}   U_CE = {}", fmt_si(v.i, "A"), fmt_si(v.u, "V"))))
        .sticky(),
    ];
    Lesson {
        title: format!("{}-Transistor als Schalter", if npn { "NPN" } else { "PNP" }),
        intro: if npn {
            "Ein kleiner Basisstrom schaltet einen großen Kollektorstrom. Du baust einen Schalter, der eine LED mit einem Taster steuert.".into()
        } else {
            "Der PNP-Transistor schaltet auf der Plus-Seite („High-Side“). Er leitet, wenn die Basis Richtung Masse gezogen wird.".into()
        },
        outro: if npn {
            "Merke: NPN – Emitter an Masse, Last zwischen Plus und Kollektor, Basis immer mit Widerstand. Faustregel zum sicheren Schalten: I_B ≈ I_C / 10.".into()
        } else {
            "Merke: PNP – Emitter an Plus, Last zwischen Kollektor und Masse, Basis über Widerstand nach Masse zieht ihn ein.".into()
        },
        steps,
        start_kind: PartKind::Transistor { kind: t },
        task: false,
    }
}

// ---------------------------------------------------------------------------
// MOSFET als Schalter
// ---------------------------------------------------------------------------

fn mosfet(t: TransistorType) -> Lesson {
    let n = t.family() == TransistorFamily::Nmos;
    let (s_target, s_word, other_word) = if n { ("Masse", "an die Masse", "Plus") } else { ("Plus", "an den Pluspol", "Masse") };
    let to_s = move |c: &Ctx, x: Option<usize>| if n { c.is_gnd(x) } else { c.is_plus(x) };
    let others = move |c: &Ctx| if n { c.plus() } else { c.gnd_nets() };
    let same = move |c: &Ctx| if n { c.gnd_nets() } else { c.plus() };
    let steps = vec![
        supply_step(5.0, 24.0, 12.0),
        Step::new(
            format!("Source an {s_target}"),
            format!("Verbinde Source (S) {s_word}."),
            "Die Steuerspannung U_GS wird zwischen Gate und Source gemessen. Liegt Source fest an der Versorgung, steuert die Gate-Spannung direkt.",
            &[2],
            move |c| to_s(c, c.pin(2)),
        ),
        Step::new(
            "Last am Drain",
            format!("Verbinde Drain (D) über die Last mit {other_word}: eine Glühlampe oder eine LED mit Vorwiderstand."),
            "MOSFETs schalten große Ströme mit sehr kleinem Widerstand (R_DS(on)). Ideal für Lampen, Motoren, LED-Streifen.",
            &[1],
            move |c| c.path(c.pin(1), &others(c), load_or_resistor, is_light),
        ),
        Step::new(
            "Gate über Taster ansteuern",
            format!("Verbinde das Gate (G) über einen Taster (gern mit einem kleinen Widerstand, z. B. 1 kΩ) mit {other_word}."),
            "Das Gate ist isoliert: Im Ruhezustand fließt kein Strom hinein, es zählt nur die Spannung. Deshalb reicht ein schwaches Signal, etwa ein Mikrocontroller-Pin.",
            &[0],
            move |c| c.path(c.pin(0), &others(c), resistor_or_switch, is_switch),
        ),
        Step::new(
            if n { "Pull-down am Gate" } else { "Pull-up am Gate" },
            format!("Setz einen Widerstand (z. B. 100 kΩ) vom Gate nach {s_target}."),
            "Ein offenes Gate behält seine Ladung oder fängt Störungen ein – der MOSFET schaltet dann unberechenbar. Der Widerstand zieht es sicher auf „aus“, wenn der Taster offen ist.",
            &[0],
            move |c| c.path(c.pin(0), &same(c), is_resistor, is_resistor),
        ),
        Step::new(
            "Einschalten",
            "Halte den Taster gedrückt. Die Last geht an.",
            "Über der Schwellspannung (2N7000: etwa 2 V) bildet sich ein leitender Kanal. Voll durchgeschaltet ist U_DS sehr klein.",
            &[0],
            |c| c.anchor_values().is_some_and(|v| v.region == Some(Region::On)) && c.light_current() > 1e-3,
        )
        .live(|c| c.anchor_values().map(|v| format!("U_GS = {}   I_D = {}   U_DS = {}", fmt_si(v.ctrl_u, "V"), fmt_si(v.i, "A"), fmt_si(v.u, "V"))))
        .sticky(),
        Step::new(
            "Ausschalten",
            "Lass den Taster los. Der Pull-Widerstand entlädt das Gate, der MOSFET sperrt.",
            "Vergleiche mit dem Bipolartransistor: Dort fließt dauernd Basisstrom, hier nur kurz beim Umladen des Gates.",
            &[],
            |c| c.anchor_values().is_some_and(|v| v.region == Some(Region::Off)),
        )
        .sticky(),
    ];
    Lesson {
        title: format!("{}-MOSFET als Schalter", if n { "N-Kanal" } else { "P-Kanal" }),
        intro: "Ein MOSFET wird mit Spannung statt mit Strom gesteuert. Du schaltest eine Last mit einem Taster am Gate.".into(),
        outro: if n {
            "Merke: N-MOSFET – Source an Masse, Last zwischen Plus und Drain, Gate mit Pull-down. Die Gate-Spannung muss deutlich über der Schwelle liegen.".into()
        } else {
            "Merke: P-MOSFET – Source an Plus, Last zwischen Drain und Masse, Gate mit Pull-up; Gate nach Masse ziehen schaltet ein.".into()
        },
        steps,
        start_kind: PartKind::Transistor { kind: t },
        task: false,
    }
}

// ---------------------------------------------------------------------------
// Logik-IC
// ---------------------------------------------------------------------------

fn gate_ic(gate: GateType) -> Lesson {
    let n_in = gate.inputs();
    let (y, vcc, gnd) = (n_in, n_in + 1, n_in + 2);
    let ins: Vec<usize> = (0..n_in).collect();
    let rest = gate.to_sim().eval(&vec![Logic::L0; n_in]) == Logic::L1;
    let steps = vec![
        supply_step(4.5, 5.5, 5.0).hint(|c| {
            c.supplies()
                .iter()
                .find(|s| s.2 > 7.0)
                .map(|s| format!("{} liefert {}: Über 7 V wird ein 74HC zerstört. Stell 5 V ein.", s.0.name, fmt_si(s.2, "V")))
        }),
        Step::new(
            "Versorgung anschließen",
            "Verbinde VCC (oben) mit dem Pluspol und GND (unten) mit der Masse.",
            "Ohne Versorgung arbeitet kein IC. Die Ausgänge holen ihren Strom aus VCC.",
            &[vcc, gnd],
            move |c| c.is_plus(c.pin(vcc)) && c.is_gnd(c.pin(gnd)),
        )
        .hint(move |c| (c.is_gnd(c.pin(vcc)) && c.is_plus(c.pin(gnd))).then(|| "VCC und GND sind vertauscht! Verpolt geht ein echter IC sofort kaputt.".into())),
        Step::new(
            "Pull-down an jedem Eingang",
            "Jeder Eingang bekommt einen Widerstand (10 kΩ) nach Masse.",
            "CMOS-Eingänge sind so hochohmig, dass ein offener Eingang irgendeinen Pegel annimmt (elab zeigt X). Der Pull-down legt ihn sicher auf 0, solange nichts anderes ihn hochzieht.",
            &ins,
            move |c| (0..n_in).all(|k| c.path(c.pin(k), &c.gnd_nets(), is_resistor, is_resistor) || c.is_gnd(c.pin(k)) || c.is_plus(c.pin(k))),
        ),
        Step::new(
            "Taster nach Plus",
            "Verbinde die Eingänge zusätzlich über je einen Taster mit Plus. Gedrückt wird der Eingang 1.",
            "Der Taster ist stärker als der Pull-down: gedrückt liegen 5 V am Eingang, über dem Widerstand fließen nur 0,5 mA.",
            &ins,
            move |c| (0..n_in).all(|k| c.path(c.pin(k), &c.plus(), is_switch, is_switch) || c.is_plus(c.pin(k)) || c.is_gnd(c.pin(k))),
        )
        .hint(move |c| {
            (0..n_in).any(|k| c.is_plus(c.pin(k)) && c.is_gnd(c.pin(k))).then(|| "Ein Eingang hängt an Plus und Masse zugleich: Kurzschluss!".into())
        }),
        Step::new(
            "LED am Ausgang",
            "Verbinde den Ausgang Y über einen Vorwiderstand (470 Ω) und eine LED mit Masse.",
            "Ein 74HC-Ausgang liefert etwa 20 mA. Der Strom fließt aus VCC durch den IC in die LED.",
            &[y],
            move |c| c.path(c.pin(y), &c.gnd_nets(), load_or_resistor, is_led),
        ),
        Step::new(
            format!("LED {}schalten", if rest { "aus" } else { "ein" }),
            format!(
                "Ohne gedrückte Taster sind alle Eingänge 0, und beim {} ist die LED {}. Finde eine Tastenkombination, bei der sie {} geht.",
                gate.label(),
                if rest { "an" } else { "aus" },
                if rest { "aus" } else { "an" }
            ),
            "Vergleiche mit der Wahrheitstabelle des Gatters. Fahr über den IC: Die Pins zeigen, welche Pegel er sieht.",
            &ins,
            move |c| if rest { c.light_current() < 0.3e-3 } else { c.light_current() > 1e-3 },
        )
        .live(move |c| {
            let v: Vec<String> = (0..n_in).filter_map(|k| c.net_voltage(c.pin(k)).map(|u| format!("U_{} = {}", (b'A' + k as u8) as char, fmt_si(u, "V")))).collect();
            Some(format!("{}   Y = {}", v.join("   "), c.net_voltage(c.pin(y)).map_or("–".into(), |u| fmt_si(u, "V"))))
        })
        .sticky(),
    ];
    Lesson {
        title: format!("Logik-IC beschalten ({})", gate.label()),
        intro: "Ein echter Logikbaustein braucht Versorgung und saubere Eingangspegel. Du baust ihn so auf, wie man es auf dem Steckbrett macht."
            .into(),
        outro: "Merke: VCC und GND zuerst, kein Eingang bleibt offen (Pull-down oder Pull-up), Ausgänge nur mit Vorwiderstand belasten.".into(),
        steps,
        start_kind: PartKind::GateIc { gate, family: crate::model::LogicFamily::Hc },
        task: false,
    }
}

// ---------------------------------------------------------------------------
// Timer 555
// ---------------------------------------------------------------------------

fn timer555() -> Lesson {
    // Pins: TRIG 0, THR 1, CTRL 2, OUT 3, DIS 4, VCC 5, /RES 6, GND 7
    let values = |c: &Ctx| -> Option<(f64, f64, f64)> {
        let ra = c.plus().iter().find_map(|&p| c.between(Some(p), c.pin(4), is_resistor))?;
        let rb = c.between(c.pin(4), c.pin(1), is_resistor)?;
        let cap = c.gnd_nets().iter().find_map(|&g| c.between(c.pin(1), Some(g), is_capacitor))?;
        match (&ra.kind, &rb.kind, &cap.kind) {
            (PartKind::Resistor { r: a }, PartKind::Resistor { r: b }, PartKind::Capacitor { c }) => Some((*a, *b, *c)),
            _ => None,
        }
    };
    let steps = vec![
        supply_step(4.5, 16.0, 9.0),
        Step::new(
            "Versorgung und Reset",
            "Verbinde VCC und /RES mit Plus, GND mit Masse.",
            "/RES ist low-aktiv: Liegt er auf 0, bleibt der Ausgang aus. Für den Dauerbetrieb gehört er an Plus.",
            &[5, 6, 7],
            |c| c.is_plus(c.pin(5)) && c.is_plus(c.pin(6)) && c.is_gnd(c.pin(7)),
        ),
        Step::new(
            "Zeitkondensator",
            "Verbinde TRIG und THR miteinander und schließe dort einen Kondensator (z. B. 10 µF) gegen Masse an.",
            "Der Timer schaut auf die Spannung an diesem Kondensator: Bei ⅔ der Versorgung schaltet er aus, bei ⅓ wieder ein.",
            &[0, 1],
            |c| c.pin(0) == c.pin(1) && c.gnd_nets().iter().any(|&g| c.between(c.pin(1), Some(g), is_capacitor).is_some()),
        ),
        Step::new(
            "R_B zwischen DIS und THR",
            "Setz einen Widerstand R_B (z. B. 47 kΩ) zwischen DIS und den Kondensator-Anschluss (THR/TRIG).",
            "Über R_B wird der Kondensator entladen, wenn DIS nach Masse schaltet.",
            &[4, 1],
            |c| c.between(c.pin(4), c.pin(1), is_resistor).is_some(),
        ),
        Step::new(
            "R_A zwischen Plus und DIS",
            "Setz einen Widerstand R_A (z. B. 10 kΩ) zwischen Plus und DIS.",
            "Geladen wird über R_A + R_B, entladen nur über R_B. Darum ist die Leuchtphase etwas länger als die Pause. R_A nicht unter 1 kΩ, sonst fließt beim Entladen zu viel Strom in DIS.",
            &[4],
            |c| c.plus().iter().any(|&p| c.between(Some(p), c.pin(4), is_resistor).is_some()),
        ),
        Step::new(
            "LED am Ausgang",
            "Schließe am Ausgang OUT eine LED mit Vorwiderstand (1 kΩ) an, nach Masse oder nach Plus.",
            "Nach Masse leuchtet die LED, wenn der Ausgang high ist. Nach Plus leuchtet sie, wenn er low ist.",
            &[3],
            |c| c.path(c.pin(3), &c.gnd_nets(), load_or_resistor, is_led) || c.path(c.pin(3), &c.plus(), load_or_resistor, is_led),
        ),
        Step::new(
            "Frequenz nachrechnen",
            "Beobachte das Blinken und vergleiche mit der Formel. Ändere C oder R_B und schau, was passiert.",
            "f = 1,44 / ((R_A + 2 · R_B) · C). Das Oszilloskop zeigt am Kondensator die Sägezahnspannung zwischen ⅓ und ⅔ der Versorgung.",
            &[],
            |c| c.light_current() > 1e-3,
        )
        .live(move |c| {
            values(c).map(|(a, b, cap)| {
                let f = 1.44 / ((a + 2.0 * b) * cap);
                format!("f = {}   t_high = {}   t_low = {}", fmt_si(f, "Hz"), fmt_si(0.693 * (a + b) * cap, "s"), fmt_si(0.693 * b * cap, "s"))
            })
        })
        .sticky(),
    ];
    Lesson {
        title: "Timer 555 als Blinker".into(),
        intro: "Der NE555 erzeugt mit zwei Widerständen und einem Kondensator einen Takt. Du baust die klassische astabile Schaltung.".into(),
        outro: "Merke: VCC und /RES an Plus, TRIG und THR verbunden am Kondensator, R_A von Plus nach DIS, R_B von DIS zum Kondensator. f = 1,44 / ((R_A + 2·R_B)·C).".into(),
        steps,
        start_kind: PartKind::Timer555,
        task: false,
    }
}

// ---------------------------------------------------------------------------
// Operationsverstärker
// ---------------------------------------------------------------------------

fn opamp(kind: OpAmpType) -> Lesson {
    // Pins: + 0, − 1, Ausgang 2, V+ 3, V− 4
    let gain = |c: &Ctx| -> Option<f64> {
        let r2 = c.between(c.pin(2), c.pin(1), is_resistor)?;
        let r1 = c.gnd_nets().iter().find_map(|&g| c.between(c.pin(1), Some(g), is_resistor))?;
        match (&r1.kind, &r2.kind) {
            (PartKind::Resistor { r: a }, PartKind::Resistor { r: b }) => Some(1.0 + b / a),
            _ => None,
        }
    };
    let steps = vec![
        supply_step(5.0, 30.0, 12.0),
        Step::new(
            "Versorgung anschließen",
            "Verbinde V+ (oben) mit Plus und V− (unten) mit Masse.",
            "Der OpAmp kann am Ausgang nur Spannungen innerhalb seiner Versorgung liefern. Mit V− an Masse arbeitet er an einer „einfachen“ Versorgung.",
            &[3, 4],
            |c| c.is_plus(c.pin(3)) && c.is_gnd(c.pin(4)),
        ),
        Step::new(
            "Eingangsspannung mit Poti",
            "Setz ein Potentiometer zwischen Plus und Masse und verbinde seinen Schleifer (Mitte) mit dem +-Eingang.",
            "Das Poti ist ein verstellbarer Spannungsteiler: Am Schleifer liegt eine Spannung zwischen 0 V und der Versorgung.",
            &[0],
            |c| {
                c.doc.parts.iter().any(|p| {
                    matches!(p.kind, PartKind::Potentiometer { .. })
                        && c.nl.pin_nets.get(&p.id).is_some_and(|pn| {
                            let ends = [Some(pn[0]), Some(pn[2])];
                            ends.iter().any(|&e| c.is_plus(e)) && ends.iter().any(|&e| c.is_gnd(e)) && Some(pn[1]) == c.pin(0)
                        })
                })
            },
        ),
        Step::new(
            "Gegenkopplung R2",
            "Setz einen Widerstand R2 (z. B. 20 kΩ) vom Ausgang zum −-Eingang.",
            "Gegenkopplung: Der OpAmp stellt seinen Ausgang so ein, dass am −-Eingang dieselbe Spannung liegt wie am +-Eingang.",
            &[2, 1],
            |c| c.between(c.pin(2), c.pin(1), is_resistor).is_some(),
        ),
        Step::new(
            "R1 nach Masse",
            "Setz einen Widerstand R1 (z. B. 10 kΩ) vom −-Eingang nach Masse.",
            "R2 und R1 teilen die Ausgangsspannung. Weil der −-Eingang U_ein erreichen muss, gilt U_aus = U_ein · (1 + R2/R1).",
            &[1],
            |c| c.gnd_nets().iter().any(|&g| c.between(c.pin(1), Some(g), is_resistor).is_some()),
        )
        .live(move |c| gain(c).map(|v| format!("Verstärkung V = 1 + R2/R1 = {v:.2}"))),
        Step::new(
            "Ausgang messen",
            "Setz ein Voltmeter an den Ausgang (Plus an den Ausgang, Minus an Masse).",
            "Das Voltmeter belastet die Schaltung nicht. Vergleiche U_aus mit U_ein · V.",
            &[2],
            |c| c.on_net(c.pin(2), |k| matches!(k, PartKind::Voltmeter)),
        )
        .live(move |c| {
            let (ue, ua) = (c.net_voltage(c.pin(0))?, c.net_voltage(c.pin(2))?);
            Some(format!("U_ein = {}   U_aus = {}   U_aus / U_ein = {}", fmt_si(ue, "V"), fmt_si(ua, "V"), if ue.abs() > 1e-6 { format!("{:.2}", ua / ue) } else { "–".into() }))
        }),
        Step::new(
            "Übersteuern",
            "Dreh das Poti hoch (Pfeiltasten oder Inspektor), bis U_aus nicht mehr weiter steigt.",
            match kind {
                OpAmpType::Lm358 => "Der LM358 kommt nur bis etwa 1,5 V unter V+. Darüber kann er nicht mehr regeln: + und − sind nicht mehr gleich, der OpAmp ist übersteuert.",
                OpAmpType::Mcp6001 => "Ein Rail-to-Rail-OpAmp kommt bis an die Versorgung heran. Darüber ist er übersteuert: + und − sind nicht mehr gleich.",
            },
            &[0],
            |c| c.anchor_values().is_some_and(|v| v.ctrl_u.abs() > 5e-3),
        )
        .live(|c| c.anchor_values().map(|v| format!("U₊ − U₋ = {}   U_aus = {}", fmt_si(v.ctrl_u, "V"), fmt_si(v.u, "V"))))
        .sticky(),
    ];
    Lesson {
        title: "OpAmp als Verstärker".into(),
        intro:
            "Du baust einen nichtinvertierenden Verstärker: Die Eingangsspannung kommt aus einem Poti, zwei Widerstände legen die Verstärkung fest."
                .into(),
        outro: "Merke: Mit Gegenkopplung gilt U₊ = U₋. Nichtinvertierend: V = 1 + R2/R1. Der Ausgang bleibt immer innerhalb der Versorgung.".into(),
        steps,
        start_kind: PartKind::OpAmp { kind },
        task: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{examples, netlist, sim};

    /// Prüft eine Übung an einem fertigen Schaltplan: Alle Strukturschritte erfüllt,
    /// die Übung steht beim ersten Aktionsschritt.
    fn state(doc: &Document, anchor: &str, pressed: &[&str], scope: bool, progress: &mut Progress) -> (Vec<bool>, Option<usize>) {
        let id = doc.part_by_name(anchor).unwrap().id;
        let pressed: HashSet<u32> = pressed.iter().map(|n| doc.part_by_name(n).unwrap().id).collect();
        let nl = netlist::build_with(doc, &pressed);
        let leds: Vec<u32> = doc.parts.iter().filter(|p| is_light(&p.kind)).map(|p| p.id).collect();
        let op = if nl.mixed.is_some() {
            let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
            ms.advance(3.0, None).unwrap();
            // Blinker: einen Moment mit leuchtender LED suchen
            let mut v = sim::mixed_view(&nl, &ms);
            for _ in 0..200 {
                if leds.iter().any(|id| v.parts.get(id).is_some_and(|p| p.i.abs() > 2e-3)) {
                    break;
                }
                ms.advance(ms.time() + 0.02, None).unwrap();
                v = sim::mixed_view(&nl, &ms);
            }
            v
        } else {
            sim::operating_point(&nl).unwrap()
        };
        let lesson = lesson_for(&doc.part(id).unwrap().kind).unwrap();
        let ctx = Ctx { doc, nl: &nl, op: Some(&op), anchor: id, scope_open: scope };
        progress.update(&lesson, &ctx)
    }

    fn example(title: &str) -> Document {
        examples::all().into_iter().find(|e| e.title.starts_with(title)).unwrap().doc
    }

    #[test]
    fn empty_sheet_starts_at_first_step() {
        for l in all() {
            let mut d = Document::default();
            let id = d.add_part(l.start_kind.clone(), crate::model::P::new(0, 0), 0);
            let nl = netlist::build(&d);
            let ctx = Ctx { doc: &d, nl: &nl, op: None, anchor: id, scope_open: false };
            let (done, cur) = Progress::default().update(&l, &ctx);
            assert_eq!(cur, Some(0), "{}", l.title);
            assert!(!done[0]);
        }
    }

    #[test]
    fn led_lesson() {
        let d = example("LED mit Vorwiderstand");
        let mut p = Progress::default();
        let (_, cur) = state(&d, "LED1", &[], false, &mut p);
        // 220 Ω: Strom > 12 mA, letzter Schritt verlangt ≥ 1 kΩ
        assert_eq!(cur, Some(4));
        let mut d2 = d.clone();
        let r = d2.part_by_name("R1").unwrap().id;
        d2.part_mut(r).unwrap().kind = PartKind::Resistor { r: 1000.0 };
        assert_eq!(state(&d2, "LED1", &[], false, &mut p).1, None);
        // verdrehte LED wird erkannt
        let mut d3 = d.clone();
        let led = d3.part_by_name("LED1").unwrap().id;
        d3.part_mut(led).unwrap().rot = 3;
        let nl = netlist::build(&d3);
        let lesson = lesson_for(&PartKind::Led { color: LedColor::Rot }).unwrap();
        let ctx = Ctx { doc: &d3, nl: &nl, op: None, anchor: led, scope_open: false };
        assert!((lesson.steps[1].hint)(&ctx).unwrap().contains("verkehrt"));
    }

    #[test]
    fn capacitor_lesson() {
        let d = example("RC-Tiefpass");
        let mut p = Progress::default();
        assert_eq!(state(&d, "C1", &[], false, &mut p).1, Some(5));
        assert_eq!(state(&d, "C1", &[], true, &mut p).1, None);
    }

    #[test]
    fn bjt_lesson() {
        let d = example("Transistor als Schalter");
        let mut p = Progress::default();
        assert_eq!(state(&d, "T1", &[], false, &mut p).1, Some(4));
        assert_eq!(state(&d, "T1", &["S1"], false, &mut p).1, Some(5));
        assert_eq!(state(&d, "T1", &[], false, &mut p).1, None);
        // Basis ohne Widerstand: Hinweis
        let mut d2 = d.clone();
        let rb = d2.part_by_name("R_B").unwrap().id;
        d2.remove(&[rb].into(), &Default::default());
        let t = d2.part_by_name("T1").unwrap().clone();
        d2.add_wire(t.pins()[0], crate::model::P::new(13, -8));
        let nl = netlist::build(&d2);
        let lesson = lesson_for(&t.kind).unwrap();
        let ctx = Ctx { doc: &d2, nl: &nl, op: None, anchor: t.id, scope_open: false };
        assert!((lesson.steps[3].hint)(&ctx).is_some());
    }

    #[test]
    fn mosfet_lesson() {
        let d = example("MOSFET schaltet eine Lampe");
        let mut p = Progress::default();
        assert_eq!(state(&d, "T1", &[], false, &mut p).1, Some(5));
        assert_eq!(state(&d, "T1", &["S1"], false, &mut p).1, Some(6));
        assert_eq!(state(&d, "T1", &[], false, &mut p).1, None);
    }

    #[test]
    fn gate_ic_lesson() {
        let d = example("NAND-Baustein");
        let mut p = Progress::default();
        assert_eq!(state(&d, "U1", &[], false, &mut p).1, Some(5));
        assert_eq!(state(&d, "U1", &["S_A"], false, &mut p).1, Some(5));
        assert_eq!(state(&d, "U1", &["S_A", "S_B"], false, &mut p).1, None);
    }

    #[test]
    fn timer_and_opamp_lessons() {
        let d = example("555-Blinker");
        let mut p = Progress::default();
        assert_eq!(state(&d, "U1", &[], false, &mut p).1, None);

        let d = example("OpAmp-Verstärker");
        let mut p = Progress::default();
        assert_eq!(state(&d, "U1", &[], false, &mut p).1, Some(6));
        let mut d2 = d.clone();
        let pot = d2.part_by_name("P1").unwrap().id;
        d2.part_mut(pot).unwrap().kind = PartKind::Potentiometer { r: 10e3, pos: 0.7 };
        assert_eq!(state(&d2, "U1", &[], false, &mut p).1, None);
    }

    #[test]
    fn progress_falls_back_when_wiring_breaks() {
        let d = example("Transistor als Schalter");
        let mut p = Progress::default();
        state(&d, "T1", &["S1"], false, &mut p);
        assert_eq!(p.reached, 5);
        // Emitter-Leitung weg: zurück auf Schritt 1, Aktionsschritt bleibt erledigt
        let mut d2 = d.clone();
        let e = d2.part_by_name("T1").unwrap().pins()[2];
        let w: HashSet<u32> = d2.wires.iter().filter(|w| w.a == e || w.b == e).map(|w| w.id).collect();
        d2.remove(&HashSet::new(), &w);
        d2.add_wire(crate::model::P::new(0, 8), crate::model::P::new(12, 8));
        let (done, cur) = state(&d2, "T1", &[], false, &mut p);
        assert_eq!(cur, Some(1));
        assert!(done[4]);
    }
}
