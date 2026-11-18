//! Transistormodelle: Bipolartransistor (Ebers-Moll) und MOSFET (Shichman-Hodges, „Level 1“).
//!
//! Beide Modelle liefern Ströme und Ableitungen nach den *Steuerspannungen*
//! (U_BE/U_BC bzw. U_GS/U_DS), bezogen auf den NPN- bzw. N-Kanal-Fall. Für PNP und
//! P-Kanal werden die Spannungen beim Einsetzen mit −1 multipliziert und die Ströme
//! ebenso. So braucht jede Formel nur einmal programmiert zu werden.

use crate::diode::VT_300K;

/// Bipolartransistor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BjtModel {
    /// true = NPN, false = PNP
    pub npn: bool,
    /// Transport-Sättigungsstrom in A
    pub is: f64,
    /// Stromverstärkung vorwärts (β, h_FE)
    pub bf: f64,
    /// Stromverstärkung rückwärts
    pub br: f64,
}

impl BjtModel {
    /// Kleinsignal-NPN ähnlich BC547B.
    pub const BC547: BjtModel = BjtModel { npn: true, is: 2.4e-14, bf: 290.0, br: 7.5 };
    /// Kleinsignal-PNP ähnlich BC557B.
    pub const BC557: BjtModel = BjtModel { npn: false, is: 1.0e-14, bf: 250.0, br: 5.0 };
    /// Universal-NPN ähnlich 2N2222 (bis 600 mA).
    pub const N2222: BjtModel = BjtModel { npn: true, is: 1.4e-14, bf: 200.0, br: 6.0 };
    /// NPN für mittlere Ströme ähnlich BC337 (bis 800 mA).
    pub const BC337: BjtModel = BjtModel { npn: true, is: 4.1e-14, bf: 250.0, br: 6.0 };
    /// NPN-Leistungstransistor ähnlich BD139 (bis 1,5 A, β niedriger).
    pub const BD139: BjtModel = BjtModel { npn: true, is: 1.0e-13, bf: 100.0, br: 5.0 };

    pub fn polarity(&self) -> f64 {
        if self.npn {
            1.0
        } else {
            -1.0
        }
    }

    pub fn vcrit(&self) -> f64 {
        VT_300K * (VT_300K / (std::f64::consts::SQRT_2 * self.is)).ln()
    }

    /// Ströme und Ableitungen für den NPN-Fall.
    pub fn eval(&self, vbe: f64, vbc: f64) -> BjtOp {
        let (iff, gf) = junction(self.is, vbe);
        let (ir, gr) = junction(self.is, vbc);
        let k = 1.0 + 1.0 / self.br;
        BjtOp { ic: iff - ir * k, ib: iff / self.bf + ir / self.br, dic_dvbe: gf, dic_dvbc: -gr * k, dib_dvbe: gf / self.bf, dib_dvbc: gr / self.br }
    }
}

/// Arbeitspunkt des Bipolartransistors (NPN-Bezug).
#[derive(Clone, Copy, Debug)]
pub struct BjtOp {
    /// Kollektorstrom (in den Kollektor hinein)
    pub ic: f64,
    /// Basisstrom (in die Basis hinein)
    pub ib: f64,
    pub dic_dvbe: f64,
    pub dic_dvbc: f64,
    pub dib_dvbe: f64,
    pub dib_dvbc: f64,
}

/// pn-Übergang mit Emissionskoeffizient 1, oberhalb von 40·U_T linear fortgesetzt.
fn junction(is: f64, v: f64) -> (f64, f64) {
    let x = v / VT_300K;
    const XMAX: f64 = 40.0;
    if x > XMAX {
        let e = XMAX.exp();
        let g = is * e / VT_300K;
        (is * (e - 1.0) + g * (v - XMAX * VT_300K), g)
    } else {
        let e = x.exp();
        (is * (e - 1.0), is * e / VT_300K)
    }
}

/// Feldeffekttransistor (Anreicherungstyp, Substrat an Source).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MosModel {
    /// true = N-Kanal, false = P-Kanal
    pub nmos: bool,
    /// Schwellspannung als Betrag in V (bei P-Kanal ist U_GS negativ)
    pub vth: f64,
    /// Steilheitsparameter K in A/V²: I_D = K · (U_GS − U_th)² in Sättigung
    pub k: f64,
    /// Kanallängenmodulation in 1/V
    pub lambda: f64,
}

impl MosModel {
    /// Kleinsignal-N-Kanal ähnlich 2N7000.
    pub const N2N7000: MosModel = MosModel { nmos: true, vth: 2.1, k: 0.06, lambda: 0.01 };
    /// Logic-Level-Leistungs-MOSFET ähnlich IRLZ44N: schaltet schon mit 5 V am Gate voll durch
    /// (R_DS(on) ≈ 25 mΩ bei U_GS = 5 V).
    pub const IRLZ44N: MosModel = MosModel { nmos: true, vth: 1.5, k: 5.7, lambda: 0.005 };
    /// Kleinsignal-P-Kanal ähnlich BS250.
    pub const BS250: MosModel = MosModel { nmos: false, vth: 2.1, k: 0.06, lambda: 0.01 };
    /// Logik-N-Kanal (wie in einem CMOS-Gatter, U_th ≈ 1 V).
    pub const CMOS_N: MosModel = MosModel { nmos: true, vth: 1.0, k: 5e-4, lambda: 0.02 };
    /// Logik-P-Kanal (wie in einem CMOS-Gatter, U_th ≈ −1 V).
    pub const CMOS_P: MosModel = MosModel { nmos: false, vth: 1.0, k: 5e-4, lambda: 0.02 };

    pub fn polarity(&self) -> f64 {
        if self.nmos {
            1.0
        } else {
            -1.0
        }
    }

    /// Drainstrom und Ableitungen (N-Kanal-Bezug), auch für U_DS < 0 (Drain und Source vertauscht).
    pub fn eval(&self, vgs: f64, vds: f64) -> MosOp {
        if vds >= 0.0 {
            self.forward(vgs, vds)
        } else {
            // Rolle von Drain und Source tauschen: i(vgs, vds) = −f(vgs − vds, −vds)
            let f = self.forward(vgs - vds, -vds);
            MosOp { id: -f.id, gm: -f.gm, gds: f.gm + f.gds, region: f.region }
        }
    }

    fn forward(&self, vgs: f64, vds: f64) -> MosOp {
        let vov = vgs - self.vth;
        let cl = 1.0 + self.lambda * vds;
        if vov <= 0.0 {
            MosOp { id: 0.0, gm: 0.0, gds: 0.0, region: MosRegion::Off }
        } else if vds < vov {
            let i0 = self.k * (2.0 * vov * vds - vds * vds);
            MosOp {
                id: i0 * cl,
                gm: self.k * 2.0 * vds * cl,
                gds: self.k * (2.0 * vov - 2.0 * vds) * cl + i0 * self.lambda,
                region: MosRegion::Linear,
            }
        } else {
            let i0 = self.k * vov * vov;
            MosOp { id: i0 * cl, gm: 2.0 * self.k * vov * cl, gds: i0 * self.lambda, region: MosRegion::Saturation }
        }
    }
}

/// Betriebsbereich des MOSFET.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MosRegion {
    /// gesperrt (U_GS unter der Schwelle)
    Off,
    /// linearer Bereich / Triodenbereich (wirkt wie ein Widerstand: Schalter ein)
    Linear,
    /// Sättigung (Stromquelle: Verstärkerbetrieb)
    Saturation,
}

/// Arbeitspunkt des MOSFET (N-Kanal-Bezug).
#[derive(Clone, Copy, Debug)]
pub struct MosOp {
    pub id: f64,
    pub gm: f64,
    pub gds: f64,
    pub region: MosRegion,
}

/// Spannungsbegrenzung für FET-Steuerspannungen: große Sprünge in Schritten.
pub fn fetlim(vnew: f64, vold: f64, max_step: f64) -> (f64, bool) {
    let d = vnew - vold;
    if d.abs() > max_step {
        (vold + max_step.copysign(d), true)
    } else {
        (vnew, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num_deriv(f: impl Fn(f64) -> f64, x: f64) -> f64 {
        let h = 1e-6;
        (f(x + h) - f(x - h)) / (2.0 * h)
    }

    #[test]
    fn bjt_derivatives_match_numeric() {
        let m = BjtModel::BC547;
        for &(vbe, vbc) in &[(0.65, -4.0), (0.7, 0.6), (0.5, 0.1), (-1.0, -5.0)] {
            let op = m.eval(vbe, vbc);
            let tol = |a: f64, b: f64| (a - b).abs() <= 1e-4 * a.abs().max(b.abs()) + 1e-15;
            assert!(tol(op.dic_dvbe, num_deriv(|v| m.eval(v, vbc).ic, vbe)), "dIc/dVbe bei {vbe},{vbc}");
            assert!(tol(op.dic_dvbc, num_deriv(|v| m.eval(vbe, v).ic, vbc)), "dIc/dVbc");
            assert!(tol(op.dib_dvbe, num_deriv(|v| m.eval(v, vbc).ib, vbe)), "dIb/dVbe");
            assert!(tol(op.dib_dvbc, num_deriv(|v| m.eval(vbe, v).ib, vbc)), "dIb/dVbc");
        }
    }

    #[test]
    fn bjt_forward_active_gain_is_beta() {
        let m = BjtModel::BC547;
        let op = m.eval(0.65, -5.0);
        assert!((op.ic / op.ib - m.bf).abs() / m.bf < 1e-6);
    }

    #[test]
    fn mos_derivatives_match_numeric_in_all_regions() {
        let m = MosModel::N2N7000;
        for &(vgs, vds) in &[(5.0, 0.3), (3.0, 4.0), (1.0, 2.0), (5.0, -0.5), (4.0, -3.0)] {
            let op = m.eval(vgs, vds);
            let gm = num_deriv(|v| m.eval(v, vds).id, vgs);
            let gds = num_deriv(|v| m.eval(vgs, v).id, vds);
            assert!((op.gm - gm).abs() < 1e-5 * gm.abs().max(1e-6), "gm bei {vgs},{vds}: {} vs {gm}", op.gm);
            assert!((op.gds - gds).abs() < 1e-5 * gds.abs().max(1e-6), "gds bei {vgs},{vds}: {} vs {gds}", op.gds);
        }
    }

    #[test]
    fn mos_regions() {
        let m = MosModel::N2N7000;
        assert_eq!(m.eval(1.0, 5.0).region, MosRegion::Off);
        assert_eq!(m.eval(5.0, 0.1).region, MosRegion::Linear);
        assert_eq!(m.eval(3.0, 5.0).region, MosRegion::Saturation);
        // Sättigungsstrom K·(U_GS − U_th)²·(1 + λ·U_DS)
        let id = m.eval(3.0, 5.0).id;
        assert!((id - 0.06 * 0.81 * 1.05).abs() < 1e-12);
    }
}
