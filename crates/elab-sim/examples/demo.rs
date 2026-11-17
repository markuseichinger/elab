//! Demo und Mini-Benchmark des Simulationskerns.
//!
//! ```text
//! cargo run --release --example demo            # Übersicht im Terminal
//! cargo run --release --example demo -- out/    # zusätzlich CSV-Dateien je Schaltung
//! ```

use elab_sim::*;
use std::time::Instant;

struct Case {
    name: &'static str,
    circuit: Circuit,
    opts: TranOptions,
    probes: Vec<&'static str>,
}

fn rc() -> Case {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::square(0.0, 5.0, 1e3))
        .unwrap();
    c.resistor("R1", "in", "out", 1e3).unwrap();
    c.capacitor("C1", "out", "gnd", 1e-7).unwrap();
    Case {
        name: "rc_rechteck",
        circuit: c,
        opts: TranOptions::new(3e-3, 1e-5),
        probes: vec!["in", "out"],
    }
}

fn rlc() -> Case {
    let mut c = Circuit::new();
    c.vsource("V1", "in", "gnd", Waveform::step(1.0, 0.0))
        .unwrap();
    c.resistor("R1", "in", "a", 10.0).unwrap();
    c.inductor("L1", "a", "b", 1e-3).unwrap();
    c.capacitor("C1", "b", "gnd", 1e-6).unwrap();
    Case {
        name: "rlc_sprung",
        circuit: c,
        opts: TranOptions::new(1e-3, 2e-6),
        probes: vec!["in", "b"],
    }
}

fn halfwave() -> Case {
    let mut c = Circuit::new();
    c.vsource("V1", "ac", "gnd", Waveform::sine(10.0, 50.0))
        .unwrap();
    c.diode("D1", "ac", "out", DiodeModel::SI_1N4007).unwrap();
    c.capacitor("C1", "out", "gnd", 470e-6).unwrap();
    c.resistor("RL", "out", "gnd", 1e3).unwrap();
    Case {
        name: "einweg_gleichrichter",
        circuit: c,
        opts: TranOptions::new(0.1, 1e-4),
        probes: vec!["ac", "out"],
    }
}

fn bridge() -> Case {
    let m = DiodeModel::SI_1N4007;
    let mut c = Circuit::new();
    c.vsource("V1", "ac1", "ac2", Waveform::sine(325.0, 50.0))
        .unwrap();
    c.diode("D1", "ac1", "plus", m).unwrap();
    c.diode("D2", "ac2", "plus", m).unwrap();
    c.diode("D3", "gnd", "ac1", m).unwrap();
    c.diode("D4", "gnd", "ac2", m).unwrap();
    c.capacitor("C1", "plus", "gnd", 22e-6).unwrap();
    c.resistor("RL", "plus", "gnd", 10e3).unwrap();
    Case {
        name: "brueckengleichrichter",
        circuit: c,
        opts: TranOptions::new(0.08, 1e-4),
        probes: vec!["ac1", "plus"],
    }
}

fn lc() -> Case {
    let mut c = Circuit::new();
    let n = c.node("n");
    c.add(
        "C1",
        ElementKind::Capacitor {
            a: n,
            b: GND,
            c: 1e-6,
            ic: Some(1.0),
        },
    )
    .unwrap();
    c.add(
        "L1",
        ElementKind::Inductor {
            a: n,
            b: GND,
            l: 1e-3,
            ic: Some(0.0),
        },
    )
    .unwrap();
    Case {
        name: "lc_schwingkreis",
        circuit: c,
        opts: TranOptions::new(2e-3, 2e-6).with_ic(),
        probes: vec!["n"],
    }
}

fn ladder(stages: usize) -> Case {
    let mut c = Circuit::new();
    c.vsource("V1", "n0", "gnd", Waveform::step(1.0, 0.0))
        .unwrap();
    for k in 0..stages {
        let (a, b) = (format!("n{k}"), format!("n{}", k + 1));
        c.resistor(&format!("R{k}"), &a, &b, 100.0).unwrap();
        c.capacitor(&format!("C{k}"), &b, "gnd", 1e-9).unwrap();
    }
    let last: &'static str = Box::leak(format!("n{stages}").into_boxed_str());
    Case {
        name: "rc_kette_100",
        circuit: c,
        opts: TranOptions::new(2e-3, 1e-5),
        probes: vec!["n1", last],
    }
}

fn main() {
    let out_dir = std::env::args().nth(1);
    if let Some(d) = &out_dir {
        std::fs::create_dir_all(d).expect("Ausgabeordner anlegen");
    }
    println!(
        "{:<24} {:>6} {:>8} {:>9} {:>9} {:>10}",
        "Schaltung", "Größe", "Schritte", "verworfen", "Newton", "Rechenzeit"
    );
    for case in [rc(), rlc(), halfwave(), bridge(), lc(), ladder(100)] {
        let mut sim = Simulator::new(&case.circuit).expect("Schaltung gültig");
        let t0 = Instant::now();
        let res = sim
            .transient(&case.opts)
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let dt = t0.elapsed();
        println!(
            "{:<24} {:>6} {:>8} {:>9} {:>9} {:>10}",
            case.name,
            sim.system_size(),
            sim.stats.accepted_steps,
            sim.stats.rejected_steps,
            sim.stats.newton_iterations,
            fmt_si(dt.as_secs_f64(), "s")
        );
        if let Some(d) = &out_dir {
            let cols: Vec<Vec<f64>> = case
                .probes
                .iter()
                .map(|p| res.voltage(p).unwrap())
                .collect();
            let mut s = String::from("t");
            for p in &case.probes {
                s.push_str(&format!(",{p}"));
            }
            s.push('\n');
            for (k, t) in res.time.iter().enumerate() {
                s.push_str(&format!("{t:e}"));
                for col in &cols {
                    s.push_str(&format!(",{:e}", col[k]));
                }
                s.push('\n');
            }
            std::fs::write(format!("{d}/{}.csv", case.name), s).unwrap();
        }
    }
}
