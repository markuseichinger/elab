//! Datenblatt und richtige Beschaltung eines Bauteils.
//!
//! [`datasheet`] liefert die Kenndaten des gewählten Typs (was er kann und verträgt),
//! [`pin_guide`] je Anschluss, was dort hingehört. [`check_pins`] prüft die Beschaltung
//! am aktuellen Schaltplan und Simulationsstand: ✔ richtig, ⚠ auffällig, ✗ falsch.

use crate::model::{Document, IcPin, LogicFamily, Part, PartKind, TransistorFamily};
use crate::netlist::Netlist;
use crate::sim::OpView;
use elab_sim::digital::Logic;
use elab_sim::fmt_si as si;

/// Eine Zeile im Datenblatt: (Größe, Wert)
pub type Row = (&'static str, String);

fn level_text(l: elab_sim::Level, at: f64) -> String {
    if l.frac > 0.0 {
        format!("{:.0} % von VCC ({} bei {})", l.frac * 100.0, si(l.frac * at + l.abs, "V"), si(at, "V"))
    } else {
        si(l.abs, "V")
    }
}

/// Typische Versorgung, für die Pegel im Datenblatt ausgerechnet werden
fn typical_vcc(f: LogicFamily) -> f64 {
    if f == LogicFamily::Cd4000 {
        10.0
    } else {
        5.0
    }
}

fn family_rows(f: LogicFamily) -> Vec<Row> {
    let v = typical_vcc(f);
    let (il, ih) = f.thresholds();
    let mut rows = vec![
        ("Familie", format!("{} – {}", f.label(), if f == LogicFamily::Ls { "TTL (bipolar)" } else { "CMOS" })),
        ("Versorgung", format!("{} … {} (zerstört über {})", si(f.v_min(), "V"), si(f.v_rec_max(), "V"), si(f.v_max(), "V"))),
        ("Eingang 0", format!("unter {}", level_text(il, v))),
        ("Eingang 1", format!("über {}", level_text(ih, v))),
        ("Ausgang", format!("Innenwiderstand ≈ {}", si(f.r_out(), "Ω"))),
    ];
    if f.high_drop() > 0.0 {
        rows.push(("Ausgang 1", format!("≈ VCC − {} (≈ {} bei 5 V)", si(f.high_drop(), "V"), si(5.0 - f.high_drop(), "V"))));
    }
    rows.push(("Laufzeit", format!("≈ {} ns", f.delay(9))));
    rows.push(("Ruhestrom", format!("≈ {} bei {}", si(v / f.r_quiescent(), "A"), si(v, "V"))));
    rows
}

/// Kenndaten des Bauteils (wie im Datenblatt, auf das Wesentliche gekürzt).
pub fn datasheet(kind: &PartKind) -> Vec<Row> {
    use PartKind::*;
    match kind {
        Resistor { r } => vec![
            ("Bauform", "Kohleschicht, ¼ W, 5 % Toleranz".into()),
            ("Belastbarkeit", "0,25 W".into()),
            ("Größter Strom", format!("{} (bei ¼ W)", si((0.25 / r).sqrt(), "A"))),
            ("Größte Spannung", format!("{} (bei ¼ W)", si((0.25 * r).sqrt().min(250.0), "V"))),
        ],
        Capacitor { c, kind: ck } => {
            let mut v = vec![
                ("Bauart", ck.label().into()),
                ("Kapazität", si(*c, "F")),
                ("Spannungsfestigkeit", si(ck.v_max(), "V")),
                ("Gepolt", if ck.polarized() { "ja: Anschluss + an die höhere Spannung".into() } else { "nein".into() }),
            ];
            v.push(("Ladung bei 5 V", si(c * 5.0, "C")));
            v
        }
        Inductor { l } => vec![("Induktivität", si(*l, "H")), ("Verhalten", "sperrt schnelle Stromänderungen, Gleichstrom fließt frei".into())],
        Lamp { v_nom, p_nom } => vec![
            ("Nennwerte", format!("{} / {}", si(*v_nom, "V"), si(*p_nom, "W"))),
            ("Nennstrom", si(p_nom / v_nom, "A")),
            ("Widerstand", format!("{} (im Modell fest)", si(v_nom * v_nom / p_nom, "Ω"))),
            ("Brennt durch", format!("über {}", si(2.0 * p_nom, "W"))),
        ],
        DcVoltage { v } => vec![("Spannung", si(*v, "V")), ("Innenwiderstand", "0 Ω (ideal): liefert jeden Strom".into())],
        AcVoltage { amp, freq, .. } => {
            vec![("Scheitelwert", si(*amp, "V")), ("Effektivwert", si(amp / 2f64.sqrt(), "V")), ("Frequenz", si(*freq, "Hz"))]
        }
        SquareVoltage { low, high, freq } => {
            vec![("Pegel", format!("{} / {}", si(*low, "V"), si(*high, "V"))), ("Frequenz", si(*freq, "Hz")), ("Periode", si(1.0 / freq, "s"))]
        }
        CurrentSource { i } => vec![("Strom", si(*i, "A")), ("Verhalten", "treibt den Strom durch jede Last (ideal)".into())],
        Diode { kind: d } => {
            let m = d.model();
            let vf = |i: f64| m.nvt() * (i / m.is + 1.0).ln();
            let mut v =
                vec![("Typ", d.label().into()), ("Flussspannung", format!("{} bei 10 mA, {} bei 100 mA", si(vf(0.01), "V"), si(vf(0.1), "V")))];
            match d {
                crate::model::DiodeType::Si1N4148 => v.extend([("Größter Strom", "300 mA".into()), ("Sperrspannung", "bis 100 V".into())]),
                crate::model::DiodeType::Si1N4007 => {
                    v.extend([("Größter Strom", "1 A (Spitze 1,5 A)".into()), ("Sperrspannung", "bis 1000 V".into())])
                }
                crate::model::DiodeType::Schottky1N5819 => v.extend([("Größter Strom", "1 A".into()), ("Sperrspannung", "bis 40 V".into())]),
                _ => {}
            }
            if let Some(vz) = d.zener() {
                v.extend([
                    ("Zenerspannung", format!("{} bei 5 mA", si(vz, "V"))),
                    ("Leistung", "höchstens 0,5 W".into()),
                    ("Größter Zenerstrom", si(0.5 / vz, "A")),
                    ("Vorwiderstand", format!("R = (U − {}) / I_Z, z. B. {} an 9 V für 10 mA", si(vz, "V"), si((9.0 - vz) / 0.01, "Ω"))),
                ]);
            }
            v
        }
        Led { color } => {
            let r = |u: f64| si((u - color.vf()) / 0.015, "Ω");
            vec![
                ("Farbe", color.label().into()),
                ("Flussspannung", format!("≈ {} bei 20 mA", si(color.vf(), "V"))),
                ("Betriebsstrom", "2 … 20 mA (hell schon ab 5 mA)".into()),
                ("Brennt durch", "über 30 mA".into()),
                ("Vorwiderstand", format!("für 15 mA: {} an 5 V, {} an 9 V", r(5.0), r(9.0))),
            ]
        }
        Transistor { kind: t } => {
            let (i, p) = t.ratings();
            let mut v = vec![
                ("Typ", format!("{} ({})", t.label(), family_name(t.family()))),
                (if t.is_bjt() { "Größter Kollektorstrom" } else { "Größter Drainstrom" }, si(i, "A")),
                ("Verlustleistung", format!("höchstens {}", si(p, "W"))),
                (if t.is_bjt() { "Größte U_CE" } else { "Größte U_DS" }, si(t.v_max(), "V")),
            ];
            if let Some(m) = t.bjt() {
                v.push(("Stromverstärkung β", format!("≈ {:.0}", m.bf)));
                v.push(("Basisstrom zum Schalten", format!("I_B ≈ I_C / {:.0} (lieber 5× mehr)", (m.bf / 10.0).max(10.0))));
            }
            if let Some(m) = t.mos() {
                v.push(("Schwellspannung", format!("≈ {}", si(m.vth, "V"))));
                let ron = |ugs: f64| if ugs > m.vth { si(1.0 / (2.0 * m.k * (ugs - m.vth)), "Ω") } else { "sperrt".into() };
                v.push(("R_DS(on)", format!("{} bei 5 V, {} bei 10 V am Gate", ron(5.0), ron(10.0))));
            }
            v
        }
        Potentiometer { r, .. } => vec![("Gesamtwiderstand", si(*r, "Ω")), ("Belastbarkeit", "0,1 W".into())],
        Switch { .. } | PushButton => vec![("Kontakt", "1 mΩ geschlossen, offen praktisch unendlich".into())],
        Voltmeter => vec![("Innenwiderstand", "unendlich (ideal): misst ohne zu belasten".into())],
        Ammeter => vec![("Innenwiderstand", "0 Ω (ideal): wird in die Leitung eingefügt".into())],
        Gate { .. } | LogicInput { .. } | Button | Clock { .. } | LogicHigh | LogicLed { .. } | DFlipFlop | JkFlipFlop | Counter4 | SevenSeg => {
            let mut v = vec![
                ("Art", "idealer Logikbaustein: nur Pegel 0, 1, X, Z".into()),
                ("Versorgung", "keine nötig (wird intern mit 5 V gerechnet)".into()),
                ("Laufzeit", "≈ 10 ns".into()),
            ];
            if let Some(real) = real_counterpart(kind) {
                v.push(("Echtes Gegenstück", format!("{} (Logik-ICs, mit VCC und GND)", real.ic_type().unwrap_or_default())));
            }
            v
        }
        GateIc { family, .. } | SchmittIc { family } | DffIc { family } | CounterIc { family } => {
            let mut v = vec![("Baustein", kind.ic_type().unwrap_or_default())];
            v.extend(family_rows(*family));
            if matches!(kind, SchmittIc { .. }) {
                let (lo, hi) = family.schmitt();
                v.push((
                    "Hysterese",
                    format!("schaltet bei {} auf 1 und bei {} auf 0", level_text(hi, typical_vcc(*family)), level_text(lo, typical_vcc(*family))),
                ));
            }
            v
        }
        Timer555 => vec![
            ("Baustein", "NE555 (bipolar)".into()),
            ("Versorgung", "4,5 … 16 V (zerstört über 18 V)".into()),
            ("Ausgang 1", "≈ VCC − 1,7 V, bis 200 mA".into()),
            ("Schwellen", "TRIG unter ⅓ VCC startet, THR über ⅔ VCC beendet".into()),
            ("Ruhestrom", "≈ 3 mA".into()),
            ("Astabil", "f = 1,44 / ((R_A + 2 R_B) · C)".into()),
        ],
        OpAmp { kind: o } => {
            let m = o.model();
            let (lo, hi) = o.supply();
            vec![
                ("Typ", o.label().into()),
                ("Versorgung", format!("{} … {} (V+ − V−)", si(lo, "V"), si(hi, "V"))),
                ("Ausgangsbereich", format!("V− + {} … V+ − {}", si(m.drop_lo, "V"), si(m.drop_hi, "V"))),
                ("Leerlaufverstärkung", format!("{:.0}", m.gain)),
                ("Ausgangswiderstand", si(m.r_out, "Ω")),
            ]
        }
        Adc4 => vec![("Auflösung", "4 Bit (16 Stufen)".into()), ("Code", "⌊16 · U_IN / U_REF⌋".into()), ("Versorgung", "2 … 6 V wie 74HC".into())],
        Dac4 => vec![("Auflösung", "4 Bit".into()), ("Ausgang", "U_REF · Code / 16, Innenwiderstand 10 kΩ".into())],
        _ => Vec::new(),
    }
}

fn family_name(f: TransistorFamily) -> &'static str {
    match f {
        TransistorFamily::Npn => "NPN",
        TransistorFamily::Pnp => "PNP",
        TransistorFamily::Nmos => "N-Kanal-MOSFET",
        TransistorFamily::Pmos => "P-Kanal-MOSFET",
    }
}

/// Echtes IC mit Versorgung, das einem idealen Logikbaustein entspricht.
pub fn real_counterpart(kind: &PartKind) -> Option<PartKind> {
    let f = LogicFamily::Hc;
    Some(match kind {
        PartKind::Gate { gate } => PartKind::GateIc { gate: *gate, family: f },
        PartKind::DFlipFlop => PartKind::DffIc { family: f },
        PartKind::Counter4 => PartKind::CounterIc { family: f },
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Beschaltung
// ---------------------------------------------------------------------------

/// Was an einem Anschluss erwartet wird.
#[derive(Clone, Debug, PartialEq)]
pub enum Expect {
    /// Versorgung gegen den Masseanschluss `gnd` (Index) bzw. gegen Masse
    Supply { min: f64, max: f64, destroy: f64, gnd: Option<usize> },
    /// an Masse
    Gnd,
    /// Logikeingang: nie offen lassen
    Input,
    /// low-aktiver Eingang: für den Normalbetrieb an 1 (Plus)
    LowActive,
    /// Ausgang mit größtem Strom in A
    Output { i_max: f64 },
    /// analoger Eingang, darf nicht offen sein
    Analog,
    /// nur Hinweis; offen ist auffällig
    Note,
    /// darf offen bleiben
    Free,
    /// Plus eines gepolten Bauteils: muss die höhere Spannung haben
    PolarPlus,
}

/// Ein Anschluss mit seiner Aufgabe.
#[derive(Clone, Debug)]
pub struct PinGuide {
    pub pin: usize,
    pub what: String,
    pub expect: Expect,
}

fn g(pin: usize, what: impl Into<String>, expect: Expect) -> PinGuide {
    PinGuide { pin, what: what.into(), expect }
}

/// Was an jeden Anschluss gehört.
pub fn pin_guide(kind: &PartKind) -> Vec<PinGuide> {
    use Expect::*;
    use PartKind::*;
    let fam_supply = |f: LogicFamily, gnd: usize| Supply { min: f.v_min(), max: f.v_rec_max(), destroy: f.v_max(), gnd: Some(gnd) };
    let n = kind.local_pins().len();
    match kind {
        GateIc { family, .. } => gate_guide(*family, gate_inputs(kind)),
        SchmittIc { family } => gate_guide(*family, 1),
        DffIc { family } => vec![
            g(0, "Dateneingang: wird bei der steigenden Flanke an C übernommen", Input),
            g(1, "Takt: steigende Flanke speichert D", Input),
            g(2, "Ausgang Q", Output { i_max: output_limit(*family) }),
            g(3, "Ausgang /Q (umgekehrt)", Output { i_max: output_limit(*family) }),
            g(4, "Plus der Versorgung", fam_supply(*family, 5)),
            g(5, "Masse", Gnd),
        ],
        CounterIc { family } => {
            let mut v = vec![
                g(0, "Takt: jede steigende Flanke zählt eins weiter", Input),
                g(1, "/MR (low-aktiv): an Plus zum Zählen, kurz auf 0 setzt auf 0 zurück", LowActive),
            ];
            for k in 0..4 {
                v.push(g(2 + k, format!("Ausgang Bit {k} (Wert {})", 1 << k), Output { i_max: output_limit(*family) }));
            }
            v.push(g(6, "Plus der Versorgung", fam_supply(*family, 7)));
            v.push(g(7, "Masse", Gnd));
            v
        }
        Timer555 => vec![
            g(0, "TRIG: unter ⅓ VCC startet einen Puls. Beim Blinker mit THR am Kondensator verbunden", Analog),
            g(1, "THR: über ⅔ VCC endet der Puls", Analog),
            g(2, "CTRL: frei lassen (oder 10 nF nach Masse)", Free),
            g(3, "Ausgang: bis 200 mA, High ≈ VCC − 1,7 V", Output { i_max: 0.2 }),
            g(4, "DIS: entlädt den Zeitkondensator (zwischen R_A und R_B)", Note),
            g(5, "Plus der Versorgung (4,5 … 16 V)", Supply { min: 4.5, max: 16.0, destroy: 18.0, gnd: Some(7) }),
            g(6, "/RES (low-aktiv): an Plus, sonst bleibt der Ausgang aus", LowActive),
            g(7, "Masse", Gnd),
        ],
        OpAmp { kind: o } => {
            let (lo, hi) = o.supply();
            vec![
                g(0, "+ Eingang: das Signal (nichtinvertierender Verstärker) oder die Vergleichsspannung", Analog),
                g(1, "− Eingang: Gegenkopplung vom Ausgang (Spannungsteiler) für einen Verstärker", Analog),
                g(2, "Ausgang: regelt so, dass + und − gleich sind (mit Gegenkopplung)", Output { i_max: 0.02 }),
                g(
                    3,
                    format!("V+: Plus der Versorgung ({} … {} gesamt)", si(lo, "V"), si(hi, "V")),
                    Supply { min: lo, max: hi, destroy: hi * 1.1, gnd: Some(4) },
                ),
                g(4, "V−: Masse (einfache Versorgung) oder negative Spannung", Note),
            ]
        }
        Adc4 => {
            let mut v = vec![g(0, "Analogeingang: die zu messende Spannung (0 … U_REF)", Analog)];
            for k in 0..4 {
                v.push(g(1 + k, format!("Ausgang Bit {k}"), Output { i_max: 0.004 }));
            }
            v.push(g(5, "Plus der Versorgung (2 … 6 V)", Supply { min: 2.0, max: 6.0, destroy: 7.0, gnd: Some(7) }));
            v.push(g(6, "Referenz: Spannung für den Code 16, meist an VCC", Analog));
            v.push(g(7, "Masse", Gnd));
            v
        }
        Dac4 => {
            let mut v: Vec<PinGuide> = (0..4).map(|k| g(k, format!("Eingang Bit {k}"), Input)).collect();
            v.push(g(4, "Analogausgang (10 kΩ Innenwiderstand)", Free));
            v.push(g(5, "Plus der Versorgung (2 … 6 V)", Supply { min: 2.0, max: 6.0, destroy: 7.0, gnd: Some(7) }));
            v.push(g(6, "Referenz: Spannung bei Code 16, meist an VCC", Analog));
            v.push(g(7, "Masse", Gnd));
            v
        }
        Led { .. } => vec![g(0, "Anode: über den Vorwiderstand zum Plus", PolarPlus), g(1, "Kathode (Strich, kurzes Bein): Richtung Minus", Note)],
        Diode { kind: d } if d.zener().is_some() => vec![
            g(0, "Anode: an Masse (Minus)", Note),
            g(1, "Kathode (Strich): über einen Vorwiderstand zum Plus – Zener arbeiten in Sperrrichtung", Note),
        ],
        Diode { .. } => vec![g(0, "Anode: Strom fließt von hier …", PolarPlus), g(1, "Kathode (Strich): … nach hier", Note)],
        Capacitor { kind: ck, .. } if ck.polarized() => {
            vec![g(0, "Plus (langes Bein): an die höhere Spannung", PolarPlus), g(1, "Minus (Streifen auf dem Gehäuse)", Note)]
        }
        Transistor { kind: t } => match t.family() {
            TransistorFamily::Npn => vec![
                g(0, "Basis: über einen Basiswiderstand (1 … 10 kΩ) ansteuern, nie direkt an Plus", Note),
                g(1, "Kollektor: Last (LED mit Vorwiderstand, Lampe, Relais) zum Plus", Note),
                g(2, "Emitter: an Masse", Gnd),
            ],
            TransistorFamily::Pnp => vec![
                g(0, "Basis: über einen Widerstand nach Masse = an, nach Plus = aus", Note),
                g(1, "Kollektor: Last nach Masse", Note),
                g(2, "Emitter: an Plus", Note),
            ],
            TransistorFamily::Nmos => vec![
                g(0, "Gate: Steuerspannung. Pull-down (10 … 100 kΩ) nach Masse, sonst schwebt es", Analog),
                g(1, "Drain: Last zum Plus", Note),
                g(2, "Source: an Masse", Gnd),
            ],
            TransistorFamily::Pmos => vec![
                g(0, "Gate: an Masse = an, an Plus = aus. Pull-up nach Plus", Analog),
                g(1, "Drain: Last nach Masse", Note),
                g(2, "Source: an Plus", Note),
            ],
        },
        Gate { .. } | DFlipFlop | JkFlipFlop | Counter4 | LogicLed { .. } | SevenSeg => {
            let roles = kind.pin_roles();
            let names = kind.pin_names();
            (0..n)
                .map(|k| match roles[k] {
                    crate::model::PinRole::In => g(k, format!("Eingang {}", names[k]), Input),
                    _ => g(k, format!("Ausgang {}", names[k]), Free),
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn gate_guide(family: LogicFamily, n_in: usize) -> Vec<PinGuide> {
    use Expect::*;
    let mut v: Vec<PinGuide> =
        (0..n_in).map(|k| g(k, "Eingang: an ein Signal oder über Pull-down (10 kΩ) an Masse – nie offen lassen", Input)).collect();
    v.push(g(n_in, "Ausgang: LED nur mit Vorwiderstand (≥ 330 Ω)", Output { i_max: output_limit(family) }));
    v.push(g(
        n_in + 1,
        format!("Plus der Versorgung ({} … {})", si(family.v_min(), "V"), si(family.v_rec_max(), "V")),
        Supply { min: family.v_min(), max: family.v_rec_max(), destroy: family.v_max(), gnd: Some(n_in + 2) },
    ));
    v.push(g(n_in + 2, "Masse", Gnd));
    v
}

fn gate_inputs(kind: &PartKind) -> usize {
    kind.ic_pins().iter().filter(|p| **p == IcPin::In).count()
}

/// Größter sinnvoller Ausgangsstrom einer Logikfamilie in A
fn output_limit(f: LogicFamily) -> f64 {
    match f {
        LogicFamily::Hc | LogicFamily::Hct => 0.025,
        LogicFamily::Ls => 0.016,
        LogicFamily::Cd4000 => 0.005,
    }
}

/// Ergebnis der Prüfung eines Anschlusses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// richtig
    Ok,
    /// nur Hinweis, nichts zu prüfen
    Info,
    /// auffällig
    Warn,
    /// falsch
    Bad,
}

#[derive(Clone, Debug)]
pub struct PinCheck {
    pub name: String,
    pub what: String,
    pub status: Status,
    /// Messwert oder Problem
    pub detail: String,
}

/// Prüft die Beschaltung eines Bauteils.
///
/// `logic`: Pegel, die ein IC an seinen Anschlüssen sieht (Echtzeitbetrieb).
pub fn check_pins(doc: &Document, nl: &Netlist, op: Option<&OpView>, logic: Option<&[Option<Logic>]>, part: &Part) -> Vec<PinCheck> {
    let names = part.kind.pin_names();
    let nets = nl.pin_nets.get(&part.id);
    let net = |k: usize| nets.and_then(|n| n.get(k).copied());
    let volt = |k: usize| op.and_then(|o| o.net_v.get(net(k)?).copied().flatten());
    let is_gnd = |k: usize| net(k).is_some_and(|n| nl.nets[n].is_gnd);
    // Wie viele Anschlüsse (aller Bauteile) liegen an einem Netz?
    let fanout = |n: usize| nl.pin_nets.values().flatten().filter(|&&m| m == n).count();
    let open = |k: usize| net(k).is_none_or(|n| fanout(n) <= 1);
    let pin_i = |k: usize| op.and_then(|o| o.pin_i.get(&part.id)?.get(k).copied());
    let lvl = |k: usize| logic.and_then(|l| l.get(k).copied().flatten());
    let _ = doc;
    pin_guide(&part.kind)
        .into_iter()
        .map(|pg| {
            let k = pg.pin;
            let (status, detail) = match &pg.expect {
                Expect::Gnd if open(k) => (Status::Bad, "offen: an Masse anschließen".into()),
                Expect::Gnd if is_gnd(k) => (Status::Ok, "an Masse".into()),
                Expect::Gnd => (Status::Bad, format!("nicht an Masse{}", volt(k).map(|v| format!(" ({})", si(v, "V"))).unwrap_or_default())),
                Expect::Supply { min, max, destroy, gnd } => {
                    if open(k) {
                        (Status::Bad, "offen: an Plus anschließen".into())
                    } else {
                        let base = gnd.and_then(volt).unwrap_or(0.0);
                        match volt(k).map(|v| v - base) {
                            None => (Status::Warn, "keine Spannung berechnet".into()),
                            Some(u) if u < -0.3 => (Status::Bad, format!("verpolt ({}): Plus und Masse vertauscht", si(u, "V"))),
                            Some(u) if u < *min => (Status::Bad, format!("nur {} – braucht {} … {}", si(u, "V"), si(*min, "V"), si(*max, "V"))),
                            Some(u) if u > *destroy => (Status::Bad, format!("{} – zerstört über {}", si(u, "V"), si(*destroy, "V"))),
                            Some(u) if u > *max => (Status::Warn, format!("{} – über der Empfehlung ({})", si(u, "V"), si(*max, "V"))),
                            Some(u) => (Status::Ok, si(u, "V")),
                        }
                    }
                }
                Expect::Input | Expect::LowActive if open(k) => (Status::Bad, "offen: Signal oder Pull-down/-up anschließen".into()),
                Expect::Input | Expect::LowActive => match lvl(k) {
                    Some(Logic::X) => (Status::Warn, "unbestimmt (X): hängt in der Luft oder Spannung zwischen den Schwellen".into()),
                    Some(Logic::L0) if pg.expect == Expect::LowActive => (Status::Warn, "0: aktiv – der Baustein wird zurückgesetzt".into()),
                    Some(l) => (Status::Ok, format!("Pegel {}{}", l.symbol(), volt(k).map(|v| format!(" ({})", si(v, "V"))).unwrap_or_default())),
                    None => (Status::Ok, volt(k).map(|v| si(v, "V")).unwrap_or_else(|| "angeschlossen".into())),
                },
                Expect::Analog if open(k) => (Status::Bad, "offen: muss angeschlossen sein".into()),
                Expect::Analog => (Status::Ok, volt(k).map(|v| si(v, "V")).unwrap_or_else(|| "angeschlossen".into())),
                Expect::Output { i_max } => match pin_i(k) {
                    Some(i) if i.abs() > *i_max => (Status::Warn, format!("überlastet: {} (höchstens {})", si(i.abs(), "A"), si(*i_max, "A"))),
                    Some(i) if i.abs() > 1e-6 => (Status::Ok, format!("liefert {}", si(i.abs(), "A"))),
                    _ if open(k) => (Status::Info, "noch nichts angeschlossen".into()),
                    _ => (Status::Ok, volt(k).map(|v| si(v, "V")).unwrap_or_default()),
                },
                Expect::PolarPlus => {
                    let u = op.and_then(|o| o.parts.get(&part.id)).map(|v| v.u);
                    match u {
                        Some(u) if u < -0.5 => (Status::Bad, format!("verkehrt herum: {} gegen den anderen Anschluss", si(u, "V"))),
                        _ if open(k) => (Status::Bad, "offen".into()),
                        Some(u) => (Status::Ok, format!("{} höher als der andere Anschluss", si(u.max(0.0), "V"))),
                        None => (Status::Info, String::new()),
                    }
                }
                Expect::Note if open(k) => (Status::Warn, "offen".into()),
                Expect::Note | Expect::Free => (Status::Info, volt(k).map(|v| si(v, "V")).unwrap_or_default()),
            };
            PinCheck { name: names.get(k).cloned().unwrap_or_default(), what: pg.what, status, detail }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{examples, netlist, sim};

    fn example(title: &str) -> Document {
        examples::all().into_iter().find(|e| e.title.starts_with(title)).unwrap().doc
    }

    #[test]
    fn every_palette_part_has_a_datasheet_and_valid_guides() {
        for (_, kinds) in PartKind::palette() {
            for k in kinds {
                let n = k.local_pins().len();
                for pg in pin_guide(&k) {
                    assert!(pg.pin < n, "{}: Anschluss {} von {n}", k.label(), pg.pin);
                }
                if !matches!(k, PartKind::Ground | PartKind::Probe) {
                    assert!(!datasheet(&k).is_empty(), "{} hat kein Datenblatt", k.label());
                }
            }
        }
    }

    #[test]
    fn timer_example_is_wired_correctly() {
        let d = example("555-Blinker");
        let nl = netlist::build(&d);
        let mut ms = sim::mixed_sim(&nl, 1e-3).unwrap();
        ms.advance(0.3, None).unwrap();
        let v = sim::mixed_view(&nl, &ms);
        let u1 = d.part_by_name("U1").unwrap();
        let checks = check_pins(&d, &nl, Some(&v), None, u1);
        let bad: Vec<_> = checks.iter().filter(|c| c.status >= Status::Warn).collect();
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn open_supply_and_ground_are_reported() {
        let mut d = Document::default();
        let id = d.add_part(PartKind::GateIc { gate: crate::model::GateType::Nand, family: LogicFamily::Hc }, crate::model::P::new(0, 0), 0);
        let nl = netlist::build(&d);
        let part = d.part(id).unwrap();
        let checks = check_pins(&d, &nl, None, None, part);
        let st = |name: &str| checks.iter().find(|c| c.name == name).unwrap().status;
        assert_eq!(st("VCC"), Status::Bad);
        assert_eq!(st("GND"), Status::Bad);
        assert_eq!(st("A"), Status::Bad);
        assert_eq!(st("Y"), Status::Info);
    }
}
