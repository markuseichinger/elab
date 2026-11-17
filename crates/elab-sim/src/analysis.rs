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
    circuit: &'c Circuit,
    layout: Layout,
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

impl<'c> Simulator<'c> {
    pub fn new(circuit: &'c Circuit) -> Result<Self, SimError> {
        circuit.check_connectivity()?;
        Ok(Self { circuit, layout: Layout::new(circuit), options: SimOptions::default(), stats: Stats::default() })
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
                    let (vd, lim) = pnjlim(vraw, dv[di], model.nvt(), model.vcrit());
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
            let mut conv = !limited;
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
        Ok(OpResult { probe: Probe::new(self.circuit), x, currents, term })
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

    /// Simuliert den zeitlichen Verlauf von 0 bis `opts.t_stop`.
    ///
    /// Trapezregel mit Schrittweitensteuerung über den lokalen Abbruchfehler;
    /// direkt nach Knickstellen der Quellen ein Schritt Euler-rückwärts, um
    /// das typische Klingeln der Trapezregel zu vermeiden.
    pub fn transient(&mut self, opts: &TranOptions) -> Result<TranResult, SimError> {
        let n_el = self.circuit.elements().len();
        let nv = self.layout.n_nodes - 1;
        let mut react = vec![Reactive::default(); n_el];

        // Anfangszustand
        let x0 = if opts.use_ic {
            let ctx = Ctx { t: 0.0, mode: Mode::InitIc, src_scale: 1.0, g_extra: 0.0, react: &react };
            let mut x = vec![0.0; self.layout.size];
            self.newton(&mut x, &ctx, self.options.max_iter_dc).map_err(|f| self.map_fail(f, Some(0.0)))?;
            x
        } else {
            self.solve_dc(0.0)?
        };
        for (ei, e) in self.circuit.elements().iter().enumerate() {
            match e.kind {
                ElementKind::Capacitor { a, b, ic, .. } => {
                    let v = if opts.use_ic { ic.unwrap_or(0.0) } else { volt(&x0, a) - volt(&x0, b) };
                    react[ei] = Reactive { v, i: 0.0 };
                }
                ElementKind::Inductor { a, b, .. } => {
                    let br = self.layout.branch[ei].unwrap();
                    react[ei] = Reactive { v: volt(&x0, a) - volt(&x0, b), i: x0[br] };
                    if !opts.use_ic {
                        react[ei].v = 0.0;
                    }
                }
                _ => {}
            }
        }

        let mut result = TranResult {
            probe: Probe::new(self.circuit),
            time: vec![0.0],
            x: vec![x0.clone()],
            currents: vec![self.element_currents(&x0, &react, 0.0)],
        };

        let bps = self.breakpoints(opts.t_stop);
        let mut bp_i = 0;
        let mut t = 0.0;
        let mut x = x0;
        let mut after_bp = true;
        let mut hist: Vec<(f64, Vec<f64>)> = vec![(0.0, x.clone())];
        let mut h = opts.h_max.min(bps[0]) * 0.02;

        while bp_i < bps.len() {
            let next_bp = bps[bp_i];
            let mut t_new = t + h;
            let mut hit = false;
            if t_new >= next_bp || next_bp - t_new < 0.2 * h {
                t_new = next_bp;
                hit = true;
            }
            let h_eff = t_new - t;
            let method = if after_bp { Method::BackwardEuler } else { Method::Trapezoidal };

            // Startwert: Extrapolation, falls verfügbar
            let mut x_new = if hist.len() >= 2 && !after_bp {
                let (t1, x1) = &hist[hist.len() - 2];
                let (t2, x2) = &hist[hist.len() - 1];
                let f = (t_new - t2) / (t2 - t1);
                x2.iter().zip(x1).map(|(b, a)| b + (b - a) * f).collect()
            } else {
                x.clone()
            };
            let ctx = Ctx { t: t_new, mode: Mode::Tran { h: h_eff, method }, src_scale: 1.0, g_extra: 0.0, react: &react };
            match self.newton(&mut x_new, &ctx, self.options.max_iter_tran) {
                Ok(_) => {}
                Err(NewtonFail::Singular(r)) => return Err(self.map_fail(NewtonFail::Singular(r), Some(t_new))),
                Err(NewtonFail::NoConvergence) => {
                    self.stats.rejected_steps += 1;
                    h = h_eff / 8.0;
                    if h < opts.h_min {
                        return Err(SimError::TimestepTooSmall { time: t });
                    }
                    continue;
                }
            }

            // Lokaler Abbruchfehler über Prädiktor-Korrektor-Vergleich (Milne):
            // Trapezregel hat Fehlerkonstante 1/12, quadratische Extrapolation 1,
            // daher LTE ≈ |x_korr − x_präd| / 13.
            let mut growth = 2.0;
            if method == Method::Trapezoidal && hist.len() >= 3 {
                let k = hist.len();
                let (ta, xa) = (&hist[k - 3].0, &hist[k - 3].1);
                let (tb, xb) = (&hist[k - 2].0, &hist[k - 2].1);
                let (tc, xc) = (&hist[k - 1].0, &hist[k - 1].1);
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
                    h = h_eff * (0.9 * ratio.powf(-1.0 / 3.0)).max(0.2);
                    continue;
                }
                growth = if ratio > 0.0 { (0.9 * ratio.powf(-1.0 / 3.0)).clamp(0.5, 2.0) } else { 2.0 };
            }

            // Schritt annehmen: Zustände der Energiespeicher fortschreiben
            for (ei, e) in self.circuit.elements().iter().enumerate() {
                match e.kind {
                    ElementKind::Capacitor { a, b, c, .. } => {
                        let st = react[ei];
                        let v = volt(&x_new, a) - volt(&x_new, b);
                        let i = match method {
                            Method::BackwardEuler => c / h_eff * (v - st.v),
                            Method::Trapezoidal => 2.0 * c / h_eff * (v - st.v) - st.i,
                        };
                        react[ei] = Reactive { v, i };
                    }
                    ElementKind::Inductor { a, b, .. } => {
                        let br = self.layout.branch[ei].unwrap();
                        react[ei] = Reactive { v: volt(&x_new, a) - volt(&x_new, b), i: x_new[br] };
                    }
                    _ => {}
                }
            }
            self.stats.accepted_steps += 1;
            t = t_new;
            x = x_new;
            result.time.push(t);
            result.currents.push(self.element_currents(&x, &react, t));
            result.x.push(x.clone());

            if hit {
                bp_i += 1;
                after_bp = true;
                hist.clear();
                hist.push((t, x.clone()));
                if bp_i < bps.len() {
                    let gap = bps[bp_i] - t;
                    h = opts.h_max.min(gap) * 0.02;
                }
            } else {
                after_bp = false;
                hist.push((t, x.clone()));
                if hist.len() > 3 {
                    hist.remove(0);
                }
                h = (h_eff * growth).min(opts.h_max);
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
