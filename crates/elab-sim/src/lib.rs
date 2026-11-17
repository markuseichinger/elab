//! # elab-sim
//!
//! Analoger Schaltungssimulator für das elab-Lernlabor.
//!
//! - **Modifizierte Knotenanalyse (MNA)**: Knotenspannungen plus Zweigströme
//!   von Spannungsquellen und Spulen als Unbekannte
//! - **Newton-Raphson** für nichtlineare Bauteile, mit pn-Spannungsbegrenzung,
//!   gmin-Stepping und Source-Stepping als Rückfallebenen
//! - **Transientenanalyse** mit Trapezregel, Euler-rückwärts nach Knickstellen
//!   und Schrittweitensteuerung über den lokalen Abbruchfehler
//!
//! ```
//! use elab_sim::{Circuit, Simulator};
//!
//! let mut c = Circuit::new();
//! c.vsource("V1", "in", "gnd", 10.0).unwrap();
//! c.resistor("R1", "in", "out", 1e3).unwrap();
//! c.resistor("R2", "out", "gnd", 1e3).unwrap();
//! let op = Simulator::new(&c).unwrap().operating_point().unwrap();
//! assert!((op.voltage("out").unwrap() - 5.0).abs() < 1e-6);
//! ```

pub mod analysis;
pub mod circuit;
pub mod digital;
pub mod diode;
pub mod linalg;
pub mod waveform;

pub use analysis::{OpResult, SimOptions, Simulator, Stats, TranOptions, TranResult};
pub use circuit::{Circuit, Element, ElementKind, NodeId, SimError, GND};
pub use diode::DiodeModel;
pub use waveform::Waveform;

/// Formatiert einen Wert mit SI-Präfix und deutschem Dezimalkomma, z. B. `1,5 ms`.
pub fn fmt_si(v: f64, unit: &str) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{} {unit}", if v == 0.0 { "0".to_string() } else { v.to_string() });
    }
    const PREFIX: [(f64, &str); 11] =
        [(1e12, "T"), (1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p"), (1e-15, "f"), (1e-18, "a")];
    let a = v.abs();
    let (scale, p) = PREFIX.iter().find(|(s, _)| a >= *s * 0.999_5).copied().unwrap_or((1e-18, "a"));
    let x = v / scale;
    let digits = if x.abs() >= 100.0 {
        0
    } else if x.abs() >= 10.0 {
        1
    } else {
        2
    };
    let s = format!("{x:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    format!("{} {p}{unit}", s.replace('.', ","))
}

#[cfg(test)]
mod tests {
    use super::fmt_si;

    #[test]
    fn si_formatting() {
        assert_eq!(fmt_si(0.0015, "s"), "1,5 ms");
        assert_eq!(fmt_si(4700.0, "Ω"), "4,7 kΩ");
        assert_eq!(fmt_si(-2.2e-6, "F"), "-2,2 µF");
        assert_eq!(fmt_si(230.0, "V"), "230 V");
        assert_eq!(fmt_si(0.0, "A"), "0 A");
    }
}
