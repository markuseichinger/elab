//! Diodenmodell nach Shockley mit Spannungsbegrenzung für Newton-Raphson.

/// Temperaturspannung bei 300,15 K (27 °C): k·T/q.
pub const VT_300K: f64 = 0.025_864;

/// Parameter einer Halbleiterdiode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiodeModel {
    /// Sättigungssperrstrom in A.
    pub is: f64,
    /// Emissionskoeffizient (Idealitätsfaktor), typ. 1 … 2.
    pub n: f64,
}

impl DiodeModel {
    /// Lehrbuchdiode: Is = 1e-14 A, n = 1.
    pub const IDEAL: DiodeModel = DiodeModel { is: 1e-14, n: 1.0 };
    /// Kleinsignal-Siliziumdiode ähnlich 1N4148.
    pub const SI_1N4148: DiodeModel = DiodeModel { is: 2.52e-9, n: 1.752 };
    /// Gleichrichterdiode ähnlich 1N4007.
    pub const SI_1N4007: DiodeModel = DiodeModel { is: 7.0e-9, n: 1.8 };
    /// Rote LED (Flussspannung ≈ 1,8 V bei 20 mA).
    pub const LED_RED: DiodeModel = DiodeModel { is: 3.0e-18, n: 1.9 };

    #[inline]
    pub fn nvt(&self) -> f64 {
        self.n * VT_300K
    }

    /// Kritische Spannung, ab der das Limiting greift (wie in SPICE).
    pub fn vcrit(&self) -> f64 {
        let nvt = self.nvt();
        nvt * (nvt / (std::f64::consts::SQRT_2 * self.is)).ln()
    }

    /// Strom und differentieller Leitwert bei Spannung `vd`.
    ///
    /// Oberhalb von 40·n·Vt wird die Exponentialfunktion linear fortgesetzt,
    /// damit auch bei schlechten Startwerten kein Überlauf entsteht.
    pub fn eval(&self, vd: f64) -> (f64, f64) {
        let nvt = self.nvt();
        let x = vd / nvt;
        const XMAX: f64 = 40.0;
        if x > XMAX {
            let e = XMAX.exp();
            let i0 = self.is * (e - 1.0);
            let g = self.is * e / nvt;
            (i0 + g * (vd - XMAX * nvt), g)
        } else {
            let e = x.exp();
            (self.is * (e - 1.0), self.is * e / nvt)
        }
    }
}

/// Spannungsbegrenzung für pn-Übergänge (SPICE `pnjlim`).
///
/// Verhindert, dass Newton-Raphson bei einem großen Sprung der Diodenspannung
/// in den Exponentialbereich schießt. Gibt die begrenzte Spannung und ein
/// Flag zurück, ob begrenzt wurde.
pub fn pnjlim(vnew: f64, vold: f64, nvt: f64, vcrit: f64) -> (f64, bool) {
    if vnew > vcrit && (vnew - vold).abs() > 2.0 * nvt {
        let v = if vold > 0.0 {
            let arg = 1.0 + (vnew - vold) / nvt;
            if arg > 0.0 {
                vold + nvt * arg.ln()
            } else {
                vcrit
            }
        } else {
            nvt * (vnew / nvt).ln()
        };
        (v, true)
    } else {
        (vnew, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_current_grows_exponentially() {
        let d = DiodeModel::IDEAL;
        let (i1, _) = d.eval(0.6);
        let (i2, _) = d.eval(0.6 + d.nvt() * std::f64::consts::LN_10);
        assert!((i2 / i1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn reverse_current_is_saturation() {
        let d = DiodeModel::IDEAL;
        let (i, _) = d.eval(-5.0);
        assert!((i + d.is).abs() < 1e-20);
    }

    #[test]
    fn limiting_reduces_large_step() {
        let d = DiodeModel::IDEAL;
        let (v, lim) = pnjlim(10.0, 0.0, d.nvt(), d.vcrit());
        assert!(lim);
        assert!(v < 1.0);
    }

    #[test]
    fn led_forward_voltage_plausible() {
        // Bei 20 mA sollte eine rote LED um 1,8 V liegen.
        let d = DiodeModel::LED_RED;
        let v = d.nvt() * (0.02 / d.is + 1.0).ln();
        assert!(v > 1.6 && v < 2.0, "v = {v}");
    }
}
