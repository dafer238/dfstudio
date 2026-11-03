//! Background worker: compiles, analyses and (optionally) solves the script
//! off the UI thread so typing never blocks. Requests are coalesced: only
//! the most recent text is processed.

use dfcore::solver::{initial_values, solve_with_plan, Plan};
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
    /// Guesses from a previous solution (warm start), by canonical name.
    pub warm: Option<Arc<HashMap<String, f64>>>,
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
    let model = compile(&req.text, &req.opts);
    let compile_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let mut plan = Plan::new(&model, &[]);
    let mut control_log = Vec::new();
    let solution = if req.solve && !model.has_errors() {
        let warm: Option<Vec<f64>> = req.warm.as_ref().map(|w| model.vars.iter().map(|v| w.get(&v.name).copied().unwrap_or(f64::NAN)).collect());
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
            let mut x = initial_values(&model, &req.solve_opts);
            if let Some(w) = &warm {
                for (i, v) in w.iter().enumerate() {
                    if v.is_finite() && !model.guesses.contains_key(&(i as u32)) {
                        x[i] = *v;
                    }
                }
            }
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
    }
}
