//! External (non-algebraic) functions: the [`ExtFn`] trait, user-defined
//! `Function`/`Procedure` blocks and their interpreter.

use crate::ast::FuncDef;
use crate::tape::{Op, Scratch, Tape};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

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
}

/// A single-root tape.
#[derive(Debug, Clone)]
pub struct Expr1 {
    pub tape: Tape,
    pub root: u32,
}

#[derive(Debug)]
pub struct CompiledFunc {
    pub name: String,
    pub nparams: usize,
    pub nslots: usize,
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
}

enum Flow {
    Normal,
    Return,
}

impl Interp {
    fn ev(&mut self, e: &Expr1, locals: &[f64]) -> Result<f64, String> {
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
    let mut it = Interp { steps: 0, scratch: Scratch::default() };
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
    Expr1 { tape: t, root: r }
}
