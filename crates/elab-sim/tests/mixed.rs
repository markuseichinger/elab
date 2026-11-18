//! Mixed-Signal und Operationsverstärker gegen Handrechnung.

use elab_sim::digital::{Comp, CompKind, Logic};
use elab_sim::*;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn opamp_invertierender_verstaerker() {
    let mut c = Circuit::new();
    c.vsource("VP", "vp", "gnd", 12.0).unwrap();
    c.vsource("VN", "vn", "gnd", -12.0).unwrap();
    c.vsource("VIN", "in", "gnd", 0.5).unwrap();
    c.resistor("R1", "in", "minus", 1e3).unwrap();
    c.resistor("R2", "minus", "out", 10e3).unwrap();
    let (p, n, out, vp, vn) = (c.node("gnd"), c.node("minus"), c.node("out"), c.node("vp"), c.node("vn"));
    c.add("U1", ElementKind::OpAmp { p, n, out, vcc: vp, vee: vn, model: OpAmpModel::LM358 }).unwrap();
    let op = Simulator::new(&c).unwrap().operating_point().unwrap();
    assert!(close(op.voltage("out").unwrap(), -5.0, 1e-3), "{}", op.voltage("out").unwrap());
    assert!(op.voltage("minus").unwrap().abs() < 1e-3);
    // Übersteuert: Ausgang an der negativen Grenze
    let mut c2 = c.clone();
    c2.update("VIN", ElementKind::VSource { p: c.node("in"), n: 0, wave: Waveform::Dc(2.0) }).unwrap();
    let op = Simulator::new(&c2).unwrap().operating_point().unwrap();
    assert!(close(op.voltage("out").unwrap(), -11.917, 0.01), "{}", op.voltage("out").unwrap());
}

#[test]
fn opamp_folger_lm358_kommt_nicht_an_vcc() {
    let mut c = Circuit::new();
    c.vsource("VCC", "vcc", "gnd", 5.0).unwrap();
    c.vsource("VIN", "in", "gnd", 4.5).unwrap();
    let (p, out, vcc) = (c.node("in"), c.node("out"), c.node("vcc"));
    c.add("U1", ElementKind::OpAmp { p, n: out, out, vcc, vee: 0, model: OpAmpModel::LM358 }).unwrap();
    c.resistor("RL", "out", "gnd", 10e3).unwrap();
    let op = Simulator::new(&c).unwrap().operating_point().unwrap();
    assert!(close(op.voltage("out").unwrap(), 3.5, 0.05), "{}", op.voltage("out").unwrap());
    // Laststrom kommt aus VCC
    assert!(close(op.current("VCC").unwrap(), op.voltage("out").unwrap() / 10e3, 1e-6));
    // Rail-to-Rail folgt
    let mut c2 = c.clone();
    c2.update("U1", ElementKind::OpAmp { p, n: out, out, vcc, vee: 0, model: OpAmpModel::RAIL_TO_RAIL }).unwrap();
    let op = Simulator::new(&c2).unwrap().operating_point().unwrap();
    assert!(close(op.voltage("out").unwrap(), 4.5, 1e-3), "{}", op.voltage("out").unwrap());
}

#[test]
fn opamp_komparator_und_schmitt_trigger() {
    // Komparator offen: Eingang knapp über Referenz → Ausgang oben
    let mut c = Circuit::new();
    c.vsource("VCC", "vcc", "gnd", 5.0).unwrap();
    c.vsource("VIN", "in", "gnd", 2.51).unwrap();
    c.resistor("Ra", "vcc", "ref", 10e3).unwrap();
    c.resistor("Rb", "ref", "gnd", 10e3).unwrap();
    let (p, n, out, vcc) = (c.node("in"), c.node("ref"), c.node("out"), c.node("vcc"));
    c.add("U1", ElementKind::OpAmp { p, n, out, vcc, vee: 0, model: OpAmpModel::RAIL_TO_RAIL }).unwrap();
    let op = Simulator::new(&c).unwrap().operating_point().unwrap();
    assert!(close(op.voltage("out").unwrap(), 4.98, 0.01), "{}", op.voltage("out").unwrap());

    // Sinus durch einen invertierenden Schmitt-Trigger (Mitkopplung): kippt bei ±Schwelle
    let mut c = Circuit::new();
    c.vsource("VP", "vp", "gnd", 12.0).unwrap();
    c.vsource("VN", "vn", "gnd", -12.0).unwrap();
    c.vsource("VIN", "in", "gnd", Waveform::Sine { offset: 0.0, amplitude: 5.0, freq: 100.0, phase_deg: 0.0, delay: 0.0 }).unwrap();
    c.resistor("R1", "out", "plus", 10e3).unwrap();
    c.resistor("R2", "plus", "gnd", 2e3).unwrap();
    let (p, n, out, vp, vn) = (c.node("plus"), c.node("in"), c.node("out"), c.node("vp"), c.node("vn"));
    c.add("U1", ElementKind::OpAmp { p, n, out, vcc: vp, vee: vn, model: OpAmpModel::RAIL_TO_RAIL }).unwrap();
    let res = Simulator::new(&c).unwrap().transient(&TranOptions::new(20e-3, 20e-6)).unwrap();
    let (vin, vout) = (res.voltage("in").unwrap(), res.voltage("out").unwrap());
    // Kippschwelle ±12 V · 2k/12k = ±2 V
    let mut flips = Vec::new();
    for k in 1..vout.len() {
        if (vout[k - 1] > 0.0) != (vout[k] > 0.0) {
            flips.push(vin[k]);
        }
    }
    assert!(flips.len() >= 3, "{flips:?}");
    for f in &flips[1..] {
        assert!(close(f.abs(), 2.0, 0.1), "{flips:?}");
    }
}

/// NE555 als astabiler Multivibrator, aus den Mixed-Signal-Bausteinen zusammengesetzt
/// (so wie es die App für das Bauteil tut).
#[test]
fn timer_555_astabil() {
    let (ra, rb, cap) = (1e3, 10e3, 1e-6);
    let mut c = Circuit::new();
    c.vsource("V1", "vcc", "gnd", 9.0).unwrap();
    c.resistor("RA", "vcc", "dis", ra).unwrap();
    c.resistor("RB", "dis", "thr", rb).unwrap();
    c.capacitor("C", "thr", "gnd", cap).unwrap();
    // innen: Teiler 3 × 5 kΩ, Komparatoren, Latch, Ausgangsstufen
    c.resistor("U:R1", "vcc", "ctrl", 5e3).unwrap();
    c.resistor("U:R2", "ctrl", "third", 5e3).unwrap();
    c.resistor("U:R3", "third", "gnd", 5e3).unwrap();
    c.vsource("U:VOH", "vcc", "oh", 1.7).unwrap();
    let (vcc, oh, out, dis, thr, third, ctrl) =
        (c.node("vcc"), c.node("oh"), c.node("out"), c.node("dis"), c.node("thr"), c.node("third"), c.node("ctrl"));
    let e_out = c.add("U:OUT", ElementKind::Driver { out, hi: oh, lo: 0, r: 10.0 }).unwrap();
    let e_dis = c.add("U:DIS", ElementKind::Driver { out: dis, hi: 0, lo: 0, r: 10.0 }).unwrap();
    c.resistor("RL", "out", "gnd", 1e3).unwrap();
    let input = |net| Comp { kind: CompKind::Input(Logic::X), inputs: vec![], outputs: vec![net], delay: 0 };
    let spec = MixedSpec {
        n_nets: 5,
        comps: vec![
            input(0),
            input(1),
            Comp { kind: CompKind::Const(Logic::L1), inputs: vec![], outputs: vec![2], delay: 0 },
            Comp { kind: CompKind::Latch555, inputs: vec![0, 1, 2], outputs: vec![3, 4], delay: 100 },
        ],
        observers: vec![
            Observer { kind: ObsKind::Compare { plus: third, minus: thr }, inputs: vec![0], group: Some(0), floating: None },
            Observer { kind: ObsKind::Compare { plus: thr, minus: ctrl }, inputs: vec![1], group: Some(0), floating: None },
        ],
        drivers: vec![
            DriverLink { elem: e_out, src: DriveSrc::Net(3), group: Some(0), open_drain: false },
            DriverLink { elem: e_dis, src: DriveSrc::Net(4), group: Some(0), open_drain: true },
        ],
        groups: vec![PowerGroup { name: "U".into(), vcc, gnd: 0, v_min: 4.5, v_max: 16.0 }],
    };
    let mut m = MixedSim::new(c, spec, 50e-6).unwrap();
    let res = m.run(100e-3).unwrap();
    let v = res.voltage("out").unwrap();
    let mut rise = Vec::new();
    let mut fall = Vec::new();
    for k in 1..v.len() {
        if v[k - 1] < 3.0 && v[k] >= 3.0 {
            rise.push(res.time[k]);
        }
        if v[k - 1] >= 3.0 && v[k] < 3.0 {
            fall.push(res.time[k]);
        }
    }
    // t_high = ln2·(RA+RB)·C = 7,62 ms, t_low ≈ ln2·RB·C = 6,93 ms
    // Entladetransistor (10 Ω) zieht DIS nicht ganz auf 0 V, weil RA nachliefert
    let vd = 9.0 * 10.0 / (ra + 10.0);
    let (th, tl) = (2f64.ln() * (ra + rb) * cap, ((6.0_f64 - vd) / (3.0 - vd)).ln() * rb * cap);
    let period = (rise[rise.len() - 1] - rise[1]) / (rise.len() - 2) as f64;
    assert!(close(period, th + tl, 0.01 * (th + tl)), "T = {period} statt {}", th + tl);
    let k = fall.iter().position(|&f| f > rise[1]).unwrap();
    assert!(close(fall[k] - rise[1], th, 0.015 * th), "t_high = {} statt {th}", fall[k] - rise[1]);
    // High-Pegel des NE555: etwa VCC − 1,7 V
    let vmax = v.iter().cloned().fold(f64::MIN, f64::max);
    assert!(close(vmax, 9.0 - 1.7 - 0.07, 0.05), "{vmax}");
    // Kondensator pendelt zwischen 3 V und 6 V
    let vc = res.voltage("thr").unwrap();
    let late: Vec<f64> = vc.iter().zip(&res.time).filter(|(_, &t)| t > 20e-3).map(|(v, _)| *v).collect();
    let (mn, mx) = late.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    assert!(close(mn, 3.0, 0.03) && close(mx, 6.0, 0.03), "{mn} {mx}");
    println!("555: {} Schritte, {} verworfen", m.stats().accepted_steps, m.stats().rejected_steps);
}

/// Echtzeitbetrieb: schrittweise vorrücken und zwischendurch einen Schalter umlegen.
#[test]
fn schrittweise_mit_schalter() {
    let mut c = Circuit::new();
    c.vsource("V1", "vcc", "gnd", 5.0).unwrap();
    c.switch("S1", "vcc", "a", false).unwrap();
    c.resistor("R1", "a", "b", 1e3).unwrap();
    c.capacitor("C1", "b", "gnd", 1e-6).unwrap();
    c.resistor("R2", "a", "gnd", 1e6).unwrap();
    let mut m = MixedSim::new(c.clone(), MixedSpec::default(), 1e-4).unwrap();
    m.advance(5e-3, None).unwrap();
    let b = c.find_node("b").unwrap();
    assert!(m.voltage(b).abs() < 1e-6);
    let (vcc, a) = (c.find_node("vcc").unwrap(), c.find_node("a").unwrap());
    m.update_element("S1", ElementKind::Switch { a: vcc, b: a, closed: true, r_on: 1e-3, r_off: 1e12 }).unwrap();
    m.advance(6e-3, None).unwrap();
    // eine Zeitkonstante nach dem Einschalten: 63 %
    assert!(close(m.voltage(b), 5.0 * (1.0 - (-1.0f64).exp()), 0.01), "{}", m.voltage(b));
}

/// ADC und DAC hintereinander: Ausgang gibt den Eingang in 16 Stufen wieder.
#[test]
fn adc_dac_kette() {
    for vin in [0.0, 0.7, 1.6, 2.5, 3.33, 4.99] {
        let mut c = Circuit::new();
        c.vsource("V1", "vcc", "gnd", 5.0).unwrap();
        c.vsource("VIN", "in", "gnd", vin).unwrap();
        let (vcc, inp, out) = (c.node("vcc"), c.node("in"), c.node("out"));
        let d: Vec<usize> = (0..4).map(|b| c.node(&format!("d{b}"))).collect();
        let mut drivers = Vec::new();
        let mut comps = Vec::new();
        // ADC: 4 Eingabebausteine (Netze 0..3), Ausgangsstufen auf d0..d3
        for (b, &db) in d.iter().enumerate() {
            comps.push(Comp { kind: CompKind::Input(Logic::X), inputs: vec![], outputs: vec![b], delay: 0 });
            let e = c.add(&format!("ADC:D{b}"), ElementKind::Driver { out: db, hi: vcc, lo: 0, r: 50.0 }).unwrap();
            drivers.push(DriverLink { elem: e, src: DriveSrc::Net(b), group: Some(0), open_drain: false });
        }
        // DAC: Eingänge lesen d0..d3 (Netze 4..7), Ausgang 10 kΩ zwischen VREF=VCC und GND
        let mut observers =
            vec![Observer { kind: ObsKind::Adc { node: inp, gnd: 0, vref: vcc, bits: 4 }, inputs: vec![0, 1, 2, 3], group: Some(0), floating: None }];
        for (b, &db) in d.iter().enumerate() {
            comps.push(Comp { kind: CompKind::Input(Logic::X), inputs: vec![], outputs: vec![4 + b], delay: 0 });
            observers.push(Observer {
                kind: ObsKind::Threshold { node: db, gnd: 0, vcc, il: Level::frac(0.3), ih: Level::frac(0.7), schmitt: false },
                inputs: vec![4 + b],
                group: Some(1),
                floating: None,
            });
        }
        let e = c.add("DAC:OUT", ElementKind::Driver { out, hi: vcc, lo: 0, r: 10e3 }).unwrap();
        drivers.push(DriverLink { elem: e, src: DriveSrc::Code { nets: vec![4, 5, 6, 7], full: 16.0 }, group: Some(1), open_drain: false });
        let groups = vec![
            PowerGroup { name: "ADC".into(), vcc, gnd: 0, v_min: 2.0, v_max: 7.0 },
            PowerGroup { name: "DAC".into(), vcc, gnd: 0, v_min: 2.0, v_max: 7.0 },
        ];
        c.resistor("RL", "out", "gnd", 1e9).unwrap();
        let m = MixedSim::new(c, MixedSpec { n_nets: 8, comps, observers, drivers, groups }, 1e-6).unwrap();
        let code = (vin / 5.0 * 16.0).floor().min(15.0);
        assert!(close(m.voltage(out), 5.0 * code / 16.0, 1e-3), "{vin}: {} statt {}", m.voltage(out), 5.0 * code / 16.0);
    }
}
