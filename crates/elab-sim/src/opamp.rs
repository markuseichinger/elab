//! Operationsverstärker als Verhaltensmodell mit Versorgungsgrenzen.
//!
//! Der Ausgang strebt `U_ziel = U_mitte + U_halb · tanh(A · U_diff / U_halb)` an:
//! im Kleinsignalbereich die Leerlaufverstärkung `A`, weich begrenzt auf den
//! Aussteuerbereich zwischen den Versorgungsspannungen (abzüglich der
//! Ausgangsreserve, beim LM358 rund 1,5 V unter V+). Der Ausgangsstrom fließt
//! über `r_out` und kommt – wie in echt – aus dem V+- bzw. V−-Anschluss.

/// Modellparameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpAmpModel {
    /// Leerlaufverstärkung
    pub gain: f64,
    /// Ausgangswiderstand in Ω
    pub r_out: f64,
    /// Kleinster Abstand des Ausgangs zu V− in V
    pub drop_lo: f64,
    /// Kleinster Abstand des Ausgangs zu V+ in V
    pub drop_hi: f64,
    /// Nachlaufzeit des Ausgangs in s (begrenzt, wie schnell er umschwingt)
    pub tau: f64,
}

/// Ausgangsziel und Ableitungen.
#[derive(Clone, Copy, Debug)]
pub struct OpAmpOp {
    pub v_target: f64,
    /// ∂U_ziel/∂U_diff
    pub d_diff: f64,
    pub d_vcc: f64,
    pub d_vee: f64,
}

impl OpAmpModel {
    /// LM358: Standard-Doppel-OpAmp, Ausgang kommt nicht ganz an V+ heran.
    pub const LM358: Self = Self { gain: 1e5, r_out: 50.0, drop_lo: 0.02, drop_hi: 1.5, tau: 2e-6 };
    /// Rail-to-Rail-OpAmp (z. B. MCP6001): Ausgang bis an beide Versorgungen.
    pub const RAIL_TO_RAIL: Self = Self { gain: 1e5, r_out: 30.0, drop_lo: 0.02, drop_hi: 0.02, tau: 1e-6 };

    pub fn eval(&self, diff: f64, vcc: f64, vee: f64) -> OpAmpOp {
        let hi = vcc - self.drop_hi;
        let lo = vee + self.drop_lo;
        let raw_half = (hi - lo) * 0.5;
        let (half, dh) = if raw_half > 1e-3 { (raw_half, 1.0) } else { (1e-3, 0.0) };
        let mid = (hi + lo) * 0.5;
        let u = self.gain * diff / half;
        let t = u.tanh();
        let sech2 = 1.0 - t * t;
        // ∂(half · tanh(A·d/half))/∂half = tanh(u) − u·sech²(u)
        let dhalf_term = if sech2 > 0.0 { t - u * sech2 } else { t };
        OpAmpOp { v_target: mid + half * t, d_diff: self.gain * sech2, d_vcc: 0.5 + 0.5 * dh * dhalf_term, d_vee: 0.5 - 0.5 * dh * dhalf_term }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturates_at_rails() {
        let m = OpAmpModel::LM358;
        assert!((m.eval(1.0, 12.0, -12.0).v_target - 10.5).abs() < 1e-6);
        assert!((m.eval(-1.0, 12.0, -12.0).v_target + 11.98).abs() < 1e-6);
        let small = m.eval(1e-6, 12.0, -12.0);
        assert!((small.v_target - (-0.74 + 0.1)).abs() < 0.01, "{}", small.v_target);
    }

    #[test]
    fn derivatives_match_numeric() {
        let m = OpAmpModel::RAIL_TO_RAIL;
        for &(d, vcc, vee) in &[(1e-5, 5.0, 0.0), (-3e-5, 5.0, 0.0), (2e-4, 9.0, -3.0), (0.0, 5.0, 0.0)] {
            let op = m.eval(d, vcc, vee);
            let h = 1e-9;
            let nd = (m.eval(d + h, vcc, vee).v_target - m.eval(d - h, vcc, vee).v_target) / (2.0 * h);
            assert!((nd - op.d_diff).abs() <= 1e-3 * op.d_diff.abs().max(1.0), "{nd} {}", op.d_diff);
            let h = 1e-6;
            let nc = (m.eval(d, vcc + h, vee).v_target - m.eval(d, vcc - h, vee).v_target) / (2.0 * h);
            let ne = (m.eval(d, vcc, vee + h).v_target - m.eval(d, vcc, vee - h).v_target) / (2.0 * h);
            assert!((nc - op.d_vcc).abs() < 1e-4, "{nc} {}", op.d_vcc);
            assert!((ne - op.d_vee).abs() < 1e-4, "{ne} {}", op.d_vee);
        }
    }
}
