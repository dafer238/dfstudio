//! Compiled expression tapes with forward evaluation and reverse-mode
//! automatic differentiation (Griewank & Walther, *Evaluating Derivatives*,
//! SIAM 2008).
//!
//! Each equation residual `lhs - rhs` is lowered into an SSA list of nodes
//! whose operands always precede them. One forward sweep yields the value,
//! one backward sweep yields the exact gradient with respect to every
//! variable of the equation, at a cost of a small constant times the forward
//! sweep regardless of the number of variables.

use crate::funcs::ExtFn;
use std::sync::Arc;

pub type VarId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F1 {
    Neg,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Ln,
    Log10,
    Sqrt,
    Abs,
    Sign,
    Floor,
    Ceil,
    Round,
    Trunc,
    Step,
    Not,
    /// Identity whose result has no physical dimension for the unit
    /// inference (a number in another scale, e.g. converttemp).
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F2 {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Atan2,
    Min,
    Max,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Clone)]
pub enum Op {
    Const(f64),
    Var(VarId),
    F1(F1, u32),
    F2(F2, u32, u32),
    /// cond ? a : b  (derivative flows through the selected branch)
    Select(u32, u32, u32),
    /// External (non-algebraic) function: CoolProp, user functions, ...
    /// Operands are `args[start..start+len]` of the tape.
    Ext(Arc<dyn ExtFn>, u32, u32),
}

impl std::fmt::Debug for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Op::Const(c) => write!(f, "Const({c})"),
            Op::Var(v) => write!(f, "Var({v})"),
            Op::F1(o, a) => write!(f, "{o:?}({a})"),
            Op::F2(o, a, b) => write!(f, "{o:?}({a},{b})"),
            Op::Select(c, a, b) => write!(f, "Select({c},{a},{b})"),
            Op::Ext(e, s, l) => write!(f, "Ext({}, {s}..{})", e.name(), s + l),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Tape {
    pub ops: Vec<Op>,
    pub args: Vec<u32>,
}

/// Reusable scratch buffers to avoid allocation in hot loops.
#[derive(Default)]
pub struct Scratch {
    vals: Vec<f64>,
    adj: Vec<f64>,
    ext_grads: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalError(pub String);

impl Tape {
    pub fn push(&mut self, op: Op) -> u32 {
        self.ops.push(op);
        (self.ops.len() - 1) as u32
    }
    pub fn constant(&mut self, c: f64) -> u32 {
        self.push(Op::Const(c))
    }
    pub fn len(&self) -> usize {
        self.ops.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Variables referenced by the sub-graph rooted at `root`, sorted and unique.
    pub fn vars_of(&self, root: u32) -> Vec<VarId> {
        let mut seen = vec![false; self.ops.len()];
        let mut stack = vec![root];
        let mut out = Vec::new();
        while let Some(n) = stack.pop() {
            if seen[n as usize] {
                continue;
            }
            seen[n as usize] = true;
            match &self.ops[n as usize] {
                Op::Var(v) => out.push(*v),
                Op::Const(_) => {}
                Op::F1(_, a) => stack.push(*a),
                Op::F2(_, a, b) => {
                    stack.push(*a);
                    stack.push(*b)
                }
                Op::Select(c, a, b) => {
                    stack.push(*c);
                    stack.push(*a);
                    stack.push(*b)
                }
                Op::Ext(_, s, l) => stack.extend_from_slice(&self.args[*s as usize..(*s + *l) as usize]),
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Forward sweep; returns all node values in `scratch.vals`.
    fn forward(&self, x: &[f64], s: &mut Scratch, need_grad: bool) -> Result<(), EvalError> {
        let n = self.ops.len();
        s.vals.clear();
        s.vals.resize(n, 0.0);
        if need_grad {
            if s.ext_grads.len() < n {
                s.ext_grads.resize_with(n, Vec::new);
            }
        }
        for i in 0..n {
            let v = match &self.ops[i] {
                Op::Const(c) => *c,
                Op::Var(id) => x[*id as usize],
                Op::F1(f, a) => f1(*f, s.vals[*a as usize]),
                Op::F2(f, a, b) => f2(*f, s.vals[*a as usize], s.vals[*b as usize]),
                Op::Select(c, a, b) => {
                    if s.vals[*c as usize] != 0.0 {
                        s.vals[*a as usize]
                    } else {
                        s.vals[*b as usize]
                    }
                }
                Op::Ext(fun, st, l) => {
                    let idx = &self.args[*st as usize..(*st + *l) as usize];
                    let mut argv = [0.0f64; 16];
                    let mut heap;
                    let a: &mut [f64] = if idx.len() <= 16 {
                        &mut argv[..idx.len()]
                    } else {
                        heap = vec![0.0; idx.len()];
                        &mut heap[..]
                    };
                    for (k, &j) in idx.iter().enumerate() {
                        a[k] = s.vals[j as usize];
                    }
                    if need_grad {
                        let g = &mut s.ext_grads[i];
                        g.clear();
                        g.resize(idx.len(), 0.0);
                        fun.eval_grad(a, g).map_err(EvalError)?
                    } else {
                        fun.eval(a).map_err(EvalError)?
                    }
                }
            };
            s.vals[i] = v;
        }
        Ok(())
    }

    pub fn eval(&self, root: u32, x: &[f64], s: &mut Scratch) -> Result<f64, EvalError> {
        self.forward(x, s, false)?;
        Ok(s.vals[root as usize])
    }

    /// Evaluate `root` and accumulate `d root / d x[v]` into `grad` for the
    /// listed variables (`grad[k]` corresponds to `vars[k]`).
    pub fn eval_grad(&self, root: u32, x: &[f64], vars: &[VarId], grad: &mut [f64], s: &mut Scratch) -> Result<f64, EvalError> {
        self.forward(x, s, true)?;
        let n = self.ops.len();
        s.adj.clear();
        s.adj.resize(n, 0.0);
        s.adj[root as usize] = 1.0;
        for g in grad.iter_mut() {
            *g = 0.0;
        }
        for i in (0..=root as usize).rev() {
            let w = s.adj[i];
            if w == 0.0 {
                continue;
            }
            match &self.ops[i] {
                Op::Const(_) => {}
                Op::Var(id) => {
                    if let Ok(k) = vars.binary_search(id) {
                        grad[k] += w;
                    }
                }
                Op::F1(f, a) => {
                    let xa = s.vals[*a as usize];
                    s.adj[*a as usize] += w * d1(*f, xa, s.vals[i]);
                }
                Op::F2(f, a, b) => {
                    let (xa, xb) = (s.vals[*a as usize], s.vals[*b as usize]);
                    let (da, db) = d2(*f, xa, xb, s.vals[i]);
                    s.adj[*a as usize] += w * da;
                    s.adj[*b as usize] += w * db;
                }
                Op::Select(c, a, b) => {
                    if s.vals[*c as usize] != 0.0 {
                        s.adj[*a as usize] += w;
                    } else {
                        s.adj[*b as usize] += w;
                    }
                }
                Op::Ext(_, st, l) => {
                    let idx = &self.args[*st as usize..(*st + *l) as usize];
                    for (k, &j) in idx.iter().enumerate() {
                        s.adj[j as usize] += w * s.ext_grads[i][k];
                    }
                }
            }
        }
        Ok(s.vals[root as usize])
    }

    /// Value of an arbitrary node after the last forward sweep.
    pub fn node_value(s: &Scratch, node: u32) -> f64 {
        s.vals.get(node as usize).copied().unwrap_or(f64::NAN)
    }

    /// Try to fold the node into a constant when it depends on no variable.
    pub fn const_value(&self, root: u32) -> Option<f64> {
        if !self.vars_of(root).is_empty() {
            return None;
        }
        let mut s = Scratch::default();
        self.eval(root, &[], &mut s).ok()
    }
}

#[inline]
pub fn f1(f: F1, a: f64) -> f64 {
    match f {
        F1::Neg => -a,
        F1::Sin => a.sin(),
        F1::Cos => a.cos(),
        F1::Tan => a.tan(),
        F1::Asin => a.asin(),
        F1::Acos => a.acos(),
        F1::Atan => a.atan(),
        F1::Sinh => a.sinh(),
        F1::Cosh => a.cosh(),
        F1::Tanh => a.tanh(),
        F1::Exp => a.exp(),
        F1::Ln => a.ln(),
        F1::Log10 => a.log10(),
        F1::Sqrt => a.sqrt(),
        F1::Abs => a.abs(),
        F1::Sign => {
            if a > 0.0 {
                1.0
            } else if a < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        F1::Floor => a.floor(),
        F1::Ceil => a.ceil(),
        F1::Round => a.round(),
        F1::Trunc => a.trunc(),
        F1::Step => (a >= 0.0) as i32 as f64,
        F1::Not => (a == 0.0) as i32 as f64,
        F1::Opaque => a,
    }
}

#[inline]
fn d1(f: F1, a: f64, y: f64) -> f64 {
    match f {
        F1::Neg => -1.0,
        F1::Sin => a.cos(),
        F1::Cos => -a.sin(),
        F1::Tan => 1.0 + y * y,
        F1::Asin => 1.0 / (1.0 - a * a).sqrt(),
        F1::Acos => -1.0 / (1.0 - a * a).sqrt(),
        F1::Atan => 1.0 / (1.0 + a * a),
        F1::Sinh => a.cosh(),
        F1::Cosh => a.sinh(),
        F1::Tanh => 1.0 - y * y,
        F1::Exp => y,
        F1::Ln => 1.0 / a,
        F1::Log10 => 1.0 / (a * std::f64::consts::LN_10),
        F1::Sqrt => 0.5 / y,
        F1::Abs => {
            if a >= 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        F1::Opaque => 1.0,
        _ => 0.0,
    }
}

#[inline]
pub fn f2(f: F2, a: f64, b: f64) -> f64 {
    let t = |c: bool| c as i32 as f64;
    match f {
        F2::Add => a + b,
        F2::Sub => a - b,
        F2::Mul => a * b,
        F2::Div => a / b,
        F2::Pow => pow(a, b),
        F2::Atan2 => a.atan2(b),
        F2::Min => a.min(b),
        F2::Max => a.max(b),
        F2::Mod => a.rem_euclid(b),
        F2::Lt => t(a < b),
        F2::Le => t(a <= b),
        F2::Gt => t(a > b),
        F2::Ge => t(a >= b),
        F2::Eq => t(a == b),
        F2::Ne => t(a != b),
        F2::And => t(a != 0.0 && b != 0.0),
        F2::Or => t(a != 0.0 || b != 0.0),
    }
}

#[inline]
fn pow(a: f64, b: f64) -> f64 {
    if b.fract() == 0.0 && b.abs() < 64.0 {
        a.powi(b as i32)
    } else {
        a.powf(b)
    }
}

#[inline]
fn d2(f: F2, a: f64, b: f64, y: f64) -> (f64, f64) {
    match f {
        F2::Add => (1.0, 1.0),
        F2::Sub => (1.0, -1.0),
        F2::Mul => (b, a),
        F2::Div => (1.0 / b, -a / (b * b)),
        F2::Pow => {
            let da = if b == 0.0 { 0.0 } else { b * pow(a, b - 1.0) };
            let db = if a > 0.0 { y * a.ln() } else { 0.0 };
            (da, db)
        }
        F2::Atan2 => {
            let r = a * a + b * b;
            (b / r, -a / r)
        }
        F2::Min => {
            if a <= b {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        }
        F2::Max => {
            if a >= b {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        }
        F2::Mod => (1.0, -(a / b).floor()),
        _ => (0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grad_matches_fd() {
        // r = x0^2 * sin(x1) - exp(x0/x1)
        let mut t = Tape::default();
        let x0 = t.push(Op::Var(0));
        let x1 = t.push(Op::Var(1));
        let two = t.constant(2.0);
        let p = t.push(Op::F2(F2::Pow, x0, two));
        let s = t.push(Op::F1(F1::Sin, x1));
        let m = t.push(Op::F2(F2::Mul, p, s));
        let d = t.push(Op::F2(F2::Div, x0, x1));
        let e = t.push(Op::F1(F1::Exp, d));
        let r = t.push(Op::F2(F2::Sub, m, e));
        let x = [1.3, 0.7];
        let mut sc = Scratch::default();
        let mut g = [0.0; 2];
        let v = t.eval_grad(r, &x, &[0, 1], &mut g, &mut sc).unwrap();
        for k in 0..2 {
            let mut xp = x;
            xp[k] += 1e-6;
            let vp = t.eval(r, &xp, &mut sc).unwrap();
            assert!(((vp - v) / 1e-6 - g[k]).abs() < 1e-4, "k={k}");
        }
    }
}
