//! External (non-algebraic) functions: the [`ExtFn`] trait, user-defined
//! `Function`/`Procedure` blocks and their interpreter.

use crate::ast::FuncDef;
use crate::tape::{Op, Scratch, Tape};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// What an argument of an external function must physically be. A variable
/// passed directly as such an argument can only take values in that range
/// at any valid solution, which the solver uses for safe automatic bounds
/// and starting values (see `solver::auto_start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ArgKind {
    /// Thermodynamic (absolute) temperature: > 0 K.
    AbsTemp,
    /// Absolute pressure: > 0 Pa.
    AbsPressure,
    /// Vapour quality or relative humidity: 0 ..= 1.
    Fraction,
    /// Density: > 0.
    Positive,
}

/// A scalar function of numeric arguments evaluated outside the tape.
///
/// Implementations must be thread-safe: parametric studies evaluate models
/// on several threads simultaneously.
pub trait ExtFn: Send + Sync {
    fn name(&self) -> &str;
    /// Physical dimensions of the result and of each argument, if known
    /// (used for unit inference).
    fn dims(&self) -> Option<(crate::units::Dim, Vec<crate::units::Dim>)> {
        None
    }
    fn eval(&self, args: &[f64]) -> Result<f64, String>;
    /// Physical range of each argument, if restricted (empty: unknown).
    fn arg_kinds(&self) -> Vec<Option<ArgKind>> {
        Vec::new()
    }
    /// Value and gradient. The default uses central finite differences with
    /// a relative step of 1e-6 (Dennis & Schnabel 1996, §5.6).
    fn eval_grad(&self, args: &[f64], grad: &mut [f64]) -> Result<f64, String> {
        let y = self.eval(args)?;
        fd_grad(self, args, grad)?;
        Ok(y)
    }
}

pub fn fd_grad<F: ExtFn + ?Sized>(f: &F, args: &[f64], grad: &mut [f64]) -> Result<(), String> {
    let mut a = args.to_vec();
    for k in 0..args.len() {
        let x = args[k];
        let h = 1e-6 * x.abs().max(1e-6);
        a[k] = x + h;
        let fp = f.eval(&a);
        a[k] = x - h;
        let fm = f.eval(&a);
        a[k] = x;
        grad[k] = match (fp, fm) {
            (Ok(p), Ok(m)) => (p - m) / (2.0 * h),
            (Ok(p), Err(_)) => (p - f.eval(args)?) / h,
            (Err(_), Ok(m)) => (f.eval(args)? - m) / h,
            (Err(e), Err(_)) => return Err(e),
        };
    }
    Ok(())
}

// ------------------------------------------------------------------ user functions

/// Compiled function statement. Expressions are tapes whose `Var(k)` refer to
/// local slot `k`.
#[derive(Debug, Clone)]
pub enum IStmt {
    Assign(u32, Expr1),
    If(Expr1, Vec<IStmt>, Vec<IStmt>),
    Repeat(Vec<IStmt>, Expr1),
    While(Expr1, Vec<IStmt>),
    For {
        slot: u32,
        from: Expr1,
        to: Expr1,
        step: Option<Expr1>,
        body: Vec<IStmt>,
    },
    /// Call a procedure: (fn cell, inputs, output slots)
    Call(Arc<FuncCell>, Vec<Expr1>, Vec<u32>),
    Return,
    /// `Call Error('message XXXF2', x)`: stop with the message
    Error(String, Option<Expr1>),
    /// `a[i, j] := value` on a local array
    AssignIdx(u32, Vec<Expr1>, Expr1),
}

/// Read of a local array element into a slot, done before the expression
/// that uses it is evaluated.
#[derive(Debug, Clone)]
pub struct Load {
    pub slot: u32,
    pub array: u32,
    pub name: String,
    pub idx: Vec<Expr1>,
}

/// EES error message: `XXX` (optionally `XXXF2`, `XXXE3`, `XXXA`) is
/// replaced by the value, with 2 decimals, in exponent form with 3, or as is.
pub fn error_message(msg: &str, value: Option<f64>) -> String {
    let Some(v) = value else { return msg.to_string() };
    let Some(k) = msg.find("XXX") else { return format!("{msg} ({v})") };
    let rest = &msg[k + 3..];
    let kind = rest.chars().next().filter(|c| matches!(c.to_ascii_uppercase(), 'F' | 'E' | 'A'));
    let digits: String = rest.chars().skip(kind.is_some() as usize).take_while(|c| c.is_ascii_digit()).collect();
    let used = kind.map_or(0, |_| 1 + digits.len());
    let n: usize = digits.parse().unwrap_or(3);
    let text = match kind.map(|c| c.to_ascii_uppercase()) {
        Some('F') => format!("{v:.n$}"),
        Some('E') => format!("{v:.n$e}"),
        _ => format!("{v}"),
    };
    format!("{}{text}{}", &msg[..k], &rest[used..])
}

/// A single-root tape.
#[derive(Debug, Clone)]
pub struct Expr1 {
    pub tape: Tape,
    pub root: u32,
    pub loads: Vec<Load>,
}

#[derive(Debug)]
pub struct CompiledFunc {
    pub name: String,
    pub nparams: usize,
    pub nslots: usize,
    /// Local arrays (`a[i] := ...` in the body)
    pub narrays: usize,
    /// Slots holding the results (function: its own name; procedure: outputs).
    pub out_slots: Vec<u32>,
    pub body: Vec<IStmt>,
}

/// Late-bound function cell so functions can call each other in any order.
#[derive(Debug, Default)]
pub struct FuncCell {
    pub name: String,
    pub def: OnceLock<CompiledFunc>,
    pub source: Option<FuncDef>,
}

impl std::fmt::Debug for dyn ExtFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExtFn({})", self.name())
    }
}

const MAX_STEPS: usize = 10_000_000;

struct Interp {
    steps: usize,
    scratch: Scratch,
    /// Local arrays: element (indices) → value
    arrays: Vec<HashMap<Vec<i64>, f64>>,
}

fn index_of(v: f64) -> Result<i64, String> {
    if (v - v.round()).abs() > 1e-9 || !v.is_finite() {
        return Err(format!("array index {v} is not an integer"));
    }
    Ok(v.round() as i64)
}

enum Flow {
    Normal,
    Return,
}

impl Interp {
    fn ev(&mut self, e: &Expr1, locals: &mut [f64]) -> Result<f64, String> {
        for l in &e.loads {
            let mut key = Vec::with_capacity(l.idx.len());
            for i in &l.idx {
                key.push(index_of(self.ev(i, locals)?)?);
            }
            let v = self.arrays[l.array as usize].get(&key).copied();
            let shown = key.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",");
            locals[l.slot as usize] = v.ok_or_else(|| format!("{} has no value", l.name.replace("...", &shown)))?;
        }
        e.tape.eval(e.root, locals, &mut self.scratch).map_err(|e| e.0)
    }
    fn run(&mut self, body: &[IStmt], locals: &mut Vec<f64>) -> Result<Flow, String> {
        for s in body {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return Err("function exceeded the iteration limit (infinite loop?)".into());
            }
            match s {
                IStmt::Assign(slot, e) => {
                    let v = self.ev(e, locals)?;
                    locals[*slot as usize] = v;
                }
                IStmt::If(c, t, f) => {
                    let flow = if self.ev(c, locals)? != 0.0 { self.run(t, locals)? } else { self.run(f, locals)? };
                    if let Flow::Return = flow {
                        return Ok(Flow::Return);
                    }
                }
                IStmt::Repeat(b, until) => loop {
                    if let Flow::Return = self.run(b, locals)? {
                        return Ok(Flow::Return);
                    }
                    if self.ev(until, locals)? != 0.0 {
                        break;
                    }
                    self.steps += 1;
                    if self.steps > MAX_STEPS {
                        return Err("Repeat loop exceeded the iteration limit".into());
                    }
                },
                IStmt::While(c, b) => {
                    while self.ev(c, locals)? != 0.0 {
                        if let Flow::Return = self.run(b, locals)? {
                            return Ok(Flow::Return);
                        }
                        self.steps += 1;
                        if self.steps > MAX_STEPS {
                            return Err("While loop exceeded the iteration limit".into());
                        }
                    }
                }
                IStmt::For { slot, from, to, step, body } => {
                    let a = self.ev(from, locals)?;
                    let b = self.ev(to, locals)?;
                    let st = match step {
                        Some(s) => self.ev(s, locals)?,
                        None => 1.0,
                    };
                    if st == 0.0 {
                        return Err("For loop with zero step".into());
                    }
                    let mut i = a;
                    while (st > 0.0 && i <= b + 1e-12) || (st < 0.0 && i >= b - 1e-12) {
                        locals[*slot as usize] = i;
                        if let Flow::Return = self.run(body, locals)? {
                            return Ok(Flow::Return);
                        }
                        i += st;
                    }
                }
                IStmt::AssignIdx(a, idx, e) => {
                    let v = self.ev(e, locals)?;
                    let mut key = Vec::with_capacity(idx.len());
                    for i in idx {
                        key.push(index_of(self.ev(i, locals)?)?);
                    }
                    self.arrays[*a as usize].insert(key, v);
                }
                IStmt::Error(msg, x) => {
                    let v = match x {
                        Some(e) => Some(self.ev(e, locals)?),
                        None => None,
                    };
                    return Err(error_message(msg, v));
                }
                IStmt::Call(cell, ins, outs) => {
                    let mut args = Vec::with_capacity(ins.len());
                    for e in ins {
                        args.push(self.ev(e, locals)?);
                    }
                    let r = call_func(cell, &args)?;
                    for (k, slot) in outs.iter().enumerate() {
                        locals[*slot as usize] = r[k];
                    }
                }
                IStmt::Return => return Ok(Flow::Return),
            }
        }
        Ok(Flow::Normal)
    }
}

thread_local! {
    static PROC_CACHE: RefCell<HashMap<usize, (Vec<f64>, Vec<f64>)>> = RefCell::new(HashMap::new());
}

/// Run a compiled function/procedure and return all its outputs.
pub fn call_func(cell: &Arc<FuncCell>, args: &[f64]) -> Result<Vec<f64>, String> {
    let f = cell.def.get().ok_or_else(|| format!("function '{}' is not compiled", cell.name))?;
    if args.len() != f.nparams {
        return Err(format!("'{}' expects {} arguments, got {}", f.name, f.nparams, args.len()));
    }
    let key = Arc::as_ptr(cell) as usize;
    if let Some(hit) = PROC_CACHE.with(|c| c.borrow().get(&key).filter(|(a, _)| a.as_slice() == args).map(|(_, o)| o.clone())) {
        return Ok(hit);
    }
    let mut locals = vec![f64::NAN; f.nslots];
    locals[..args.len()].copy_from_slice(args);
    let mut it = Interp { steps: 0, scratch: Scratch::default(), arrays: vec![HashMap::new(); f.narrays] };
    it.run(&f.body, &mut locals)?;
    let out: Vec<f64> = f.out_slots.iter().map(|s| locals[*s as usize]).collect();
    if let Some(k) = out.iter().position(|v| v.is_nan()) {
        return Err(format!("'{}' did not assign output #{}", f.name, k + 1));
    }
    PROC_CACHE.with(|c| c.borrow_mut().insert(key, (args.to_vec(), out.clone())));
    Ok(out)
}

/// Adapter exposing output `index` of a user function as an [`ExtFn`].
pub struct UserFn {
    pub cell: Arc<FuncCell>,
    pub index: usize,
    pub label: String,
}

impl ExtFn for UserFn {
    fn name(&self) -> &str {
        &self.label
    }
    fn eval(&self, args: &[f64]) -> Result<f64, String> {
        Ok(call_func(&self.cell, args)?[self.index])
    }
}

/// Helper used by tests and by the compiler for trivial tapes.
pub fn single(op_root: Op) -> Expr1 {
    let mut t = Tape::default();
    let r = t.push(op_root);
    Expr1 { tape: t, root: r, loads: Vec::new() }
}
