//! Zeitverläufe für unabhängige Quellen.

use std::f64::consts::PI;

/// Zeitverlauf einer Spannungs- oder Stromquelle.
#[derive(Clone, Debug, PartialEq)]
pub enum Waveform {
    /// Konstanter Wert.
    Dc(f64),
    /// `offset + amplitude * sin(2π f (t - delay) + phase)`, vor `delay` nur `offset + amplitude * sin(phase)`.
    Sine { offset: f64, amplitude: f64, freq: f64, phase_deg: f64, delay: f64 },
    /// Trapezförmiger Puls wie in SPICE: Ruhewert `v1`, Pulswert `v2`.
    /// `period = 0` bedeutet ein einzelner Puls.
    Pulse { v1: f64, v2: f64, delay: f64, rise: f64, fall: f64, width: f64, period: f64 },
    /// Stückweise linear, Stützstellen `(t, wert)` aufsteigend nach `t`.
    Pwl(Vec<(f64, f64)>),
}

/// Kleinste Flankendauer, damit ein "idealer" Sprung numerisch handhabbar bleibt.
const MIN_EDGE: f64 = 1e-9;

impl Waveform {
    /// Spannungssprung von 0 auf `v` zum Zeitpunkt `at` (Flanke 1 ns).
    pub fn step(v: f64, at: f64) -> Self {
        Waveform::Pulse { v1: 0.0, v2: v, delay: at, rise: MIN_EDGE, fall: MIN_EDGE, width: f64::INFINITY, period: 0.0 }
    }

    /// Sinus ohne Offset und Phase.
    pub fn sine(amplitude: f64, freq: f64) -> Self {
        Waveform::Sine { offset: 0.0, amplitude, freq, phase_deg: 0.0, delay: 0.0 }
    }

    /// Rechteck zwischen `low` und `high` mit Tastgrad 50 %.
    pub fn square(low: f64, high: f64, freq: f64) -> Self {
        let period = 1.0 / freq;
        let edge = (period * 1e-4).max(MIN_EDGE);
        Waveform::Pulse { v1: low, v2: high, delay: 0.0, rise: edge, fall: edge, width: period / 2.0 - edge, period }
    }

    /// Wert zum Zeitpunkt `t`.
    pub fn value(&self, t: f64) -> f64 {
        match self {
            Waveform::Dc(v) => *v,
            Waveform::Sine { offset, amplitude, freq, phase_deg, delay } => {
                let ph = phase_deg.to_radians();
                let tt = (t - delay).max(0.0);
                offset + amplitude * (2.0 * PI * freq * tt + ph).sin()
            }
            Waveform::Pulse { v1, v2, delay, rise, fall, width, period } => {
                let rise = rise.max(MIN_EDGE);
                let fall = fall.max(MIN_EDGE);
                if t < *delay {
                    return *v1;
                }
                let mut tt = t - delay;
                if *period > 0.0 && period.is_finite() {
                    tt %= period;
                }
                if tt < rise {
                    v1 + (v2 - v1) * tt / rise
                } else if tt < rise + width {
                    *v2
                } else if tt < rise + width + fall {
                    v2 + (v1 - v2) * (tt - rise - width) / fall
                } else {
                    *v1
                }
            }
            Waveform::Pwl(pts) => {
                if pts.is_empty() {
                    return 0.0;
                }
                if t <= pts[0].0 {
                    return pts[0].1;
                }
                for w in pts.windows(2) {
                    let (t0, v0) = w[0];
                    let (t1, v1) = w[1];
                    if t <= t1 {
                        if t1 <= t0 {
                            return v1;
                        }
                        return v0 + (v1 - v0) * (t - t0) / (t1 - t0);
                    }
                }
                pts[pts.len() - 1].1
            }
        }
    }

    /// Fügt alle Knickstellen im Intervall `(0, t_stop]` zu `out` hinzu.
    /// An diesen Stellen muss die Transientenanalyse exakt einen Zeitschritt setzen.
    pub fn breakpoints(&self, t_stop: f64, out: &mut Vec<f64>) {
        match self {
            Waveform::Dc(_) => {}
            Waveform::Sine { delay, .. } => {
                if *delay > 0.0 && *delay < t_stop {
                    out.push(*delay);
                }
            }
            Waveform::Pulse { delay, rise, fall, width, period, .. } => {
                let rise = rise.max(MIN_EDGE);
                let fall = fall.max(MIN_EDGE);
                let corners = [0.0, rise, rise + width, rise + width + fall];
                let mut start = *delay;
                let periodic = *period > 0.0 && period.is_finite();
                loop {
                    for c in corners {
                        let t = start + c;
                        if t.is_finite() && t > 0.0 && t <= t_stop {
                            out.push(t);
                        }
                    }
                    if !periodic {
                        break;
                    }
                    start += period;
                    if start > t_stop {
                        break;
                    }
                }
            }
            Waveform::Pwl(pts) => {
                for &(t, _) in pts {
                    if t > 0.0 && t <= t_stop {
                        out.push(t);
                    }
                }
            }
        }
    }
}

impl From<f64> for Waveform {
    fn from(v: f64) -> Self {
        Waveform::Dc(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_shape() {
        let w = Waveform::Pulse { v1: 0.0, v2: 5.0, delay: 1.0, rise: 0.1, fall: 0.1, width: 1.0, period: 0.0 };
        assert_eq!(w.value(0.5), 0.0);
        assert!((w.value(1.05) - 2.5).abs() < 1e-12);
        assert_eq!(w.value(1.5), 5.0);
        assert!((w.value(2.15) - 2.5).abs() < 1e-12);
        assert_eq!(w.value(3.0), 0.0);
    }

    #[test]
    fn pulse_breakpoints_periodic() {
        let w = Waveform::square(0.0, 1.0, 1000.0);
        let mut bp = Vec::new();
        w.breakpoints(2e-3, &mut bp);
        // Periode 1: 3 Ecken (t=0 ausgenommen), Periode 2: 4 Ecken, dazu t=2 ms als Beginn der dritten
        assert_eq!(bp.len(), 8);
    }

    #[test]
    fn pwl_interpolates() {
        let w = Waveform::Pwl(vec![(0.0, 0.0), (1.0, 10.0), (2.0, 10.0)]);
        assert!((w.value(0.25) - 2.5).abs() < 1e-12);
        assert_eq!(w.value(5.0), 10.0);
    }
}
