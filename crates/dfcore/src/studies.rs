//! Studies built on top of the block solver: parametric tables, goal seek,
//! optimisation and control loops.
//!
//! Every study works by *fixing* some variables (inputs chosen by the study)
//! and *deactivating* the equations that set those variables to constants in
//! the script (`P[7] = 282`). The structural plan is computed once and then
//! reused for every evaluation, so a study evaluation costs exactly one
//! block-wise solve.
//!
//! Algorithms: Brent's bounded minimiser (Brent, *Algorithms for
//! Minimization without Derivatives*, Prentice-Hall 1973, ch. 5),
//! Nelder–Mead simplex (Nelder & Mead, *Computer Journal* 7(4):308–313,
//! 1965, doi:10.1093/comjnl/7.4.308), differential evolution DE/rand/1/bin
//! (Storn & Price, *J. Global Optimization* 11:341–359, 1997,
//! doi:10.1023/A:1008202821328), and the secant/bisection safeguarded root
//! finder of Brent (1973, ch. 4) for one-dimensional goal seek.

use crate::compile::Model;
use crate::solver::{initial_values, solve_with_plan, start_values, Plan, Solution, SolveOptions};
use crate::tape::{Scratch, VarId};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Equations whose only variable is `v` (e.g. `x = 5 [bar]`).
pub fn constant_eqs_of(model: &Model, v: VarId) -> Vec<u32> {
    model.eqs.iter().enumerate().filter(|(_, e)| e.vars.len() == 1 && e.vars[0] == v).map(|(i, _)| i as u32).collect()
}

/// Value of `v` as set by its constant equation, if any.
pub fn script_value(model: &Model, v: VarId) -> Option<f64> {
    let e = *constant_eqs_of(model, v).first()?;
    let eq = &model.eqs[e as usize];
    // residual is linear in v for `v = c`: evaluate at 0 and 1
    let mut s = Scratch::default();
    let mut x = vec![0.0; model.vars.len()];
    let r0 = eq.tape.eval(eq.root, &x, &mut s).ok()?;
    x[v as usize] = 1.0;
    let r1 = eq.tape.eval(eq.root, &x, &mut s).ok()?;
    let d = r1 - r0;
    (d != 0.0).then(|| -r0 / d)
}

/// A reusable evaluator: fix `inputs`, solve the rest.
pub struct Study<'m> {
    pub model: &'m Model,
    pub plan: Plan,
    pub inputs: Vec<VarId>,
    pub base: Vec<f64>,
    pub opts: SolveOptions,
}

impl<'m> Study<'m> {
    pub fn new(model: &'m Model, inputs: &[VarId], opts: &SolveOptions, warm: Option<&[f64]>) -> Result<Study<'m>, String> {
        let mut inactive = Vec::new();
        for &v in inputs {
            inactive.extend(constant_eqs_of(model, v));
        }
        let plan = Plan::with_inactive(model, inputs, &inactive);
        let st = &plan.structure;
        if !st.redundant.is_empty() {
            let names: Vec<String> = inputs.iter().filter(|v| st.var_block[**v as usize].is_none()).map(|v| model.vars[*v as usize].display.clone()).collect();
            return Err(format!(
                "the system is over-specified when these inputs are fixed ({} extra equation(s)); an input is probably computed by other equations{}",
                st.redundant.len(),
                if names.is_empty() { String::new() } else { format!(": {}", names.join(", ")) }
            ));
        }
        if st.dof > 0 {
            let pending: Vec<&str> =
                st.var_status.iter().enumerate().filter(|(_, s)| **s == crate::VarStatus::UnderDetermined).take(8).map(|(i, _)| model.vars[i].display.as_str()).collect();
            return Err(format!("{} degree(s) of freedom remain with these inputs (undetermined: {})", st.dof, pending.join(", ")));
        }
        let base = start_values(model, opts, warm).0;
        Ok(Study { model, plan, inputs: inputs.to_vec(), base, opts: opts.clone() })
    }

    /// Solve with `vals` for the inputs, starting from `x` (updated in place).
    pub fn eval_from(&self, vals: &[f64], x: &mut [f64]) -> Solution {
        for (k, v) in self.inputs.iter().enumerate() {
            x[*v as usize] = vals[k];
        }
        solve_with_plan(self.model, &self.plan, x, &self.opts)
    }

    pub fn eval(&self, vals: &[f64]) -> Solution {
        let mut x = self.base.clone();
        self.eval_from(vals, &mut x)
    }
}

// ------------------------------------------------------------------ parametric

/// Solve one row per entry of `rows` (values of `inputs`, in model units).
/// Rows are split into contiguous chunks solved in parallel; inside a chunk
/// each row is warm-started from the previous one (continuation).
pub fn parametric(model: &Model, inputs: &[VarId], rows: &[Vec<f64>], opts: &SolveOptions, warm: Option<&[f64]>) -> Result<Vec<Solution>, String> {
    let study = Study::new(model, inputs, opts, warm)?;
    let nthreads = rayon::current_num_threads().max(1);
    let chunk = rows.len().div_ceil(nthreads).max(1);
    let out: Vec<Vec<Solution>> = rows
        .par_chunks(chunk)
        .map(|rs| {
            let mut x = study.base.clone();
            let mut res = Vec::with_capacity(rs.len());
            for r in rs {
                let mut sol = study.eval_from(r, &mut x);
                if !sol.converged {
                    // retry from the base point
                    let mut x2 = study.base.clone();
                    let s2 = study.eval_from(r, &mut x2);
                    if s2.converged {
                        x = x2;
                        sol = s2;
                    } else {
                        x = study.base.clone();
                    }
                }
                res.push(sol);
            }
            res
        })
        .collect();
    Ok(out.into_iter().flatten().collect())
}

// ------------------------------------------------------------------ goal seek

#[derive(Debug, Clone, Serialize)]
pub struct GoalResult {
    pub value: f64,
    pub solution: Solution,
    pub evaluations: usize,
    pub method: String,
}

/// Find the value of `vary` that makes `target_var == target`.
/// First tries the simultaneous formulation (swap: fix the target, free the
/// varied variable); if that is not structurally square or does not
/// converge, falls back to an outer safeguarded secant loop.
pub fn goal_seek(model: &Model, vary: VarId, target_var: VarId, target: f64, opts: &SolveOptions, warm: Option<&[f64]>) -> Result<GoalResult, String> {
    let x0 = warm.map(|w| w[vary as usize]).filter(|v| v.is_finite()).or_else(|| script_value(model, vary)).unwrap_or(opts.default_guess);
    // simultaneous
    let inactive = constant_eqs_of(model, vary);
    let plan = Plan::with_inactive(model, &[target_var], &inactive);
    if plan.structure.is_square() {
        let mut x = initial_values(model, opts);
        if let Some(w) = warm {
            for (i, v) in w.iter().enumerate() {
                if v.is_finite() {
                    x[i] = *v;
                }
            }
        }
        x[vary as usize] = x0;
        x[target_var as usize] = target;
        let sol = solve_with_plan(model, &plan, &mut x, opts);
        if sol.converged {
            return Ok(GoalResult { value: sol.x[vary as usize], solution: sol, evaluations: 1, method: "simultaneous".into() });
        }
    }
    // outer loop
    let study = Study::new(model, &[vary], opts, warm)?;
    let mut evals = 0;
    let mut last: Option<Solution> = None;
    let mut f = |v: f64, last: &mut Option<Solution>| -> Option<f64> {
        evals += 1;
        let s = study.eval(&[v]);
        let y = s.x[target_var as usize];
        let ok = s.converged;
        *last = Some(s);
        ok.then_some(y - target)
    };
    let (a, fa) = (x0, f(x0, &mut last).ok_or("the model does not solve at the starting value")?);
    if fa == 0.0 {
        let s = last.unwrap();
        return Ok(GoalResult { value: a, solution: s, evaluations: evals, method: "outer secant".into() });
    }
    let bounds = model.bounds.get(&vary).copied().unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
    // secant iterations with step control
    let mut b = if x0 != 0.0 { x0 * 1.01 } else { 1e-3 };
    b = b.clamp(bounds.0, bounds.1);
    let mut fb = f(b, &mut last).ok_or("the model does not solve near the starting value")?;
    let (mut a, mut fa) = (a, fa);
    for _ in 0..100 {
        let tol = opts.rtol.max(1e-12) * target.abs().max(1.0);
        if fb.abs() <= tol * 10.0 {
            let s = last.unwrap();
            return Ok(GoalResult { value: b, solution: s, evaluations: evals, method: "outer secant".into() });
        }
        let denom = fb - fa;
        let mut c = if denom != 0.0 { b - fb * (b - a) / denom } else { b * 1.1 + 1e-3 };
        // limit the step to 4x the previous one
        let maxstep = 4.0 * (b - a).abs().max(1e-9 * b.abs().max(1.0));
        c = c.clamp(b - maxstep, b + maxstep).clamp(bounds.0, bounds.1);
        let mut fc = f(c, &mut last);
        let mut tries = 0;
        while fc.is_none() && tries < 20 {
            c = 0.5 * (b + c);
            fc = f(c, &mut last);
            tries += 1;
        }
        let Some(fcv) = fc else { return Err("goal seek: model fails to solve along the search".into()) };
        a = b;
        fa = fb;
        b = c;
        fb = fcv;
        if (b - a).abs() <= 1e-14 * b.abs().max(1.0) {
            break;
        }
    }
    let s = last.unwrap();
    if fb.abs() < 1e-6 * target.abs().max(1.0) {
        Ok(GoalResult { value: b, solution: s, evaluations: evals, method: "outer secant".into() })
    } else {
        Err(format!("goal seek did not converge (residual {fb:.3e})"))
    }
}

// ------------------------------------------------------------------ optimisation

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptMethod {
    /// 1-D: Brent's bounded minimiser (golden section + parabolic steps).
    Brent,
    /// n-D local: Nelder–Mead simplex with bounds.
    NelderMead,
    /// n-D global: differential evolution (population evaluated in parallel).
    DifferentialEvolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptSpec {
    pub objective: VarId,
    pub maximize: bool,
    /// (variable, lower, upper) in model units
    pub vars: Vec<(VarId, f64, f64)>,
    pub method: OptMethod,
    pub max_evals: usize,
    pub tol: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptResult {
    pub best: Vec<f64>,
    pub objective: f64,
    pub solution: Solution,
    /// Best objective after each evaluation.
    pub history: Vec<f64>,
    pub evaluations: usize,
    pub message: String,
}

pub fn optimize(model: &Model, spec: &OptSpec, opts: &SolveOptions, warm: Option<&[f64]>) -> Result<OptResult, String> {
    let inputs: Vec<VarId> = spec.vars.iter().map(|v| v.0).collect();
    let study = Study::new(model, &inputs, opts, warm)?;
    let lo: Vec<f64> = spec.vars.iter().map(|v| v.1).collect();
    let hi: Vec<f64> = spec.vars.iter().map(|v| v.2).collect();
    for k in 0..lo.len() {
        if !(lo[k] < hi[k]) || !lo[k].is_finite() || !hi[k].is_finite() {
            return Err(format!("invalid bounds for {}", model.vars[inputs[k] as usize].display));
        }
    }
    let sign = if spec.maximize { -1.0 } else { 1.0 };
    let obj = |p: &[f64]| -> f64 {
        let s = study.eval(p);
        if s.converged {
            let v = s.x[spec.objective as usize];
            if v.is_finite() {
                return sign * v;
            }
        }
        f64::INFINITY
    };
    let mut history: Vec<f64> = Vec::new();
    let mut best_f = f64::INFINITY;
    let mut record = |f: f64, history: &mut Vec<f64>| {
        if f < best_f {
            best_f = f;
        }
        history.push(sign * best_f);
    };
    let n = lo.len();
    let best: Vec<f64> = match spec.method {
        OptMethod::Brent => {
            if n != 1 {
                return Err("Brent's method optimises exactly one variable; use Nelder-Mead or DE".into());
            }
            brent_min(
                |x| {
                    let f = obj(&[x]);
                    record(f, &mut history);
                    f
                },
                lo[0],
                hi[0],
                spec.tol.max(1e-10),
                spec.max_evals,
            )
        }
        OptMethod::NelderMead => {
            let x0: Vec<f64> = (0..n)
                .map(|k| {
                    let w = study.base[inputs[k] as usize];
                    let sv = script_value(model, inputs[k]).unwrap_or(w);
                    if sv.is_finite() && sv >= lo[k] && sv <= hi[k] {
                        sv
                    } else {
                        0.5 * (lo[k] + hi[k])
                    }
                })
                .collect();
            nelder_mead(
                |p| {
                    let f = obj(p);
                    record(f, &mut history);
                    f
                },
                &x0,
                &lo,
                &hi,
                spec.tol.max(1e-10),
                spec.max_evals,
            )
        }
        OptMethod::DifferentialEvolution => {
            let (b, hist) = differential_evolution(&obj, &lo, &hi, spec.max_evals, spec.tol);
            for f in hist {
                record(f, &mut history);
            }
            b
        }
    };
    let solution = study.eval(&best);
    let objective = solution.x[spec.objective as usize];
    if !solution.converged {
        return Err(format!("no feasible point found: {}", solution.message));
    }
    let evaluations = history.len();
    Ok(OptResult {
        message: format!("{} {} = {} after {} evaluations", if spec.maximize { "max" } else { "min" }, model.vars[spec.objective as usize].display, objective, evaluations),
        best,
        objective,
        solution,
        history,
        evaluations,
    })
}

/// Brent's bounded scalar minimiser.
pub fn brent_min(mut f: impl FnMut(f64) -> f64, a0: f64, b0: f64, tol: f64, max_evals: usize) -> Vec<f64> {
    let cgold = 0.381_966_011_250_105_1;
    let (mut a, mut b) = (a0, b0);
    let mut x = a + cgold * (b - a);
    let (mut w, mut v) = (x, x);
    let mut fx = f(x);
    let (mut fw, mut fv) = (fx, fx);
    let (mut d, mut e) = (0.0f64, 0.0f64);
    for _ in 0..max_evals.max(3) {
        let xm = 0.5 * (a + b);
        let tol1 = tol * x.abs() + 1e-12;
        let tol2 = 2.0 * tol1;
        if (x - xm).abs() <= tol2 - 0.5 * (b - a) {
            break;
        }
        let mut golden = true;
        if e.abs() > tol1 && fx.is_finite() && fw.is_finite() && fv.is_finite() {
            let r = (x - w) * (fx - fv);
            let mut q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            }
            q = q.abs();
            let etemp = e;
            e = d;
            if !(p.abs() >= (0.5 * q * etemp).abs() || p <= q * (a - x) || p >= q * (b - x)) {
                d = p / q;
                let u = x + d;
                if u - a < tol2 || b - u < tol2 {
                    d = if xm >= x { tol1 } else { -tol1 };
                }
                golden = false;
            }
        }
        if golden {
            e = if x >= xm { a - x } else { b - x };
            d = cgold * e;
        }
        let u = if d.abs() >= tol1 { x + d } else { x + if d >= 0.0 { tol1 } else { -tol1 } };
        let fu = f(u);
        if fu <= fx {
            if u >= x {
                a = x
            } else {
                b = x
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u < x {
                a = u
            } else {
                b = u
            }
            if fu <= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    vec![x]
}

/// Nelder–Mead with box constraints enforced by projection.
pub fn nelder_mead(mut f: impl FnMut(&[f64]) -> f64, x0: &[f64], lo: &[f64], hi: &[f64], tol: f64, max_evals: usize) -> Vec<f64> {
    let n = x0.len();
    let clamp = |p: &mut Vec<f64>| {
        for k in 0..n {
            p[k] = p[k].clamp(lo[k], hi[k]);
        }
    };
    let mut simplex: Vec<Vec<f64>> = vec![x0.to_vec()];
    for k in 0..n {
        let mut p = x0.to_vec();
        let step = 0.1 * (hi[k] - lo[k]);
        p[k] = if p[k] + step <= hi[k] { p[k] + step } else { p[k] - step };
        simplex.push(p);
    }
    let mut fs: Vec<f64> = simplex.iter().map(|p| f(p)).collect();
    let mut evals = n + 1;
    while evals < max_evals {
        let mut idx: Vec<usize> = (0..=n).collect();
        idx.sort_by(|a, b| fs[*a].partial_cmp(&fs[*b]).unwrap_or(std::cmp::Ordering::Equal));
        simplex = idx.iter().map(|i| simplex[*i].clone()).collect();
        fs = idx.iter().map(|i| fs[*i]).collect();
        let spread = (0..n).map(|k| simplex.iter().map(|p| ((p[k] - simplex[0][k]) / (hi[k] - lo[k])).abs()).fold(0.0, f64::max)).fold(0.0, f64::max);
        let fspread = (fs[n] - fs[0]).abs();
        if spread < tol && (fspread <= tol * fs[0].abs().max(1e-12) || !fs[n].is_finite()) && fs[0].is_finite() {
            break;
        }
        let mut c = vec![0.0; n];
        for p in &simplex[..n] {
            for k in 0..n {
                c[k] += p[k] / n as f64;
            }
        }
        let along = |t: f64| -> Vec<f64> {
            let mut p: Vec<f64> = (0..n).map(|k| c[k] + t * (simplex[n][k] - c[k])).collect();
            clamp(&mut p);
            p
        };
        let xr = along(-1.0);
        let fr = f(&xr);
        evals += 1;
        if fr < fs[0] {
            let xe = along(-2.0);
            let fe = f(&xe);
            evals += 1;
            if fe < fr {
                simplex[n] = xe;
                fs[n] = fe;
            } else {
                simplex[n] = xr;
                fs[n] = fr;
            }
        } else if fr < fs[n - 1] {
            simplex[n] = xr;
            fs[n] = fr;
        } else {
            let xc = if fr < fs[n] { along(-0.5) } else { along(0.5) };
            let fc = f(&xc);
            evals += 1;
            if fc < fs[n].min(fr) {
                simplex[n] = xc;
                fs[n] = fc;
            } else {
                // shrink
                for i in 1..=n {
                    let p: Vec<f64> = (0..n).map(|k| simplex[0][k] + 0.5 * (simplex[i][k] - simplex[0][k])).collect();
                    fs[i] = f(&p);
                    simplex[i] = p;
                    evals += 1;
                }
            }
        }
    }
    let i = (0..=n).min_by(|a, b| fs[*a].partial_cmp(&fs[*b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
    simplex[i].clone()
}

/// DE/rand/1/bin with a deterministic generator; the population of each
/// generation is evaluated in parallel.
pub fn differential_evolution(f: &(impl Fn(&[f64]) -> f64 + Sync), lo: &[f64], hi: &[f64], max_evals: usize, tol: f64) -> (Vec<f64>, Vec<f64>) {
    let n = lo.len();
    let np = (10 * n).clamp(16, 60);
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut pop: Vec<Vec<f64>> = (0..np).map(|_| (0..n).map(|k| lo[k] + rand() * (hi[k] - lo[k])).collect()).collect();
    let mut fit: Vec<f64> = pop.par_iter().map(|p| f(p)).collect();
    let mut hist: Vec<f64> = fit.clone();
    let (fw, cr) = (0.7, 0.9);
    while hist.len() + np <= max_evals.max(np * 2) {
        let trials: Vec<Vec<f64>> = (0..np)
            .map(|i| {
                let mut pick = || loop {
                    let j = (rand() * np as f64) as usize % np;
                    if j != i {
                        return j;
                    }
                };
                let (a, b, c) = (pick(), pick(), pick());
                let jr = (rand() * n as f64) as usize % n;
                (0..n).map(|k| if rand() < cr || k == jr { (pop[a][k] + fw * (pop[b][k] - pop[c][k])).clamp(lo[k], hi[k]) } else { pop[i][k] }).collect()
            })
            .collect();
        let tf: Vec<f64> = trials.par_iter().map(|p| f(p)).collect();
        for i in 0..np {
            if tf[i] <= fit[i] {
                pop[i] = trials[i].clone();
                fit[i] = tf[i];
            }
        }
        hist.extend_from_slice(&tf);
        let fin: Vec<f64> = fit.iter().copied().filter(|v| v.is_finite()).collect();
        if fin.len() == np {
            let mean = fin.iter().sum::<f64>() / np as f64;
            let sd = (fin.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / np as f64).sqrt();
            if sd <= tol * mean.abs().max(1e-12) {
                break;
            }
        }
    }
    let i = (0..np).min_by(|a, b| fit[*a].partial_cmp(&fit[*b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
    (pop[i].clone(), hist)
}

// ------------------------------------------------------------------ control loops

/// Solve a model honouring its `$Control` directives. Each control makes a
/// target variable reach a value by varying one variable (a "control loop"). Several controls are solved together: first as a
/// simultaneous system (the targets are fixed, the varied variables become
/// unknowns), and if that fails with an outer quasi-Newton (Broyden) loop.
pub fn solve_with_controls(model: &Model, opts: &SolveOptions, warm: Option<&[f64]>) -> Result<(Plan, Solution, Vec<String>), String> {
    if model.controls.is_empty() {
        let plan = Plan::new(model, &[]);
        let mut x = start_values(model, opts, warm).0;
        let s = solve_with_plan(model, &plan, &mut x, opts);
        return Ok((plan, s, vec![]));
    }
    let mut vary = Vec::new();
    let mut targets = Vec::new();
    let mut tvals = Vec::new();
    for c in &model.controls {
        if c.vary.len() != 1 {
            return Err("each $Control must vary exactly one variable (use one directive per control)".into());
        }
        let v = model.var(&c.vary[0]).ok_or_else(|| format!("$Control: unknown variable '{}'", c.vary[0]))?;
        let (lhs, rhs) = c.target.split_once('=').ok_or("$Control: expected `to make y = value`")?;
        let t = model.var(lhs.trim()).ok_or_else(|| format!("$Control: unknown variable '{}'", lhs.trim()))?;
        let sub = crate::compile(
            &format!("__target = {}", rhs.trim()),
            &crate::CompileOptions { unit_system: Some(model.unit_system.clone()), check_fluids: false, files: Default::default() },
        );
        let (_, ss) = crate::solve(&sub, opts);
        let tv = sub.var("__target").map(|i| ss.x[i as usize]).filter(|v| v.is_finite()).ok_or("$Control: target must be a constant")?;
        vary.push(v);
        targets.push(t);
        tvals.push(tv);
    }
    match_targets(model, &vary, &targets, &tvals, opts, warm)
}

/// Vary `vary[k]` so that `targets[k] = tvals[k]` for all k (n × n matching,
/// used by `$Control` loops and by multi-variable goal seek). First the
/// simultaneous formulation (targets fixed, varied variables become
/// unknowns, their `x = constant` equations disabled); if that is not square
/// or does not converge, an outer quasi-Newton loop (finite-difference
/// Jacobian, Broyden rank-1 updates, backtracking on the max-norm; Broyden
/// 1965, Dennis & Schnabel 1996 §8.1).
pub fn match_targets(model: &Model, vary: &[VarId], targets: &[VarId], tvals: &[f64], opts: &SolveOptions, warm: Option<&[f64]>) -> Result<(Plan, Solution, Vec<String>), String> {
    if vary.len() != targets.len() || targets.len() != tvals.len() || vary.is_empty() {
        return Err("matching needs as many varied variables as targets".into());
    }
    let vary = vary.to_vec();
    let targets = targets.to_vec();
    let mut inactive = Vec::new();
    for &v in &vary {
        inactive.extend(constant_eqs_of(model, v));
    }
    let mut log = Vec::new();
    let plan = Plan::with_inactive(model, &targets, &inactive);
    let mut x = initial_values(model, opts);
    if let Some(w) = warm {
        for (i, v) in w.iter().enumerate() {
            if v.is_finite() {
                x[i] = *v;
            }
        }
    }
    for (k, &v) in vary.iter().enumerate() {
        if let Some(sv) = script_value(model, v) {
            if !warm.map(|w| w[v as usize].is_finite()).unwrap_or(false) {
                x[v as usize] = sv;
            }
        }
        x[targets[k] as usize] = tvals[k];
    }
    if plan.structure.is_square() {
        let s = solve_with_plan(model, &plan, &mut x, opts);
        if s.converged {
            log.push("solved simultaneously".into());
            return Ok((plan, s, log));
        }
        log.push(format!("simultaneous formulation failed ({}); using outer loop", s.message));
    }
    // outer Broyden loop
    let study = Study::new(model, &vary, opts, warm)?;
    let n = vary.len();
    let mut p: Vec<f64> = vary.iter().map(|v| script_value(model, *v).unwrap_or(study.base[*v as usize])).collect();
    let resid = |p: &[f64]| -> Option<(Vec<f64>, Solution)> {
        let s = study.eval(p);
        if !s.converged {
            return None;
        }
        Some(((0..n).map(|k| s.x[targets[k] as usize] - tvals[k]).collect(), s))
    };
    let (mut r, mut sol) = resid(&p).ok_or("model does not solve at the initial control values")?;
    // finite-difference Jacobian
    let mut jac = vec![0.0; n * n];
    for j in 0..n {
        let h = 1e-6 * p[j].abs().max(1e-3);
        let mut pp = p.clone();
        pp[j] += h;
        let (rp, _) = resid(&pp).ok_or("model does not solve near the initial control values")?;
        for i in 0..n {
            jac[i * n + j] = (rp[i] - r[i]) / h;
        }
    }
    for it in 0..100 {
        let scale: f64 = tvals.iter().map(|t| t.abs().max(1.0)).fold(0.0, f64::max);
        let rn = r.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if rn <= 1e-9 * scale {
            log.push(format!("converged in {it} outer iterations"));
            return Ok((study.plan.clone(), sol, log));
        }
        let mut dx = vec![0.0; n];
        if !crate::solver::dense_solve(&jac, &r, &mut dx, n) {
            return Err("Jacobian is singular (does each varied variable influence the targets?)".into());
        }
        let mut t = 1.0;
        let mut accepted = None;
        for _ in 0..20 {
            let pn: Vec<f64> = (0..n).map(|k| p[k] + t * dx[k]).collect();
            if let Some((rn2, s2)) = resid(&pn) {
                if rn2.iter().fold(0.0f64, |m, v| m.max(v.abs())) < rn {
                    accepted = Some((pn, rn2, s2));
                    break;
                }
            }
            t *= 0.5;
        }
        let Some((pn, rn2, s2)) = accepted else { return Err("line search failed (targets not reachable from the start values?)".into()) };
        // Broyden rank-1 update
        let s: Vec<f64> = (0..n).map(|k| pn[k] - p[k]).collect();
        let y: Vec<f64> = (0..n).map(|k| rn2[k] - r[k]).collect();
        let ss: f64 = s.iter().map(|v| v * v).sum();
        if ss > 0.0 {
            for i in 0..n {
                let js: f64 = (0..n).map(|j| jac[i * n + j] * s[j]).sum();
                for j in 0..n {
                    jac[i * n + j] += (y[i] - js) * s[j] / ss;
                }
            }
        }
        p = pn;
        r = rn2;
        sol = s2;
    }
    Err("did not converge in 100 outer iterations".into())
}

#[derive(Debug, Clone)]
pub struct GoalMultiResult {
    /// Values of the varied variables (model units), in input order.
    pub values: Vec<f64>,
    pub solution: Solution,
    pub method: String,
}

/// Goal seek with several pairs: vary `vary[k]` until `targets[k].0` equals
/// `targets[k].1`. One pair uses [`goal_seek`]; more use [`match_targets`].
pub fn goal_seek_multi(model: &Model, vary: &[VarId], targets: &[(VarId, f64)], opts: &SolveOptions, warm: Option<&[f64]>) -> Result<GoalMultiResult, String> {
    if vary.len() != targets.len() || vary.is_empty() {
        return Err("give as many variables to change as targets".into());
    }
    if vary.len() == 1 {
        let r = goal_seek(model, vary[0], targets[0].0, targets[0].1, opts, warm)?;
        return Ok(GoalMultiResult { values: vec![r.value], solution: r.solution, method: r.method });
    }
    let tv: Vec<VarId> = targets.iter().map(|t| t.0).collect();
    let vals: Vec<f64> = targets.iter().map(|t| t.1).collect();
    let (_, sol, log) = match_targets(model, vary, &tv, &vals, opts, warm)?;
    Ok(GoalMultiResult { values: vary.iter().map(|v| sol.x[*v as usize]).collect(), solution: sol, method: log.join("; ") })
}
