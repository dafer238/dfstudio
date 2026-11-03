//! Numerical solution of the block-triangularised system.
//!
//! * Size-1 blocks whose equation is explicit in its unknown (`x = f(...)`)
//!   are evaluated directly — no iteration.
//! * Other blocks are solved with a damped Newton–Raphson method with
//!   backtracking (Armijo) line search on the scaled residual norm, falling
//!   back to Levenberg–Marquardt steps when the Jacobian is singular
//!   (Dennis & Schnabel, *Numerical Methods for Unconstrained Optimization
//!   and Nonlinear Equations*, SIAM 1996, ch. 6; Nocedal & Wright,
//!   *Numerical Optimization*, 2nd ed., Springer 2006, ch. 11).
//! * Jacobians are exact (reverse-mode AD on the tapes, analytic CoolProp
//!   partials).

use crate::compile::Model;
use crate::structure::{analyze_active, EqStatus, Structure, VarStatus};
use crate::tape::{Op, Scratch, VarId};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct SolveOptions {
    pub max_iter: usize,
    /// Convergence on the scaled residual `|lhs - rhs| / s`, where `s` is the
    /// largest absolute additive term of the equation (1 if that is ~0).
    pub rtol: f64,
    /// Default guess for variables without guess (1).
    pub default_guess: f64,
}

impl Default for SolveOptions {
    fn default() -> Self {
        SolveOptions { max_iter: 250, rtol: 1e-10, default_guess: 1.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockReport {
    pub size: usize,
    pub iterations: usize,
    pub residual: f64,
    pub converged: bool,
    pub explicit: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Solution {
    pub x: Vec<f64>,
    /// Which variables hold a computed (or fixed) value.
    pub known: Vec<bool>,
    pub converged: bool,
    pub blocks: Vec<BlockReport>,
    /// Scaled residual of every equation (NaN if not evaluated).
    pub residuals: Vec<f64>,
    pub max_residual: f64,
    pub elapsed_ms: f64,
    pub message: String,
}

/// Precomputed solution plan for a given set of fixed variables. Reusable
/// for many solves (parametric tables, optimisation).
#[derive(Debug, Clone)]
pub struct Plan {
    pub structure: Structure,
    pub fixed: Vec<bool>,
    blocks: Vec<PBlock>,
}

#[derive(Debug, Clone)]
struct PBlock {
    eqs: Vec<u32>,
    vars: Vec<VarId>,
    /// for each eq, position of each eq var in `vars` (or usize::MAX)
    cols: Vec<Vec<usize>>,
    explicit: Option<Explicit>,
}

#[derive(Debug, Clone, Copy)]
struct Explicit {
    /// The unknown is on the lhs (else rhs)
    on_lhs: bool,
}

impl Plan {
    pub fn new(model: &Model, fixed_vars: &[VarId]) -> Plan {
        Plan::with_inactive(model, fixed_vars, &[])
    }

    /// Plan with some variables fixed and some equations disabled (studies
    /// replace e.g. `x = 5` by a value chosen by the optimiser).
    pub fn with_inactive(model: &Model, fixed_vars: &[VarId], inactive_eqs: &[u32]) -> Plan {
        let mut fixed = vec![false; model.vars.len()];
        for &v in fixed_vars {
            fixed[v as usize] = true;
        }
        let mut active = vec![true; model.eqs.len()];
        for &e in inactive_eqs {
            active[e as usize] = false;
        }
        let eq_vars: Vec<&[VarId]> = model.eqs.iter().map(|e| e.vars.as_slice()).collect();
        let structure = analyze_active(model.vars.len(), &eq_vars, &fixed, Some(&active));
        let blocks = structure
            .blocks
            .iter()
            .map(|b| {
                let cols = b.eqs.iter().map(|&e| model.eqs[e as usize].vars.iter().map(|v| b.vars.iter().position(|x| x == v).unwrap_or(usize::MAX)).collect()).collect();
                let explicit = if b.eqs.len() == 1 {
                    let eq = &model.eqs[b.eqs[0] as usize];
                    let v = b.vars[0];
                    let is_var = |n: u32| matches!(eq.tape.ops[n as usize], Op::Var(x) if x == v);
                    if is_var(eq.lhs) && !eq.tape.vars_of(eq.rhs).contains(&v) {
                        Some(Explicit { on_lhs: true })
                    } else if is_var(eq.rhs) && !eq.tape.vars_of(eq.lhs).contains(&v) {
                        Some(Explicit { on_lhs: false })
                    } else {
                        None
                    }
                } else {
                    None
                };
                PBlock { eqs: b.eqs.clone(), vars: b.vars.clone(), cols, explicit }
            })
            .collect();
        Plan { structure, fixed, blocks }
    }
}

/// Initial values: guesses, then defaults.
pub fn initial_values(model: &Model, opts: &SolveOptions) -> Vec<f64> {
    let mut x = vec![opts.default_guess; model.vars.len()];
    for (v, g) in &model.guesses {
        x[*v as usize] = *g;
    }
    x
}

#[inline]
fn scale_of(sc: &Scratch, terms: &[u32]) -> f64 {
    let mut s = 0.0f64;
    for &t in terms {
        s = s.max(Scratch_val(sc, t).abs());
    }
    if s > 1e-12 && s.is_finite() {
        s
    } else {
        1.0
    }
}

struct Ctx<'a> {
    model: &'a Model,
    sc: Scratch,
    grad: Vec<f64>,
    /// Newton on the equations as written instead of the form with the
    /// denominators cleared (fallback when that form has a spurious root).
    natural: bool,
}

impl Ctx<'_> {
    /// scaled residual of equation e
    fn resid(&mut self, e: u32, x: &[f64]) -> Option<f64> {
        let eq = &self.model.eqs[e as usize];
        let r = eq.tape.eval(eq.root, x, &mut self.sc).ok()?;
        let v = r / scale_of(&self.sc, &eq.terms);
        v.is_finite().then_some(v)
    }
}

#[allow(non_snake_case)]
#[inline]
fn Scratch_val(s: &Scratch, n: u32) -> f64 {
    crate::tape::Tape::node_value(s, n)
}

/// Solve the model with a plan. `x` holds fixed values and initial guesses
/// on entry and the solution on exit.
pub fn solve_with_plan(model: &Model, plan: &Plan, x: &mut [f64], opts: &SolveOptions) -> Solution {
    let t0 = Instant::now();
    let mut ctx = Ctx { model, sc: Scratch::default(), grad: Vec::new(), natural: false };
    let mut reports = Vec::with_capacity(plan.blocks.len());
    let mut known = plan.fixed.clone();
    let mut converged = true;
    let mut first_fail: Option<String> = None;
    for (bi, b) in plan.blocks.iter().enumerate() {
        // bounds clamp of the initial guess
        for &v in &b.vars {
            if let Some((lo, hi)) = model.bounds.get(&v) {
                x[v as usize] = x[v as usize].clamp(*lo, *hi);
            }
        }
        let rep = if let Some(ex) = b.explicit {
            solve_explicit(&mut ctx, b, ex, x)
        } else {
            let start: Vec<f64> = b.vars.iter().map(|v| x[*v as usize]).collect();
            ctx.natural = false;
            let rep = newton(&mut ctx, b, x, opts);
            if rep.converged {
                rep
            } else {
                // clearing denominators adds roots where a denominator is
                // zero (e.g. ln(dT1/dT2) = 0 in an LMTD); retry the block on
                // the equations as written from the same start
                let first: Vec<f64> = b.vars.iter().map(|v| x[*v as usize]).collect();
                for (k, v) in b.vars.iter().enumerate() {
                    x[*v as usize] = start[k];
                }
                presweep(&mut ctx, b, x);
                ctx.natural = true;
                let rep2 = newton(&mut ctx, b, x, opts);
                ctx.natural = false;
                if rep2.converged {
                    rep2
                } else {
                    for (k, v) in b.vars.iter().enumerate() {
                        x[*v as usize] = first[k];
                    }
                    rep
                }
            }
        };
        if rep.converged {
            for &v in &b.vars {
                known[v as usize] = true;
            }
        } else {
            converged = false;
            if first_fail.is_none() {
                let names: Vec<&str> = b.vars.iter().take(6).map(|v| model.vars[*v as usize].display.as_str()).collect();
                first_fail = Some(format!(
                    "block {} ({} eq.) did not converge for {}{}: {}",
                    bi + 1,
                    b.vars.len(),
                    names.join(", "),
                    if b.vars.len() > 6 { ", ..." } else { "" },
                    rep.message.clone().unwrap_or_default()
                ));
            }
        }
        reports.push(rep);
    }
    // residuals of all evaluable equations
    let mut residuals = vec![f64::NAN; model.eqs.len()];
    let mut max_res: f64 = 0.0;
    for (e, eq) in model.eqs.iter().enumerate() {
        if matches!(plan.structure.eq_status[e], EqStatus::UnderDetermined | EqStatus::Inactive) {
            continue;
        }
        if eq.vars.iter().all(|v| known[*v as usize]) {
            let r = ctx.resid(e as u32, x).unwrap_or(f64::NAN);
            residuals[e] = r;
            if r.is_nan() || r.abs() > max_res {
                max_res = if r.is_nan() { f64::INFINITY } else { r.abs() };
            }
        }
    }
    let mut msg = String::new();
    let s = &plan.structure;
    if first_fail.is_none() && !max_res.is_finite() {
        // a computed equation that cannot be evaluated at the solution
        converged = false;
        let bad = residuals.iter().position(|r| r.is_nan()).map(|e| model.eqs[e].label.clone()).unwrap_or_default();
        first_fail = Some(format!("an equation cannot be evaluated at the solution: {bad}"));
    }
    if let Some(f) = first_fail {
        msg = f;
    } else if s.dof > 0 {
        msg = format!("{} degree(s) of freedom left: {} variable(s) are not determined", s.dof, s.count(VarStatus::UnderDetermined));
    } else if !s.redundant.is_empty() {
        let bad = s.redundant.iter().filter(|e| residuals[**e as usize].abs() > opts.rtol.sqrt()).count();
        msg = if bad > 0 {
            converged = false;
            format!("{} equation(s) more than unknowns; {} of them are not satisfied", s.redundant.len(), bad)
        } else {
            format!("{} redundant equation(s) (consistent)", s.redundant.len())
        };
    }
    if converged && msg.is_empty() {
        msg = format!("Solved {} equations in {} blocks, max residual {:.2e}", model.eqs.len(), plan.blocks.len(), max_res);
    }
    Solution {
        x: x.to_vec(),
        known,
        converged: converged && s.dof == 0,
        blocks: reports,
        residuals,
        max_residual: max_res,
        elapsed_ms: t0.elapsed().as_secs_f64() * 1e3,
        message: msg,
    }
}

/// Convenience: analyse and solve with default guesses.
pub fn solve(model: &Model, opts: &SolveOptions) -> (Plan, Solution) {
    let plan = Plan::new(model, &[]);
    let mut x = initial_values(model, opts);
    let sol = solve_with_plan(model, &plan, &mut x, opts);
    (plan, sol)
}

/// Start point for the retry of a block: evaluate the block's definitions
/// (`v = expression without v`) in turn from the current values, so that
/// e.g. temperature differences follow from guessed temperatures.
fn presweep(ctx: &mut Ctx, b: &PBlock, x: &mut [f64]) {
    for _ in 0..2 {
        for &e in &b.eqs {
            let eq = &ctx.model.eqs[e as usize];
            for (side, other) in [(eq.lhs, eq.rhs), (eq.rhs, eq.lhs)] {
                if let Op::Var(v) = eq.tape.ops[side as usize] {
                    if b.vars.contains(&v) && !eq.tape.vars_of(other).contains(&v) {
                        if let Ok(val) = eq.tape.eval(other, x, &mut ctx.sc) {
                            if val.is_finite() {
                                let (lo, hi) = ctx.model.bounds.get(&v).copied().unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
                                x[v as usize] = val.clamp(lo, hi);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}

fn solve_explicit(ctx: &mut Ctx, b: &PBlock, ex: Explicit, x: &mut [f64]) -> BlockReport {
    let eq = &ctx.model.eqs[b.eqs[0] as usize];
    let v = b.vars[0] as usize;
    let side = if ex.on_lhs { eq.rhs } else { eq.lhs };
    let res = eq.tape.eval(side, x, &mut ctx.sc);
    match res {
        Ok(val) if val.is_finite() => {
            x[v] = val;
            BlockReport { size: 1, iterations: 0, residual: 0.0, converged: true, explicit: true, message: None }
        }
        Ok(val) => BlockReport { size: 1, iterations: 0, residual: f64::NAN, converged: false, explicit: true, message: Some(format!("evaluates to {val}")) },
        Err(e) => BlockReport { size: 1, iterations: 0, residual: f64::NAN, converged: false, explicit: true, message: Some(e.0) },
    }
}

/// Residual vector (Newton form, divided by the fixed scales `sc`) and
/// optionally the Jacobian of a block. With `sc[i] == 0` the scale is
/// computed at `x` and stored.
fn block_eval(ctx: &mut Ctx, b: &PBlock, x: &[f64], r: &mut [f64], jac: Option<&mut [f64]>, sc: &mut [f64]) -> Result<(), String> {
    let n = b.vars.len();
    let mut jac = jac;
    for (i, &e) in b.eqs.iter().enumerate() {
        let eq = &ctx.model.eqs[e as usize];
        if let Some(j) = jac.as_deref_mut() {
            ctx.grad.clear();
            ctx.grad.resize(eq.vars.len(), 0.0);
            let (root, terms) = if ctx.natural { (eq.root, &eq.terms) } else { (eq.nroot, &eq.nterms) };
            let val = eq.tape.eval_grad(root, x, &eq.vars, &mut ctx.grad, &mut ctx.sc).map_err(|e| e.0)?;
            if sc[i] == 0.0 {
                sc[i] = scale_of(&ctx.sc, terms);
            }
            let s = sc[i];
            r[i] = val / s;
            for c in 0..n {
                j[i * n + c] = 0.0;
            }
            for (k, &col) in b.cols[i].iter().enumerate() {
                if col != usize::MAX {
                    j[i * n + col] = ctx.grad[k] / s;
                }
            }
        } else {
            let (root, terms) = if ctx.natural { (eq.root, &eq.terms) } else { (eq.nroot, &eq.nterms) };
            let val = eq.tape.eval(root, x, &mut ctx.sc).map_err(|e| e.0)?;
            if sc[i] == 0.0 {
                sc[i] = scale_of(&ctx.sc, terms);
            }
            r[i] = val / sc[i];
        }
        if !r[i].is_finite() {
            return Err(format!("non-finite residual in: {}", eq.label));
        }
    }
    Ok(())
}

/// Max relative residual of the original equations of a block.
fn true_residual(ctx: &mut Ctx, b: &PBlock, x: &[f64]) -> f64 {
    let mut m = 0.0f64;
    for &e in &b.eqs {
        // NaN must not pass as converged (f64::max ignores NaN)
        match ctx.resid(e, x) {
            Some(v) if v.is_finite() => m = m.max(v.abs()),
            _ => return f64::INFINITY,
        }
    }
    m
}

fn norm2(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum::<f64>()
}

fn newton(ctx: &mut Ctx, b: &PBlock, x: &mut [f64], opts: &SolveOptions) -> BlockReport {
    let n = b.vars.len();
    let mut r = vec![0.0; n];
    let mut jac = vec![0.0; n * n];
    let mut dx = vec![0.0; n];
    let mut x_old = vec![0.0; n];
    let mut r_try = vec![0.0; n];
    let bounds: Vec<(f64, f64)> = b.vars.iter().map(|v| ctx.model.bounds.get(v).copied().unwrap_or((f64::NEG_INFINITY, f64::INFINITY))).collect();
    let mut lambda = 0.0f64; // LM damping
    let mut it = 0;
    let mut scl = vec![0.0; n];
    let fail = |it, res: f64, m: String| BlockReport { size: n, iterations: it, residual: res, converged: false, explicit: false, message: Some(m) };

    if let Err(e) = block_eval(ctx, b, x, &mut r, Some(&mut jac), &mut scl) {
        // try to move away from a bad initial point (e.g. guesses of 1)
        return fail(0, f64::NAN, format!("cannot evaluate at the initial guess: {e}"));
    }
    let mut f = norm2(&r);
    loop {
        let maxr = true_residual(ctx, b, x);
        if maxr <= opts.rtol {
            return BlockReport { size: n, iterations: it, residual: maxr, converged: true, explicit: false, message: None };
        }
        if it >= opts.max_iter {
            return fail(it, maxr, format!("no convergence after {it} iterations (residual {maxr:.3e})"));
        }
        it += 1;
        // Newton step  J dx = -r  (LM if singular)
        let ok = if lambda == 0.0 { lu_solve(&jac, &r, &mut dx, n) } else { false };
        if !ok {
            lambda = if lambda == 0.0 { 1e-6 } else { (lambda * 10.0).min(1e6) };
            lm_step(&jac, &r, &mut dx, n, lambda);
        }
        // line search
        for k in 0..n {
            x_old[k] = x[b.vars[k] as usize];
        }
        let slope = -2.0 * f; // directional derivative of |r|^2 along a Newton step
        let mut t = 1.0;
        let mut accepted = false;
        for _ in 0..40 {
            for k in 0..n {
                let v = (x_old[k] + t * dx[k]).clamp(bounds[k].0, bounds[k].1);
                x[b.vars[k] as usize] = v;
            }
            if block_eval(ctx, b, x, &mut r_try, None, &mut scl).is_ok() {
                let ft = norm2(&r_try);
                if ft <= f + 1e-4 * t * slope.min(0.0) || ft < f * (1.0 - 1e-4 * t) {
                    accepted = true;
                    break;
                }
            }
            t *= 0.5;
        }
        if !accepted {
            // restore, try stronger damping next
            for k in 0..n {
                x[b.vars[k] as usize] = x_old[k];
            }
            if r.iter().all(|v| v.abs() <= 1e-13) {
                // the equations as written must hold too: the form with the
                // denominators cleared can vanish where they cannot be evaluated
                let maxr = true_residual(ctx, b, x);
                if maxr <= opts.rtol.sqrt() {
                    return BlockReport { size: n, iterations: it, residual: maxr, converged: true, explicit: false, message: Some("converged at machine precision".into()) };
                }
                return fail(
                    it,
                    maxr,
                    format!("reached a point where the equations as written do not hold (residual {maxr:.3e}): a denominator is zero there; give guess values or bounds"),
                );
            }
            if lambda >= 1e6 {
                let maxr = r.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                return fail(it, maxr, format!("line search failed (residual {maxr:.3e}); check guess values or bounds"));
            }
            lambda = if lambda == 0.0 { 1e-3 } else { lambda * 10.0 };
            continue;
        }
        lambda = (lambda * 0.1).max(0.0);
        if lambda < 1e-9 {
            lambda = 0.0;
        }
        // refresh scales every few iterations (the start point may be poor)
        if it % 8 == 0 {
            scl.iter_mut().for_each(|s| *s = 0.0);
        }
        if let Err(e) = block_eval(ctx, b, x, &mut r, Some(&mut jac), &mut scl) {
            return fail(it, f64::NAN, e);
        }
        let fnew = norm2(&r);
        // stagnation: tiny step and tiny change
        let step = (0..n).map(|k| (t * dx[k]).abs() / x_old[k].abs().max(1e-8)).fold(0.0f64, f64::max);
        f = fnew;
        if step < 1e-15 && f.sqrt() > opts.rtol {
            let maxr = true_residual(ctx, b, x);
            if maxr < opts.rtol.sqrt() {
                return BlockReport { size: n, iterations: it, residual: maxr, converged: true, explicit: false, message: Some("converged to reduced tolerance".into()) };
            }
            return fail(it, maxr, format!("stagnated (residual {maxr:.3e})"));
        }
    }
}

/// Solve `J dx = -r` (dense, partial pivoting). Public helper for studies.
pub fn dense_solve(jac: &[f64], r: &[f64], dx: &mut [f64], n: usize) -> bool {
    lu_solve(jac, r, dx, n)
}

/// Dense LU with partial pivoting; solves `J dx = -r`. Returns false if singular.
fn lu_solve(jac: &[f64], r: &[f64], dx: &mut [f64], n: usize) -> bool {
    if n == 1 {
        if jac[0] == 0.0 || !jac[0].is_finite() {
            return false;
        }
        dx[0] = -r[0] / jac[0];
        return dx[0].is_finite();
    }
    let mut a = jac.to_vec();
    let mut b: Vec<f64> = r.iter().map(|v| -v).collect();
    let maxabs = a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if maxabs == 0.0 || !maxabs.is_finite() {
        return false;
    }
    for k in 0..n {
        let (mut p, mut pv) = (k, a[k * n + k].abs());
        for i in k + 1..n {
            let v = a[i * n + k].abs();
            if v > pv {
                p = i;
                pv = v;
            }
        }
        if pv <= 1e-14 * maxabs {
            return false;
        }
        if p != k {
            for c in 0..n {
                a.swap(k * n + c, p * n + c);
            }
            b.swap(k, p);
        }
        let d = a[k * n + k];
        for i in k + 1..n {
            let f = a[i * n + k] / d;
            if f != 0.0 {
                a[i * n + k] = 0.0;
                for c in k + 1..n {
                    a[i * n + c] -= f * a[k * n + c];
                }
                b[i] -= f * b[k];
            }
        }
    }
    for k in (0..n).rev() {
        let mut s = b[k];
        for c in k + 1..n {
            s -= a[k * n + c] * dx[c];
        }
        dx[k] = s / a[k * n + k];
    }
    dx.iter().all(|v| v.is_finite())
}

/// Levenberg–Marquardt step: (JᵀJ + λ diag(JᵀJ) + λ I) dx = -Jᵀ r
fn lm_step(jac: &[f64], r: &[f64], dx: &mut [f64], n: usize, lambda: f64) {
    let mut a = vec![0.0; n * n];
    let mut g = vec![0.0; n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..n {
                s += jac[k * n + i] * jac[k * n + j];
            }
            a[i * n + j] = s;
        }
        let mut s = 0.0;
        for k in 0..n {
            s += jac[k * n + i] * r[k];
        }
        g[i] = s;
    }
    for i in 0..n {
        a[i * n + i] += lambda * (a[i * n + i] + 1.0);
    }
    if !lu_solve(&a, &g, dx, n) {
        for i in 0..n {
            dx[i] = -g[i] / (a[i * n + i].max(1e-300));
        }
    }
}
