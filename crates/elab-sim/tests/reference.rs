//! Referenztests: Simulationsergebnisse gegen analytische Lösungen.
//!
//! Jeder Test entspricht einer klassischen Lehrbuchschaltung. Wenn ein Test
//! hier fehlschlägt, rechnet der Simulator falsch, nicht nur ungenau.

use elab_sim::*;
use std::f64::consts::PI;

fn op(c: &Circuit) -> OpResult {
    Simulator::new(c).unwrap().operating_point().unwrap()
}

fn tran(c: &Circuit, opts: TranOptions) -> TranResult {
    Simulator::new(c).unwrap().transient(&opts).unwrap()
}

fn assert_close(actual: f64, expected: f64, tol: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: erwartet {expected:.9}, simuliert {actual:.9} (Abweichung {:.3e}, erlaubt {tol:.1e})",
        (actual - expected).abs()
    );
}

/// Löst f(x) = 0 per Bisektion für die analytischen Diodenreferenzen.
fn bisect(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> f64 {
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(lo) * f(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

// ---------------------------------------------------------------------------
// Gleichstrom, linear
// ---------------------------------------------------------------------------

#[test]
fn spannungsteiler() {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 10.0).unwrap();
    c.resistor("R1", "in", "out", 1e3).unwrap();
    c.resistor("R2", "out", "gnd", 2e3).unwrap();
    let r = op(&c);
    assert_close(r.voltage("out").unwrap(), 20.0 / 3.0, 1e-6, "U_out");
    // Die Quelle liefert denselben Strom, der durch die Widerstände fließt.
    assert_close(r.current("V1").unwrap(), 10.0 / 3e3, 1e-10, "I_V1");
    assert_close(r.current("R1").unwrap(), 10.0 / 3e3, 1e-10, "I_R1");
    assert_close(r.power("V1").unwrap(), 100.0 / 3e3, 1e-9, "P_V1");
}

#[test]
fn stromquelle_an_widerstand() {
    let mut c = Circuit::new();
    c.isource("I1", "a", "gnd", 1e-3).unwrap();
    c.resistor("R1", "a", "gnd", 1e3).unwrap();
    assert_close(op(&c).voltage("a").unwrap(), 1.0, 1e-6, "U_a");
}

#[test]
fn ueberlagerung_spannungs_und_stromquelle() {
    // 10 V über 1k/1k-Teiler, zusätzlich 1 mA in den Mittelpunkt:
    // U_m = 5 V + 1 mA · (1k ∥ 1k) = 5,5 V
    let mut c = Circuit::new();
    c.vsource("V1", "a", "gnd", 10.0).unwrap();
    c.resistor("R1", "a", "m", 1e3).unwrap();
    c.resistor("R2", "m", "gnd", 1e3).unwrap();
    c.isource("I1", "m", "gnd", 1e-3).unwrap();
    assert_close(op(&c).voltage("m").unwrap(), 5.5, 1e-6, "U_m");
}

#[test]
fn wheatstone_bruecke() {
    // Verstimmte Brücke: links 1k/2k, rechts 2k/3k an 12 V.
    let mut c = Circuit::new();
    c.vsource("V1", "top", "gnd", 12.0).unwrap();
    c.resistor("R1", "top", "l", 1e3).unwrap();
    c.resistor("R2", "l", "gnd", 2e3).unwrap();
    c.resistor("R3", "top", "r", 2e3).unwrap();
    c.resistor("R4", "r", "gnd", 3e3).unwrap();
    c.resistor("Rm", "l", "r", 5e3).unwrap();
    // Referenz per Knotenanalyse von Hand (2x2-System):
    // l: (U_l-12)/1k + U_l/2k + (U_l-U_r)/5k = 0
    // r: (U_r-12)/2k + U_r/3k + (U_r-U_l)/5k = 0
    let (a11, a12, b1) = (1.0 / 1e3 + 1.0 / 2e3 + 1.0 / 5e3, -1.0 / 5e3, 12.0 / 1e3);
    let (a21, a22, b2) = (-1.0 / 5e3, 1.0 / 2e3 + 1.0 / 3e3 + 1.0 / 5e3, 12.0 / 2e3);
    let det = a11 * a22 - a12 * a21;
    let ul = (b1 * a22 - a12 * b2) / det;
    let ur = (a11 * b2 - b1 * a21) / det;
    let r = op(&c);
    assert_close(r.voltage("l").unwrap(), ul, 1e-6, "U_l");
    assert_close(r.voltage("r").unwrap(), ur, 1e-6, "U_r");
    assert_close(r.current("Rm").unwrap(), (ul - ur) / 5e3, 1e-9, "I_Rm");
}

#[test]
fn arbeitspunkt_spule_kurz_kondensator_offen() {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 10.0).unwrap();
    c.inductor("L1", "in", "m", 1e-3).unwrap();
    c.resistor("R1", "m", "gnd", 1e3).unwrap();
    c.capacitor("C1", "m", "gnd", 1e-6).unwrap();
    let r = op(&c);
    assert_close(r.voltage("m").unwrap(), 10.0, 1e-6, "U_m");
    assert_close(r.current("L1").unwrap(), 10e-3, 1e-9, "I_L");
    assert_close(r.current("C1").unwrap(), 0.0, 1e-15, "I_C");
}

#[test]
fn schalter_offen_und_geschlossen() {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 5.0).unwrap();
    c.switch("S1", "in", "out", false).unwrap();
    c.resistor("R1", "out", "gnd", 1e3).unwrap();
    assert!(op(&c).voltage("out").unwrap().abs() < 1e-6);
    let s = c.elements()[c.find_element("S1").unwrap()].kind.clone();
    if let ElementKind::Switch { a, b, r_on, r_off, .. } = s {
        c.update("S1", ElementKind::Switch { a, b, closed: true, r_on, r_off }).unwrap();
    }
    assert_close(op(&c).voltage("out").unwrap(), 5.0, 1e-5, "U_out geschlossen");
}

// ---------------------------------------------------------------------------
// Gleichstrom, nichtlinear
// ---------------------------------------------------------------------------

#[test]
fn diode_in_durchlassrichtung() {
    let m = DiodeModel::IDEAL;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 5.0).unwrap();
    c.resistor("R1", "in", "a", 1e3).unwrap();
    c.diode("D1", "a", "gnd", m).unwrap();
    let vd = bisect(0.0, 1.0, |v| (5.0 - v) / 1e3 - m.is * ((v / m.nvt()).exp() - 1.0));
    let r = op(&c);
    assert_close(r.voltage("a").unwrap(), vd, 1e-6, "U_D");
    assert_close(r.current("D1").unwrap(), (5.0 - vd) / 1e3, 1e-8, "I_D");
}

#[test]
fn diode_in_sperrrichtung() {
    let m = DiodeModel::IDEAL;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", -5.0).unwrap();
    c.resistor("R1", "in", "a", 1e3).unwrap();
    c.diode("D1", "a", "gnd", m).unwrap();
    let r = op(&c);
    assert_close(r.voltage("a").unwrap(), -5.0, 1e-6, "U_D");
    assert!(r.current("D1").unwrap().abs() < 1e-11, "Sperrstrom zu groß");
}

#[test]
fn led_mit_vorwiderstand() {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 5.0).unwrap();
    c.resistor("R1", "in", "a", 150.0).unwrap();
    c.diode("LED", "a", "gnd", DiodeModel::LED_RED).unwrap();
    let r = op(&c);
    let u = r.voltage("a").unwrap();
    let i = r.current("LED").unwrap();
    assert!(u > 1.7 && u < 1.9, "U_LED = {u}");
    assert_close(i, (5.0 - u) / 150.0, 1e-7, "I_LED");
}

#[test]
fn harter_fall_diode_fast_direkt_an_quelle() {
    // 1 Ω Vorwiderstand: Newton muss mehrere Ampere finden, ohne zu explodieren.
    let m = DiodeModel::IDEAL;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 20.0).unwrap();
    c.resistor("R1", "in", "a", 1.0).unwrap();
    c.diode("D1", "a", "gnd", m).unwrap();
    let vd = bisect(0.0, 2.0, |v| (20.0 - v) / 1.0 - m.is * ((v / m.nvt()).exp() - 1.0));
    assert_close(op(&c).voltage("a").unwrap(), vd, 1e-5, "U_D");
}

#[test]
fn diodenkette_und_antiparallel() {
    // Drei Dioden in Reihe plus eine antiparallele: nur die Kette leitet.
    let m = DiodeModel::SI_1N4148;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", 12.0).unwrap();
    c.resistor("R1", "in", "a", 470.0).unwrap();
    c.diode("D1", "a", "b", m).unwrap();
    c.diode("D2", "b", "c", m).unwrap();
    c.diode("D3", "c", "gnd", m).unwrap();
    c.diode("D4", "gnd", "a", m).unwrap();
    let r = op(&c);
    let ua = r.voltage("a").unwrap();
    // 3 gleiche Dioden teilen sich die Spannung gleichmäßig
    let i = (12.0 - ua) / 470.0;
    let vd_one = m.nvt() * (i / m.is + 1.0).ln();
    assert_close(ua, 3.0 * vd_one, 1e-4, "U_a");
    assert!(r.current("D4").unwrap() < 0.0 && r.current("D4").unwrap() > -1e-8);
}

// ---------------------------------------------------------------------------
// Zeitverhalten
// ---------------------------------------------------------------------------

#[test]
fn rc_ladekurve() {
    let (r, cap, u0) = (1e3, 1e-6, 5.0);
    let tau = r * cap;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(u0, 0.0)).unwrap();
    c.resistor("R1", "in", "out", r).unwrap();
    c.capacitor("C1", "out", "gnd", cap).unwrap();
    let res = tran(&c, TranOptions::new(5.0 * tau, tau / 50.0));
    let (t, v) = (&res.time, res.voltage("out").unwrap());
    let mut worst: f64 = 0.0;
    for k in 0..t.len() {
        let exact = u0 * (1.0 - (-(t[k] - 1e-9).max(0.0) / tau).exp());
        worst = worst.max((v[k] - exact).abs());
    }
    assert!(worst < 1e-3 * u0, "maximale Abweichung {worst:.3e} V");
    // Nach einer Zeitkonstante: 63,2 %
    assert_close(res.voltage_at("out", tau).unwrap(), u0 * (1.0 - (-1.0f64).exp()), 5e-3, "U(τ)");
    // Ladestrom zu Beginn ≈ U0/R, Kondensatorstrom = Widerstandsstrom
    let ic = res.current_at("C1", 0.5 * tau).unwrap();
    let ir = res.current_at("R1", 0.5 * tau).unwrap();
    assert_close(ic, ir, 1e-6, "I_C = I_R");
}

#[test]
fn rc_genauigkeit_bei_grobem_hmax() {
    // h_max = τ: Die Fehlersteuerung muss die Schritte selbst klein genug halten.
    let tau = 1e-3;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(1.0, 0.0)).unwrap();
    c.resistor("R1", "in", "out", 1e3).unwrap();
    c.capacitor("C1", "out", "gnd", 1e-6).unwrap();
    let mut sim = Simulator::new(&c).unwrap();
    let res = sim.transient(&TranOptions::new(5.0 * tau, tau)).unwrap();
    let v = res.voltage("out").unwrap();
    for (k, &t) in res.time.iter().enumerate() {
        let exact = 1.0 - (-(t - 1e-9).max(0.0) / tau).exp();
        assert!((v[k] - exact).abs() < 2e-3, "t = {t:.3e}: {} statt {exact}", v[k]);
    }
}

#[test]
fn rl_einschaltvorgang() {
    let (r, l, u0) = (100.0, 0.1, 10.0);
    let tau = l / r;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(u0, 0.0)).unwrap();
    c.resistor("R1", "in", "m", r).unwrap();
    c.inductor("L1", "m", "gnd", l).unwrap();
    let res = tran(&c, TranOptions::new(5.0 * tau, tau / 50.0));
    let i = res.current("L1").unwrap();
    for (k, &t) in res.time.iter().enumerate() {
        let exact = u0 / r * (1.0 - (-(t - 1e-9).max(0.0) / tau).exp());
        assert!((i[k] - exact).abs() < 1e-3 * u0 / r, "t = {t:.3e}: {} statt {exact}", i[k]);
    }
}

#[test]
fn lc_schwingkreis_frequenz_und_energie() {
    let (l, cap): (f64, f64) = (1e-3, 1e-6);
    let f0 = 1.0 / (2.0 * PI * (l * cap).sqrt());
    let period = 1.0 / f0;
    let mut c = Circuit::new();
    let n = c.node("n");
    c.add("C1", ElementKind::Capacitor { a: n, b: GND, c: cap, ic: Some(1.0) }).unwrap();
    c.add("L1", ElementKind::Inductor { a: n, b: GND, l, ic: Some(0.0) }).unwrap();
    let res = tran(&c, TranOptions::new(10.0 * period, period / 100.0).with_ic());
    let v = res.voltage("n").unwrap();
    let t = &res.time;

    // Periodendauer aus Nulldurchgängen von + nach − (linear interpoliert)
    let mut crossings = Vec::new();
    for k in 1..v.len() {
        if v[k - 1] > 0.0 && v[k] <= 0.0 {
            crossings.push(t[k - 1] + (t[k] - t[k - 1]) * v[k - 1] / (v[k - 1] - v[k]));
        }
    }
    assert!(crossings.len() >= 9, "zu wenige Schwingungen: {}", crossings.len());
    let measured = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
    assert_close(measured, period, 2e-3 * period, "Periodendauer");

    // Trapezregel ist energieerhaltend: Amplitude bleibt nahezu konstant.
    let last_period: Vec<f64> = t.iter().zip(&v).filter(|(tt, _)| **tt > 9.0 * period).map(|(_, vv)| vv.abs()).collect();
    let amp = last_period.iter().cloned().fold(0.0, f64::max);
    assert!(amp > 0.98 && amp < 1.01, "Amplitude nach 10 Perioden: {amp}");
}

#[test]
fn rlc_sprungantwort_unterdaempft() {
    let (r, l, cap, u0): (f64, f64, f64, f64) = (10.0, 1e-3, 1e-6, 1.0);
    let alpha = r / (2.0 * l);
    let w0 = 1.0 / (l * cap).sqrt();
    let wd = (w0 * w0 - alpha * alpha).sqrt();
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(u0, 0.0)).unwrap();
    c.resistor("R1", "in", "a", r).unwrap();
    c.inductor("L1", "a", "b", l).unwrap();
    c.capacitor("C1", "b", "gnd", cap).unwrap();
    let res = tran(&c, TranOptions::new(1e-3, 1e-6));
    let v = res.voltage("b").unwrap();
    let mut worst: f64 = 0.0;
    for (k, &t) in res.time.iter().enumerate() {
        let tt = (t - 1e-9).max(0.0);
        let exact = u0 * (1.0 - (-alpha * tt).exp() * ((wd * tt).cos() + alpha / wd * (wd * tt).sin()));
        worst = worst.max((v[k] - exact).abs());
    }
    assert!(worst < 5e-3 * u0, "maximale Abweichung {worst:.3e} V");
    // Überschwingen deutlich sichtbar (theoretisch ≈ 85 %)
    let peak = v.iter().cloned().fold(f64::MIN, f64::max);
    let expected_peak = u0 * (1.0 + (-alpha * PI / wd).exp());
    assert_close(peak, expected_peak, 5e-3, "Spitzenwert");
}

#[test]
fn rechteck_auf_rc_trifft_knickstellen() {
    // 1 kHz Rechteck 0/5 V auf RC mit τ = 0,1 ms.
    let tau = 1e-4;
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::square(0.0, 5.0, 1e3)).unwrap();
    c.resistor("R1", "in", "out", 1e3).unwrap();
    c.capacitor("C1", "out", "gnd", 1e-7).unwrap();
    let res = tran(&c, TranOptions::new(3e-3, 2e-5));
    // Die Flanken des Rechtecks sind exakt als Zeitpunkte enthalten
    for edge in [0.5e-3, 1.0e-3, 1.5e-3, 2.0e-3] {
        assert!(res.time.iter().any(|t| (t - edge).abs() < 1e-9), "Flanke bei {edge} fehlt");
    }
    // Am Ende der High-Phase: 5 V · (1 − e^(−5))
    let v = res.voltage_at("out", 0.5e-3 - 1e-7).unwrap();
    assert_close(v, 5.0 * (1.0 - (-5.0f64).exp()), 0.02, "U_out vor fallender Flanke");
    // Am Ende der Low-Phase ist der Kondensator fast leer
    let v = res.voltage_at("out", 1.0e-3 - 1e-7).unwrap();
    assert!(v < 0.05, "U_out vor steigender Flanke = {v}");
    let _ = tau;
}

#[test]
fn einweggleichrichter_mit_glaettung() {
    let mut c = Circuit::new();
    c.vsource("V1", "ac", "gnd", Waveform::sine(10.0, 50.0)).unwrap();
    c.diode("D1", "ac", "out", DiodeModel::SI_1N4007).unwrap();
    c.capacitor("C1", "out", "gnd", 470e-6).unwrap();
    c.resistor("RL", "out", "gnd", 1e3).unwrap();
    let res = tran(&c, TranOptions::new(0.1, 1e-4));
    let v = res.voltage("out").unwrap();
    let id = res.current("D1").unwrap();
    let peak = v.iter().cloned().fold(f64::MIN, f64::max);
    assert!(peak > 9.0 && peak < 9.6, "Spitzenspannung {peak}");
    // Brummspannung nach dem Einschwingen ≈ I/(f·C) ≈ 0,4 V
    let settled: Vec<f64> = res.time.iter().zip(&v).filter(|(t, _)| **t > 0.06).map(|(_, x)| *x).collect();
    let ripple = settled.iter().cloned().fold(f64::MIN, f64::max) - settled.iter().cloned().fold(f64::MAX, f64::min);
    assert!(ripple > 0.25 && ripple < 0.5, "Brummspannung {ripple}");
    // Die Diode sperrt: kein nennenswerter Rückstrom
    assert!(id.iter().cloned().fold(f64::MAX, f64::min) > -1e-6);
}

#[test]
fn brueckengleichrichter_230v() {
    let m = DiodeModel::SI_1N4007;
    let mut c = Circuit::new();
    c.vsource("V1", "ac1", "ac2", Waveform::sine(325.0, 50.0)).unwrap();
    c.diode("D1", "ac1", "plus", m).unwrap();
    c.diode("D2", "ac2", "plus", m).unwrap();
    c.diode("D3", "gnd", "ac1", m).unwrap();
    c.diode("D4", "gnd", "ac2", m).unwrap();
    c.capacitor("C1", "plus", "gnd", 100e-6).unwrap();
    c.resistor("RL", "plus", "gnd", 10e3).unwrap();
    let res = tran(&c, TranOptions::new(0.06, 1e-4));
    let v = res.voltage("plus").unwrap();
    let peak = v.iter().cloned().fold(f64::MIN, f64::max);
    assert!(peak > 320.0 && peak < 325.0, "Spitzenspannung {peak}");
}

// ---------------------------------------------------------------------------
// Fehlerfälle mit verständlichen Meldungen
// ---------------------------------------------------------------------------

#[test]
fn fehler_parallele_spannungsquellen() {
    let mut c = Circuit::new();
    c.vsource("V1", "a", "gnd", 5.0).unwrap();
    c.vsource("V2", "a", "gnd", 3.0).unwrap();
    c.resistor("R1", "a", "gnd", 1e3).unwrap();
    let err = Simulator::new(&c).unwrap().operating_point().unwrap_err();
    assert!(matches!(err, SimError::Singular { .. }), "{err:?}");
    println!("Meldung: {err}");
}

#[test]
fn fehler_schwebender_knoten() {
    let mut c = Circuit::new();
    c.vsource("V1", "a", "gnd", 5.0).unwrap();
    c.resistor("R1", "a", "gnd", 1e3).unwrap();
    c.resistor("R2", "x", "y", 1e3).unwrap();
    match Simulator::new(&c) {
        Err(SimError::FloatingNodes(ns)) => assert_eq!(ns, vec!["x".to_string(), "y".to_string()]),
        other => panic!("erwartet FloatingNodes, bekommen {:?}", other.err()),
    }
}

#[test]
fn fehler_ungueltige_werte() {
    let mut c = Circuit::new();
    assert!(matches!(c.resistor("R1", "a", "gnd", 0.0), Err(SimError::InvalidValue { .. })));
    assert!(matches!(c.capacitor("C1", "a", "gnd", -1e-6), Err(SimError::InvalidValue { .. })));
    c.resistor("R2", "a", "gnd", 1.0).unwrap();
    assert!(matches!(c.resistor("R2", "a", "gnd", 1.0), Err(SimError::DuplicateName(_))));
}

#[test]
fn kondensator_nur_ueber_c_angebunden() {
    // Knoten "m" hängt nur an Kondensatoren: im DC-Fall unbestimmt, darf aber nicht abstürzen.
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(10.0, 0.0)).unwrap();
    c.capacitor("C1", "in", "m", 1e-6).unwrap();
    c.capacitor("C2", "m", "gnd", 1e-6).unwrap();
    let res = tran(&c, TranOptions::new(1e-3, 1e-5));
    // Kapazitiver Teiler: zwei gleiche C halbieren den Sprung.
    assert_close(res.voltage_at("m", 0.5e-3).unwrap(), 5.0, 1e-3, "U_m");
}

// ---------------------------------------------------------------------------
// Transistoren
// ---------------------------------------------------------------------------

/// Arbeitspunkt im aktiven Bereich aus dem Modell per Bisektion: (U_BE, I_B, I_C).
fn bjt_active_reference(m: &BjtModel, vbb: f64, rb: f64) -> (f64, f64, f64) {
    let vt = elab_sim::diode::VT_300K;
    let ib = |v: f64| m.is / m.bf * ((v / vt).exp() - 1.0);
    let vbe = bisect(0.0, 1.0, |v| (vbb - v) / rb - ib(v));
    (vbe, ib(vbe), m.is * ((vbe / vt).exp() - 1.0))
}

#[test]
fn npn_emitterschaltung_aktiver_bereich() {
    let m = BjtModel::BC547;
    let mut c = Circuit::new();
    c.vsource("VCC", "vcc", "gnd", 10.0).unwrap();
    c.vsource("VBB", "vbb", "gnd", 2.0).unwrap();
    c.resistor("RB", "vbb", "b", 220e3).unwrap();
    c.resistor("RC", "vcc", "c", 1e3).unwrap();
    c.bjt("T1", "c", "b", "gnd", m).unwrap();
    let r = op(&c);
    let (vbe, ib, ic) = bjt_active_reference(&m, 2.0, 220e3);
    let t = r.terminal_currents("T1").unwrap();
    assert_close(r.voltage("b").unwrap(), vbe, 1e-4, "U_BE");
    assert_close(t[1], ib, 1e-9, "I_B");
    assert_close(t[0], ic, 1e-6, "I_C");
    assert_close(t[0] / t[1], m.bf, 0.5, "β = I_C / I_B");
    assert_close(t[0] + t[1] + t[2], 0.0, 1e-12, "Knotenregel am Transistor");
    assert_close(r.voltage("c").unwrap(), 10.0 - 1e3 * ic, 1e-3, "U_C");
}

#[test]
fn npn_als_schalter_in_saettigung() {
    let mut c = Circuit::new();
    c.vsource("VCC", "vcc", "gnd", 10.0).unwrap();
    c.vsource("VIN", "in", "gnd", 5.0).unwrap();
    c.resistor("RB", "in", "b", 4.7e3).unwrap();
    c.resistor("RC", "vcc", "c", 1e3).unwrap();
    c.bjt("T1", "c", "b", "gnd", BjtModel::BC547).unwrap();
    let r = op(&c);
    let vce = r.voltage("c").unwrap();
    let t = r.terminal_currents("T1").unwrap();
    assert!(vce > 0.0 && vce < 0.2, "U_CE,sat = {vce}");
    assert!(t[0] < BjtModel::BC547.bf * t[1] * 0.5, "übersteuert: I_C deutlich kleiner als β·I_B");
    assert_close(t[0], (10.0 - vce) / 1e3, 1e-7, "I_C über R_C");
}

#[test]
fn pnp_ist_spiegelbild_des_npn() {
    let (mn, mp) = (BjtModel::BC547, BjtModel { npn: false, ..BjtModel::BC547 });
    let build = |m: BjtModel, s: f64| {
        let mut c = Circuit::new();
        c.vsource("VCC", "vcc", "gnd", 10.0 * s).unwrap();
        c.vsource("VBB", "vbb", "gnd", 2.0 * s).unwrap();
        c.resistor("RB", "vbb", "b", 220e3).unwrap();
        c.resistor("RC", "vcc", "c", 1e3).unwrap();
        c.bjt("T1", "c", "b", "gnd", m).unwrap();
        op(&c)
    };
    let (n, p) = (build(mn, 1.0), build(mp, -1.0));
    assert_close(p.voltage("c").unwrap(), -n.voltage("c").unwrap(), 1e-6, "U_C gespiegelt");
    assert_close(p.terminal_currents("T1").unwrap()[0], -n.terminal_currents("T1").unwrap()[0], 1e-9, "I_C gespiegelt");
}

#[test]
fn nmos_saettigung() {
    let m = MosModel::N2N7000;
    let mut c = Circuit::new();
    c.vsource("VDD", "vdd", "gnd", 10.0).unwrap();
    c.vsource("VG", "g", "gnd", 3.0).unwrap();
    c.resistor("RD", "vdd", "d", 100.0).unwrap();
    c.mosfet("M1", "d", "g", "gnd", m).unwrap();
    let r = op(&c);
    let vds = r.voltage("d").unwrap();
    let id = r.terminal_currents("M1").unwrap()[0];
    // Sättigung: I_D = K·(U_GS − U_th)²·(1 + λ·U_DS) und U_DS = 10 V − 100 Ω · I_D
    let expect = m.k * 0.81 * (1.0 + m.lambda * vds);
    assert!(vds > 3.0 - m.vth, "Sättigung erwartet, U_DS = {vds}");
    assert_close(id, expect, 1e-8, "I_D");
    assert_close(vds, 10.0 - 100.0 * id, 1e-6, "U_DS");
    assert_eq!(m.eval(3.0, vds).region, MosRegion::Saturation);
}

#[test]
fn nmos_als_schalter_linearer_bereich() {
    let m = MosModel::N2N7000;
    let mut c = Circuit::new();
    c.vsource("VDD", "vdd", "gnd", 5.0).unwrap();
    c.vsource("VG", "g", "gnd", 10.0).unwrap();
    c.resistor("RD", "vdd", "d", 1e3).unwrap();
    c.mosfet("M1", "d", "g", "gnd", m).unwrap();
    let r = op(&c);
    let vds = r.voltage("d").unwrap();
    let id = (5.0 - vds) / 1e3;
    let vov: f64 = 10.0 - m.vth;
    let model_id = m.k * (2.0 * vov * vds - vds * vds) * (1.0 + m.lambda * vds);
    assert!(vds < 0.05, "Schalter ein: U_DS = {vds}");
    assert_close(model_id, id, 1e-7, "I_D im linearen Bereich");
}

#[test]
fn cmos_inverter_kennlinie() {
    let out = |vin: f64| {
        let mut c = Circuit::new();
        c.vsource("VDD", "vdd", "gnd", 5.0).unwrap();
        c.vsource("VIN", "in", "gnd", vin).unwrap();
        c.mosfet("MP", "out", "in", "vdd", MosModel::CMOS_P).unwrap();
        c.mosfet("MN", "out", "in", "gnd", MosModel::CMOS_N).unwrap();
        c.resistor("RL", "out", "gnd", 10e6).unwrap();
        op(&c).voltage("out").unwrap()
    };
    let vs: Vec<f64> = (0..=50).map(|k| out(k as f64 * 0.1)).collect();
    assert!(vs[0] > 4.99 && vs[50] < 0.01, "Ausgang {} … {}", vs[0], vs[50]);
    assert!(vs.windows(2).all(|w| w[1] <= w[0] + 1e-9), "monoton fallend");
    // Symmetrische Transistoren: Schaltschwelle bei U_DD / 2
    assert!(out(2.4) > 2.5 && out(2.6) < 2.5, "Schwelle: {} / {}", out(2.4), out(2.6));
}

#[test]
fn transistorschalter_im_zeitverlauf() {
    let mut c = Circuit::new();
    c.vsource("VCC", "vcc", "gnd", 10.0).unwrap();
    c.vsource("VIN", "in", "gnd", Waveform::square(0.0, 5.0, 1e3)).unwrap();
    c.resistor("RB", "in", "b", 4.7e3).unwrap();
    c.resistor("RC", "vcc", "c", 1e3).unwrap();
    c.capacitor("CL", "c", "gnd", 10e-9).unwrap();
    c.bjt("T1", "c", "b", "gnd", BjtModel::BC547).unwrap();
    let res = tran(&c, TranOptions::new(3e-3, 1e-5));
    // Eingang hoch → Transistor leitet → Kollektor unten; Eingang tief → oben
    assert!(res.voltage_at("c", 0.4e-3).unwrap() < 0.2);
    assert!(res.voltage_at("c", 0.9e-3).unwrap() > 9.9);
    assert!(res.voltage_at("c", 2.4e-3).unwrap() < 0.2);
}
