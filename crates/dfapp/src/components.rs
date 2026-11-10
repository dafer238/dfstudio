//! Component results computed from the cycle diagram and the solution, as a
//! component report: the streams entering and leaving
//! a component, its balances, performance indicators, sizing quantities and
//! characteristic diagrams (T-Q for heat exchangers, process paths in a
//! property diagram, the shaft-to-grid energy chain of generators and
//! motors).
//!
//! Stream states come from the state arrays of the model (`T[k]`, `P[k]`,
//! `h[k]`, `s[k]`, `m[k]`) bound to each stream of the diagram. Additional
//! properties (isentropic end states, densities, specific heats, profiles
//! along a heat exchanger) come from CoolProp (Bell et al., 2014) for streams
//! whose fluid is known. Enthalpy differences are taken within one property
//! model: when the model's enthalpy reference of a stream differs from
//! CoolProp's (e.g. a gas mixture written as a sum of components), the
//! offset is measured at the stream's own state and removed.
//!
//! Definitions (Bejan, 2016; Incropera et al., 2007; Kehlhofer et al., 2009):
//! * isentropic efficiency of an expander `(h_in - h_out)/(h_in - h_out,s)`,
//!   of a compressor or pump `(h_out,s - h_in)/(h_out - h_in)`, with
//!   `h_out,s = h(P_out, s_in)`;
//! * heat-exchanger effectiveness `Q / Q_max`, with `Q_max` the duty of the
//!   stream that would first reach the other's inlet temperature;
//! * `UA = sum dQ / LMTD_j` over `N` duty intervals of a counter-flow
//!   exchanger (exact for piecewise-constant heat capacity), `A = UA/U`;
//! * entropy generation `S_gen = sum m s (out) - sum m s (in) + Q_loss/T0`
//!   and exergy destruction `T0 S_gen` (Gouy–Stodola).

use crate::cycle::{fmt_sig, ports, CycleDiagram, CycleInputs, Kind, Link};
use dfcore::props::{backend, coolprop_fluid, Key};
use dfcore::units::Dim;
use std::collections::BTreeMap;

/// Thermodynamic state of a stream (SI; NaN when unknown).
#[derive(Clone, Debug)]
pub struct St {
    pub k: i64,
    pub fluid: Option<String>,
    pub m: f64,
    pub t: f64,
    pub p: f64,
    pub h: f64,
    pub s: f64,
    /// h_model - h_CoolProp at this state (reference offset).
    pub h_off: f64,
}

impl St {
    fn ok(&self) -> bool {
        self.fluid.is_some()
    }
    /// CoolProp outputs at (P, h_model).
    pub fn at_ph(&self, p: f64, h: f64, outs: &[Key]) -> Option<Vec<f64>> {
        let f = self.fluid.as_ref()?;
        let r = backend::state_point(f, [Key::P, Key::H], [p, h - self.h_off], outs).ok()?;
        r.iter().all(|x| x.is_finite()).then_some(r)
    }
    /// Model enthalpy at (P, T).
    pub fn h_pt(&self, p: f64, t: f64) -> Option<f64> {
        let f = self.fluid.as_ref()?;
        let r = backend::state_point(f, [Key::P, Key::T], [p, t], &[Key::H]).ok()?;
        r[0].is_finite().then_some(r[0] + self.h_off)
    }
    /// Model enthalpy at (P, s) with s on CoolProp's entropy scale of this state.
    pub fn h_ps(&self, p: f64, s: f64) -> Option<f64> {
        let f = self.fluid.as_ref()?;
        let r = backend::state_point(f, [Key::P, Key::S], [p, s], &[Key::H]).ok()?;
        r[0].is_finite().then_some(r[0] + self.h_off)
    }
    /// CoolProp entropy of this state.
    pub fn s_cp(&self) -> Option<f64> {
        self.at_ph(self.p, self.h, &[Key::S]).map(|r| r[0])
    }
    /// Saturation at pressure `p`: (h_liquid, h_vapour) on the model scale
    /// and (T_bubble, T_dew). `None` above the critical pressure.
    pub fn sat(&self, p: f64) -> Option<(f64, f64, f64, f64)> {
        let f = self.fluid.as_ref()?;
        let l = backend::state_point(f, [Key::P, Key::Q], [p, 0.0], &[Key::H, Key::T]).ok()?;
        let v = backend::state_point(f, [Key::P, Key::Q], [p, 1.0], &[Key::H, Key::T]).ok()?;
        (l.iter().chain(&v).all(|x| x.is_finite()) && v[0] > l[0]).then(|| (l[0] + self.h_off, v[0] + self.h_off, l[1], v[1]))
    }
    /// Phase of the state: quality (two-phase only) and the degree of
    /// subcooling (< 0) or superheating (> 0) in K, with a label.
    pub fn phase(&self) -> Phase {
        if !self.ok() || !(self.p > 0.0 && self.h.is_finite()) {
            return Phase::Unknown;
        }
        let t = if self.t.is_finite() { self.t } else { self.at_ph(self.p, self.h, &[Key::T]).map(|r| r[0]).unwrap_or(f64::NAN) };
        match self.sat(self.p) {
            Some((hl, hv, tb, td)) => {
                if self.h < hl {
                    Phase::Subcooled(tb - t)
                } else if self.h > hv {
                    Phase::Superheated(t - td)
                } else {
                    Phase::TwoPhase((self.h - hl) / (hv - hl))
                }
            }
            None => {
                let pc = backend::limits(self.fluid.as_ref().unwrap()).map(|l| l.2).unwrap_or(f64::NAN);
                if self.p >= pc {
                    Phase::Supercritical
                } else {
                    Phase::Unknown
                }
            }
        }
    }
    /// Density, specific volume, cp (SI) of this state.
    pub fn derived(&self, key: Key) -> f64 {
        let k = if key == Key::V { Key::D } else { key };
        match self.at_ph(self.p, self.h, &[k]) {
            Some(r) if key == Key::V => 1.0 / r[0],
            Some(r) => r[0],
            None => f64::NAN,
        }
    }
}

/// Phase of a stream state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Unknown,
    /// degree of subcooling T_bubble - T (K)
    Subcooled(f64),
    /// vapour quality
    TwoPhase(f64),
    /// degree of superheating T - T_dew (K)
    Superheated(f64),
    Supercritical,
}

impl Phase {
    /// (quality column, subcooling/superheating column)
    pub fn cells(self) -> (String, String) {
        match self {
            Phase::Unknown => ("—".into(), "—".into()),
            Phase::Subcooled(d) => ("liquid".into(), format!("{} K subcooled", fmt_sig(d))),
            Phase::TwoPhase(x) => (format!("{x:.4}"), "saturated".into()),
            Phase::Superheated(d) => ("vapour".into(), format!("{} K superheated", fmt_sig(d))),
            Phase::Supercritical => ("—".into(), "supercritical (P > P_c)".into()),
        }
    }
}

/// Fractions `f` in (0, 1) of a process from `a` to `b` (h and P linear in
/// f) at which the stream crosses a saturation line, so that profiles and
/// paths have their kinks exactly on the dome.
fn phase_fracs(a: &St, b: &St) -> Vec<f64> {
    let mut out = Vec::new();
    if !a.ok() || !(a.p > 0.0 && b.p > 0.0) || (b.h - a.h).abs() < 1e-9 {
        return out;
    }
    for side in 0..2 {
        let mut f = 0.5;
        let mut ok = false;
        for _ in 0..6 {
            let p = a.p + (b.p - a.p) * f;
            let Some((hl, hv, _, _)) = a.sat(p) else { break };
            let hs = if side == 0 { hl } else { hv };
            let nf = (hs - a.h) / (b.h - a.h);
            ok = (nf - f).abs() < 1e-10;
            f = nf;
            if ok || !(0.0..=1.0).contains(&f) {
                break;
            }
        }
        if (ok || a.p == b.p) && f > 1e-9 && f < 1.0 - 1e-9 {
            out.push(f);
        }
    }
    out
}

/// Uniform fractions 0..=1 in `n` steps merged with the phase-change
/// fractions of the given processes.
fn fractions(n: usize, procs: &[(&St, &St)]) -> Vec<f64> {
    let mut f: Vec<f64> = (0..=n).map(|j| j as f64 / n as f64).collect();
    for (a, b) in procs {
        f.extend(phase_fracs(a, b));
    }
    f.sort_by(f64::total_cmp);
    f.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
    f
}

/// Main working fluid of a script: the fluid named most often as the first
/// argument of a property call.
pub fn main_fluid(script: &str) -> Option<String> {
    let low = script.to_lowercase();
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for (f, _) in dfcore::props::PROPERTY_FUNCTIONS {
        let pat = format!("{f}(");
        let mut from = 0;
        while let Some(p) = low[from..].find(&pat) {
            let at = from + p;
            from = at + pat.len();
            if at > 0 && (low.as_bytes()[at - 1].is_ascii_alphanumeric() || low.as_bytes()[at - 1] == b'_') {
                continue;
            }
            let arg = script[from..].split([',', ')']).next().unwrap_or("").trim().trim_matches(['\'', '"', '$']).to_string();
            if !arg.is_empty() && !arg.contains('=') && arg.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                *count.entry(arg).or_default() += 1;
            }
        }
    }
    count.into_iter().max_by_key(|(_, n)| *n).map(|(f, _)| f)
}

/// Fluid of a stream and whether it was set explicitly: explicit, from its
/// colour preset, or the model's main fluid.
pub fn stream_fluid(l: &Link, default: &Option<String>) -> Option<String> {
    stream_fluid_ex(l, default).map(|x| x.0)
}

pub(crate) fn stream_fluid_ex(l: &Link, default: &Option<String>) -> Option<(String, bool)> {
    match l.fluid.trim() {
        "-" => None,
        "" => match l.color {
            Some([0, 60, 220]) | Some([230, 120, 0]) => Some(("Water".into(), false)),
            Some([0, 150, 70]) => Some(("Air".into(), false)),
            Some([220, 0, 0]) => None,
            _ => default.clone().map(|f| (f, false)),
        },
        f => Some((f.to_string(), true)),
    }
}

/// Array bases used for T, P, h, s, m.
pub struct Bases {
    pub t: Option<String>,
    pub p: Option<String>,
    pub h: Option<String>,
    pub s: Option<String>,
    pub m: Option<String>,
}

pub fn bases(inp: &CycleInputs) -> Bases {
    let m = &inp.analysis.model;
    let has = |b: &str| m.vars.iter().any(|v| v.index.len() == 1 && v.base == b);
    let pick = |c: &[&str]| c.iter().find(|b| has(b)).map(|s| s.to_string());
    Bases { t: pick(&["t", "temp"]), p: pick(&["p", "pres"]), h: pick(&["h", "enth"]), s: pick(&["s", "entr"]), m: pick(&["m", "m_dot", "mdot"]) }
}

/// Model value of a variable in SI.
pub fn var_si(inp: &CycleInputs, name: &str, dim: Dim) -> f64 {
    let a = inp.analysis;
    let (Some(v), Some(sol)) = (a.model.var(name), a.solution.as_ref()) else { return f64::NAN };
    if !sol.known[v as usize] {
        return f64::NAN;
    }
    a.model.unit_system.unit_for(dim).to_si(sol.x[v as usize])
}

/// State `k` of a stream. An automatically chosen fluid is used only when
/// CoolProp reproduces the model's T(P, h) of the state; an explicitly set
/// fluid is also accepted with an enthalpy-reference offset.
pub fn state(inp: &CycleInputs, b: &Bases, k: i64, fluid: Option<(String, bool)>) -> St {
    let get = |base: &Option<String>, dim: Dim| base.as_ref().map(|bb| var_si(inp, &format!("{bb}[{k}]"), dim)).unwrap_or(f64::NAN);
    let mut st = St {
        k,
        fluid: None,
        m: get(&b.m, Dim::MASS_FLOW),
        t: get(&b.t, Dim::TEMP),
        p: get(&b.p, Dim::PRESSURE),
        h: get(&b.h, Dim::SPEC_ENERGY),
        s: get(&b.s, Dim::SPEC_ENTROPY),
        h_off: 0.0,
    };
    // the fluid is usable when CoolProp reproduces the state; measure the
    // enthalpy reference offset at (P, T)
    if let Some((f, explicit)) = fluid.map(|(f, e)| (coolprop_fluid(&f), e)) {
        if st.p.is_finite() && st.t.is_finite() {
            let t_ph = if st.h.is_finite() { backend::state_point(&f, [Key::P, Key::H], [st.p, st.h], &[Key::T]).ok().map(|r| r[0]) } else { None };
            match t_ph {
                Some(t) if (t - st.t).abs() < 0.5 => st.fluid = Some(f),
                _ if !explicit && st.h.is_finite() => {}
                _ => {
                    if let Ok(r) = backend::state_point(&f, [Key::P, Key::T], [st.p, st.t], &[Key::H]) {
                        if r[0].is_finite() {
                            st.h_off = if st.h.is_finite() { st.h - r[0] } else { 0.0 };
                            if !st.h.is_finite() {
                                st.h = r[0];
                            }
                            st.fluid = Some(f);
                        }
                    }
                }
            }
        } else if st.p.is_finite() && st.h.is_finite() {
            if let Ok(r) = backend::state_point(&f, [Key::P, Key::H], [st.p, st.h], &[Key::T]) {
                if r[0].is_finite() {
                    st.t = r[0];
                    st.fluid = Some(f);
                }
            }
        }
    }
    st
}

// ------------------------------------------------------------------ report

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub label: String,
    pub value: String,
    pub unit: String,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Row>,
    /// Why the section has no rows (shown instead of the table).
    pub missing: String,
}

impl Section {
    fn new(title: &str, rows: Vec<Row>) -> Self {
        Section { title: title.into(), rows, missing: String::new() }
    }
    /// Placeholder of a section that cannot be computed yet.
    fn missing(title: &str, why: &str) -> Self {
        Section { title: title.into(), rows: Vec::new(), missing: why.into() }
    }
}

#[derive(Clone, Debug)]
pub struct ChartLine {
    pub name: String,
    pub color: [u8; 3],
    pub pts: Vec<[f64; 2]>,
    pub dashed: bool,
    pub markers: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Chart {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub lines: Vec<ChartLine>,
    /// Bar chart (label, value, colour) instead of lines.
    pub bars: Vec<(String, f64, [u8; 3])>,
    pub log_y: bool,
    /// Full-height vertical markers (x, y of the annotation, annotation,
    /// colour), e.g. the pinch.
    pub vlines: Vec<(f64, f64, String, [u8; 3])>,
    /// Stable identity of the chart across components (the title may name
    /// the fluid); empty: the title.
    pub key: &'static str,
    /// Why the chart cannot be drawn (shown instead of the plot).
    pub missing: String,
}

#[derive(Clone, Debug)]
pub struct StreamRow {
    pub port: String,
    pub inlet: bool,
    pub st: St,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub title: String,
    pub streams: Vec<StreamRow>,
    pub sections: Vec<Section>,
    pub charts: Vec<Chart>,
    pub notes: Vec<String>,
    /// Physically suspicious results (temperature cross, internal pinch).
    pub warnings: Vec<String>,
}

/// Property diagram used for process paths.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum PathDiagram {
    #[default]
    Ts,
    Hs,
    Ph,
    Th,
    Pv,
}

impl PathDiagram {
    pub const ALL: [PathDiagram; 5] = [PathDiagram::Ts, PathDiagram::Hs, PathDiagram::Ph, PathDiagram::Th, PathDiagram::Pv];
    pub fn label(self) -> &'static str {
        match self {
            PathDiagram::Ts => "T-s",
            PathDiagram::Hs => "h-s (Mollier)",
            PathDiagram::Ph => "P-h",
            PathDiagram::Th => "T-h",
            PathDiagram::Pv => "P-v",
        }
    }
    /// (x, y, log y)
    fn axes(self) -> (Key, Key, bool) {
        match self {
            PathDiagram::Ts => (Key::S, Key::T, false),
            PathDiagram::Hs => (Key::S, Key::H, false),
            PathDiagram::Ph => (Key::H, Key::P, true),
            PathDiagram::Th => (Key::H, Key::T, false),
            PathDiagram::Pv => (Key::V, Key::P, true),
        }
    }
}

struct Fmt<'a> {
    inp: &'a CycleInputs<'a>,
}

impl Fmt<'_> {
    fn unit(&self, dim: Dim) -> dfcore::units::Unit {
        self.inp.analysis.model.unit_system.unit_for(dim)
    }
    /// SI value -> (text, unit) in the model's unit system.
    fn q(&self, dim: Dim, si: f64) -> (String, String) {
        let u = self.unit(dim);
        (fmt_sig(u.from_si(si)), if u.symbol == "-" { String::new() } else { u.symbol })
    }
    fn row(&self, label: &str, dim: Dim, si: f64, note: &str) -> Row {
        let (v, u) = self.q(dim, si);
        Row { label: label.into(), value: v, unit: u, note: note.into() }
    }
    /// Temperature difference (K).
    fn dt(&self, label: &str, v: f64, note: &str) -> Row {
        Row { label: label.into(), value: fmt_sig(v), unit: "K".into(), note: note.into() }
    }
    fn pct(&self, label: &str, v: f64, note: &str) -> Row {
        Row { label: label.into(), value: fmt_sig(100.0 * v), unit: "%".into(), note: note.into() }
    }
    fn num(&self, label: &str, v: f64, unit: &str, note: &str) -> Row {
        Row { label: label.into(), value: fmt_sig(v), unit: unit.into(), note: note.into() }
    }
    fn key_unit(&self, k: Key) -> dfcore::units::Unit {
        self.unit(k.dim())
    }
    fn key_name(k: Key) -> &'static str {
        match k {
            Key::T => "T",
            Key::P => "P",
            Key::H => "h",
            Key::S => "s",
            Key::V => "v",
            _ => "?",
        }
    }
    fn axis(&self, k: Key) -> String {
        let u = self.key_unit(k);
        format!("{} [{}]", Self::key_name(k), u.symbol)
    }
}

/// Resolve a component parameter: a number (SI), a variable name, or a
/// product of them (`eta_mec_t*eta_mec_gear`).
pub fn param(inp: &CycleInputs, params: &BTreeMap<String, String>, key: &str, dim: Dim) -> Option<f64> {
    let s = params.get(key)?.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains('*') {
        let mut one = BTreeMap::new();
        let mut x = 1.0;
        for f in s.split('*') {
            one.insert(String::new(), f.trim().to_string());
            x *= param(inp, &one, "", dim)?;
        }
        return Some(x);
    }
    if let Ok(x) = s.parse::<f64>() {
        return Some(x);
    }
    let a = inp.analysis;
    let v = a.model.var(s)?;
    let d = a.model.var_dim(v).unwrap_or(dim);
    let x = var_si(inp, s, if d.is_none() { dim } else { d });
    x.is_finite().then_some(x)
}

/// Machines on the shaft of a generator or motor (explicit list, or the
/// nearest machines of the right kind when the list is empty).
pub fn shaft_machines(d: &CycleDiagram, gen: u32) -> (Vec<u32>, bool) {
    let Some(g) = d.comps.iter().find(|c| c.id == gen) else { return (Vec::new(), false) };
    if !g.shaft.is_empty() {
        return (g.shaft.iter().copied().filter(|id| d.comps.iter().any(|c| c.id == *id)).collect(), false);
    }
    let explicit: Vec<u32> = d.comps.iter().filter(|c| matches!(c.kind.base(), Kind::Generator | Kind::Motor)).flat_map(|c| c.shaft.iter().copied()).collect();
    let dist = |c: &crate::cycle::Comp| (c.pos[0] - g.pos[0]).powi(2) + (c.pos[1] - g.pos[1]).powi(2);
    let explicit = &explicit;
    let free = |k: Kind| d.comps.iter().filter(move |c| c.kind.base() == k && !explicit.contains(&c.id));
    let n_gen = d.comps.iter().filter(|c| c.kind.base() == Kind::Generator).count();
    let ids = match g.kind {
        Kind::Generator if n_gen == 1 => free(Kind::Turbine).map(|c| c.id).collect(),
        Kind::Generator => free(Kind::Turbine).min_by(|a, b| dist(a).total_cmp(&dist(b))).map(|c| vec![c.id]).unwrap_or_default(),
        _ => free(Kind::Compressor).chain(free(Kind::Pump)).min_by(|a, b| dist(a).total_cmp(&dist(b))).map(|c| vec![c.id]).unwrap_or_default(),
    };
    (ids, true)
}

/// Streams at the ports of a component (port index -> state).
fn port_states(d: &CycleDiagram, inp: &CycleInputs, b: &Bases, id: u32, kind: Kind) -> Vec<Option<St>> {
    let default = main_fluid(inp.script);
    ports(kind)
        .iter()
        .enumerate()
        .map(|(k, _)| {
            let l = d.links.iter().find(|l| l.from == (id, k) || l.to == (id, k))?;
            Some(state(inp, b, l.state?, stream_fluid_ex(l, &default)))
        })
        .collect()
}

/// Mechanical power of a turbomachine (positive: produced by a turbine,
/// absorbed by a compressor or pump), SI.
pub fn machine_power(d: &CycleDiagram, inp: &CycleInputs, id: u32) -> Option<(Kind, String, f64)> {
    let c = d.comps.iter().find(|c| c.id == id)?;
    let b = bases(inp);
    let ps = port_states(d, inp, &b, id, c.kind);
    let (i, o) = (ps.first()?.as_ref()?, ps.get(1)?.as_ref()?);
    let m = if i.m.is_finite() { i.m } else { o.m };
    let w = match c.kind.base() {
        Kind::Turbine => m * (i.h - o.h),
        Kind::Compressor | Kind::Pump => m * (o.h - i.h),
        _ => return None,
    };
    w.is_finite().then(|| (c.kind.base(), c.label.clone(), w))
}

/// Duty intervals of the T-Q profile (phase-change points are added).
const TQ_STEPS: usize = 400;

const HOT_C: [u8; 3] = [213, 94, 0];
const COLD_C: [u8; 3] = [0, 114, 178];
const GREY: [u8; 3] = [130, 130, 130];
const GREEN: [u8; 3] = [0, 158, 115];

/// How the intermediate states of a process are constructed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Proc {
    /// heat exchange, pipes: P and h linear along the process, with the
    /// saturation crossings inserted exactly
    Linear,
    /// throttling: h linear, P logarithmic
    Throttle,
    /// compression or expansion at constant polytropic (small-stage)
    /// efficiency; `true` spaces the pressure logarithmically
    Polytropic(bool),
}

/// (P, h) points of a compression or expansion from `a` to `b` at constant
/// small-stage efficiency: each of `n` pressure steps follows
/// `dh = dh_s / eta_p` (compression) or `dh = eta_p dh_s` (expansion), with
/// `dh_s` the isentropic enthalpy change of the step; `eta_p` is found so
/// that the path ends at `b`. Entropy then changes monotonically (it rises
/// whenever the isentropic efficiency is below one). Dixon & Hall (2014),
/// ch. 2.
fn polytropic(a: &St, b: &St, n: usize, log_p: bool) -> Option<Vec<(f64, f64)>> {
    if !a.ok() || !(a.p > 0.0 && b.p > 0.0) || (b.p - a.p).abs() < 1e-9 * a.p {
        return None;
    }
    let grid = |n: usize| -> Vec<f64> {
        (0..=n)
            .map(|j| {
                let f = j as f64 / n as f64;
                if log_p {
                    (a.p.ln() + (b.p.ln() - a.p.ln()) * f).exp()
                } else {
                    a.p + (b.p - a.p) * f
                }
            })
            .collect()
    };
    // h along the pressure grid `ps` for a step factor k (1/eta_p or eta_p)
    let march = |ps: &[f64], k: f64| -> Option<Vec<f64>> {
        let mut hs = Vec::with_capacity(ps.len());
        let mut h = a.h;
        hs.push(h);
        for j in 1..ps.len() {
            let s = a.at_ph(ps[j - 1], h, &[Key::S])?[0];
            let h_s = a.h_ps(ps[j], s)?;
            h += k * (h_s - h);
            hs.push(h);
        }
        Some(hs)
    };
    let h_s_end = a.h_ps(b.p, a.s_cp()?)?;
    let dh = b.h - a.h;
    if dh.abs() < 1e-9 || (h_s_end - a.h).abs() < 1e-9 {
        return None;
    }
    // secant on k from the isentropic ratio: first on a coarse grid, then
    // refined on the final grid (k depends only weakly on the grid)
    let solve = |ps: &[f64], mut k0: f64, iters: usize| -> Option<(f64, Vec<f64>)> {
        let mut v0 = march(ps, k0)?;
        let mut g0 = v0[ps.len() - 1] - b.h;
        let mut k1 = k0 * (1.0 + 1e-3);
        for _ in 0..iters {
            if g0.abs() < 1e-8 * dh.abs() {
                break;
            }
            let v1 = march(ps, k1)?;
            let g1 = v1[ps.len() - 1] - b.h;
            let k2 = k1 - g1 * (k1 - k0) / (g1 - g0);
            (k0, g0, v0) = (k1, g1, v1);
            if !(k2 > 0.0 && k2.is_finite()) {
                break;
            }
            k1 = k2;
        }
        (g0.abs() < 1e-3 * dh.abs()).then_some((k0, v0))
    };
    let k = dh / (h_s_end - a.h);
    if k <= 0.0 {
        return None;
    }
    let (k, _) = solve(&grid(n.min(16)), k, 12)?;
    // one march on the final grid; the remaining end mismatch (from the grid
    // dependence of k, O(1/n^2)) is distributed linearly in the step index
    let ps = grid(n);
    let mut v = march(&ps, k)?;
    if (b.h - v[n]).abs() > 1e-3 * dh.abs() {
        v = solve(&ps, k, 8)?.1;
    }
    let e = b.h - v[n];
    for (j, h) in v.iter_mut().enumerate() {
        *h += e * j as f64 / n as f64;
    }
    v[n] = b.h;
    Some(ps.into_iter().zip(v).collect())
}

/// Points of a process from `a` to `b` in a property diagram, states from
/// CoolProp at (P, h).
pub(crate) fn path(a: &St, b: &St, x: Key, y: Key, proc_: Proc, n: usize) -> Vec<[f64; 2]> {
    if !a.ok() || !(a.p > 0.0 && b.p > 0.0 && a.h.is_finite() && b.h.is_finite()) {
        return Vec::new();
    }
    let ph: Vec<(f64, f64)> = match proc_ {
        Proc::Polytropic(log_p) => match polytropic(a, b, n, log_p) {
            Some(v) => v,
            None => return path(a, b, x, y, Proc::Throttle, n),
        },
        Proc::Throttle => (0..=n)
            .map(|j| {
                let f = j as f64 / n as f64;
                ((a.p.ln() + (b.p.ln() - a.p.ln()) * f).exp(), a.h + (b.h - a.h) * f)
            })
            .collect(),
        Proc::Linear => fractions(n, &[(a, b)]).into_iter().map(|f| (a.p + (b.p - a.p) * f, a.h + (b.h - a.h) * f)).collect(),
    };
    let ks = [if x == Key::V { Key::D } else { x }, if y == Key::V { Key::D } else { y }];
    let fx = |k: Key, v: f64| {
        if k == Key::V {
            1.0 / v
        } else if k == Key::H {
            v + a.h_off
        } else {
            v
        }
    };
    ph.into_iter()
        .filter_map(|(p, h)| {
            let r = a.at_ph(p, h, &ks)?;
            Some([fx(x, r[0]), fx(y, r[1])])
        })
        .collect()
}

/// Kind of process between two consecutive states, from the states alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProcessKind {
    /// heat transfer (heating, cooling, with or without pressure loss)
    Heat,
    /// adiabatic compression; isentropic efficiency
    Compression(f64),
    /// adiabatic expansion; isentropic efficiency
    Expansion(f64),
    /// throttling (h nearly constant, pressure drop)
    Throttle,
}

impl ProcessKind {
    pub(crate) fn proc_(self) -> Proc {
        match self {
            ProcessKind::Heat => Proc::Linear,
            ProcessKind::Compression(_) | ProcessKind::Expansion(_) => Proc::Polytropic(true),
            ProcessKind::Throttle => Proc::Throttle,
        }
    }
    pub(crate) fn label(self, dh: f64) -> String {
        match self {
            ProcessKind::Heat if dh >= 0.0 => "heating".into(),
            ProcessKind::Heat => "cooling".into(),
            ProcessKind::Compression(e) => format!("compression, eta_s = {e:.3}"),
            ProcessKind::Expansion(e) => format!("expansion, eta_s = {e:.3}"),
            ProcessKind::Throttle => "throttling".into(),
        }
    }
}

/// Classify the process from `a` to `b` by the ratio of the actual to the
/// isentropic enthalpy change between the two pressures,
/// r = (h_b − h_a) / (h_s(P_b, s_a) − h_a):
/// * pressure rise with 1 ≤ r ≤ 5: adiabatic compression, η_s = 1/r;
/// * pressure drop with 0.05 ≤ r ≤ 1: adiabatic expansion, η_s = r;
/// * pressure drop with |r| < 0.05: throttling (h ≈ const);
/// * anything else (equal pressures, heat added or removed beyond the
///   work-only range): heat transfer, with P varying linearly with h
///   between the inlet and outlet isobars.
///
/// An adiabatic process with η_s = 1 is isentropic, and irreversibility
/// can only raise the entropy (second law), which bounds r on each side.
pub(crate) fn classify(a: &St, b: &St) -> ProcessKind {
    if (b.p - a.p).abs() <= 1e-6 * a.p.abs().max(b.p.abs()) {
        return ProcessKind::Heat;
    }
    let Some(hs) = a.s_cp().and_then(|s| a.h_ps(b.p, s)) else { return ProcessKind::Heat };
    let dhs = hs - a.h;
    if dhs.abs() < 1e-12 {
        return ProcessKind::Heat;
    }
    let r = (b.h - a.h) / dhs;
    if b.p > a.p && (0.999..=5.0).contains(&r) {
        ProcessKind::Compression((1.0 / r).min(1.0))
    } else if b.p < a.p && (0.05..=1.001).contains(&r) {
        ProcessKind::Expansion(r.min(1.0))
    } else if b.p < a.p && r.abs() < 0.05 {
        ProcessKind::Throttle
    } else {
        ProcessKind::Heat
    }
}

/// Isentropic line from state `a` down/up to pressure `p_out`.
fn isentrope(a: &St, p_out: f64, x: Key, y: Key, n: usize) -> Vec<[f64; 2]> {
    let (Some(f), Some(s)) = (a.fluid.as_ref(), a.s_cp()) else { return Vec::new() };
    (0..=n)
        .filter_map(|j| {
            let p = (a.p.ln() + (p_out.ln() - a.p.ln()) * j as f64 / n as f64).exp();
            let ks = [if x == Key::V { Key::D } else { x }, if y == Key::V { Key::D } else { y }];
            let r = backend::state_point(f, [Key::P, Key::S], [p, s], &ks).ok()?;
            let fx = |k: Key, v: f64| {
                if k == Key::V {
                    1.0 / v
                } else if k == Key::H {
                    v + a.h_off
                } else {
                    v
                }
            };
            r.iter().all(|v| v.is_finite()).then(|| [fx(x, r[0]), fx(y, r[1])])
        })
        .collect()
}

/// Process diagram of a component: saturation dome of the first fluid and
/// the paths of its streams (plus isentropic references for turbomachines).
fn process_chart(fm: &Fmt, diag: PathDiagram, pairs: &[(&str, &St, &St, Proc, [u8; 3])], isentropic: Option<(&St, f64)>) -> Option<Chart> {
    let (x, y, log_y) = diag.axes();
    let fluid = pairs.iter().find_map(|p| p.1.fluid.clone())?;
    let (xu, yu) = (fm.key_unit(x), fm.key_unit(y));
    let tr = |c: Vec<[f64; 2]>| -> Vec<[f64; 2]> { c.into_iter().map(|p| [xu.from_si(p[0]), yu.from_si(p[1])]).collect() };
    let mut ch = Chart { title: format!("{} diagram — {}", diag.label(), fluid), x_label: fm.axis(x), y_label: fm.axis(y), log_y, ..Default::default() };
    // dome of the main fluid (entropy and enthalpy on CoolProp's scale; the
    // offset of the first stream shifts enthalpies to the model scale)
    let off = pairs.iter().find(|p| p.1.fluid.as_deref() == Some(fluid.as_str())).map(|p| p.1.h_off).unwrap_or(0.0);
    if let Ok(dm) = dfcore::diagrams::dome(&fluid, if x == Key::V { Key::V } else { x }, y, 240) {
        let fix = |c: Vec<[f64; 2]>| c.into_iter().map(|p| [if x == Key::H { p[0] + off } else { p[0] }, if y == Key::H { p[1] + off } else { p[1] }]).collect::<Vec<_>>();
        ch.lines.push(ChartLine { name: "saturated liquid".into(), color: GREY, pts: tr(fix(dm.liquid)), dashed: false, markers: false });
        ch.lines.push(ChartLine { name: "saturated vapour".into(), color: GREY, pts: tr(fix(dm.vapor)), dashed: false, markers: false });
    }
    let mut any = false;
    for (name, a, b, proc_, col) in pairs {
        let pts = path(a, b, x, y, *proc_, if matches!(proc_, Proc::Polytropic(_)) { 60 } else { 120 });
        if pts.len() >= 2 {
            any = true;
            let ends = vec![pts[0], *pts.last().unwrap()];
            ch.lines.push(ChartLine { name: format!("{name} ({} -> {})", a.k, b.k), color: *col, pts: tr(pts), dashed: false, markers: false });
            ch.lines.push(ChartLine { name: String::new(), color: *col, pts: tr(ends), dashed: false, markers: true });
        }
    }
    if let Some((a, p_out)) = isentropic {
        let pts = isentrope(a, p_out, x, y, 60);
        if pts.len() >= 2 {
            ch.lines.push(ChartLine { name: "isentropic".into(), color: GREY, pts: tr(pts), dashed: true, markers: false });
        }
    }
    any.then_some(ch)
}

/// T-Q profile of a counter-flow exchanger in `n` equal duty intervals plus
/// the points where either stream crosses a saturation line:
/// (Q_j, T_hot, T_cold) with Q from the cold inlet end, SI.
pub fn tq_profile(hi: &St, ho: &St, ci: &St, co: &St, n: usize) -> Option<Vec<(f64, f64, f64)>> {
    let qh = hi.m * (hi.h - ho.h);
    let qc = ci.m * (co.h - ci.h);
    if !(qh.is_finite() && qc.is_finite()) || qh <= 0.0 || qc <= 0.0 {
        return None;
    }
    // temperature at a fraction f of a stream's enthalpy change (linear in h
    // when the fluid is unknown)
    let temp = |a: &St, b: &St, f: f64| -> f64 {
        let h = a.h + (b.h - a.h) * f;
        let p = a.p + (b.p - a.p) * f;
        match a.at_ph(p, h, &[Key::T]) {
            Some(r) => r[0],
            None => a.t + (b.t - a.t) * f,
        }
    };
    let q = 0.5 * (qh + qc);
    Some(
        fractions(n, &[(ho, hi), (ci, co)])
            .into_iter()
            .map(|f| {
                // at the cold inlet end the hot stream leaves
                (q * f, temp(ho, hi, f), temp(ci, co, f))
            })
            .collect(),
    )
}

/// UA and minimum approach from a T-Q profile.
pub fn ua_pinch(pr: &[(f64, f64, f64)]) -> (f64, f64) {
    let mut ua = 0.0;
    let mut pinch = f64::INFINITY;
    for w in pr.windows(2) {
        let (q0, h0, c0) = w[0];
        let (q1, h1, c1) = w[1];
        let (d0, d1) = (h0 - c0, h1 - c1);
        pinch = pinch.min(d0).min(d1);
        let lm = if (d0 - d1).abs() < 1e-9 * d0.abs().max(1e-12) { d0 } else { (d0 - d1) / (d0 / d1).ln() };
        ua += (q1 - q0) / lm;
    }
    (if pinch > 0.0 { ua } else { f64::NAN }, pinch)
}

const TQ_TITLE: &str = "T-Q diagram (counter-flow)";

/// Compute the report of component `id`.
pub fn report(d: &CycleDiagram, inp: &CycleInputs, id: u32, diag: PathDiagram) -> Report {
    let fm = Fmt { inp };
    let Some(c) = d.comps.iter().find(|c| c.id == id) else { return Report::default() };
    let b = bases(inp);
    let pl = ports(c.kind);
    let ps = port_states(d, inp, &b, id, c.kind);
    let mut r = Report { title: format!("{} — {}", c.label, c.kind.name()), ..Default::default() };
    if inp.analysis.solution.is_none() {
        r.notes.push("Solve the model to see results.".into());
    }
    for (k, s) in ps.iter().enumerate() {
        if let Some(s) = s {
            r.streams.push(StreamRow { port: pl[k].name.into(), inlet: pl[k].inlet, st: s.clone() });
        }
    }
    let t0 = d.t0;
    let solved = inp.analysis.solution.is_some();
    // reason shown in a tab that cannot be filled (an unsolved model has no states)
    let need = |why: &str| -> String {
        if solved {
            why.into()
        } else {
            "Solve the model to see this.".into()
        }
    };
    const CONNECT2: &str = "Connect the inlet and outlet streams (with state numbers).";
    const CONNECT4: &str = "Connect the four streams (hot in/out, cold in/out) with state numbers.";
    const NEED_FLUID: &str = "Needs the fluid of the streams (Stream > Fluid) to evaluate properties.";
    // the process path on the selected property diagram, or why it is missing
    let proc_chart = |r: &mut Report, ch: Option<Chart>, why: &str| {
        r.charts.push(match ch {
            Some(ch) => Chart { key: "process", ..ch },
            None => Chart { title: format!("{} diagram", diag.label()), key: "process", missing: need(why), ..Default::default() },
        })
    };
    let p_ = |k: usize| ps.get(k).and_then(|s| s.as_ref());
    let mflow = |a: &St, b: &St| if a.m.is_finite() { a.m } else { b.m };
    // entropy generation of the component (adiabatic unless q_loss given)
    let sgen = |q_loss: f64| -> f64 {
        let mut s = 0.0;
        for (k, st) in ps.iter().enumerate() {
            let Some(st) = st else { return f64::NAN };
            let sign = if pl[k].inlet { -1.0 } else { 1.0 };
            s += sign * st.m * st.s;
        }
        s + if q_loss.is_finite() { q_loss / t0 } else { 0.0 }
    };
    let second_law = |r: &mut Report, q_loss: f64| {
        let sg = sgen(q_loss);
        if sg.is_finite() {
            let pw = Dim::POWER;
            r.sections.push(Section::new(
                "Second law",
                vec![
                    fm.num("Entropy generation S_gen", sg / 1000.0, "kW/K", "sum m s (out - in) + Q_loss/T0"),
                    fm.row("Exergy destruction T0 S_gen", pw, t0 * sg, &format!("T0 = {:.2} K (Gouy–Stodola)", t0)),
                ],
            ));
        } else {
            r.sections.push(Section::missing("Second law", &need("Needs the entropy of every port: connect all streams with state numbers and set their fluid.")));
        }
    };
    let pw = Dim::POWER;
    let pr = Dim::PRESSURE;
    match c.kind.base() {
        Kind::Turbine | Kind::Compressor | Kind::Pump => {
            let shaft: Vec<String> = d
                .comps
                .iter()
                .filter(|g| matches!(g.kind, Kind::Generator | Kind::Motor))
                .filter(|g| shaft_machines(d, g.id).0.contains(&id))
                .map(|g| format!("{} ({})", g.label, g.kind.name()))
                .collect();
            let shaft = Section::new(
                "Shaft",
                vec![Row {
                    label: "Connected to".into(),
                    value: if shaft.is_empty() { "—".into() } else { shaft.join(", ") },
                    unit: String::new(),
                    note: "set in the generator / motor".into(),
                }],
            );
            let (Some(i), Some(o)) = (p_(0), p_(1)) else {
                r.notes.push(CONNECT2.into());
                for t in ["Performance", "Sizing"] {
                    r.sections.push(Section::missing(t, &need(CONNECT2)));
                }
                r.sections.push(shaft);
                second_law(&mut r, f64::NAN);
                proc_chart(&mut r, None, CONNECT2);
                return r;
            };
            let m = mflow(i, o);
            let turb = c.kind.base() == Kind::Turbine;
            let w = if turb { m * (i.h - o.h) } else { m * (o.h - i.h) };
            let hs = i.s_cp().and_then(|s| i.h_ps(o.p, s));
            let eta = hs.map(|hs| if turb { (i.h - o.h) / (i.h - hs) } else { (hs - i.h) / (o.h - i.h) }).unwrap_or(f64::NAN);
            let mut perf = vec![
                fm.row(if turb { "Power from the fluid" } else { "Power to the fluid" }, pw, w, "m |h_out - h_in| (before mechanical losses)"),
                fm.row("Mass flow", Dim::MASS_FLOW, m, ""),
                fm.num(if turb { "Pressure ratio P_in/P_out" } else { "Pressure ratio P_out/P_in" }, if turb { i.p / o.p } else { o.p / i.p }, "-", ""),
                fm.dt("Temperature change", o.t - i.t, ""),
                fm.row("Specific work", Dim::SPEC_ENERGY, (i.h - o.h).abs(), ""),
            ];
            if eta.is_finite() && eta > 1.0 + 1e-6 {
                r.warnings.push(format!(
                    "Isentropic efficiency {} % > 100 %: the outlet entropy is below the inlet entropy, which an adiabatic machine cannot achieve.",
                    fmt_sig(100.0 * eta)
                ));
            }
            if eta.is_finite() {
                perf.push(fm.pct("Isentropic efficiency", eta, "h_out,s = h(P_out, s_in), CoolProp"));
                perf.push(fm.row("Isentropic outlet enthalpy h_out,s", Dim::SPEC_ENERGY, hs.unwrap(), ""));
                let ts = i.s_cp().and_then(|s| backend::state_point(i.fluid.as_ref().unwrap(), [Key::P, Key::S], [o.p, s], &[Key::T]).ok()).map(|r| r[0]);
                if let Some(ts) = ts {
                    perf.push(fm.row("Isentropic outlet temperature", Dim::TEMP, ts, ""));
                }
                perf.push(fm.row("Isentropic power", pw, m * (i.h - hs.unwrap()).abs(), ""));
            } else {
                r.notes.push("Isentropic efficiency needs the stream fluid (Stream > Fluid) and P, h arrays.".into());
            }
            r.sections.push(Section::new("Performance", perf));
            // sizing
            let (vi, vo) = (i.derived(Key::V), o.derived(Key::V));
            let mut size = Vec::new();
            if vi.is_finite() {
                size.push(fm.num("Inlet volume flow", m * vi, "m3/s", ""));
                size.push(fm.num("Outlet volume flow", m * vo, "m3/s", ""));
                if c.kind.base() == Kind::Pump {
                    let dp = o.p - i.p;
                    size.push(fm.row("Hydraulic power V dP", pw, m * vi * dp, ""));
                    size.push(fm.num("Head", dp * vi / 9.80665, "m", "dP/(rho g)"));
                    if w > 0.0 {
                        size.push(fm.pct("Hydraulic efficiency V dP / W", m * vi * dp / w, ""));
                    }
                }
            }
            r.sections.push(if size.is_empty() {
                Section::missing("Sizing", &need("Needs the specific volume of the streams: set their fluid (Stream > Fluid)."))
            } else {
                Section::new("Sizing", size)
            });
            r.sections.push(shaft);
            second_law(&mut r, f64::NAN);
            let ch = process_chart(
                &fm,
                diag,
                &[(if turb { "expansion" } else { "compression" }, i, o, Proc::Polytropic(c.kind.base() != Kind::Pump), if turb { HOT_C } else { COLD_C })],
                Some((i, o.p)),
            );
            proc_chart(&mut r, ch, NEED_FLUID);
        }
        Kind::HeatExchanger => {
            let (Some(hi), Some(ho), Some(ci), Some(co)) = (p_(0), p_(1), p_(2), p_(3)) else {
                r.notes.push(CONNECT4.into());
                for t in ["Heat transfer", "Sizing", "Pressure drops"] {
                    r.sections.push(Section::missing(t, &need(CONNECT4)));
                }
                second_law(&mut r, f64::NAN);
                r.charts.push(Chart { title: TQ_TITLE.into(), missing: need(CONNECT4), ..Default::default() });
                proc_chart(&mut r, None, CONNECT4);
                return r;
            };
            let (mh, mc) = (mflow(hi, ho), mflow(ci, co));
            let qh = mh * (hi.h - ho.h);
            let qc = mc * (co.h - ci.h);
            let loss = if (qh - qc).abs() < 1e-9 * qh.abs() { 0.0 } else { qh - qc };
            let mut rows = vec![
                fm.row("Duty, hot side", pw, qh, "m_h (h_hi - h_ho)"),
                fm.row("Duty, cold side", pw, qc, "m_c (h_co - h_ci)"),
                fm.row("Heat loss", pw, loss, "hot - cold"),
                fm.pct("Heat loss / hot duty", loss / qh, ""),
                fm.pct("Thermal efficiency Q_c/Q_h", qc / qh, ""),
            ];
            // effectiveness: Q_max of the stream that would reach the other's inlet temperature
            let cp_lin = |a: &St, b: &St| (a.h - b.h) / (a.t - b.t);
            let h_at = |s: &St, other: &St, p: f64, t: f64| s.h_pt(p, t).unwrap_or(s.h + cp_lin(s, other) * (t - s.t));
            let qmax_h = mh * (hi.h - h_at(hi, ho, ho.p, ci.t));
            let qmax_c = mc * (h_at(ci, co, co.p, hi.t) - ci.h);
            let qmax = qmax_h.min(qmax_c);
            rows.push(fm.pct("Effectiveness Q_c/Q_max", qc / qmax, "Q_max = min(m_h(h_hi - h(T_ci)), m_c(h(T_hi) - h_ci))"));
            let d1 = hi.t - co.t;
            let d2 = ho.t - ci.t;
            rows.push(fm.dt("Hot-end approach T_hi - T_co", d1, ""));
            rows.push(fm.dt("Cold-end approach T_ho - T_ci", d2, ""));
            let lmtd = if (d1 - d2).abs() < 1e-9 { d1 } else { (d1 - d2) / (d1 / d2).ln() };
            rows.push(fm.dt("LMTD (counter-flow, end temperatures)", lmtd, ""));
            r.sections.push(Section::new("Heat transfer", rows));
            let mut size = Vec::new();
            if let Some(pr_) = tq_profile(hi, ho, ci, co, TQ_STEPS) {
                let (ua, pinch) = ua_pinch(&pr_);
                let jmin = (0..pr_.len()).min_by(|a, b| (pr_[*a].1 - pr_[*a].2).total_cmp(&(pr_[*b].1 - pr_[*b].2))).unwrap_or(0);
                let where_ = 100.0 * pr_[jmin].0 / pr_[pr_.len() - 1].0;
                size.push(fm.dt(
                    "Minimum approach (internal pinch)",
                    pinch,
                    &format!("{} points incl. phase changes, at {:.1} % of the duty from the cold inlet", pr_.len(), where_),
                ));
                if pinch <= 0.0 {
                    r.warnings.push(format!(
                        "Temperature cross inside the exchanger (min T_hot - T_cold = {} K at {:.0} % of the duty): the specification is infeasible for a counter-flow exchanger.",
                        fmt_sig(pinch),
                        where_
                    ));
                } else if pinch < d1.min(d2) - 0.05 {
                    r.warnings.push(format!(
                        "The internal pinch ({} K at {:.0} % of the duty) is smaller than both end approaches ({} K, {} K).",
                        fmt_sig(pinch),
                        where_,
                        fmt_sig(d1),
                        fmt_sig(d2)
                    ));
                }
                size.push(fm.num("UA", ua / 1000.0, "kW/K", "sum dQ/LMTD_j (counter-flow)"));
                size.push(fm.dt("Effective mean temperature difference Q/UA", 0.5 * (qh + qc) / ua, ""));
                let cmin = (qh / (hi.t - ho.t)).min(qc / (co.t - ci.t));
                size.push(fm.num("NTU = UA/C_min", ua / cmin, "-", "C from mean cp of each stream"));
                match param(inp, &c.params, "U", Dim::NONE) {
                    Some(u) if u > 0.0 => size.push(fm.num("Area A = UA/U", ua / u, "m2", &format!("U = {} W/m2-K", fmt_sig(u)))),
                    _ => size.push(Row { label: "Area".into(), value: "—".into(), unit: "m2".into(), note: "set U (W/m2-K) in the component parameters".into() }),
                }
                let (xu, yu) = (fm.unit(pw), fm.unit(Dim::TEMP));
                let at_end = jmin == 0 || jmin + 1 == pr_.len();
                let vl = (
                    xu.from_si(pr_[jmin].0),
                    yu.from_si(pr_[jmin].1),
                    format!(
                        "{} ΔT = {} K at {:.1} %",
                        if pinch <= 0.0 {
                            "Cross"
                        } else if at_end {
                            "Min. approach"
                        } else {
                            "Pinch"
                        },
                        fmt_sig(pinch),
                        where_
                    ),
                    if pinch <= 0.0 { [220, 0, 0] } else { [120, 120, 120] },
                );
                r.charts.push(Chart {
                    vlines: vec![vl],
                    title: TQ_TITLE.into(),
                    x_label: format!("Q [{}]", xu.symbol),
                    y_label: format!("T [{}]", yu.symbol),
                    lines: vec![
                        ChartLine {
                            name: format!("hot {} -> {}", hi.k, ho.k),
                            color: HOT_C,
                            pts: pr_.iter().map(|p| [xu.from_si(p.0), yu.from_si(p.1)]).collect(),
                            dashed: false,
                            markers: false,
                        },
                        ChartLine {
                            name: format!("cold {} -> {}", ci.k, co.k),
                            color: COLD_C,
                            pts: pr_.iter().map(|p| [xu.from_si(p.0), yu.from_si(p.2)]).collect(),
                            dashed: false,
                            markers: false,
                        },
                        ChartLine {
                            name: format!("pinch {} K", fmt_sig(pinch)),
                            color: if pinch <= 0.0 { [220, 0, 0] } else { [120, 120, 120] },
                            pts: vec![[xu.from_si(pr_[jmin].0), yu.from_si(pr_[jmin].1)], [xu.from_si(pr_[jmin].0), yu.from_si(pr_[jmin].2)]],
                            dashed: true,
                            markers: true,
                        },
                    ],
                    ..Default::default()
                });
                if !hi.ok() || !ci.ok() {
                    r.notes.push("A stream without a known fluid is drawn linear in h (constant cp).".into());
                }
            }
            let tq_missing = "The T-Q profile needs a positive duty on both sides: finite flows and enthalpies, with the hot stream cooling and the cold one heating.";
            if size.is_empty() {
                r.sections.push(Section::missing("Sizing", &need(tq_missing)));
                r.charts.push(Chart { title: TQ_TITLE.into(), missing: need(tq_missing), ..Default::default() });
            } else {
                r.sections.push(Section::new("Sizing", size));
            }
            r.sections.push(Section {
                title: "Pressure drops".into(),
                missing: String::new(),
                rows: vec![
                    fm.row("Hot side dP", pr, hi.p - ho.p, ""),
                    fm.pct("Hot side dP/P_in", (hi.p - ho.p) / hi.p, ""),
                    fm.row("Cold side dP", pr, ci.p - co.p, ""),
                    fm.pct("Cold side dP/P_in", (ci.p - co.p) / ci.p, ""),
                ],
            });
            second_law(&mut r, loss);
            let ch = process_chart(&fm, diag, &[("hot", hi, ho, Proc::Linear, HOT_C), ("cold", ci, co, Proc::Linear, COLD_C)], None);
            proc_chart(&mut r, ch, NEED_FLUID);
        }
        Kind::Heater | Kind::Cooler | Kind::Pipe | Kind::Valve => {
            let (Some(i), Some(o)) = (p_(0), p_(1)) else {
                r.notes.push(CONNECT2.into());
                r.sections.push(Section::missing("Performance", &need(CONNECT2)));
                if matches!(c.kind.base(), Kind::Valve | Kind::Pipe) {
                    second_law(&mut r, f64::NAN);
                }
                if c.kind.base() == Kind::Valve {
                    r.sections.push(Section::missing("Energy balance", &need(CONNECT2)));
                }
                proc_chart(&mut r, None, CONNECT2);
                return r;
            };
            let m = mflow(i, o);
            let q = m * (o.h - i.h);
            let mut rows = vec![fm.row("Mass flow", Dim::MASS_FLOW, m, "")];
            match c.kind.base() {
                Kind::Heater => rows.push(fm.row("Heat added", pw, q, "m (h_out - h_in)")),
                Kind::Cooler => rows.push(fm.row("Heat rejected", pw, -q, "m (h_in - h_out)")),
                Kind::Pipe => rows.push(fm.row("Heat loss", pw, -q, "m (h_in - h_out)")),
                _ => rows.push(fm.row("Enthalpy change (isenthalpic: 0)", Dim::SPEC_ENERGY, o.h - i.h, "")),
            }
            rows.push(fm.dt("Temperature change", o.t - i.t, ""));
            rows.push(fm.row("Pressure drop", pr, i.p - o.p, ""));
            rows.push(fm.pct("Pressure drop / P_out", (i.p - o.p) / o.p, ""));
            rows.push(fm.pct("Pressure drop / P_in", (i.p - o.p) / i.p, ""));
            let ds = o.s - i.s;
            if matches!(c.kind.base(), Kind::Heater | Kind::Cooler) && ds.abs() > 1e-12 {
                rows.push(fm.row("Thermodynamic mean temperature dh/ds", Dim::TEMP, (o.h - i.h) / ds, "temperature at which the heat is exchanged"));
                let tm = (o.h - i.h) / ds;
                rows.push(fm.row("Exergy of the heat Q(1 - T0/T_m)", pw, q.abs() * (1.0 - t0 / tm), ""));
            }
            if c.kind.base() == Kind::Pipe {
                let ql = -q;
                rows.push(fm.pct("Temperature drop / T_out (K)", (i.t - o.t) / o.t, ""));
                if let Some(h0) = o.h_pt(o.p, t0) {
                    rows.push(fm.pct("Heat loss / outlet heat content above T0", ql / (m * (o.h - h0)), "m (h_out - h(T0, P_out))"));
                }
                if let Some(dia) = param(inp, &c.params, "D", Dim::LENGTH) {
                    let rho = i.derived(Key::D);
                    let area = std::f64::consts::PI * dia * dia / 4.0;
                    rows.push(fm.num("Velocity (inlet)", m / (rho * area), "m/s", &format!("D = {} m", fmt_sig(dia))));
                }
            }
            r.sections.push(Section::new("Performance", rows));
            if matches!(c.kind.base(), Kind::Valve | Kind::Pipe) {
                second_law(&mut r, if c.kind.base() == Kind::Pipe { -q } else { 0.0 });
            }
            let col = match c.kind.base() {
                Kind::Heater => HOT_C,
                Kind::Cooler => COLD_C,
                _ => GREEN,
            };
            let ch = process_chart(&fm, diag, &[(c.kind.name(), i, o, if c.kind.base() == Kind::Valve { Proc::Throttle } else { Proc::Linear }, col)], None);
            proc_chart(&mut r, ch, NEED_FLUID);
        }
        Kind::Mixer | Kind::Splitter => {
            let ins: Vec<&St> = ps.iter().enumerate().filter(|(k, _)| pl[*k].inlet).filter_map(|(_, s)| s.as_ref()).collect();
            let outs: Vec<&St> = ps.iter().enumerate().filter(|(k, _)| !pl[*k].inlet).filter_map(|(_, s)| s.as_ref()).collect();
            let mi: f64 = ins.iter().map(|s| s.m).sum();
            let mo: f64 = outs.iter().map(|s| s.m).sum();
            let ei: f64 = ins.iter().map(|s| s.m * s.h).sum();
            let eo: f64 = outs.iter().map(|s| s.m * s.h).sum();
            let mut rows = vec![
                fm.row("Mass in", Dim::MASS_FLOW, mi, ""),
                fm.row("Mass balance (in - out)", Dim::MASS_FLOW, mi - mo, ""),
                fm.row("Energy balance (in - out)", pw, ei - eo, ""),
            ];
            if c.kind.base() == Kind::Splitter {
                for (k, s) in outs.iter().enumerate() {
                    rows.push(fm.pct(&format!("Split fraction to stream {} (out {})", s.k, k + 1), s.m / mi, ""));
                }
            } else if ins.len() == 2 {
                rows.push(fm.dt("Inlet temperature difference", ins[0].t - ins[1].t, "0 for isothermal mixing"));
            }
            r.sections.push(Section::new("Balances", rows));
            if c.kind.base() == Kind::Mixer {
                second_law(&mut r, f64::NAN);
            }
        }
        Kind::CoolingTower => {
            let (wi, wo, ai, ao) = (p_(0), p_(1), p_(2), p_(3));
            let (Some(wi), Some(wo)) = (wi, wo) else {
                r.notes.push("Connect the water inlet and outlet streams.".into());
                r.sections.push(Section::missing("Performance", &need("Connect the water inlet and outlet streams (with state numbers).")));
                return r;
            };
            let range = wi.t - wo.t;
            let mut rows = vec![fm.dt("Range T_w,in - T_w,out", range, "")];
            let q = wi.m * wi.h - wo.m * wo.h;
            rows.push(fm.row("Heat rejected (water side)", pw, q, "m_in h_in - m_out h_out"));
            // evaporation: water lost by the water streams, or else moisture picked up by the air
            let evap_w = wi.m - wo.m;
            let evap_a = match (ai, ao) {
                (Some(ai), Some(ao)) => ao.m - ai.m,
                _ => f64::NAN,
            };
            let (evap, how) = if evap_w.is_finite() && evap_w.abs() > 1e-9 { (evap_w, "water in - water out") } else { (evap_a, "air out - air in (moisture pickup)") };
            if evap.is_finite() {
                rows.push(fm.row("Evaporation", Dim::MASS_FLOW, evap, how));
                rows.push(fm.pct("Evaporation / circulating water", evap / wi.m, ""));
            }
            if let Some(ai) = ai {
                let rh = param(inp, &c.params, "RH", Dim::NONE).or_else(|| {
                    let v = inp.analysis.model.vars.iter().find(|v| v.index.is_empty() && v.name.starts_with("rh"))?;
                    let x = var_si(inp, &v.name, Dim::NONE);
                    x.is_finite().then_some(x)
                });
                if let Some(rh) = rh {
                    if let Ok(twb) = backend::ha_props_si("Twb", "T", ai.t, "P", ai.p, "R", rh) {
                        let approach = wo.t - twb;
                        rows.push(fm.row("Inlet air wet-bulb temperature", Dim::TEMP, twb, &format!("HAPropsSI, RH = {}", fmt_sig(rh))));
                        rows.push(fm.dt("Approach T_w,out - T_wb", approach, ""));
                        rows.push(fm.pct("Effectiveness range/(range + approach)", range / (range + approach), ""));
                    }
                } else {
                    r.notes.push("Set RH (inlet air relative humidity, number or variable) for the wet-bulb approach.".into());
                }
                if wi.m.is_finite() && ai.m.is_finite() {
                    rows.push(fm.num("Liquid/gas ratio L/G", wi.m / ai.m, "-", "water in / air in"));
                }
                if let Some(ao) = ao {
                    rows.push(fm.dt("Air temperature rise", ao.t - ai.t, ""));
                }
            }
            if let Some(wf) = param(inp, &c.params, "fan", pw) {
                rows.push(fm.row("Fan power", pw, wf, ""));
                if q > 0.0 {
                    rows.push(fm.pct("Fan power / heat rejected", wf / q, ""));
                }
            }
            r.sections.push(Section::new("Performance", rows));
        }
        Kind::Generator | Kind::Motor => {
            let (ids, auto) = shaft_machines(d, id);
            let mut rows = Vec::new();
            let mut w_shaft = 0.0;
            for m in &ids {
                if let Some((k, label, w)) = machine_power(d, inp, *m) {
                    let sign = if k == Kind::Turbine { 1.0 } else { -1.0 };
                    w_shaft += sign * w;
                    rows.push(fm.row(&format!("{} {}", label, if k == Kind::Turbine { "(produced)" } else { "(absorbed)" }), pw, w, ""));
                }
            }
            if ids.is_empty() {
                r.notes.push("No machine on this shaft: choose them in the component parameters.".into());
            }
            if auto {
                r.notes.push("Shaft machines chosen automatically (nearest); set them in the parameters.".into());
            }
            let gen = c.kind.base() == Kind::Generator;
            let w_mech = if gen { w_shaft } else { -w_shaft };
            rows.push(fm.row(if gen { "Net shaft power to the gearbox" } else { "Shaft power demand" }, pw, w_mech, ""));
            let em = param(inp, &c.params, "eta_mech", Dim::NONE);
            let ee = param(inp, &c.params, "eta_elec", Dim::NONE);
            let ev = if gen { None } else { param(inp, &c.params, "eta_vfd", Dim::NONE) };
            let (em_, ee_, ev_) = (em.unwrap_or(1.0), ee.unwrap_or(1.0), ev.unwrap_or(1.0));
            let bars;
            if gen {
                let l_mech = w_mech * (1.0 - em_);
                let w_gen_in = w_mech * em_;
                let l_el = w_gen_in * (1.0 - ee_);
                let w_el = w_gen_in * ee_;
                rows.push(fm.row("Mechanical / gearbox losses", pw, l_mech, &format!("eta_mech = {}", em.map(fmt_sig).unwrap_or("1 (not set)".into()))));
                rows.push(fm.row("Generator input", pw, w_gen_in, ""));
                rows.push(fm.row("Generator electrical losses", pw, l_el, &format!("eta_elec = {}", ee.map(fmt_sig).unwrap_or("1 (not set)".into()))));
                rows.push(fm.row("Electric power output", pw, w_el, ""));
                rows.push(fm.pct("Shaft-to-grid efficiency", w_el / w_mech, ""));
                let u = fm.unit(pw);
                bars = vec![
                    ("thermodynamic (shaft)".into(), u.from_si(w_mech), HOT_C),
                    ("mechanical losses".into(), u.from_si(l_mech), GREY),
                    ("generator input".into(), u.from_si(w_gen_in), [230, 159, 0]),
                    ("electrical losses".into(), u.from_si(l_el), GREY),
                    ("electric output".into(), u.from_si(w_el), GREEN),
                ];
            } else {
                let w_motor_out = w_mech / em_;
                let w_motor_in = w_motor_out / ee_;
                let w_grid = w_motor_in / ev_;
                rows.push(fm.row("Mechanical / gearbox losses", pw, w_motor_out - w_mech, &format!("eta_mech = {}", em.map(fmt_sig).unwrap_or("1 (not set)".into()))));
                rows.push(fm.row("Motor shaft output", pw, w_motor_out, ""));
                rows.push(fm.row("Motor electrical losses", pw, w_motor_in - w_motor_out, &format!("eta_elec = {}", ee.map(fmt_sig).unwrap_or("1 (not set)".into()))));
                rows.push(fm.row("VFD losses", pw, w_grid - w_motor_in, &format!("eta_vfd = {}", ev.map(fmt_sig).unwrap_or("1 (not set)".into()))));
                rows.push(fm.row("Electric power from the grid", pw, w_grid, ""));
                rows.push(fm.pct("Grid-to-fluid efficiency", w_mech / w_grid, ""));
                let u = fm.unit(pw);
                bars = vec![
                    ("electric input".into(), u.from_si(w_grid), GREEN),
                    ("VFD losses".into(), u.from_si(w_grid - w_motor_in), GREY),
                    ("motor electrical losses".into(), u.from_si(w_motor_in - w_motor_out), GREY),
                    ("mechanical losses".into(), u.from_si(w_motor_out - w_mech), GREY),
                    ("to the fluid (shaft)".into(), u.from_si(w_mech), COLD_C),
                ];
            }
            r.sections.push(Section::new("Energy conversion", rows));
            let u = fm.unit(pw);
            r.charts.push(Chart { title: "Energy chain".into(), x_label: String::new(), y_label: format!("Power [{}]", u.symbol), bars, ..Default::default() });
        }
        _ => {}
    }
    // energy balance of components with streams only
    if !r.streams.is_empty() && !matches!(c.kind.base(), Kind::Turbine | Kind::Compressor | Kind::Pump | Kind::Heater | Kind::Cooler | Kind::Pipe | Kind::Source | Kind::Sink) {
        let e: f64 = r.streams.iter().map(|s| if s.inlet { s.st.m * s.st.h } else { -s.st.m * s.st.h }).sum();
        if !matches!(c.kind.base(), Kind::Mixer | Kind::Splitter | Kind::HeatExchanger | Kind::CoolingTower) {
            r.sections.push(if e.is_finite() {
                Section::new("Energy balance", vec![fm.row("sum m h (in - out)", pw, e, "")])
            } else {
                Section::missing("Energy balance", &need("Needs the flow and enthalpy of every connected stream."))
            });
        }
    }
    r
}

/// Fluid-property quantities that can be shown on the stream labels.
pub const DERIVED: [(&str, &str, Key); 3] = [("rho", "Density", Key::D), ("v", "Specific volume", Key::V), ("cp", "Cp", Key::Cp)];

/// Label lines `rho = 120 kg/m3` for a stream (display units of the model).
pub fn derived_lines(d: &CycleDiagram, inp: &CycleInputs, l: &Link) -> Vec<String> {
    if d.derived.is_empty() {
        return Vec::new();
    }
    let Some(k) = l.state else { return Vec::new() };
    let b = bases(inp);
    let st = state(inp, &b, k, stream_fluid_ex(l, &main_fluid(inp.script)));
    if !st.ok() {
        return Vec::new();
    }
    let us = &inp.analysis.model.unit_system;
    DERIVED
        .iter()
        .filter(|(n, _, _)| d.derived.iter().any(|x| x == n))
        .filter_map(|(n, _, key)| {
            let v = st.derived(*key);
            if !v.is_finite() {
                return None;
            }
            let u = us.unit_for(key.dim());
            Some(format!("{n} = {} {}", fmt_sig(u.from_si(v)), u.symbol).trim_end().to_string())
        })
        .collect()
}

// ------------------------------------------------------------------ UI

use eframe::egui::{self, Color32, RichText};

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Parameters of a component used by its results.
pub fn params_ui(ui: &mut egui::Ui, d: &mut CycleDiagram, id: u32, inp: &CycleInputs) {
    let Some(kind) = d.comps.iter().find(|c| c.id == id).map(|c| c.kind.base()) else { return };
    let fields: &[(&str, &str)] = match kind {
        Kind::HeatExchanger => &[("U", "Overall heat-transfer coefficient U, W/m2-K (number or variable) -> area")],
        Kind::Generator => &[("eta_mech", "Mechanical / gearbox efficiency (number, variable or product a*b)"), ("eta_elec", "Generator efficiency")],
        Kind::Motor => &[("eta_mech", "Mechanical / gearbox efficiency"), ("eta_elec", "Motor efficiency"), ("eta_vfd", "Variable-frequency drive efficiency")],
        Kind::CoolingTower => &[("RH", "Inlet air relative humidity 0-1 (number or variable)"), ("fan", "Fan power (variable, or number in W)")],
        Kind::Pipe => &[("D", "Inner diameter, m (number or variable) -> velocity")],
        _ => &[],
    };
    ui.strong("Parameters for the results");
    if fields.is_empty() && !matches!(kind, Kind::Generator | Kind::Motor) {
        ui.label(RichText::new("None needed: results follow from the streams.").small().weak());
    }
    let scalars: Vec<String> = {
        let mut v: Vec<String> = inp.analysis.model.vars.iter().filter(|v| v.index.is_empty()).map(|v| v.display.clone()).collect();
        v.sort_by(|a, b| crate::app::natord(&a.to_lowercase(), &b.to_lowercase()));
        v
    };
    let machines: Vec<(u32, String, Kind)> =
        d.comps.iter().filter(|c| matches!(c.kind.base(), Kind::Turbine | Kind::Compressor | Kind::Pump)).map(|c| (c.id, c.label.clone(), c.kind)).collect();
    let (auto_ids, auto) = shaft_machines(d, id);
    let Some(c) = d.comps.iter_mut().find(|c| c.id == id) else { return };
    egui::Grid::new(("params", id)).num_columns(2).show(ui, |ui| {
        for (k, help) in fields {
            ui.label(*k).on_hover_text(*help);
            let v = c.params.entry(k.to_string()).or_default();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(v).desired_width(110.0).hint_text("number or var"));
                egui::ComboBox::from_id_salt(("pvar", id, *k)).selected_text("var…").width(50.0).height(300.0).show_ui(ui, |ui| {
                    for s in &scalars {
                        if ui.selectable_label(false, s).clicked() {
                            *v = s.clone();
                        }
                    }
                });
            });
            ui.end_row();
        }
    });
    c.params.retain(|_, v| !v.trim().is_empty());
    if matches!(kind, Kind::Generator | Kind::Motor) {
        ui.label(RichText::new("Machines on the shaft").small());
        for (mid, label, mk) in &machines {
            let mut on = if auto { auto_ids.contains(mid) } else { c.shaft.contains(mid) };
            if ui.checkbox(&mut on, format!("{label} ({})", mk.name())).changed() {
                if auto {
                    c.shaft = auto_ids.clone();
                }
                if on {
                    c.shaft.push(*mid);
                } else {
                    c.shaft.retain(|x| x != mid);
                }
            }
        }
        if auto {
            ui.label(RichText::new("(chosen automatically; tick to set explicitly)").small().weak());
        }
    }
}

/// Draw a report. `view`: None = everything; Some(0) = streams; Some(1..)
/// = one section, then one chart. Returns true when the property diagram
/// choice changed.
pub fn show_report(ui: &mut egui::Ui, r: &Report, view: Option<usize>, diag: &mut PathDiagram, salt: impl std::hash::Hash + Copy + std::fmt::Debug) -> bool {
    let mut changed = false;
    let show = |k: usize| view.is_none() || view == Some(k);
    if view.is_none() {
        ui.heading(&r.title);
    }
    for w in &r.warnings {
        ui.label(RichText::new(format!("Warning: {w}")).color(Color32::from_rgb(220, 60, 50)).strong());
    }
    for n in &r.notes {
        ui.label(RichText::new(n).small().weak());
    }
    let why = |ui: &mut egui::Ui, t: &str| {
        ui.label(RichText::new(t).italics().weak());
    };
    if view == Some(0) && r.streams.is_empty() {
        why(ui, "No stream with a state number is connected to this component.");
    }
    if show(0) && !r.streams.is_empty() {
        ui.strong("Streams");
        egui::ScrollArea::horizontal().id_salt(("sh", salt)).show(ui, |ui| {
            egui::Grid::new(("streams", salt)).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for h in ["port", "state", "fluid", "m", "T", "P", "h", "s", "x", "subcooling / superheat", "rho", "cp"] {
                    ui.strong(h);
                }
                ui.end_row();
                for s in &r.streams {
                    let st = &s.st;
                    ui.label(format!("{} ({})", s.port, if s.inlet { "in" } else { "out" }));
                    ui.label(st.k.to_string());
                    ui.label(st.fluid.clone().unwrap_or_else(|| "—".into()));
                    for (v, unit) in [(st.m, "kg/s"), (st.t - 273.15, "C"), (st.p / 1e5, "bar"), (st.h / 1e3, "kJ/kg"), (st.s / 1e3, "kJ/kg-K")] {
                        ui.label(RichText::new(if v.is_finite() { format!("{} {unit}", fmt_sig(v)) } else { "—".into() }).monospace());
                    }
                    let (xq, dt) = st.phase().cells();
                    ui.label(RichText::new(xq).monospace());
                    ui.label(RichText::new(dt).monospace());
                    for (v, unit) in [(st.derived(Key::D), "kg/m3"), (st.derived(Key::Cp) / 1e3, "kJ/kg-K")] {
                        ui.label(RichText::new(if v.is_finite() { format!("{} {unit}", fmt_sig(v)) } else { "—".into() }).monospace());
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(12.0);
    }
    for (i, sec) in r.sections.iter().enumerate() {
        if !show(i + 1) {
            continue;
        }
        // the summary lists what is available; a tab of its own says what is missing
        if view.is_none() && !sec.missing.is_empty() {
            continue;
        }
        ui.strong(&sec.title);
        if !sec.missing.is_empty() {
            why(ui, &sec.missing);
            ui.add_space(6.0);
            continue;
        }
        egui::Grid::new(("sec", i, salt)).striped(true).num_columns(3).spacing([18.0, 6.0]).show(ui, |ui| {
            for row in &sec.rows {
                ui.label(&row.label);
                ui.label(RichText::new(format!("{} {}", row.value, row.unit)).monospace().strong());
                ui.label(RichText::new(&row.note).small().weak());
                ui.end_row();
            }
        });
        ui.add_space(6.0);
    }
    let base = r.sections.len() + 1;
    for (i, ch) in r.charts.iter().enumerate() {
        if !show(base + i) || (view.is_none() && !ch.missing.is_empty()) {
            continue;
        }
        ui.strong(&ch.title);
        if ch.key == "process" {
            egui::ComboBox::from_id_salt(("diag", salt, i)).selected_text(diag.label()).show_ui(ui, |ui| {
                for p in PathDiagram::ALL {
                    if ui.selectable_value(diag, p, p.label()).changed() {
                        changed = true;
                    }
                }
            });
        }
        if ch.missing.is_empty() {
            chart(ui, ch, (salt, i), view.is_some());
        } else {
            why(ui, &ch.missing);
        }
        ui.add_space(6.0);
    }
    changed
}

/// `fill`: take the remaining height of the panel (a chart shown on its own).
fn chart(ui: &mut egui::Ui, ch: &Chart, salt: impl std::hash::Hash + std::fmt::Debug, fill: bool) {
    use egui_plot::{Bar, BarChart, Legend, Line, LineStyle, Plot, PlotPoint, Points, Text, VLine};
    let log = ch.log_y;
    let ty = move |y: f64| {
        if log {
            if y > 0.0 {
                y.log10()
            } else {
                f64::NAN
            }
        } else {
            y
        }
    };
    let mut plot = Plot::new(("chart", salt))
        .height(if fill { (ui.available_height() - 8.0).max(320.0) } else { 320.0 })
        .x_axis_label(ch.x_label.clone())
        .y_axis_label(ch.y_label.clone())
        .show_x(false)
        .show_y(false);
    if ch.bars.is_empty() {
        plot = plot.legend(Legend::default());
    }
    if log {
        plot = plot.y_axis_formatter(|g, _| fmt_sig(10f64.powf(g.value)));
    }
    if !ch.bars.is_empty() {
        let labels: Vec<String> = ch.bars.iter().map(|b| b.0.clone()).collect();
        plot = plot.x_axis_formatter(move |g, _| {
            let k = g.value.round();
            if (g.value - k).abs() < 1e-6 && k >= 0.0 && (k as usize) < labels.len() {
                labels[k as usize].clone()
            } else {
                String::new()
            }
        });
    }
    let r = plot.show(ui, |pu| {
        if !ch.bars.is_empty() {
            let bars: Vec<Bar> = ch.bars.iter().enumerate().map(|(k, (l, v, c))| Bar::new(k as f64, *v).name(format!("{l}: {}", fmt_sig(*v))).fill(c32(*c)).width(0.6)).collect();
            pu.bar_chart(BarChart::new("energy", bars));
        }
        for l in &ch.lines {
            let pts: Vec<[f64; 2]> = l.pts.iter().map(|p| [p[0], ty(p[1])]).filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
            if l.markers {
                pu.points(Points::new(l.name.clone(), pts.clone()).radius(3.5).color(c32(l.color)).filled(true));
                if !l.dashed {
                    continue;
                }
            }
            let mut line = Line::new(l.name.clone(), pts).color(c32(l.color)).width(if l.name.starts_with("saturated") { 1.2 } else { 2.0 });
            if l.dashed {
                line = line.style(LineStyle::dashed_dense());
            }
            pu.line(line);
        }
        // annotation just above the upper curve at the marker (a position
        // tied to the data, so it does not feed back into the auto bounds)
        for (x, y, label, c) in &ch.vlines {
            pu.vline(VLine::new("", *x).color(c32(*c)).width(1.2).style(LineStyle::Dashed { length: 8.0 }));
            // left of the line and above the upper curve, which rises with Q
            pu.text(Text::new("", PlotPoint::new(*x, ty(*y)), RichText::new(format!("{label}  ")).size(13.0).strong()).color(c32(*c)).anchor(egui::Align2::RIGHT_BOTTOM));
        }
        if !ch.bars.is_empty() {
            return None;
        }
        // hover read-out of the computed process lines only (no dome,
        // isentrope or pinch marker); snaps to the state points
        let aux = |n: &str| n.is_empty() || n.starts_with("saturated") || n.starts_with("pinch") || n.starts_with("isentropic");
        let keys: Vec<f64> = ch.lines.iter().filter(|l| l.markers && l.name.is_empty()).flat_map(|l| l.pts.iter().map(|p| p[0])).chain(ch.vlines.iter().map(|v| v.0)).collect();
        let pts: Vec<(String, Color32, Vec<[f64; 2]>)> = ch
            .lines
            .iter()
            .filter(|l| !aux(&l.name))
            .map(|l| (l.name.clone(), c32(l.color), l.pts.iter().map(|p| [p[0], ty(p[1])]).filter(|p| p[0].is_finite() && p[1].is_finite()).collect()))
            .collect();
        let series: Vec<crate::plots::HoverSeries> = pts.iter().map(|p| (p.0.clone(), p.1, p.2.as_slice())).collect();
        let (x, _, rows) = crate::plots::hover_readout(pu, &series, &keys)?;
        Some((fmt_sig(x), rows.into_iter().map(|(n, c, y)| (n, c, fmt_sig(if log { 10f64.powf(y) } else { y }))).collect::<Vec<_>>()))
    });
    if let Some((x, rows)) = r.inner {
        if r.response.hovered() {
            let xl = ch.x_label.split(" [").next().unwrap_or("x").to_string();
            crate::plots::hover_tooltip(r.response, format!("{xl} = {x}"), &rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(t: f64, h: f64) -> St {
        St { k: 0, fluid: None, m: 1.0, t, p: 1e5, h, s: f64::NAN, h_off: 0.0 }
    }

    #[test]
    fn ua_of_balanced_counterflow() {
        // equal capacity rates (cp = 1000 J/kg-K): constant dT = 10 K, UA = Q/dT
        let (hi, ho) = (st(400.0, 400e3), st(320.0, 320e3));
        let (ci, co) = (st(310.0, 310e3), st(390.0, 390e3));
        let pr = tq_profile(&hi, &ho, &ci, &co, 20).unwrap();
        let (ua, pinch) = ua_pinch(&pr);
        assert!((pinch - 10.0).abs() < 1e-9);
        assert!((ua - 80e3 / 10.0).abs() < 1e-6, "{ua}");
    }

    #[test]
    fn ua_matches_lmtd_for_constant_cp() {
        // hot cp = 2000 J/kg-K (390 -> 350 C-ish), cold cp = 1000 (300 -> 380):
        // Q = 80 kW on both sides and UA = Q / LMTD exactly
        let (hi, ho) = (st(390.0, 2000.0 * 390.0), st(350.0, 2000.0 * 350.0));
        let (ci, co) = (st(300.0, 1000.0 * 300.0), st(380.0, 1000.0 * 380.0));
        let pr = tq_profile(&hi, &ho, &ci, &co, 50).unwrap();
        let (ua, pinch) = ua_pinch(&pr);
        let (d1, d2) = (390.0 - 380.0, 350.0 - 300.0);
        let lmtd = (d1 - d2) / f64::ln(d1 / d2);
        assert!((ua - 80e3 / lmtd).abs() / ua < 1e-9, "{ua} vs {}", 80e3 / lmtd);
        assert!((pinch - 10.0).abs() < 1e-9);
    }

    /// Real-fluid state at (P, T) or (P, h) on CoolProp's scale.
    fn real(fluid: &str, p: f64, t: Option<f64>, h: Option<f64>) -> St {
        let f = coolprop_fluid(fluid);
        let r = match (t, h) {
            (Some(t), _) => backend::state_point(&f, [Key::P, Key::T], [p, t], &[Key::T, Key::H, Key::S]).unwrap(),
            (_, Some(h)) => backend::state_point(&f, [Key::P, Key::H], [p, h], &[Key::T, Key::H, Key::S]).unwrap(),
            _ => unreachable!(),
        };
        St { k: 0, fluid: Some(f), m: 1.0, t: r[0], p, h: r[1], s: r[2], h_off: 0.0 }
    }

    /// Outlet of a machine with isentropic efficiency `eta` to pressure `p`.
    fn outlet(a: &St, p: f64, eta: f64) -> St {
        let hs = a.h_ps(p, a.s_cp().unwrap()).unwrap();
        let h = if p > a.p { a.h + (hs - a.h) / eta } else { a.h - eta * (a.h - hs) };
        real(a.fluid.as_deref().unwrap(), p, None, Some(h))
    }

    fn entropy_along(a: &St, b: &St, log_p: bool) -> Vec<f64> {
        let v = polytropic(a, b, 60, log_p).expect("polytropic path");
        assert_eq!(v.last().unwrap().1, b.h);
        v.iter().map(|(p, h)| a.at_ph(*p, *h, &[Key::S]).unwrap()[0]).collect()
    }

    #[test]
    fn compression_and_expansion_paths_raise_entropy() {
        if backend::limits("CO2").is_err() {
            return; // CoolProp not available
        }
        let cases = [
            // sCO2 main compressor, near the critical point
            (real("CO2", 7.7e6, Some(305.15), None), 25e6, 0.85, true),
            // sCO2 turbine
            (real("CO2", 25e6, Some(823.15), None), 7.8e6, 0.90, true),
            // steam turbine ending wet
            (real("Water", 100e5, Some(773.15), None), 0.1e5, 0.85, true),
            // feed pump from saturated liquid
            (real("Water", 0.1e5, Some(318.0), None), 100e5, 0.75, false),
        ];
        for (a, p, eta, log_p) in cases {
            let b = outlet(&a, p, eta);
            let s = entropy_along(&a, &b, log_p);
            for w in s.windows(2) {
                assert!(w[1] >= w[0] - 1e-9 * w[0].abs().max(1.0), "entropy decreases: {} -> {} ({:?} to {p})", w[0], w[1], a.fluid);
            }
            assert!(s[s.len() - 1] > s[0]);
        }
    }

    #[test]
    fn processes_are_classified_from_the_states() {
        if backend::limits("CO2").is_err() {
            return;
        }
        let a = real("CO2", 7.7e6, Some(305.15), None);
        let b = outlet(&a, 25e6, 0.85);
        assert!(matches!(classify(&a, &b), ProcessKind::Compression(e) if (e - 0.85).abs() < 1e-9));
        let t = real("CO2", 25e6, Some(823.15), None);
        let u = outlet(&t, 7.8e6, 0.9);
        assert!(matches!(classify(&t, &u), ProcessKind::Expansion(e) if (e - 0.9).abs() < 1e-9));
        // heater with 2 % pressure loss, cooler at constant pressure, valve
        let hot = real("CO2", 24.5e6, Some(823.15), None);
        assert_eq!(classify(&b, &hot), ProcessKind::Heat);
        assert_eq!(classify(&u, &real("CO2", 7.8e6, Some(320.0), None)), ProcessKind::Heat);
        assert_eq!(classify(&hot, &real("CO2", 10e6, None, Some(hot.h))), ProcessKind::Throttle);
        // the expansion path ends at the outlet and its entropy rises
        let pts = path(&t, &u, Key::S, Key::T, classify(&t, &u).proc_(), 40);
        assert!(pts.len() > 30);
        assert!((pts.last().unwrap()[1] - u.t).abs() < 1e-6 * u.t);
        assert!(pts.windows(2).all(|w| w[1][0] >= w[0][0] - 1e-9));
    }

    #[test]
    fn phase_and_saturation_crossings() {
        if backend::limits("Water").is_err() {
            return;
        }
        // boiler at 10 bar: 50 C liquid to 300 C vapour crosses both lines
        let a = real("Water", 10e5, Some(323.15), None);
        let b = real("Water", 10e5, Some(573.15), None);
        let f = phase_fracs(&a, &b);
        assert_eq!(f.len(), 2);
        let (hl, hv, tb, _) = a.sat(10e5).unwrap();
        for (fi, hs) in f.iter().zip([hl, hv]) {
            assert!((a.h + (b.h - a.h) * fi - hs).abs() < 1e-6);
        }
        match a.phase() {
            Phase::Subcooled(d) => assert!((d - (tb - 323.15)).abs() < 1e-6),
            p => panic!("{p:?}"),
        }
        assert!(matches!(b.phase(), Phase::Superheated(d) if d > 0.0));
        let wet = real("Water", 10e5, None, Some(0.5 * (hl + hv)));
        assert!(matches!(wet.phase(), Phase::TwoPhase(x) if (x - 0.5).abs() < 1e-9));
        assert_eq!(real("CO2", 25e6, Some(800.0), None).phase(), Phase::Supercritical);
        // the T-Q profile contains both kinks
        let pr = tq_profile(&real("CO2", 25e6, Some(900.0), None), &real("CO2", 25e6, Some(700.0), None), &a, &b, 50);
        if let Some(pr) = pr {
            assert!(pr.len() >= 52);
        }
    }
}
