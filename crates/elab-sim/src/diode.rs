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
    /// Durchbruchspannung in Sperrrichtung in V (Zener-Diode), 0 = kein Durchbruch.
    /// Bei U = −bv fließen [`IBV`] in Sperrrichtung, darunter steigt der Strom steil an.
    pub bv: f64,
}

/// Strom beim Einsetzen des Durchbruchs
pub const IBV: f64 = 1e-4;
/// Steilheit des Durchbruchs: Emissionskoeffizient der Durchbruchkennlinie
pub const NBV: f64 = 4.0;

impl DiodeModel {
    /// Lehrbuchdiode: Is = 1e-14 A, n = 1.
    pub const IDEAL: DiodeModel = DiodeModel { is: 1e-14, n: 1.0, bv: 0.0 };
    /// Kleinsignal-Siliziumdiode ähnlich 1N4148.
    pub const SI_1N4148: DiodeModel = DiodeModel { is: 2.52e-9, n: 1.752, bv: 0.0 };
    /// Gleichrichterdiode ähnlich 1N4007.
    pub const SI_1N4007: DiodeModel = DiodeModel { is: 7.0e-9, n: 1.8, bv: 0.0 };
    /// Rote LED (Flussspannung ≈ 1,8 V bei 20 mA).
    pub const LED_RED: DiodeModel = DiodeModel { is: 3.0e-18, n: 1.9, bv: 0.0 };

    #[inline]
    pub fn nvt(&self) -> f64 {
        self.n * VT_300K
    }

    /// Zener-Diode, deren Zenerspannung bei 5 mA `vz` beträgt.
    pub fn zener(vz: f64) -> DiodeModel {
        let nbvt = NBV * VT_300K;
        DiodeModel { is: 2.52e-9, n: 1.752, bv: vz - nbvt * (5e-3 / IBV).ln() }
    }

    /// Spannungsbegrenzung im Durchbruchbereich: wie [`pnjlim`], gespiegelt an −bv.
    pub fn limit_reverse(&self, vnew: f64, vold: f64) -> (f64, bool) {
        if self.bv <= 0.0 || vnew > -self.bv * 0.5 {
            return (vnew, false);
        }
        let nbvt = NBV * VT_300K;
        let vcrit = nbvt * (nbvt / (std::f64::consts::SQRT_2 * IBV)).ln();
        let (r, lim) = pnjlim(-vnew - self.bv, -vold - self.bv, nbvt, vcrit);
        (-r - self.bv, lim)
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
        let (i, g) = exp_branch(self.is, self.nvt(), vd);
        if self.bv > 0.0 {
            // Durchbruch: Strom in Sperrrichtung, gespiegelt an −bv
            let (ib, gb) = exp_branch(IBV, NBV * VT_300K, -vd - self.bv);
            // ohne die −1 der Shockley-Formel: bei U = −bv fließt genau IBV
            let ib = ib + IBV;
            return (i - ib, g + gb);
        }
        (i, g)
    }
}

/// is·(e^(v/nvt) − 1) und Ableitung; oberhalb von 40·nvt linear fortgesetzt,
/// damit auch bei schlechten Startwerten kein Überlauf entsteht.
fn exp_branch(is: f64, nvt: f64, v: f64) -> (f64, f64) {
    let x = v / nvt;
    const XMAX: f64 = 40.0;
    if x > XMAX {
        let e = XMAX.exp();
        let i0 = is * (e - 1.0);
        let g = is * e / nvt;
        (i0 + g * (v - XMAX * nvt), g)
    } else {
        let e = x.exp();
        (is * (e - 1.0), is * e / nvt)
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
    fn zener_breaks_down_at_its_voltage() {
        let z = DiodeModel::zener(5.1);
        // Bei 5 mA in Sperrrichtung liegt die Zenerspannung an
        let i = |v: f64| -z.eval(v).0;
        assert!((i(-5.1) - 5e-3).abs() < 1e-4, "{}", i(-5.1));
        // darunter praktisch sperrend, darüber steiler Anstieg
        assert!(i(-3.0) < 1e-8, "{}", i(-3.0));
        assert!(i(-5.6) > 0.04, "{}", i(-5.6));
        // in Flussrichtung wie eine normale Diode
        assert!(z.eval(0.7).0 > 1e-3);
    }

    #[test]
    fn led_forward_voltage_plausible() {
        // Bei 20 mA sollte eine rote LED um 1,8 V liegen.
        let d = DiodeModel::LED_RED;
        let v = d.nvt() * (0.02 / d.is + 1.0).ln();
        assert!(v > 1.6 && v < 2.0, "v = {v}");
    }
}
