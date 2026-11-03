//! Background worker: compiles, analyses and (optionally) solves the script
//! off the UI thread so typing never blocks. Requests are coalesced: only
//! the most recent text is processed.

use dfcore::solver::{solve_with_plan, start_values, Plan, StartSource};
use dfcore::units::parse_unit;
use dfcore::{compile, CompileOptions, EqStatus, Model, Solution, SolveOptions, VarStatus};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct Request {
    pub gen: u64,
    pub text: String,
    pub opts: CompileOptions,
    pub solve: bool,
    pub solve_opts: SolveOptions,
    /// Values of a previous solution (warm start), by canonical name, with
    /// the unit they are expressed in (empty: model units, no conversion).
    pub warm: Option<Arc<HashMap<String, (f64, String)>>>,
    /// Guesses and bounds from the Initial values table, by canonical name.
    pub init: Arc<std::collections::BTreeMap<String, InitEntry>>,
}

/// Guess and bounds of one variable set in the Initial values table, in
/// `unit` (empty: model units). `None`: not set (automatic).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct InitEntry {
    pub guess: Option<f64>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub unit: String,
}

impl InitEntry {
    pub fn is_empty(&self) -> bool {
        self.guess.is_none() && self.lower.is_none() && self.upper.is_none()
    }
}

/// Convert `v` from the unit symbol `from` into the model unit of the same
/// dimension (no conversion for an empty or unknown unit).
pub fn to_model_units(model: &Model, v: f64, from: &str) -> f64 {
    to_unit_system(&model.unit_system, v, from)
}

fn to_unit_system(us: &dfcore::UnitSystem, v: f64, from: &str) -> f64 {
    if from.trim().is_empty() {
        return v;
    }
    match parse_unit(from) {
        Ok(u) if !u.dim.is_none() => u.convert(v, &us.unit_for(u.dim)),
        Ok(u) => u.to_si(v),
        Err(_) => v,
    }
}

/// Apply the Initial values table to a compiled model: its guesses replace
/// `$Guess` values, its bounds replace `$Bounds` and the automatic bounds.
pub fn apply_init(model: &mut Model, init: &std::collections::BTreeMap<String, InitEntry>) {
    for (name, e) in init {
        let Some(v) = model.var(name) else { continue };
        let us = model.unit_system.clone();
        let c = |x: f64| to_unit_system(&us, x, &e.unit);
        if let Some(g) = e.guess {
            model.guesses.insert(v, c(g));
        }
        if e.lower.is_some() || e.upper.is_some() {
            let (mut lo, mut hi) = model.bounds.get(&v).copied().unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            if model.auto_bounds.contains(&v) {
                (lo, hi) = (f64::NEG_INFINITY, f64::INFINITY);
            }
            if let Some(l) = e.lower {
                lo = c(l);
            }
            if let Some(h) = e.upper {
                hi = c(h);
            }
            model.bounds.insert(v, (lo, hi));
            model.auto_bounds.remove(&v);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineStatus {
    Determined,
    Pending,
    Over,
    Error,
    Failed,
}

pub struct Analysis {
    pub gen: u64,
    pub model: Arc<Model>,
    pub plan: Plan,
    pub solution: Option<Solution>,
    pub line_status: HashMap<u32, LineStatus>,
    /// canonical variable or array-base name -> aggregated status
    pub name_status: HashMap<String, VarStatus>,
    pub compile_ms: f64,
    pub analyse_ms: f64,
    pub explicit_solve: bool,
    pub control_log: Vec<String>,
    /// Value each variable started from in the last solve, and its source.
    pub start: Vec<f64>,
    pub start_src: Vec<StartSource>,
}

pub struct Worker {
    tx: Sender<Request>,
    rx: Receiver<Analysis>,
    pub busy: bool,
}

impl Worker {
    pub fn new(ctx: eframe::egui::Context) -> Worker {
        let (tx, rx_req) = channel::<Request>();
        let (tx_res, rx) = channel::<Analysis>();
        std::thread::Builder::new()
            .name("dfstudio-analysis".into())
            .stack_size(32 << 20)
            .spawn(move || {
                while let Ok(mut req) = rx_req.recv() {
                    // coalesce: keep only the newest request, but never drop an explicit solve
                    while let Ok(r) = rx_req.try_recv() {
                        let solve = req.solve && !r.solve;
                        req = r;
                        if solve {
                            req.solve = true;
                        }
                    }
                    let res = run(req);
                    if tx_res.send(res).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn analysis thread");
        Worker { tx, rx, busy: false }
    }

    pub fn submit(&mut self, r: Request) {
        self.busy = true;
        let _ = self.tx.send(r);
    }

    pub fn poll(&mut self) -> Option<Analysis> {
        let mut last = None;
        while let Ok(a) = self.rx.try_recv() {
            last = Some(a);
        }
        if last.is_some() {
            self.busy = false;
        }
        last
    }
}

pub(crate) fn run(req: Request) -> Analysis {
    let t0 = Instant::now();
    let mut model = compile(&req.text, &req.opts);
    apply_init(&mut model, &req.init);
    let compile_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let mut plan = Plan::new(&model, &[]);
    let mut control_log = Vec::new();
    let warm: Option<Vec<f64>> = req.warm.as_ref().map(|w| model.vars.iter().map(|v| w.get(&v.name).map(|(x, u)| to_model_units(&model, *x, u)).unwrap_or(f64::NAN)).collect());
    let (start, start_src) = start_values(&model, &req.solve_opts, warm.as_deref());
    let solution = if req.solve && !model.has_errors() {
        if !model.controls.is_empty() {
            match dfcore::studies::solve_with_controls(&model, &req.solve_opts, warm.as_deref()) {
                Ok((p, s, log)) => {
                    plan = p;
                    control_log = log;
                    Some(s)
                }
                Err(e) => {
                    control_log.push(e);
                    None
                }
            }
        } else {
            let mut x = start.clone();
            Some(solve_with_plan(&model, &plan, &mut x, &req.solve_opts))
        }
    } else {
        None
    };
    let st = &plan.structure;
    // per-line status
    let mut line_status: HashMap<u32, LineStatus> = HashMap::new();
    let rank = |s: LineStatus| match s {
        LineStatus::Determined => 0,
        LineStatus::Pending => 1,
        LineStatus::Over => 2,
        LineStatus::Failed => 3,
        LineStatus::Error => 4,
    };
    let mut put = |line: u32, s: LineStatus| {
        let e = line_status.entry(line).or_insert(s);
        if rank(s) > rank(*e) {
            *e = s;
        }
    };
    let failed_vars: Vec<bool> = match &solution {
        Some(sol) => sol.known.iter().map(|k| !k).collect(),
        None => vec![false; model.vars.len()],
    };
    for (e, eq) in model.eqs.iter().enumerate() {
        let mut s = match st.eq_status[e] {
            EqStatus::Determined | EqStatus::Inactive => LineStatus::Determined,
            EqStatus::UnderDetermined => LineStatus::Pending,
            EqStatus::OverDetermined | EqStatus::Redundant => LineStatus::Over,
        };
        if s == LineStatus::Determined {
            if let Some(v) = st.eq_match[e] {
                if failed_vars[v as usize] {
                    s = LineStatus::Failed;
                }
            }
        }
        put(eq.span.line, s);
    }
    for d in &model.diagnostics {
        if d.severity == dfcore::Severity::Error {
            put(d.span.line, LineStatus::Error);
        }
    }
    let mut name_status: HashMap<String, VarStatus> = HashMap::new();
    let vrank = |s: VarStatus| match s {
        VarStatus::Fixed | VarStatus::Determined | VarStatus::Unused => 0,
        VarStatus::UnderDetermined => 1,
        VarStatus::OverDetermined => 2,
    };
    for (i, v) in model.vars.iter().enumerate() {
        let s = st.var_status[i];
        for key in [&v.name, &v.base] {
            let e = name_status.entry(key.clone()).or_insert(s);
            if vrank(s) > vrank(*e) {
                *e = s;
            }
        }
    }
    Analysis {
        gen: req.gen,
        model: Arc::new(model),
        plan,
        solution,
        line_status,
        name_status,
        compile_ms,
        analyse_ms: t1.elapsed().as_secs_f64() * 1e3,
        explicit_solve: req.solve,
        control_log,
        start,
        start_src,
    }
}
