//! Modifizierte Knotenanalyse (MNA), DC-Arbeitspunkt und Transientenanalyse.
//!
//! Aufbau des Gleichungssystems `G·x = b`:
//! - Zeilen `0 .. N-1`: Knotenspannungen (Masse fällt weg), Kirchhoffsche Knotenregel
//! - Zeilen `N-1 ..`: Zweigströme von Spannungsquellen und Spulen
//!
//! Nichtlineare Bauteile werden pro Newton-Iteration linearisiert (Begleitmodell
//! aus Leitwert + Stromquelle), Kondensatoren und Spulen pro Zeitschritt durch
//! das Begleitmodell des Integrationsverfahrens ersetzt.

use crate::circuit::{Circuit, ElementKind, NodeId, SimError};
use crate::diode::{pnjlim, VT_300K};
use crate::linalg::Matrix;
use crate::transistor::fetlim;
use std::borrow::Cow;
use std::collections::HashMap;

/// Leitwert parallel zu jedem pn-Übergang (wie GMIN in SPICE).
const GMIN_JUNCTION: f64 = 1e-12;
/// Leitwert, mit dem Kondensatoren beim Setzen der Anfangsbedingung festgehalten werden.
const G_IC: f64 = 1e6;

/// Toleranzen des Newton-Verfahrens.
#[derive(Clone, Debug)]
pub struct SimOptions {
    /// Relative Toleranz für Spannungen und Ströme.
    pub reltol: f64,
    /// Absolute Toleranz für Knotenspannungen in V.
    pub vntol: f64,
    /// Absolute Toleranz für Zweigströme in A.
    pub abstol: f64,
    /// Leitwert jedes Knotens gegen Masse in S. Macht Knoten lösbar, die nur
    /// über Kondensatoren angebunden sind, und verfälscht praktisch nichts.
    pub gshunt: f64,
    /// Maximale Newton-Iterationen beim Arbeitspunkt.
    pub max_iter_dc: usize,
    /// Maximale Newton-Iterationen pro Zeitschritt.
    pub max_iter_tran: usize,
}

impl Default for SimOptions {
    fn default() -> Self {
        Self { reltol: 1e-4, vntol: 1e-7, abstol: 1e-10, gshunt: 1e-12, max_iter_dc: 150, max_iter_tran: 40 }
    }
}

/// Einstellungen der Transientenanalyse.
#[derive(Clone, Debug)]
pub struct TranOptions {
    /// Endzeit in s.
    pub t_stop: f64,
    /// Größter erlaubter Zeitschritt in s (bestimmt auch die Auflösung der Kurven).
    pub h_max: f64,
    /// Kleinster Zeitschritt, darunter wird abgebrochen.
    pub h_min: f64,
    /// Anfangswerte von Kondensatoren/Spulen (`ic`) verwenden statt Arbeitspunkt zu rechnen.
    pub use_ic: bool,
    /// Relative Toleranz des lokalen Abbruchfehlers.
    pub lte_reltol: f64,
    /// Absolute Toleranz des lokalen Abbruchfehlers in V.
    pub lte_abstol: f64,
}

impl TranOptions {
    pub fn new(t_stop: f64, h_max: f64) -> Self {
        Self { t_stop, h_max, h_min: (t_stop * 1e-15).max(1e-18), use_ic: false, lte_reltol: 1e-3, lte_abstol: 1e-6 }
    }

    pub fn with_ic(mut self) -> Self {
        self.use_ic = true;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Method {
    BackwardEuler,
    Trapezoidal,
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Dc,
    /// Anfangsbedingungen setzen: Kondensatoren auf `ic` festhalten, Spulenströme auf `ic`.
    InitIc,
    Tran {
        h: f64,
        method: Method,
    },
}

/// Rückmeldung eines Prüfers zu einem vorläufig berechneten Zeitschritt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepCheck {
    Accept,
    /// Schritt verwerfen und nur bis zu diesem Zeitpunkt rechnen
    Retry(f64),
}

/// Prüfer für vorläufige Zeitschritte: (t, x, t_neu, x_neu) → annehmen oder verkürzen.
pub type StepChecker<'a> = dyn FnMut(f64, &[f64], f64, &[f64]) -> StepCheck + 'a;

/// Zustand einer laufenden, schrittweisen Transientenanalyse.
#[derive(Clone, Debug)]
pub struct TranState {
    /// Aktuelle Zeit in s
    pub t: f64,
    x: Vec<f64>,
    react: Vec<Reactive>,
    hist: Vec<(f64, Vec<f64>)>,
    after_bp: bool,
    /// Vorgeschlagener nächster Zeitschritt
    pub h: f64,
}

impl TranState {
    /// Lösungsvektor (Knotenspannungen ohne Masse, dann Zweigströme).
    pub fn x(&self) -> &[f64] {
        &self.x
    }
}

/// Kennung im Feld `i`: `v` ist die nachlaufende Ausgangsspannung eines OpAmps.
const OPAMP_STATE: f64 = 1.0;

/// Gespeicherter Zustand eines Energiespeichers aus dem letzten Zeitschritt.
#[derive(Clone, Copy, Debug, Default)]
struct Reactive {
    v: f64,
    i: f64,
}

struct Ctx<'a> {
    t: f64,
    mode: Mode,
    src_scale: f64,
    g_extra: f64,
    react: &'a [Reactive],
}

enum NewtonFail {
    Singular(usize),
    NoConvergence,
}

/// Zuordnung von Knoten und Zweigen zu Zeilen des Gleichungssystems.
#[derive(Clone, Debug)]
struct Layout {
    n_nodes: usize,
    size: usize,
    branch: Vec<Option<usize>>,
    /// Erster Platz im Vektor der begrenzten Steuerspannungen (Diode: 1, Transistor: 2)
    lim: Vec<Option<usize>>,
    n_lim: usize,
    nonlinear: bool,
}

impl Layout {
    fn new(c: &Circuit) -> Self {
        let n_nodes = c.node_count();
        let mut size = n_nodes - 1;
        let mut n_lim = 0;
        let mut branch = Vec::new();
        let mut lim = Vec::new();
        for e in c.elements() {
            if e.kind.needs_branch() {
                branch.push(Some(size));
                size += 1;
            } else {
                branch.push(None);
            }
            let slots = match e.kind {
                ElementKind::Diode { .. } => 1,
                ElementKind::Bjt { .. } | ElementKind::Mosfet { .. } => 2,
                _ => 0,
            };
            if slots > 0 {
                lim.push(Some(n_lim));
                n_lim += slots;
            } else {
                lim.push(None);
            }
        }
        let nonlinear = c.elements().iter().any(|e| e.kind.is_nonlinear());
        Self { n_nodes, size, branch, lim, n_lim, nonlinear }
    }
}

#[inline]
fn row(node: NodeId) -> Option<usize> {
    node.checked_sub(1)
}

#[inline]
fn volt(x: &[f64], node: NodeId) -> f64 {
    row(node).map_or(0.0, |r| x[r])
}

/// Knotenspannung aus einem Lösungsvektor (für Mixed-Signal-Kopplung).
#[inline]
pub fn node_voltage(x: &[f64], node: NodeId) -> f64 {
    volt(x, node)
}

struct System {
    m: Matrix,
    rhs: Vec<f64>,
}

impl System {
    fn new(n: usize) -> Self {
        Self { m: Matrix::zeros(n), rhs: vec![0.0; n] }
    }

    fn clear(&mut self) {
        self.m.clear();
        self.rhs.iter_mut().for_each(|v| *v = 0.0);
    }

    #[inline]
    fn add(&mut self, r: Option<usize>, c: Option<usize>, v: f64) {
        if let (Some(r), Some(c)) = (r, c) {
            self.m.add(r, c, v);
        }
    }

    /// Leitwert `g` zwischen Knoten `a` und `b`.
    fn conductance(&mut self, a: NodeId, b: NodeId, g: f64) {
        let (ra, rb) = (row(a), row(b));
        self.add(ra, ra, g);
        self.add(rb, rb, g);
        self.add(ra, rb, -g);
        self.add(rb, ra, -g);
    }

    /// Strom `i`, der im Bauteil von `a` nach `b` fließt.
    fn current(&mut self, a: NodeId, b: NodeId, i: f64) {
        if let Some(r) = row(a) {
            self.rhs[r] -= i;
        }
        if let Some(r) = row(b) {
            self.rhs[r] += i;
        }
    }

    /// Anschlussstrom eines Bauteils, der am Knoten `node` in das Bauteil fließt und von
    /// der Spannung U(a) − U(b) abhängt: I = sign · g · (U_a − U_b) + …
    fn vccs(&mut self, node: NodeId, sign: f64, a: NodeId, b: NodeId, g: f64) {
        self.add(row(node), row(a), sign * g);
        self.add(row(node), row(b), -sign * g);
    }

    /// Konstanter Anteil eines Anschlussstroms (in das Bauteil hinein).
    fn inject(&mut self, node: NodeId, c: f64) {
        if let Some(r) = row(node) {
            self.rhs[r] -= c;
        }
    }

    /// Kopplung einer Zweigstromvariable `br` (Strom von `a` durch das Bauteil nach `b`).
    fn branch_incidence(&mut self, a: NodeId, b: NodeId, br: usize) {
        self.add(row(a), Some(br), 1.0);
        self.add(row(b), Some(br), -1.0);
        self.add(Some(br), row(a), 1.0);
        self.add(Some(br), row(b), -1.0);
    }
}

/// Simulator für eine feste Schaltungstopologie.
pub struct Simulator<'c> {
    circuit: Cow<'c, Circuit>,
    layout: Layout,
    /// Zustand der Logikausgänge ([`ElementKind::Driver`]) je Element: Anteil High oder hochohmig
    drive: Vec<Option<f64>>,
    pub options: SimOptions,
    pub stats: Stats,
}

/// Zähler für Diagnose und Benchmarks.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub newton_iterations: usize,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pub gmin_stepping: bool,
    pub source_stepping: bool,
}

impl Simulator<'static> {
    /// Simulator, der seine Schaltung selbst besitzt (z. B. für eine Echtzeitsimulation).
    pub fn owned(circuit: Circuit) -> Result<Self, SimError> {
        Self::with(Cow::Owned(circuit))
    }
}

impl<'c> Simulator<'c> {
    pub fn new(circuit: &'c Circuit) -> Result<Self, SimError> {
        Self::with(Cow::Borrowed(circuit))
    }

    fn with(circuit: Cow<'c, Circuit>) -> Result<Self, SimError> {
        circuit.check_connectivity()?;
        let layout = Layout::new(&circuit);
        let drive = vec![None; circuit.elements().len()];
        Ok(Self { circuit, layout, drive, options: SimOptions::default(), stats: Stats::default() })
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    /// Ersetzt die Werte eines Bauteils bei gleicher Verschaltung (Schalter umgelegt,
    /// Poti verstellt), ohne den Simulator neu aufzubauen.
    pub fn update_element(&mut self, name: &str, kind: ElementKind) -> Result<(), SimError> {
        let idx = self.circuit.find_element(name).ok_or_else(|| SimError::UnknownName(name.into()))?;
        let old = &self.circuit.elements()[idx].kind;
        if old.terminals() != kind.terminals() || std::mem::discriminant(old) != std::mem::discriminant(&kind) {
            return Err(SimError::InvalidValue { element: name.into(), msg: "Verschaltung geändert".into() });
        }
        self.circuit.to_mut().update(name, kind)
    }

    /// Zustand eines Logikausgangs.
    pub fn drive(&self, elem: usize) -> Option<f64> {
        self.drive[elem]
    }

    /// Setzt den Zustand eines Logikausgangs: `Some(f)` mit f ∈ [0, 1] (1 = High),
    /// `None` = hochohmig. Gibt zurück, ob sich etwas geändert hat.
    pub fn set_drive(&mut self, elem: usize, f: Option<f64>) -> bool {
        let f = f.map(|v| v.clamp(0.0, 1.0));
        if self.drive[elem] == f {
            return false;
        }
        self.drive[elem] = f;
        true
    }

    /// Anzahl der Unbekannten im Gleichungssystem.
    pub fn system_size(&self) -> usize {
        self.layout.size
    }

    // ---------------------------------------------------------------------
    // Aufstellen des Gleichungssystems
    // ---------------------------------------------------------------------

    /// Stellt das linearisierte System am Punkt `x` auf. Gibt zurück, ob eine
    /// Diodenspannung begrenzt wurde (dann ist die Iteration nicht konvergiert).
    fn assemble(&self, x: &[f64], ctx: &Ctx, dv: &mut [f64], sys: &mut System) -> bool {
        sys.clear();
        let g_node = self.options.gshunt + ctx.g_extra;
        for r in 0..self.layout.n_nodes - 1 {
            sys.m.add(r, r, g_node);
        }
        let mut limited = false;
        for (ei, e) in self.circuit.elements().iter().enumerate() {
            match &e.kind {
                ElementKind::Resistor { a, b, r } => sys.conductance(*a, *b, 1.0 / r),
                ElementKind::Switch { a, b, closed, r_on, r_off } => sys.conductance(*a, *b, 1.0 / if *closed { r_on } else { r_off }),
                ElementKind::Capacitor { a, b, c, ic } => match ctx.mode {
                    Mode::Dc => {}
                    Mode::InitIc => {
                        let v0 = ic.unwrap_or(0.0);
                        sys.conductance(*a, *b, G_IC);
                        sys.current(*a, *b, -G_IC * v0);
                    }
                    Mode::Tran { h, method } => {
                        let st = ctx.react[ei];
                        let (geq, ieq) = match method {
                            Method::BackwardEuler => (c / h, -(c / h) * st.v),
                            Method::Trapezoidal => (2.0 * c / h, -(2.0 * c / h * st.v + st.i)),
                        };
                        sys.conductance(*a, *b, geq);
                        sys.current(*a, *b, ieq);
                    }
                },
                ElementKind::Inductor { a, b, l, ic } => {
                    let br = self.layout.branch[ei].unwrap();
                    match ctx.mode {
                        Mode::Dc => sys.branch_incidence(*a, *b, br),
                        Mode::InitIc => {
                            // Zweigstrom fest vorgeben: j = ic
                            sys.add(row(*a), Some(br), 1.0);
                            sys.add(row(*b), Some(br), -1.0);
                            sys.m.add(br, br, 1.0);
                            sys.rhs[br] = ic.unwrap_or(0.0);
                        }
                        Mode::Tran { h, method } => {
                            sys.branch_incidence(*a, *b, br);
                            let st = ctx.react[ei];
                            match method {
                                Method::BackwardEuler => {
                                    sys.m.add(br, br, -l / h);
                                    sys.rhs[br] = -(l / h) * st.i;
                                }
                                Method::Trapezoidal => {
                                    sys.m.add(br, br, -2.0 * l / h);
                                    sys.rhs[br] = -(2.0 * l / h) * st.i - st.v;
                                }
                            }
                        }
                    }
                }
                ElementKind::VSource { p, n, wave } => {
                    let br = self.layout.branch[ei].unwrap();
                    sys.branch_incidence(*p, *n, br);
                    sys.rhs[br] = ctx.src_scale * wave.value(ctx.t);
                }
                ElementKind::ISource { p, n, wave } => {
                    // Strom fließt in der Quelle von n nach p und tritt bei p aus.
                    sys.current(*n, *p, ctx.src_scale * wave.value(ctx.t));
                }
                ElementKind::Diode { a, k, model } => {
                    let di = self.layout.lim[ei].unwrap();
                    let vraw = volt(x, *a) - volt(x, *k);
                    let (vd, lim) = if vraw < 0.0 && model.bv > 0.0 {
                        model.limit_reverse(vraw, dv[di])
                    } else {
                        pnjlim(vraw, dv[di], model.nvt(), model.vcrit())
                    };
                    limited |= lim;
                    dv[di] = vd;
                    let (id, gd) = model.eval(vd);
                    let (id, gd) = (id + GMIN_JUNCTION * vd, gd + GMIN_JUNCTION);
                    sys.conductance(*a, *k, gd);
                    sys.current(*a, *k, id - gd * vd);
                }
                ElementKind::Bjt { c, b, e, model } => {
                    let si = self.layout.lim[ei].unwrap();
                    let p = model.polarity();
                    let (vbe, l1) = pnjlim(p * (volt(x, *b) - volt(x, *e)), dv[si], VT_300K, model.vcrit());
                    let (vbc, l2) = pnjlim(p * (volt(x, *b) - volt(x, *c)), dv[si + 1], VT_300K, model.vcrit());
                    limited |= l1 || l2;
                    dv[si] = vbe;
                    dv[si + 1] = vbc;
                    let op = model.eval(vbe, vbc);
                    let cc = p * (op.ic - op.dic_dvbe * vbe - op.dic_dvbc * vbc);
                    let cb = p * (op.ib - op.dib_dvbe * vbe - op.dib_dvbc * vbc);
                    sys.vccs(*c, 1.0, *b, *e, op.dic_dvbe);
                    sys.vccs(*c, 1.0, *b, *c, op.dic_dvbc);
                    sys.inject(*c, cc);
                    sys.vccs(*b, 1.0, *b, *e, op.dib_dvbe);
                    sys.vccs(*b, 1.0, *b, *c, op.dib_dvbc);
                    sys.inject(*b, cb);
                    sys.vccs(*e, -1.0, *b, *e, op.dic_dvbe + op.dib_dvbe);
                    sys.vccs(*e, -1.0, *b, *c, op.dic_dvbc + op.dib_dvbc);
                    sys.inject(*e, -(cc + cb));
                    sys.conductance(*b, *e, GMIN_JUNCTION);
                    sys.conductance(*b, *c, GMIN_JUNCTION);
                }
                ElementKind::Mosfet { d, g, s, model } => {
                    let si = self.layout.lim[ei].unwrap();
                    let p = model.polarity();
                    let (vgs, l1) = fetlim(p * (volt(x, *g) - volt(x, *s)), dv[si], 1.0);
                    let (vds, l2) = fetlim(p * (volt(x, *d) - volt(x, *s)), dv[si + 1], 2.0);
                    limited |= l1 || l2;
                    dv[si] = vgs;
                    dv[si + 1] = vds;
                    let op = model.eval(vgs, vds);
                    let cd = p * (op.id - op.gm * vgs - op.gds * vds);
                    sys.vccs(*d, 1.0, *g, *s, op.gm);
                    sys.vccs(*d, 1.0, *d, *s, op.gds);
                    sys.inject(*d, cd);
                    sys.vccs(*s, -1.0, *g, *s, op.gm);
                    sys.vccs(*s, -1.0, *d, *s, op.gds);
                    sys.inject(*s, -cd);
                    sys.conductance(*d, *s, GMIN_JUNCTION);
                }
                ElementKind::Driver { out, hi, lo, r } => {
                    if let Some(f) = self.drive[ei] {
                        if f > 0.0 {
                            sys.conductance(*out, *hi, f / r);
                        }
                        if f < 1.0 {
                            sys.conductance(*out, *lo, (1.0 - f) / r);
                        }
                    } else {
                        // Hochohmig (aus oder ohne Versorgung): Leckstrom wie bei einem echten
                        // Ausgang, hält einen sonst offenen Knoten definiert
                        sys.conductance(*out, *lo, 1e-9);
                    }
                }
                ElementKind::OpAmp { p, n, out, vcc, vee, model } => {
                    let (vp, vn, vo, vc, ve) = (volt(x, *p), volt(x, *n), volt(x, *out), volt(x, *vcc), volt(x, *vee));
                    let mut op = model.eval(vp - vn, vc, ve);
                    // Im Zeitverlauf folgt der Ausgang seinem Ziel mit der Zeitkonstante tau
                    // (immer Euler-rückwärts: stabil auch bei sehr hoher Schleifenverstärkung)
                    if let Mode::Tran { h, .. } = ctx.mode {
                        let a = h / model.tau;
                        let k = a / (1.0 + a);
                        op.v_target = (ctx.react[ei].v + a * op.v_target) / (1.0 + a);
                        op.d_diff *= k;
                        op.d_vcc *= k;
                        op.d_vee *= k;
                    }
                    let g = 1.0 / model.r_out;
                    // Ausgangsstrom i = (U_ziel − U_out)/R, linearisiert um den aktuellen Punkt
                    let i0 = (op.v_target - vo) * g;
                    let d = [(*p, op.d_diff * g), (*n, -op.d_diff * g), (*vcc, op.d_vcc * g), (*vee, op.d_vee * g), (*out, -g)];
                    let lin: f64 = d.iter().map(|(k, dk)| dk * volt(x, *k)).sum();
                    // Anschlussströme in das Bauteil: Ausgang −i, Versorgung liefert i
                    let src = if i0 >= 0.0 { *vcc } else { *vee };
                    for (node, c) in [(*out, -1.0), (src, 1.0)] {
                        for (k, dk) in d {
                            sys.add(row(node), row(k), c * dk);
                        }
                        sys.inject(node, c * (i0 - lin));
                    }
                    sys.conductance(*p, *n, GMIN_JUNCTION);
                }
            }
        }
        limited
    }

    /// Startwerte der begrenzten Steuerspannungen aus dem Lösungsvektor.
    fn diode_voltages(&self, x: &[f64]) -> Vec<f64> {
        let mut dv = vec![0.0; self.layout.n_lim];
        for (ei, e) in self.circuit.elements().iter().enumerate() {
            let Some(si) = self.layout.lim[ei] else { continue };
            match e.kind {
                ElementKind::Diode { a, k, .. } => dv[si] = volt(x, a) - volt(x, k),
                ElementKind::Bjt { c, b, e, model } => {
                    let p = model.polarity();
                    dv[si] = p * (volt(x, b) - volt(x, e));
                    dv[si + 1] = p * (volt(x, b) - volt(x, c));
                }
                ElementKind::Mosfet { d, g, s, model } => {
                    let p = model.polarity();
                    dv[si] = p * (volt(x, g) - volt(x, s));
                    dv[si + 1] = p * (volt(x, d) - volt(x, s));
                }
                _ => {}
            }
        }
        dv
    }

    /// Steile Verhaltensmodelle (OpAmp): Schon winzige Spannungsänderungen am Eingang
    /// ändern den Ausgangsstrom stark. Konvergiert ist erst, wenn sich auch die
    /// Ausgangszielspannung nicht mehr ändert.
    fn targets_converged(&self, x: &[f64], sol: &[f64]) -> bool {
        let o = &self.options;
        self.circuit.elements().iter().all(|e| match &e.kind {
            ElementKind::OpAmp { p, n, vcc, vee, model, .. } => {
                let t = |v: &[f64]| model.eval(volt(v, *p) - volt(v, *n), volt(v, *vcc), volt(v, *vee)).v_target;
                let (a, b) = (t(x), t(sol));
                (a - b).abs() <= o.reltol * a.abs().max(b.abs()) + 10.0 * o.vntol
            }
            _ => true,
        })
    }

    /// Newton-Raphson ab Startwert `x`; bei Erfolg steht die Lösung in `x`.
    fn newton(&mut self, x: &mut Vec<f64>, ctx: &Ctx, max_iter: usize) -> Result<usize, NewtonFail> {
        let n = self.layout.size;
        let nv = self.layout.n_nodes - 1;
        let mut sys = System::new(n);
        let mut dv = self.diode_voltages(x);
        let o = &self.options;
        for it in 1..=max_iter {
            let limited = self.assemble(x, ctx, &mut dv, &mut sys);
            let mut sol = sys.rhs.clone();
            sys.m.solve_in_place(&mut sol).map_err(NewtonFail::Singular)?;
            self.stats.newton_iterations += 1;
            if !self.layout.nonlinear {
                *x = sol;
                return Ok(it);
            }
            let mut conv = !limited && self.targets_converged(x, &sol);
            if conv {
                for i in 0..n {
                    let tol = o.reltol * sol[i].abs().max(x[i].abs()) + if i < nv { o.vntol } else { o.abstol };
                    if (sol[i] - x[i]).abs() > tol {
                        conv = false;
                        break;
                    }
                }
            }
            if sol.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                return Err(NewtonFail::NoConvergence);
            }
            *x = sol;
            if conv && it > 1 {
                return Ok(it);
            }
        }
        Err(NewtonFail::NoConvergence)
    }

    fn singular_hint(&self, row_idx: usize) -> String {
        let nv = self.layout.n_nodes - 1;
        if row_idx < nv {
            format!(
                "Prüfe den Bereich um Knoten „{}“: Vielleicht liegen dort Spannungsquellen oder Spulen parallel, \
                 oder eine Stromquelle hat keinen geschlossenen Stromkreis.",
                self.circuit.node_name(row_idx + 1)
            )
        } else {
            let ei = self.layout.branch.iter().position(|b| *b == Some(row_idx));
            match ei {
                Some(ei) => {
                    let e = &self.circuit.elements()[ei];
                    format!("Prüfe {} „{}“: Sie liegt vermutlich in einer Masche nur aus Spannungsquellen und Spulen.", e.kind.type_name(), e.name)
                }
                None => "Prüfe die Schaltung auf Maschen aus Spannungsquellen.".into(),
            }
        }
    }

    fn map_fail(&self, f: NewtonFail, time: Option<f64>) -> SimError {
        match f {
            NewtonFail::Singular(r) => SimError::Singular { hint: self.singular_hint(r) },
            NewtonFail::NoConvergence => SimError::NoConvergence { time },
        }
    }

    // ---------------------------------------------------------------------
    // DC-Arbeitspunkt
    // ---------------------------------------------------------------------

    /// DC-Arbeitspunkt mit Quellenwerten zum Zeitpunkt `t`.
    /// Bei Problemen wird automatisch auf gmin-Stepping und Source-Stepping ausgewichen.
    fn solve_dc(&mut self, t: f64) -> Result<Vec<f64>, SimError> {
        let n = self.layout.size;
        let max_it = self.options.max_iter_dc;
        let ctx = |g_extra: f64, src_scale: f64| Ctx { t, mode: Mode::Dc, src_scale, g_extra, react: &[] };

        // 1. Direkter Versuch
        let mut x = vec![0.0; n];
        match self.newton(&mut x, &ctx(0.0, 1.0), max_it) {
            Ok(_) => return Ok(x),
            Err(NewtonFail::Singular(r)) => return Err(self.map_fail(NewtonFail::Singular(r), None)),
            Err(NewtonFail::NoConvergence) => {}
        }

        // 2. gmin-Stepping: alle Knoten zunächst stark gegen Masse ziehen, dann loslassen
        self.stats.gmin_stepping = true;
        let mut x = vec![0.0; n];
        let mut g = 1e-2;
        let mut ok = true;
        while g > self.options.gshunt {
            if self.newton(&mut x, &ctx(g, 1.0), max_it).is_err() {
                ok = false;
                break;
            }
            g /= 10.0;
        }
        if ok && self.newton(&mut x, &ctx(0.0, 1.0), max_it).is_ok() {
            return Ok(x);
        }

        // 3. Source-Stepping: alle Quellen von 0 % auf 100 % hochfahren
        self.stats.source_stepping = true;
        let mut x = vec![0.0; n];
        self.newton(&mut x, &ctx(0.0, 0.0), max_it).map_err(|f| self.map_fail(f, None))?;
        let (mut scale, mut step) = (0.0_f64, 0.1_f64);
        while scale < 1.0 {
            let next = (scale + step).min(1.0);
            let mut trial = x.clone();
            if self.newton(&mut trial, &ctx(0.0, next), max_it).is_ok() {
                x = trial;
                scale = next;
                step = (step * 1.5).min(0.25);
            } else {
                step /= 4.0;
                if step < 1e-7 {
                    return Err(SimError::NoConvergence { time: None });
                }
            }
        }
        Ok(x)
    }

    /// Berechnet den DC-Arbeitspunkt (Kondensatoren offen, Spulen kurzgeschlossen).
    pub fn operating_point(&mut self) -> Result<OpResult, SimError> {
        let x = self.solve_dc(0.0)?;
        let react = vec![Reactive::default(); self.circuit.elements().len()];
        let currents = self.element_currents(&x, &react, 0.0);
        let term = self.terminal_currents(&x, &currents);
        Ok(OpResult { probe: Probe::new(&self.circuit), x, currents, term })
    }

    // ---------------------------------------------------------------------
    // Transientenanalyse
    // ---------------------------------------------------------------------

    fn breakpoints(&self, t_stop: f64) -> Vec<f64> {
        let mut bp = Vec::new();
        for e in self.circuit.elements() {
            match &e.kind {
                ElementKind::VSource { wave, .. } | ElementKind::ISource { wave, .. } => wave.breakpoints(t_stop, &mut bp),
                _ => {}
            }
        }
        bp.push(t_stop);
        bp.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let eps = t_stop * 1e-12;
        bp.dedup_by(|a, b| (*a - *b).abs() <= eps);
        bp
    }

    /// Anfangslösung für die Transientenanalyse: Arbeitspunkt oder – mit `use_ic` –
    /// Kondensatoren auf ihrer Anfangsspannung (sonst 0 V), Spulen stromlos.
    pub fn initial_solution(&mut self, use_ic: bool) -> Result<Vec<f64>, SimError> {
        if use_ic {
            let react = vec![Reactive::default(); self.circuit.elements().len()];
            let ctx = Ctx { t: 0.0, mode: Mode::InitIc, src_scale: 1.0, g_extra: 0.0, react: &react };
            let mut x = vec![0.0; self.layout.size];
            match self.newton(&mut x, &ctx, self.options.max_iter_dc) {
                Ok(_) => Ok(x),
                Err(NewtonFail::Singular(r)) => Err(self.map_fail(NewtonFail::Singular(r), Some(0.0))),
                Err(NewtonFail::NoConvergence) => {
                    // Rückfallebene wie beim Arbeitspunkt: Quellen hochfahren
                    let mut x = vec![0.0; self.layout.size];
                    let (mut scale, mut step) = (0.0_f64, 0.1_f64);
                    while scale < 1.0 {
                        let next = (scale + step).min(1.0);
                        let mut trial = x.clone();
                        let ctx = Ctx { t: 0.0, mode: Mode::InitIc, src_scale: next, g_extra: 0.0, react: &react };
                        if self.newton(&mut trial, &ctx, self.options.max_iter_dc).is_ok() {
                            x = trial;
                            scale = next;
                            step = (step * 1.5).min(0.25);
                        } else {
                            step /= 4.0;
                            if step < 1e-7 {
                                return Err(SimError::NoConvergence { time: Some(0.0) });
                            }
                        }
                    }
                    Ok(x)
                }
            }
        } else {
            self.solve_dc(0.0)
        }
    }

    /// Startzustand einer schrittweisen Transientenanalyse aus einer Anfangslösung.
    pub fn tran_state(&self, x0: Vec<f64>, use_ic: bool) -> TranState {
        let mut react = vec![Reactive::default(); self.circuit.elements().len()];
        for (ei, e) in self.circuit.elements().iter().enumerate() {
            match e.kind {
                ElementKind::Capacitor { a, b, ic, .. } => {
                    let v = if use_ic { ic.unwrap_or(0.0) } else { volt(&x0, a) - volt(&x0, b) };
                    react[ei] = Reactive { v, i: 0.0 };
                }
                ElementKind::Inductor { a, b, .. } => {
                    let br = self.layout.branch[ei].unwrap();
                    react[ei] = Reactive { v: volt(&x0, a) - volt(&x0, b), i: x0[br] };
                    if !use_ic {
                        react[ei].v = 0.0;
                    }
                }
                ElementKind::OpAmp { p, n, vcc, vee, model, .. } => {
                    let vt = model.eval(volt(&x0, p) - volt(&x0, n), volt(&x0, vcc), volt(&x0, vee)).v_target;
                    react[ei] = Reactive { v: vt, i: OPAMP_STATE };
                }
                _ => {}
            }
        }
        TranState { t: 0.0, hist: vec![(0.0, x0.clone())], x: x0, react, after_bp: true, h: 0.0 }
    }

    /// Markiert eine Unstetigkeit (Knickstelle einer Quelle, umgeschalteter Logikausgang):
    /// Der nächste Schritt rechnet Euler-rückwärts und beginnt klein.
    pub fn mark_discontinuity(&self, st: &mut TranState, gap: f64, opts: &TranOptions) {
        st.after_bp = true;
        st.hist.clear();
        st.hist.push((st.t, st.x.clone()));
        st.h = opts.h_max.min(gap.max(opts.h_min * 10.0)) * 0.02;
    }

    /// Rechnet genau einen angenommenen Zeitschritt, höchstens bis `t_limit`.
    /// `check` sieht jeden vorläufigen Schritt (t, x, t_neu, x_neu) und kann ihn
    /// verkürzen lassen (z. B. um eine Schaltschwelle genau zu treffen).
    /// Gibt `true` zurück, wenn `t_limit` erreicht wurde.
    pub fn tran_step(&mut self, st: &mut TranState, t_limit: f64, opts: &TranOptions, check: &mut StepChecker) -> Result<bool, SimError> {
        let nv = self.layout.n_nodes - 1;
        let mut forced: Option<f64> = None;
        let mut retries = 0;
        // Nach einer Ablehnung nicht mehr an die Grenze „heranschnappen“: Sonst würde der
        // verkleinerte Schritt wieder auf die alte Länge gedehnt und erneut abgelehnt.
        let mut rejected = false;
        loop {
            let (t_new, hit) = match forced {
                Some(tf) => (tf, false),
                None => {
                    let t_new = st.t + st.h;
                    if t_new >= t_limit || (!rejected && t_limit - t_new < 0.2 * st.h) {
                        (t_limit, true)
                    } else {
                        (t_new, false)
                    }
                }
            };
            let h_eff = t_new - st.t;
            let method = if st.after_bp { Method::BackwardEuler } else { Method::Trapezoidal };

            // Startwert: Extrapolation, falls verfügbar
            let mut x_new = if st.hist.len() >= 2 && !st.after_bp {
                let (t1, x1) = &st.hist[st.hist.len() - 2];
                let (t2, x2) = &st.hist[st.hist.len() - 1];
                let f = (t_new - t2) / (t2 - t1);
                x2.iter().zip(x1).map(|(b, a)| b + (b - a) * f).collect()
            } else {
                st.x.clone()
            };
            let ctx = Ctx { t: t_new, mode: Mode::Tran { h: h_eff, method }, src_scale: 1.0, g_extra: 0.0, react: &st.react };
            match self.newton(&mut x_new, &ctx, self.options.max_iter_tran) {
                Ok(_) => {}
                Err(NewtonFail::Singular(r)) => return Err(self.map_fail(NewtonFail::Singular(r), Some(t_new))),
                Err(NewtonFail::NoConvergence) => {
                    self.stats.rejected_steps += 1;
                    rejected = true;
                    st.h = h_eff / 8.0;
                    forced = None;
                    if st.h < opts.h_min {
                        return Err(SimError::TimestepTooSmall { time: st.t });
                    }
                    continue;
                }
            }

            // Lokaler Abbruchfehler über Prädiktor-Korrektor-Vergleich (Milne):
            // Trapezregel hat Fehlerkonstante 1/12, quadratische Extrapolation 1,
            // daher LTE ≈ |x_korr − x_präd| / 13.
            let mut growth = 2.0;
            if method == Method::Trapezoidal && st.hist.len() >= 3 {
                let k = st.hist.len();
                let (ta, xa) = (&st.hist[k - 3].0, &st.hist[k - 3].1);
                let (tb, xb) = (&st.hist[k - 2].0, &st.hist[k - 2].1);
                let (tc, xc) = (&st.hist[k - 1].0, &st.hist[k - 1].1);
                let la = (t_new - tb) * (t_new - tc) / ((ta - tb) * (ta - tc));
                let lb = (t_new - ta) * (t_new - tc) / ((tb - ta) * (tb - tc));
                let lc = (t_new - ta) * (t_new - tb) / ((tc - ta) * (tc - tb));
                let mut ratio: f64 = 0.0;
                for i in 0..nv {
                    let pred = la * xa[i] + lb * xb[i] + lc * xc[i];
                    let lte = (x_new[i] - pred).abs() / 13.0;
                    let tol = opts.lte_reltol * x_new[i].abs().max(xc[i].abs()) + opts.lte_abstol;
                    ratio = ratio.max(lte / tol);
                }
                if ratio > 1.0 && h_eff > opts.h_min * 10.0 {
                    self.stats.rejected_steps += 1;
                    rejected = true;
                    st.h = h_eff * (0.9 * ratio.powf(-1.0 / 3.0)).max(0.2);
                    forced = None;
                    continue;
                }
                growth = if ratio > 0.0 { (0.9 * ratio.powf(-1.0 / 3.0)).clamp(0.5, 2.0) } else { 2.0 };
            }

            if let StepCheck::Retry(tr) = check(st.t, &st.x, t_new, &x_new) {
                if tr > st.t && tr < t_new && retries < 40 {
                    retries += 1;
                    forced = Some(tr);
                    continue;
                }
            }

            // Schritt annehmen: Zustände der Energiespeicher fortschreiben
            for (ei, e) in self.circuit.elements().iter().enumerate() {
                match e.kind {
                    ElementKind::Capacitor { a, b, c, .. } => {
                        let rs = st.react[ei];
                        let v = volt(&x_new, a) - volt(&x_new, b);
                        let i = match method {
                            Method::BackwardEuler => c / h_eff * (v - rs.v),
                            Method::Trapezoidal => 2.0 * c / h_eff * (v - rs.v) - rs.i,
                        };
                        st.react[ei] = Reactive { v, i };
                    }
                    ElementKind::Inductor { a, b, .. } => {
                        let br = self.layout.branch[ei].unwrap();
                        st.react[ei] = Reactive { v: volt(&x_new, a) - volt(&x_new, b), i: x_new[br] };
                    }
                    ElementKind::OpAmp { p, n, vcc, vee, model, .. } => {
                        let target = model.eval(volt(&x_new, p) - volt(&x_new, n), volt(&x_new, vcc), volt(&x_new, vee)).v_target;
                        let a = h_eff / model.tau;
                        st.react[ei] = Reactive { v: (st.react[ei].v + a * target) / (1.0 + a), i: OPAMP_STATE };
                    }
                    _ => {}
                }
            }
            self.stats.accepted_steps += 1;
            st.t = t_new;
            st.x = x_new;
            st.after_bp = false;
            st.hist.push((st.t, st.x.clone()));
            if st.hist.len() > 3 {
                st.hist.remove(0);
            }
            if hit || forced.is_some() {
                // verkürzter Schritt: Schrittweite nur verkleinern, wenn der Fehler es verlangt
                if growth < 1.0 {
                    st.h = st.h.min(h_eff * growth);
                }
            } else {
                st.h = (h_eff * growth).min(opts.h_max);
            }
            return Ok(hit);
        }
    }

    /// Leeres Ergebnis mit dem Anfangszustand.
    pub fn tran_result(&self, st: &TranState) -> TranResult {
        let mut r = TranResult { probe: Probe::new(&self.circuit), time: Vec::new(), x: Vec::new(), currents: Vec::new() };
        self.record(&mut r, st);
        r
    }

    /// Hängt den aktuellen Zustand an ein Ergebnis an.
    pub fn record(&self, res: &mut TranResult, st: &TranState) {
        res.time.push(st.t);
        res.currents.push(self.element_currents(&st.x, &st.react, st.t));
        res.x.push(st.x.clone());
    }

    /// Momentaufnahme des aktuellen Zustands, auswertbar wie ein Arbeitspunkt.
    pub fn snapshot(&self, st: &TranState) -> OpResult {
        let currents = self.element_currents(&st.x, &st.react, st.t);
        let term = self.terminal_currents(&st.x, &currents);
        OpResult { probe: Probe::new(&self.circuit), x: st.x.clone(), currents, term }
    }

    /// Knickstellen aller Quellen im Intervall (t0, t1].
    pub fn source_breakpoints(&self, t0: f64, t1: f64) -> Vec<f64> {
        let mut bp = Vec::new();
        for e in self.circuit.elements() {
            if let ElementKind::VSource { wave, .. } | ElementKind::ISource { wave, .. } = &e.kind {
                wave.breakpoints_between(t0, t1, &mut bp);
            }
        }
        bp.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bp
    }

    /// Simuliert den zeitlichen Verlauf von 0 bis `opts.t_stop`.
    ///
    /// Trapezregel mit Schrittweitensteuerung über den lokalen Abbruchfehler;
    /// direkt nach Knickstellen der Quellen ein Schritt Euler-rückwärts, um
    /// das typische Klingeln der Trapezregel zu vermeiden.
    pub fn transient(&mut self, opts: &TranOptions) -> Result<TranResult, SimError> {
        let x0 = self.initial_solution(opts.use_ic)?;
        let mut st = self.tran_state(x0, opts.use_ic);
        let mut result = self.tran_result(&st);
        let bps = self.breakpoints(opts.t_stop);
        st.h = opts.h_max.min(bps[0]) * 0.02;
        let mut bp_i = 0;
        while bp_i < bps.len() {
            let hit = self.tran_step(&mut st, bps[bp_i], opts, &mut |_, _, _, _| StepCheck::Accept)?;
            self.record(&mut result, &st);
            if hit {
                bp_i += 1;
                if bp_i < bps.len() {
                    let gap = bps[bp_i] - st.t;
                    self.mark_discontinuity(&mut st, gap, opts);
                }
            }
        }
        Ok(result)
    }

    /// Ströme aller Bauteile (Vorzeichenkonvention siehe [`ElementKind`]).
    fn element_currents(&self, x: &[f64], react: &[Reactive], t: f64) -> Vec<f64> {
        self.circuit
            .elements()
            .iter()
            .enumerate()
            .map(|(ei, e)| match &e.kind {
                ElementKind::Resistor { a, b, r } => (volt(x, *a) - volt(x, *b)) / r,
                ElementKind::Switch { a, b, closed, r_on, r_off } => (volt(x, *a) - volt(x, *b)) / if *closed { r_on } else { r_off },
                ElementKind::Capacitor { .. } => react.get(ei).map_or(0.0, |s| s.i),
                ElementKind::Inductor { .. } => x[self.layout.branch[ei].unwrap()],
                ElementKind::VSource { .. } => -x[self.layout.branch[ei].unwrap()],
                ElementKind::ISource { wave, .. } => wave.value(t),
                ElementKind::Diode { a, k, model } => model.eval(volt(x, *a) - volt(x, *k)).0,
                ElementKind::Bjt { c, b, e, model } => {
                    let p = model.polarity();
                    p * model.eval(p * (volt(x, *b) - volt(x, *e)), p * (volt(x, *b) - volt(x, *c))).ic
                }
                ElementKind::Mosfet { d, g, s, model } => {
                    let p = model.polarity();
                    p * model.eval(p * (volt(x, *g) - volt(x, *s)), p * (volt(x, *d) - volt(x, *s))).id
                }
                // Logikausgang, OpAmp: Strom, der am Ausgang herausfließt
                ElementKind::Driver { out, hi, lo, r } => self.drive[ei].map_or(0.0, |f| {
                    let vo = volt(x, *out);
                    (f * (volt(x, *hi) - vo) + (1.0 - f) * (volt(x, *lo) - vo)) / r
                }),
                ElementKind::OpAmp { p, n, out, vcc, vee, model } => {
                    let vt = match react.get(ei) {
                        Some(r) if r.i == OPAMP_STATE => r.v,
                        _ => model.eval(volt(x, *p) - volt(x, *n), volt(x, *vcc), volt(x, *vee)).v_target,
                    };
                    (vt - volt(x, *out)) / model.r_out
                }
            })
            .collect()
    }

    /// Ströme in jeden Anschluss des Bauteils hinein (Summe = 0).
    fn terminal_currents(&self, x: &[f64], main: &[f64]) -> Vec<Vec<f64>> {
        self.circuit
            .elements()
            .iter()
            .enumerate()
            .map(|(ei, e)| match &e.kind {
                ElementKind::Bjt { c, b, e, model } => {
                    let p = model.polarity();
                    let op = model.eval(p * (volt(x, *b) - volt(x, *e)), p * (volt(x, *b) - volt(x, *c)));
                    let (ic, ib) = (p * op.ic, p * op.ib);
                    vec![ic, ib, -(ic + ib)]
                }
                ElementKind::Mosfet { .. } => vec![main[ei], 0.0, -main[ei]],
                ElementKind::Driver { out, hi, lo, r } => match self.drive[ei] {
                    Some(f) => {
                        let vo = volt(x, *out);
                        let (ih, il) = (f * (volt(x, *hi) - vo) / r, (1.0 - f) * (volt(x, *lo) - vo) / r);
                        vec![-(ih + il), ih, il]
                    }
                    None => vec![0.0; 3],
                },
                ElementKind::OpAmp { .. } => {
                    let i = main[ei];
                    if i >= 0.0 {
                        vec![0.0, 0.0, -i, i, 0.0]
                    } else {
                        vec![0.0, 0.0, -i, 0.0, i]
                    }
                }
                // Quellen: Hauptstrom ist der abgegebene Strom (aus dem Pluspol heraus)
                ElementKind::VSource { .. } | ElementKind::ISource { .. } => vec![-main[ei], main[ei]],
                _ => vec![main[ei], -main[ei]],
            })
            .collect()
    }
}

// -------------------------------------------------------------------------
// Ergebnisse
// -------------------------------------------------------------------------

/// Namensauflösung für Ergebnisse.
#[derive(Clone, Debug)]
struct Probe {
    nodes: HashMap<String, NodeId>,
    elems: HashMap<String, usize>,
    terminals: Vec<Vec<NodeId>>,
}

impl Probe {
    fn new(c: &Circuit) -> Self {
        let mut nodes = HashMap::new();
        for i in 0..c.node_count() {
            nodes.insert(c.node_name(i).to_string(), i);
        }
        for alias in ["0", "gnd", "GND", "masse"] {
            nodes.insert(alias.into(), 0);
        }
        let elems = c.elements().iter().enumerate().map(|(i, e)| (e.name.clone(), i)).collect();
        let terminals = c.elements().iter().map(|e| e.kind.terminals()).collect();
        Self { nodes, elems, terminals }
    }

    fn node(&self, name: &str) -> Result<NodeId, SimError> {
        self.nodes.get(name).copied().ok_or_else(|| SimError::UnknownName(name.into()))
    }

    fn elem(&self, name: &str) -> Result<usize, SimError> {
        self.elems.get(name).copied().ok_or_else(|| SimError::UnknownName(name.into()))
    }
}

/// Ergebnis des DC-Arbeitspunkts.
#[derive(Clone, Debug)]
pub struct OpResult {
    probe: Probe,
    x: Vec<f64>,
    currents: Vec<f64>,
    term: Vec<Vec<f64>>,
}

impl OpResult {
    /// Spannung eines Knotens gegen Masse in V.
    pub fn voltage(&self, node: &str) -> Result<f64, SimError> {
        Ok(volt(&self.x, self.probe.node(node)?))
    }

    /// Strom durch ein Bauteil in A.
    pub fn current(&self, elem: &str) -> Result<f64, SimError> {
        Ok(self.currents[self.probe.elem(elem)?])
    }

    /// Spannung über einem Bauteil (erster minus zweiter Anschluss) in V.
    pub fn element_voltage(&self, elem: &str) -> Result<f64, SimError> {
        let t = &self.probe.terminals[self.probe.elem(elem)?];
        let (a, b) = (t[0], t[1]);
        Ok(volt(&self.x, a) - volt(&self.x, b))
    }

    /// Ströme in jeden Anschluss hinein (Transistor: [I_C, I_B, I_E] bzw. [I_D, I_G, I_S]).
    pub fn terminal_currents(&self, elem: &str) -> Result<Vec<f64>, SimError> {
        Ok(self.term[self.probe.elem(elem)?].clone())
    }

    /// Knotennummern der Anschlüsse.
    pub fn terminal_nodes(&self, elem: &str) -> Result<Vec<NodeId>, SimError> {
        Ok(self.probe.terminals[self.probe.elem(elem)?].clone())
    }

    /// Namen aller Bauteile.
    pub fn element_names(&self) -> impl Iterator<Item = &str> {
        self.probe.elems.keys().map(|s| s.as_str())
    }

    /// Spannung eines Knotens nach Nummer.
    pub fn node_voltage(&self, node: NodeId) -> f64 {
        volt(&self.x, node)
    }

    /// Spannungen an den Anschlüssen gegen Masse.
    pub fn terminal_voltages(&self, elem: &str) -> Result<Vec<f64>, SimError> {
        Ok(self.probe.terminals[self.probe.elem(elem)?].iter().map(|&n| volt(&self.x, n)).collect())
    }

    /// Leistung in W: aufgenommen bei passiven Bauteilen, abgegeben bei Quellen.
    /// Bei Transistoren: gesamte Verlustleistung Σ U_k · I_k.
    pub fn power(&self, elem: &str) -> Result<f64, SimError> {
        let e = self.probe.elem(elem)?;
        if self.probe.terminals[e].len() > 2 {
            let v = self.terminal_voltages(elem)?;
            return Ok(v.iter().zip(&self.term[e]).map(|(u, i)| u * i).sum());
        }
        Ok(self.element_voltage(elem)? * self.current(elem)?)
    }
}

/// Ergebnis der Transientenanalyse: alle angenommenen Zeitpunkte.
#[derive(Clone, Debug)]
pub struct TranResult {
    probe: Probe,
    pub time: Vec<f64>,
    x: Vec<Vec<f64>>,
    currents: Vec<Vec<f64>>,
}

impl TranResult {
    /// Spannungsverlauf eines Knotens.
    pub fn voltage(&self, node: &str) -> Result<Vec<f64>, SimError> {
        let n = self.probe.node(node)?;
        Ok(self.x.iter().map(|x| volt(x, n)).collect())
    }

    /// Stromverlauf eines Bauteils.
    pub fn current(&self, elem: &str) -> Result<Vec<f64>, SimError> {
        let e = self.probe.elem(elem)?;
        Ok(self.currents.iter().map(|c| c[e]).collect())
    }

    /// Spannung eines Knotens zum Zeitpunkt `t` (linear interpoliert).
    pub fn voltage_at(&self, node: &str, t: f64) -> Result<f64, SimError> {
        let n = self.probe.node(node)?;
        Ok(self.interp(t, |k| volt(&self.x[k], n)))
    }

    /// Strom eines Bauteils zum Zeitpunkt `t` (linear interpoliert).
    pub fn current_at(&self, elem: &str, t: f64) -> Result<f64, SimError> {
        let e = self.probe.elem(elem)?;
        Ok(self.interp(t, |k| self.currents[k][e]))
    }

    fn interp(&self, t: f64, f: impl Fn(usize) -> f64) -> f64 {
        let ts = &self.time;
        if t <= ts[0] {
            return f(0);
        }
        let k = ts.partition_point(|&v| v < t);
        if k >= ts.len() {
            return f(ts.len() - 1);
        }
        let (t0, t1) = (ts[k - 1], ts[k]);
        let w = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
        f(k - 1) * (1.0 - w) + f(k) * w
    }

    /// Anzahl gespeicherter Zeitpunkte.
    pub fn len(&self) -> usize {
        self.time.len()
    }

    pub fn is_empty(&self) -> bool {
        self.time.is_empty()
    }
}
