//! Lowering of the AST into a flat [`Model`]: `Duplicate` blocks and `Sum`
//! are unrolled, array elements become scalar variables (`T[3]`), numeric
//! literals with units are converted to the model unit system, and each
//! equation becomes a differentiable tape of its residual `lhs - rhs`.

use crate::ast::*;
use crate::funcs::{CompiledFunc, Expr1, FuncCell, IStmt, UserFn};
use crate::lexer::Span;
use crate::parser::{canon, parse, Diagnostic, Severity};
use crate::props::{self, Key, PropCall, RawCall, RawKind};
use crate::tape::{Op, Tape, VarId, F1, F2};
use crate::units::{parse_unit, Dim, Unit, UnitSystem};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct VarInfo {
    /// Canonical key, e.g. `t[3]`.
    pub name: String,
    /// As first written by the user, e.g. `T[3]`.
    pub display: String,
    /// Canonical array base (`t`) — equals `name` for scalars.
    pub base: String,
    pub index: Vec<i64>,
    /// Unit hint from a literal (`= 10 [bar]`), a trailing `{kJ/kg}` comment
    /// or a `$Units` directive.
    pub unit_hint: Option<String>,
    /// First line where the variable appears.
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Equation {
    pub tape: Tape,
    pub root: u32,
    pub lhs: u32,
    pub rhs: u32,
    pub vars: Vec<VarId>,
    /// Newton form of the residual with top-level denominators cleared
    /// (`a = b/c` -> `a*c - b`), which removes poles and makes many
    /// implicit equations linear in their unknown. Same as `root` when no
    /// division is present.
    pub nroot: u32,
    pub nlhs: u32,
    pub nrhs: u32,
    /// Top-level additive terms of both sides (original / Newton form);
    /// the residual is scaled by the largest term magnitude so that
    /// balances such as `0 = m1*h1 - m2*h2` get a meaningful relative error.
    pub terms: Vec<u32>,
    pub nterms: Vec<u32>,
    /// Dimensions of literals written with a unit (`10 [bar]`).
    pub const_dims: Vec<(u32, Dim)>,
    pub span: Span,
    /// Source text (after unrolling, loop indices are shown in `label`).
    pub text: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub vars: Vec<VarInfo>,
    pub var_index: HashMap<String, VarId>,
    pub eqs: Vec<Equation>,
    pub unit_system: UnitSystem,
    pub guesses: HashMap<VarId, f64>,
    pub bounds: HashMap<VarId, (f64, f64)>,
    pub display_units: HashMap<VarId, String>,
    pub diagnostics: Vec<Diagnostic>,
    pub funcs: Vec<Arc<FuncCell>>,
    /// `$Control` loops (vary, target equation)
    pub controls: Vec<ControlSpec>,
    /// Inferred physical dimension of every variable (see `dims.rs`).
    pub var_dims: Vec<Option<Dim>>,
    /// Equations whose additive terms have inconsistent dimensions.
    pub unit_conflicts: Vec<u32>,
    /// Physical range implied by the use of each variable as a property
    /// argument (`P=P[1]`: absolute pressure; `x=x[2]`: quality, ...).
    pub var_kinds: Vec<Option<crate::funcs::ArgKind>>,
    /// Variables whose entry in `bounds` was set automatically from
    /// `var_kinds` (not by `$Bounds`).
    pub auto_bounds: std::collections::HashSet<VarId>,
}

#[derive(Debug, Clone)]
pub struct ControlSpec {
    pub vary: Vec<String>,
    pub target: String,
    pub span: Span,
}

impl Model {
    pub fn var(&self, name: &str) -> Option<VarId> {
        self.var_index.get(&canon(name)).copied()
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
    /// Unit in which a variable is displayed and the conversion from its
    /// model value.
    pub fn display_unit(&self, v: VarId) -> Option<(Unit, Unit)> {
        if let Some(s) = self.display_units.get(&v).or(self.vars[v as usize].unit_hint.as_ref()) {
            if let Ok(du) = parse_unit(s) {
                let mu = self.unit_system.unit_for(du.dim);
                return Some((mu, du));
            }
        }
        let d = (*self.var_dims.get(v as usize)?)?;
        let mu = self.unit_system.unit_for(d);
        Some((mu.clone(), mu))
    }
    /// Dimension of a variable (declared or inferred).
    pub fn var_dim(&self, v: VarId) -> Option<Dim> {
        if let Some(s) = self.display_units.get(&v).or(self.vars[v as usize].unit_hint.as_ref()) {
            if let Ok(du) = parse_unit(s) {
                return Some(du.dim);
            }
        }
        *self.var_dims.get(v as usize)?
    }
}

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub unit_system: Option<UnitSystem>,
    /// Validate fluid names against CoolProp at compile time.
    pub check_fluids: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions { unit_system: None, check_fluids: true }
    }
}

const MAX_UNROLL: i64 = 1_000_000;

struct Builder<'s> {
    src: &'s str,
    m: Model,
    consts: HashMap<String, f64>,
    func_map: HashMap<String, Arc<FuncCell>>,
    opts: CompileOptions,
    /// `a = b - c/d [bar]`: (equation index, unit, span)
    eq_units: Vec<(usize, String, Span)>,
}

fn err(msg: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic { msg: msg.into(), span, severity: Severity::Error }
}

/// Parse and compile a script.
pub fn compile(src: &str, opts: &CompileOptions) -> Model {
    let pr = parse(src);
    compile_program(src, &pr.program, pr.diagnostics, opts)
}

pub fn compile_program(src: &str, prog: &Program, diags: Vec<Diagnostic>, opts: &CompileOptions) -> Model {
    let mut b = Builder { src, m: Model::default(), consts: HashMap::new(), func_map: HashMap::new(), opts: opts.clone(), eq_units: Vec::new() };
    b.m.diagnostics = diags;
    b.m.unit_system = opts.unit_system.clone().unwrap_or_default();
    // 1. unit system directive (affects literal conversion)
    for s in &prog.stmts {
        if let Stmt::Directive { name, args, span } = s {
            if name == "unitsystem" {
                match UnitSystem::from_directive(args) {
                    Ok(us) => b.m.unit_system = us,
                    Err(e) => b.m.diagnostics.push(err(e, *span)),
                }
            }
        }
    }
    // 2. register functions (late-bound so they may call each other)
    for f in &prog.funcs {
        if b.func_map.contains_key(&f.name) {
            b.m.diagnostics.push(err(format!("function '{}' defined twice", f.display), f.span));
            continue;
        }
        let cell = Arc::new(FuncCell { name: f.display.clone(), def: Default::default(), source: Some(f.clone()) });
        b.func_map.insert(f.name.clone(), cell.clone());
        b.m.funcs.push(cell);
    }
    for f in &prog.funcs {
        if let Err(d) = b.compile_func(f) {
            b.m.diagnostics.push(d);
        }
    }
    // 3. constants usable in indices / Duplicate bounds
    b.discover_consts(&prog.stmts);
    // 4. equations
    let env = HashMap::new();
    for s in &prog.stmts {
        b.stmt(s, &env);
    }
    // 5. remaining directives
    for s in &prog.stmts {
        if let Stmt::Directive { name, args, span } = s {
            b.directive(name, args, *span);
        }
    }
    // 5b. equation units: the unit belongs to the variable the equation
    // computes, from the same matching the solver uses (no fixed variables)
    if !b.eq_units.is_empty() {
        let ev: Vec<&[VarId]> = b.m.eqs.iter().map(|e| e.vars.as_slice()).collect();
        let st = crate::structure::analyze(b.m.vars.len(), &ev, &vec![false; b.m.vars.len()]);
        for (e, u, span) in std::mem::take(&mut b.eq_units) {
            match st.eq_match.get(e).copied().flatten() {
                Some(v) => b.m.vars[v as usize].unit_hint = Some(u),
                None => b.m.diagnostics.push(Diagnostic {
                    msg: format!("[{u}]: this equation does not compute a variable (it checks or over-specifies the model); the unit is ignored"),
                    span,
                    severity: Severity::Warning,
                }),
            }
        }
    }
    // 6. unit inference
    let (dims, conflicts) = crate::dims::infer(&b.m);
    b.m.var_dims = dims;
    b.m.unit_conflicts = conflicts;
    // 7. physical ranges from property arguments, and safe bounds from them
    b.m.var_kinds = var_kinds(&b.m);
    for (v, k) in b.m.var_kinds.iter().enumerate() {
        let v = v as VarId;
        if let Some(k) = k {
            if !b.m.bounds.contains_key(&v) {
                b.m.bounds.insert(v, crate::solver::kind_bounds(*k, &b.m.unit_system));
                b.m.auto_bounds.insert(v);
            }
        }
    }
    b.m
}

/// Range implied for each variable that is passed directly (not inside an
/// expression) as an argument of a property function. At any solution the
/// call must evaluate, so the variable lies in that range: bounds derived
/// from it never exclude a valid solution. Conflicting uses give no kind.
fn var_kinds(m: &Model) -> Vec<Option<crate::funcs::ArgKind>> {
    let mut kinds: Vec<Option<Option<crate::funcs::ArgKind>>> = vec![None; m.vars.len()];
    for eq in &m.eqs {
        let t = &eq.tape;
        for op in &t.ops {
            let Op::Ext(f, s, l) = op else { continue };
            let ak = f.arg_kinds();
            for (i, k) in ak.iter().enumerate().take(*l as usize) {
                let Some(k) = k else { continue };
                let node = t.args[(*s as usize) + i] as usize;
                if let Op::Var(v) = t.ops[node] {
                    let slot = &mut kinds[v as usize];
                    *slot = match *slot {
                        None => Some(Some(*k)),
                        Some(Some(prev)) if prev == *k => Some(Some(prev)),
                        _ => Some(None),
                    };
                }
            }
        }
    }
    kinds.into_iter().map(|k| k.flatten()).collect()
}

impl<'s> Builder<'s> {
    // ------------------------------------------------------------ constants
    fn discover_consts(&mut self, stmts: &[Stmt]) {
        loop {
            let mut changed = false;
            for s in stmts {
                if let Stmt::Equation { lhs, rhs, .. } = s {
                    for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                        if let Expr::Var { .. } = a {
                            if let Ok(key) = self.var_key(a, &HashMap::new()) {
                                if !self.consts.contains_key(&key.0) {
                                    if let Some(v) = self.const_eval(b, &HashMap::new()) {
                                        self.consts.insert(key.0, v);
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn literal(&self, value: f64, unit: &Option<String>, span: Span) -> Result<f64, Diagnostic> {
        match unit {
            None => Ok(value),
            Some(u) => {
                let un = parse_unit(u).map_err(|e| err(e, span))?;
                if un.dim.is_none() {
                    return Ok(un.to_si(value));
                }
                let target = self.m.unit_system.unit_for(un.dim);
                Ok(un.convert(value, &target))
            }
        }
    }

    /// Evaluate an expression that must be a compile-time constant.
    fn const_eval(&self, e: &Expr, env: &HashMap<String, f64>) -> Option<f64> {
        Some(match e {
            Expr::Num { value, unit, span } => self.literal(*value, unit, *span).ok()?,
            Expr::Var { name, idx, .. } if idx.is_empty() => {
                if let Some(v) = env.get(name) {
                    *v
                } else if let Some(c) = builtin_const(name, &self.m.unit_system) {
                    c
                } else {
                    *self.consts.get(name)?
                }
            }
            Expr::Var { .. } => {
                let (k, ..) = self.var_key(e, env).ok()?;
                *self.consts.get(&k)?
            }
            Expr::Neg(a, _) => -self.const_eval(a, env)?,
            Expr::Bin(op, a, b, _) => {
                let (x, y) = (self.const_eval(a, env)?, self.const_eval(b, env)?);
                match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                    BinOp::Pow => x.powf(y),
                    BinOp::And => ((x != 0.0) && (y != 0.0)) as i32 as f64,
                    BinOp::Or => ((x != 0.0) || (y != 0.0)) as i32 as f64,
                }
            }
            Expr::Call { name, args, .. } if args.iter().all(|a| a.name.is_none()) => {
                let v: Option<Vec<f64>> = args.iter().map(|a| self.const_eval(&a.value, env)).collect();
                let v = v?;
                match (name.as_str(), v.len()) {
                    ("round", 1) => v[0].round(),
                    ("floor", 1) | ("int", 1) => v[0].floor(),
                    ("ceil", 1) => v[0].ceil(),
                    ("trunc", 1) => v[0].trunc(),
                    ("abs", 1) => v[0].abs(),
                    ("sqrt", 1) => v[0].sqrt(),
                    ("min", _) if !v.is_empty() => v.iter().cloned().fold(f64::INFINITY, f64::min),
                    ("max", _) if !v.is_empty() => v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    fn int_eval(&self, e: &Expr, env: &HashMap<String, f64>) -> Result<i64, Diagnostic> {
        let v = self.const_eval(e, env).ok_or_else(|| err("index/range must be a constant (a number, a loop index or a variable set to a constant)", e.span()))?;
        if (v - v.round()).abs() > 1e-9 {
            return Err(err(format!("index must be an integer, got {v}"), e.span()));
        }
        Ok(v.round() as i64)
    }

    /// Canonical key and display name of a (possibly indexed) variable.
    fn var_key(&self, e: &Expr, env: &HashMap<String, f64>) -> Result<(String, String, String, Vec<i64>), Diagnostic> {
        let Expr::Var { name, display, idx, .. } = e else { unreachable!() };
        if idx.is_empty() {
            return Ok((name.clone(), display.clone(), name.clone(), vec![]));
        }
        let mut ix = Vec::with_capacity(idx.len());
        for i in idx {
            ix.push(self.int_eval(i, env)?);
        }
        let s = ix.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        Ok((format!("{name}[{s}]"), format!("{display}[{s}]"), name.clone(), ix))
    }

    fn intern(&mut self, key: String, display: String, base: String, index: Vec<i64>, line: u32) -> VarId {
        if let Some(&v) = self.m.var_index.get(&key) {
            return v;
        }
        let id = self.m.vars.len() as VarId;
        self.m.vars.push(VarInfo { name: key.clone(), display, base, index, unit_hint: None, line });
        self.m.var_index.insert(key, id);
        id
    }

    // ------------------------------------------------------------ statements
    fn stmt(&mut self, s: &Stmt, env: &HashMap<String, f64>) {
        match s {
            Stmt::Directive { .. } => {}
            Stmt::Equation { lhs, rhs, span, trailing_comment, unit } => {
                let n = self.m.eqs.len();
                if let Err(d) = self.equation(lhs, rhs, *span, env, trailing_comment.as_deref()) {
                    self.m.diagnostics.push(d);
                }
                if let Some(u) = unit {
                    if self.m.eqs.len() == n + 1 {
                        self.eq_units.push((n, u.clone(), *span));
                    }
                }
            }
            Stmt::Duplicate { var, from, to, body, span } => {
                let (a, b) = match (self.int_eval(from, env), self.int_eval(to, env)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(d), _) | (_, Err(d)) => {
                        self.m.diagnostics.push(d);
                        return;
                    }
                };
                if b - a > MAX_UNROLL {
                    self.m.diagnostics.push(err("Duplicate range too large", *span));
                    return;
                }
                let mut env2 = env.clone();
                for i in a..=b {
                    env2.insert(var.clone(), i as f64);
                    for st in body {
                        self.stmt(st, &env2);
                    }
                }
            }
            Stmt::Call { name, inputs, outputs, span } => {
                if let Err(d) = self.call_stmt(name, inputs, outputs, *span, env) {
                    self.m.diagnostics.push(d);
                }
            }
        }
    }

    fn equation(&mut self, lhs: &Expr, rhs: &Expr, span: Span, env: &HashMap<String, f64>, comment: Option<&str>) -> Result<(), Diagnostic> {
        let mut t = Tape::default();
        let mut lw = Lower { b: self, env: env.clone(), slots: None, const_dims: Vec::new() };
        let l = lw.expr(lhs, &mut t)?;
        let r = lw.expr(rhs, &mut t)?;
        let const_dims = std::mem::take(&mut lw.const_dims);
        let root = t.push(Op::F2(F2::Sub, l, r));
        let vars = t.vars_of(root);
        let (nlhs, nrhs, nroot) = clear_denominators(&mut t, l, r).unwrap_or((l, r, root));
        let text = self.src.get(span.start..span.end).unwrap_or("").to_string();
        let label = if env.is_empty() {
            text.clone()
        } else {
            let mut ks: Vec<_> = env.iter().collect();
            ks.sort_by(|a, b| a.0.cmp(b.0));
            format!("{text}   ({})", ks.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "))
        };
        if vars.is_empty() {
            // constant equation: check consistency
            let v = t.const_value(root).unwrap_or(f64::NAN);
            let sc = t.const_value(l).unwrap_or(0.0).abs().max(t.const_value(r).unwrap_or(0.0).abs()).max(1.0);
            if !(v.abs() <= 1e-9 * sc) {
                self.m.diagnostics.push(Diagnostic { msg: format!("equation has no variables and is not satisfied (residual {v:.4e})"), span, severity: Severity::Error });
            }
            return Ok(());
        }
        // unit hint: `x = 10 [bar]` or trailing `{bar}` comment
        let target = match (lhs, rhs) {
            (Expr::Var { .. }, _) => Some(lhs),
            (_, Expr::Var { .. }) => Some(rhs),
            _ => None,
        };
        if let Some(tv) = target {
            if let Ok((key, ..)) = self.var_key(tv, env) {
                if let Some(&id) = self.m.var_index.get(&key) {
                    let other = if std::ptr::eq(tv, lhs) { rhs } else { lhs };
                    let hint = match other {
                        Expr::Num { unit: Some(u), .. } => Some(u.clone()),
                        _ => comment.and_then(|c| {
                            let c = c.trim();
                            (!c.is_empty() && c.len() < 24 && parse_unit(c).is_ok()).then(|| c.to_string())
                        }),
                    };
                    if let Some(h) = hint {
                        let vi = &mut self.m.vars[id as usize];
                        if vi.unit_hint.is_none() {
                            vi.unit_hint = Some(h);
                        }
                    }
                }
            }
        }
        let terms = additive_terms(&t, &[l, r]);
        let nterms = additive_terms(&t, &[nlhs, nrhs]);
        self.m.eqs.push(Equation { tape: t, root, lhs: l, rhs: r, vars, nroot, nlhs, nrhs, terms, nterms, const_dims, span, text, label });
        Ok(())
    }

    fn call_stmt(&mut self, name: &str, inputs: &[Expr], outputs: &[Expr], span: Span, env: &HashMap<String, f64>) -> Result<(), Diagnostic> {
        let cell = self.func_map.get(name).cloned().ok_or_else(|| err(format!("unknown procedure '{name}'"), span))?;
        let src = cell.source.as_ref().unwrap();
        if !src.is_procedure {
            return Err(err(format!("'{}' is a Function; use it inside an expression", src.display), span));
        }
        if inputs.len() != src.params.len() || outputs.len() != src.outputs.len() {
            return Err(err(format!("'{}' expects {} inputs and {} outputs", src.display, src.params.len(), src.outputs.len()), span));
        }
        for (k, out) in outputs.iter().enumerate() {
            let mut t = Tape::default();
            let mut lw = Lower { b: self, env: env.clone(), slots: None, const_dims: Vec::new() };
            let l = lw.expr(out, &mut t)?;
            let mut args = Vec::new();
            for e in inputs {
                args.push(lw.expr(e, &mut t)?);
            }
            let f = Arc::new(UserFn { cell: cell.clone(), index: k, label: format!("{}:{}", src.display, src.outputs[k]) });
            let r = ext(&mut t, f, &args);
            let root = t.push(Op::F2(F2::Sub, l, r));
            let vars = t.vars_of(root);
            let text = self.src.get(span.start..span.end).unwrap_or("").to_string();
            let terms = vec![l, r];
            self.m.eqs.push(Equation {
                tape: t,
                root,
                lhs: l,
                rhs: r,
                vars,
                nroot: root,
                nlhs: l,
                nrhs: r,
                nterms: terms.clone(),
                terms,
                const_dims: Vec::new(),
                span,
                label: format!("{text}  [output {}]", k + 1),
                text,
            });
        }
        Ok(())
    }

    // ------------------------------------------------------------ functions
    fn compile_func(&mut self, f: &FuncDef) -> Result<(), Diagnostic> {
        let mut slots: HashMap<String, u32> = HashMap::new();
        for p in &f.params {
            let n = slots.len() as u32;
            slots.insert(p.clone(), n);
        }
        let out_names: Vec<String> = if f.is_procedure { f.outputs.clone() } else { vec![f.name.clone()] };
        for o in &out_names {
            if !slots.contains_key(o) {
                let n = slots.len() as u32;
                slots.insert(o.clone(), n);
            }
        }
        let mut lw = Lower { b: self, env: HashMap::new(), slots: Some(slots), const_dims: Vec::new() };
        let body = lw.fblock(&f.body)?;
        let slots = lw.slots.take().unwrap();
        let out_slots = out_names.iter().map(|o| slots[o]).collect();
        let cell = self.func_map[&f.name].clone();
        let _ = cell.def.set(CompiledFunc { name: f.display.clone(), nparams: f.params.len(), nslots: slots.len(), out_slots, body });
        Ok(())
    }

    // ------------------------------------------------------------ directives
    fn directive(&mut self, name: &str, args: &str, span: Span) {
        match name {
            "unitsystem" => {}
            "guess" => {
                for item in split_top(args) {
                    match self.assign_item(&item, span) {
                        Ok((ids, v)) => {
                            for id in ids {
                                self.m.guesses.insert(id, v);
                            }
                        }
                        Err(d) => self.m.diagnostics.push(d),
                    }
                }
            }
            "bounds" | "limits" => {
                for item in split_top(args) {
                    let Some((lhs, rhs)) = item.split_once('=') else {
                        self.m.diagnostics.push(err("expected `$Bounds x = lo .. hi`", span));
                        continue;
                    };
                    let Some((lo, hi)) = rhs.split_once("..") else {
                        self.m.diagnostics.push(err("expected `lo .. hi` in $Bounds", span));
                        continue;
                    };
                    let parse_num = |s: &str, b: &Builder| -> Option<f64> {
                        let s = s.trim();
                        match s.to_lowercase().as_str() {
                            "inf" | "+inf" | "infinity" => return Some(f64::INFINITY),
                            "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
                            _ => {}
                        }
                        let pr = parse(&format!("__x = {s}"));
                        if let Some(Stmt::Equation { rhs, .. }) = pr.program.stmts.first() {
                            b.const_eval(rhs, &HashMap::new())
                        } else {
                            None
                        }
                    };
                    let (Some(l), Some(h)) = (parse_num(lo, self), parse_num(hi, self)) else {
                        self.m.diagnostics.push(err("bounds must be constants", span));
                        continue;
                    };
                    for id in self.match_vars(lhs.trim()) {
                        self.m.bounds.insert(id, (l, h));
                    }
                }
            }
            "units" => {
                for item in split_top(args) {
                    let Some((lhs, u)) = item.split_once('=') else {
                        self.m.diagnostics.push(err("expected `$Units x = unit`", span));
                        continue;
                    };
                    if let Err(e) = parse_unit(u.trim()) {
                        self.m.diagnostics.push(err(e, span));
                        continue;
                    }
                    let ids = self.match_vars(lhs.trim());
                    if ids.is_empty() {
                        self.m.diagnostics.push(Diagnostic { msg: format!("$Units: no variable matches '{}'", lhs.trim()), span, severity: Severity::Warning });
                    }
                    for id in ids {
                        self.m.display_units.insert(id, u.trim().to_string());
                    }
                }
            }
            "control" => {
                // $Control vary a, b to make expr = expr
                let l = args.to_lowercase();
                let (Some(v0), Some(t0)) = (l.find("vary"), l.find(" to make ")) else {
                    self.m.diagnostics.push(err("expected `$Control vary x[, y] to make expr = target`", span));
                    return;
                };
                let vary = args[v0 + 4..t0].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let target = args[t0 + 9..].trim().to_string();
                self.m.controls.push(ControlSpec { vary, target, span });
            }
            "include" | "tabstops" | "arrays" | "warnings" | "checkunits" | "complex" | "uniterrors" | "ifnot" | "endif" | "if" | "common" | "reference" | "private" | "local" => {
                self.m.diagnostics.push(Diagnostic { msg: format!("${name} is ignored"), span, severity: Severity::Info });
            }
            other => self.m.diagnostics.push(Diagnostic { msg: format!("unknown directive ${other}"), span, severity: Severity::Warning }),
        }
    }

    /// `name = value` item in `$Guess`.
    fn assign_item(&self, item: &str, span: Span) -> Result<(Vec<VarId>, f64), Diagnostic> {
        let (lhs, rhs) = item.split_once('=').ok_or_else(|| err("expected `name = value`", span))?;
        let pr = parse(&format!("__x = {}", rhs.trim()));
        let v = match pr.program.stmts.first() {
            Some(Stmt::Equation { rhs, .. }) => self.const_eval(rhs, &HashMap::new()),
            _ => None,
        }
        .ok_or_else(|| err(format!("'{}' is not a constant", rhs.trim()), span))?;
        let ids = self.match_vars(lhs.trim());
        if ids.is_empty() {
            return Err(Diagnostic { msg: format!("no variable matches '{}'", lhs.trim()), span, severity: Severity::Warning });
        }
        Ok((ids, v))
    }

    /// Variables matching `name`, `name[3]` or `name[*]`.
    fn match_vars(&self, pat: &str) -> Vec<VarId> {
        let p = canon(pat).replace(' ', "");
        if let Some(base) = p.strip_suffix("[*]") {
            return self.m.vars.iter().enumerate().filter(|(_, v)| v.base == base && !v.index.is_empty()).map(|(i, _)| i as VarId).collect();
        }
        self.m.var_index.get(&p).map(|v| vec![*v]).unwrap_or_default()
    }
}

fn additive_terms(t: &Tape, roots: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut st: Vec<u32> = roots.to_vec();
    while let Some(n) = st.pop() {
        match t.ops[n as usize] {
            Op::F2(F2::Add | F2::Sub, a, b) => {
                st.push(a);
                st.push(b)
            }
            Op::F1(F1::Neg, a) => st.push(a),
            _ => out.push(n),
        }
    }
    out
}

/// `a/b = c/d` -> `a*d = c*b` when the denominators depend on variables.
fn clear_denominators(t: &mut Tape, l: u32, r: u32) -> Option<(u32, u32, u32)> {
    let div = |t: &Tape, n: u32| match t.ops[n as usize] {
        Op::F2(F2::Div, a, b) if !t.vars_of(b).is_empty() => Some((a, b)),
        _ => None,
    };
    let (dl, dr) = (div(t, l), div(t, r));
    let (nl, nr) = match (dl, dr) {
        (None, None) => return None,
        (Some((a, b)), None) => (a, t.push(Op::F2(F2::Mul, r, b))),
        (None, Some((c, d))) => (t.push(Op::F2(F2::Mul, l, d)), c),
        (Some((a, b)), Some((c, d))) => (t.push(Op::F2(F2::Mul, a, d)), t.push(Op::F2(F2::Mul, c, b))),
    };
    let root = t.push(Op::F2(F2::Sub, nl, nr));
    Some((nl, nr, root))
}

fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Built-in constants. Physical constants follow CODATA 2018 and are
/// returned in the model unit system.
pub fn builtin_const(name: &str, us: &UnitSystem) -> Option<f64> {
    let conv = |v_si: f64, dim: Dim| us.unit_for(dim).from_si(v_si);
    Some(match name {
        "pi" | "pi#" => std::f64::consts::PI,
        "e#" => std::f64::consts::E,
        "g#" => 9.80665,
        "r#" => conv(8.314462618, Dim::SPEC_ENTROPY.mul(Dim::MOLAR_MASS)),
        "sigma#" => 5.670374419e-8,
        "k#" => 1.380649e-23,
        "n_a#" | "na#" => 6.02214076e23,
        "h#" => 6.62607015e-34,
        "c#" => 299792458.0,
        "true" => 1.0,
        "false" => 0.0,
        _ => return None,
    })
}

/// Dimension of the built-in constants that carry one, for the unit check
/// and the inferred display units: R#/molarmass(N2) is a specific entropy.
pub fn builtin_const_dim(name: &str) -> Option<Dim> {
    match name {
        "r#" => Some(Dim::MOLAR_ENTROPY),
        _ => None,
    }
}

pub const BUILTIN_CONSTS: &[(&str, &str)] = &[
    ("pi", "π = 3.14159…"),
    ("g#", "Standard gravity 9.80665 m/s²"),
    ("R#", "Universal gas constant 8.314462618 J/mol-K, in model units (8.314 kJ/kmol-K in Engineering)"),
    ("sigma#", "Stefan–Boltzmann constant 5.670374419e-8 W/m²-K⁴"),
    ("k#", "Boltzmann constant 1.380649e-23 J/K"),
    ("N_A#", "Avogadro constant 6.02214076e23 1/mol"),
    ("h#", "Planck constant 6.62607015e-34 J-s"),
    ("c#", "Speed of light 299792458 m/s"),
];

fn ext(t: &mut Tape, f: Arc<dyn crate::funcs::ExtFn>, args: &[u32]) -> u32 {
    let st = t.args.len() as u32;
    t.args.extend_from_slice(args);
    t.push(Op::Ext(f, st, args.len() as u32))
}

pub fn math_f1(name: &str) -> Option<F1> {
    Some(match name {
        "sin" => F1::Sin,
        "cos" => F1::Cos,
        "tan" => F1::Tan,
        "asin" | "arcsin" => F1::Asin,
        "acos" | "arccos" => F1::Acos,
        "atan" | "arctan" => F1::Atan,
        "sinh" => F1::Sinh,
        "cosh" => F1::Cosh,
        "tanh" => F1::Tanh,
        "exp" => F1::Exp,
        "ln" | "log" => F1::Ln,
        "log10" => F1::Log10,
        "sqrt" => F1::Sqrt,
        "abs" => F1::Abs,
        "sign" => F1::Sign,
        "floor" | "int" => F1::Floor,
        "ceil" => F1::Ceil,
        "round" => F1::Round,
        "trunc" => F1::Trunc,
        "step" => F1::Step,
        _ => return None,
    })
}

pub const MATH_FUNCTIONS: &[(&str, &str)] = &[
    ("sin", "sin(x)"),
    ("cos", "cos(x)"),
    ("tan", "tan(x)"),
    ("asin", "asin(x)"),
    ("acos", "acos(x)"),
    ("atan", "atan(x)"),
    ("atan2", "atan2(y, x)"),
    ("sinh", "sinh(x)"),
    ("cosh", "cosh(x)"),
    ("tanh", "tanh(x)"),
    ("exp", "exp(x)"),
    ("ln", "Natural logarithm ln(x)"),
    ("log10", "Base-10 logarithm log10(x)"),
    ("sqrt", "sqrt(x)"),
    ("abs", "abs(x)"),
    ("sign", "sign(x)"),
    ("floor", "floor(x)"),
    ("ceil", "ceil(x)"),
    ("round", "round(x)"),
    ("trunc", "trunc(x)"),
    ("step", "step(x) = 1 if x >= 0 else 0"),
    ("min", "min(a, b, ...)"),
    ("max", "max(a, b, ...)"),
    ("mod", "mod(a, b)"),
    ("pow", "pow(a, b)"),
    ("convert", "convert(from, to): factor between two units, e.g. convert(bar, kPa) = 100"),
    ("converttemp", "converttemp(from, to, T): temperature in another scale, e.g. converttemp(C, K, 25)"),
    ("ifelse", "ifelse(condition, a, b)"),
    ("if", "if(a, b, x, y, z): x if a<b, y if a=b, z if a>b"),
    ("sum", "sum(expr, i=1, N)"),
    ("product", "product(expr, i=1, N)"),
];

/// Expression lowering, shared by equations (`slots == None`, identifiers
/// are model variables) and function bodies (identifiers are local slots).
struct Lower<'b, 's> {
    b: &'b mut Builder<'s>,
    env: HashMap<String, f64>,
    slots: Option<HashMap<String, u32>>,
    const_dims: Vec<(u32, Dim)>,
}

impl Lower<'_, '_> {
    fn slot(&mut self, name: &str) -> u32 {
        let slots = self.slots.as_mut().unwrap();
        let n = slots.len() as u32;
        *slots.entry(name.to_string()).or_insert(n)
    }

    fn fexpr(&mut self, e: &Expr) -> Result<Expr1, Diagnostic> {
        let mut t = Tape::default();
        let root = self.expr(e, &mut t)?;
        Ok(Expr1 { tape: t, root })
    }

    fn fblock(&mut self, body: &[FStmt]) -> Result<Vec<IStmt>, Diagnostic> {
        let mut out = Vec::new();
        for s in body {
            out.push(match s {
                FStmt::Assign { target, idx, value, span } => {
                    if !idx.is_empty() {
                        return Err(err("arrays are not supported inside functions yet", *span));
                    }
                    let e = self.fexpr(value)?;
                    IStmt::Assign(self.slot(target), e)
                }
                FStmt::If { cond, then, els, .. } => IStmt::If(self.fexpr(cond)?, self.fblock(then)?, self.fblock(els)?),
                FStmt::Repeat { body, until, .. } => IStmt::Repeat(self.fblock(body)?, self.fexpr(until)?),
                FStmt::While { cond, body, .. } => IStmt::While(self.fexpr(cond)?, self.fblock(body)?),
                FStmt::For { var, from, to, step, body, .. } => {
                    let slot = self.slot(var);
                    let step = match step {
                        Some(s) => Some(self.fexpr(s)?),
                        None => None,
                    };
                    IStmt::For { slot, from: self.fexpr(from)?, to: self.fexpr(to)?, step, body: self.fblock(body)? }
                }
                FStmt::Call { name, inputs, outputs, span } => {
                    let cell = self.b.func_map.get(name).cloned().ok_or_else(|| err(format!("unknown procedure '{name}'"), *span))?;
                    let ins = inputs.iter().map(|e| self.fexpr(e)).collect::<Result<Vec<_>, _>>()?;
                    let outs = outputs.iter().map(|(n, _)| self.slot(n)).collect();
                    IStmt::Call(cell, ins, outs)
                }
                FStmt::Return { .. } => IStmt::Return,
            });
        }
        Ok(out)
    }

    fn expr(&mut self, e: &Expr, t: &mut Tape) -> Result<u32, Diagnostic> {
        match e {
            Expr::Num { value, unit, span } => {
                let v = self.b.literal(*value, unit, *span)?;
                let n = t.constant(v);
                if let Some(u) = unit {
                    if let Ok(un) = parse_unit(u) {
                        self.const_dims.push((n, un.dim));
                    }
                }
                Ok(n)
            }
            Expr::Str { span, .. } => Err(err("string not allowed here", *span)),
            Expr::Var { name, idx, span, .. } => {
                if name.contains("::") {
                    return Err(err(format!("'{name}' is a fluid name (Fluid::Ideal / Fluid::Real); it is only allowed as the fluid of a property function"), *span));
                }
                if idx.is_empty() {
                    if let Some(v) = self.env.get(name) {
                        return Ok(t.constant(*v));
                    }
                    if let Some(c) = builtin_const(name, &self.b.m.unit_system) {
                        let n = t.constant(c);
                        if let Some(d) = builtin_const_dim(name) {
                            self.const_dims.push((n, d));
                        }
                        return Ok(n);
                    }
                }
                if self.slots.is_some() {
                    if !idx.is_empty() {
                        return Err(err("arrays are not supported inside functions yet", *span));
                    }
                    let s = self.slot(name);
                    return Ok(t.push(Op::Var(s)));
                }
                let (key, display, base, index) = self.b.var_key(e, &self.env)?;
                let id = self.b.intern(key, display, base, index, span.line);
                Ok(t.push(Op::Var(id)))
            }
            Expr::Neg(a, _) => {
                let x = self.expr(a, t)?;
                Ok(t.push(Op::F1(F1::Neg, x)))
            }
            Expr::Not(a, _) => {
                let x = self.expr(a, t)?;
                Ok(t.push(Op::F1(F1::Not, x)))
            }
            Expr::Bin(op, a, b, _) => {
                let x = self.expr(a, t)?;
                let y = self.expr(b, t)?;
                let f = match op {
                    BinOp::Add => F2::Add,
                    BinOp::Sub => F2::Sub,
                    BinOp::Mul => F2::Mul,
                    BinOp::Div => F2::Div,
                    BinOp::Pow => F2::Pow,
                    BinOp::And => F2::And,
                    BinOp::Or => F2::Or,
                };
                Ok(t.push(Op::F2(f, x, y)))
            }
            Expr::Cmp(op, a, b, _) => {
                let x = self.expr(a, t)?;
                let y = self.expr(b, t)?;
                let f = match op {
                    CmpOp::Lt => F2::Lt,
                    CmpOp::Le => F2::Le,
                    CmpOp::Gt => F2::Gt,
                    CmpOp::Ge => F2::Ge,
                    CmpOp::Eq => F2::Eq,
                    CmpOp::Ne => F2::Ne,
                };
                Ok(t.push(Op::F2(f, x, y)))
            }
            Expr::Call { name, display, args, span } => self.call(name, display, args, *span, t),
        }
    }

    fn positional(&mut self, args: &[Arg], n: usize, display: &str, span: Span, t: &mut Tape) -> Result<Vec<u32>, Diagnostic> {
        if args.len() != n {
            return Err(err(format!("{display}() takes {n} argument(s), got {}", args.len()), span));
        }
        args.iter().map(|a| self.expr(&a.value, t)).collect()
    }

    fn call(&mut self, name: &str, display: &str, args: &[Arg], span: Span, t: &mut Tape) -> Result<u32, Diagnostic> {
        // ---- EES unit conversions: convert(from, to), converttemp(from, to, T)
        if name == "convert" || name == "converttemp" {
            let n = if name == "convert" { 2 } else { 3 };
            if args.len() != n || args.iter().any(|a| a.name.is_some()) {
                return Err(err(
                    if n == 2 {
                        format!("{display}(from, to) takes two units, e.g. convert(kJ, J)")
                    } else {
                        format!("{display}(from, to, T) takes two temperature units and a value, e.g. converttemp(C, K, T)")
                    },
                    span,
                ));
            }
            let unit = |a: &Arg| -> Result<crate::units::Unit, Diagnostic> {
                let sp = a.value.span();
                let txt = self.b.src.get(sp.start..sp.end).unwrap_or("").trim().trim_matches(['\'', '"']).to_string();
                parse_unit(&txt).map_err(|e| err(format!("{display}: {e}"), sp))
            };
            let (from, to) = (unit(&args[0])?, unit(&args[1])?);
            if from.dim != to.dim {
                return Err(err(format!("{display}: '{}' and '{}' have different dimensions", from.symbol, to.symbol), span));
            }
            if n == 2 {
                // ratio of the scale factors (EES convert): a pure number
                return Ok(t.constant(from.factor / to.factor));
            }
            if from.dim != Dim::TEMP {
                return Err(err(format!("{display}: '{}' is not a temperature unit", from.symbol), span));
            }
            // T_to = (T_from * f_from + o_from - o_to) / f_to
            let x = self.expr(&args[2].value, t)?;
            let k = t.constant(from.factor / to.factor);
            let c = t.constant((from.offset - to.offset) / to.factor);
            let y = t.push(Op::F2(F2::Mul, x, k));
            let z = t.push(Op::F2(F2::Add, y, c));
            // the result is in the target scale, not in the model's
            // temperature unit: it must not take (or give) the dimension of
            // a model temperature, or a display unit would convert it again
            return Ok(t.push(Op::F1(F1::Opaque, z)));
        }
        // ---- unrolled reductions: sum(expr, i=a, b)
        if (name == "sum" || name == "product") && args.len() == 3 && args[1].name.is_some() {
            let var = canon(args[1].name.as_ref().unwrap());
            let a = self.b.int_eval(&args[1].value, &self.env)?;
            let bnd = self.b.int_eval(&args[2].value, &self.env)?;
            let mut acc: Option<u32> = None;
            let saved = self.env.get(&var).copied();
            for i in a..=bnd {
                self.env.insert(var.clone(), i as f64);
                let v = self.expr(&args[0].value, t)?;
                acc = Some(match acc {
                    None => v,
                    Some(p) => t.push(Op::F2(if name == "sum" { F2::Add } else { F2::Mul }, p, v)),
                });
            }
            match saved {
                Some(s) => self.env.insert(var, s),
                None => self.env.remove(&var),
            };
            return Ok(acc.unwrap_or_else(|| t.constant(if name == "sum" { 0.0 } else { 1.0 })));
        }
        if let Some(f) = math_f1(name) {
            let a = self.positional(args, 1, display, span, t)?;
            let a0 = if self.b.m.unit_system.degrees && matches!(f, F1::Sin | F1::Cos | F1::Tan) {
                let k = t.constant(std::f64::consts::PI / 180.0);
                t.push(Op::F2(F2::Mul, a[0], k))
            } else {
                a[0]
            };
            let r = t.push(Op::F1(f, a0));
            if self.b.m.unit_system.degrees && matches!(f, F1::Asin | F1::Acos | F1::Atan) {
                let k = t.constant(180.0 / std::f64::consts::PI);
                return Ok(t.push(Op::F2(F2::Mul, r, k)));
            }
            return Ok(r);
        }
        match name {
            "min" | "max" => {
                if args.is_empty() {
                    return Err(err(format!("{display}() needs arguments"), span));
                }
                let v: Vec<u32> = args.iter().map(|a| self.expr(&a.value, t)).collect::<Result<_, _>>()?;
                let f = if name == "min" { F2::Min } else { F2::Max };
                return Ok(v[1..].iter().fold(v[0], |acc, x| t.push(Op::F2(f, acc, *x))));
            }
            "pow" | "atan2" | "mod" => {
                let a = self.positional(args, 2, display, span, t)?;
                let f = match name {
                    "pow" => F2::Pow,
                    "atan2" => F2::Atan2,
                    _ => F2::Mod,
                };
                return Ok(t.push(Op::F2(f, a[0], a[1])));
            }
            "ifelse" => {
                let a = self.positional(args, 3, display, span, t)?;
                return Ok(t.push(Op::Select(a[0], a[1], a[2])));
            }
            "if" => {
                // EES IF(A, B, X, Y, Z)
                let a = self.positional(args, 5, display, span, t)?;
                let lt = t.push(Op::F2(F2::Lt, a[0], a[1]));
                let eq = t.push(Op::F2(F2::Eq, a[0], a[1]));
                let inner = t.push(Op::Select(eq, a[3], a[4]));
                return Ok(t.push(Op::Select(lt, a[2], inner)));
            }
            _ => {}
        }
        // ---- thermophysical properties
        if let Some(r) = self.property_call(name, display, args, span, t)? {
            return Ok(r);
        }
        // ---- user functions
        if let Some(cell) = self.b.func_map.get(name).cloned() {
            let src = cell.source.as_ref().unwrap();
            if src.is_procedure {
                return Err(err(format!("'{}' is a Procedure; use `Call {}(...)`", src.display, src.display), span));
            }
            let a = self.positional(args, src.params.len(), display, span, t)?;
            let f = Arc::new(UserFn { cell: cell.clone(), index: 0, label: src.display.clone() });
            return Ok(ext(t, f, &a));
        }
        Err(err(format!("unknown function '{display}'"), span))
    }

    fn fluid_arg(&self, a: &Arg) -> Result<String, Diagnostic> {
        match &a.value {
            Expr::Var { display, idx, .. } if idx.is_empty() && a.name.is_none() => Ok(display.clone()),
            Expr::Str { value, .. } => Ok(value.clone()),
            other => Err(err("expected a fluid name (e.g. CO2, Water, R134a, CO2::Ideal or 'IF97::Water')", other.span())),
        }
    }

    fn str_arg(&self, a: &Arg) -> Result<String, Diagnostic> {
        match &a.value {
            Expr::Str { value, .. } => Ok(value.clone()),
            Expr::Var { display, idx, .. } if idx.is_empty() => Ok(display.clone()),
            other => Err(err("expected a string", other.span())),
        }
    }

    fn check_fluid(&self, fluid: &str, span: Span) -> Result<(), Diagnostic> {
        if self.b.opts.check_fluids {
            props::backend::check_fluid(&props::coolprop_fluid(fluid)).map_err(|e| err(e, span))?;
        }
        Ok(())
    }

    fn property_call(&mut self, name: &str, display: &str, args: &[Arg], span: Span, t: &mut Tape) -> Result<Option<u32>, Diagnostic> {
        let us = self.b.m.unit_system.clone();
        // ---- EES humid air: enthalpy(AirH2O, T=, P=, w=), humrat(AirH2O, ...), ...
        let air_h2o = args.first().and_then(|a| self.fluid_arg(a).ok()).is_some_and(|f| props::is_humid_air(props::split_model(&f).0));
        // `Fluid::Ideal` has no saturation states
        let no_ideal = |fluid: &str| -> Result<(), Diagnostic> {
            if props::split_model(fluid).1 == props::Model::Ideal {
                return Err(err(format!("{display}: an ideal gas has no saturation states; use the real fluid ({})", props::split_model(fluid).0), span));
            }
            Ok(())
        };
        if air_h2o {
            let Some(out) = props::HaCall::output(name) else {
                return Err(err(format!("{display} is not available for AirH2O"), span));
            };
            let mut ins = Vec::new();
            let mut nodes = Vec::new();
            for a in &args[1..] {
                let Some(n) = &a.name else {
                    return Err(err(format!("{display}(AirH2O, ...): inputs must be named (T, P, w, R, B, D, h)"), a.value.span()));
                };
                let k = props::HaCall::input(n).ok_or_else(|| err(format!("unknown humid-air input '{n}' (use T, P, w, R, B = wet bulb, D = dew point, h)"), a.value.span()))?;
                ins.push(k);
                nodes.push(self.expr(&a.value, t)?);
            }
            if ins.len() != 3 || !ins.iter().any(|i| i.0 == "P") {
                return Err(err(format!("{display}(AirH2O, ...) needs P and two more inputs, e.g. T=..., P=..., R=..."), span));
            }
            let hc = props::HaCall { label: format!("{display}(AirH2O)"), out, ins, us };
            return Ok(Some(ext(t, Arc::new(hc), &nodes)));
        }
        // ---- latent heat: enthalpy_vaporization(Fluid, T=) or (Fluid, P=)
        if name == "enthalpy_vaporization" {
            let (Some(f), Some(a)) = (args.first(), args.get(1)) else {
                return Err(err(format!("{display}(Fluid, T=...) or {display}(Fluid, P=...)"), span));
            };
            let fluid = self.fluid_arg(f)?;
            self.check_fluid(&fluid, span)?;
            no_ideal(&fluid)?;
            let k = a.name.as_deref().and_then(Key::from_input).filter(|k| matches!(k, Key::T | Key::P)).ok_or_else(|| err(format!("{display}: give T=... or P=..."), span))?;
            let x = self.expr(&a.value, t)?;
            let mut h = [0u32; 2];
            for (i, q) in [1.0, 0.0].into_iter().enumerate() {
                let qn = t.constant(q);
                let pc = PropCall::new(format!("{display}({fluid})"), &fluid, Key::H, vec![k, Key::Q], Some(&us)).map_err(|e| err(e, span))?;
                h[i] = ext(t, Arc::new(pc), &[x, qn]);
            }
            return Ok(Some(t.push(Op::F2(F2::Sub, h[0], h[1]))));
        }
        let sat = match name {
            "t_sat" => Some((Key::T, Key::P)),
            "p_sat" => Some((Key::P, Key::T)),
            _ => None,
        };
        if let Some((out, inp)) = sat {
            if args.len() != 2 {
                return Err(err(format!("{display}(Fluid, {}=...) takes 2 arguments", if inp == Key::P { "P" } else { "T" }), span));
            }
            let fluid = self.fluid_arg(&args[0])?;
            self.check_fluid(&fluid, span)?;
            no_ideal(&fluid)?;
            let x = self.expr(&args[1].value, t)?;
            let q = t.constant(0.0);
            let pc = PropCall::new(format!("{display}({fluid})"), &fluid, out, vec![inp, Key::Q], Some(&us)).map_err(|e| err(e, span))?;
            return Ok(Some(ext(t, Arc::new(pc), &[x, q])));
        }
        if let Some(out) = props::property_function(name) {
            if args.is_empty() {
                return Err(err(format!("{display}() needs a fluid as first argument"), span));
            }
            let fluid = self.fluid_arg(&args[0])?;
            self.check_fluid(&fluid, span)?;
            let mut keys = Vec::new();
            let mut nodes = Vec::new();
            for a in &args[1..] {
                let Some(n) = &a.name else {
                    return Err(err(format!("{display}: inputs must be named, e.g. P=..., T=..."), a.value.span()));
                };
                let k = Key::from_input(n).ok_or_else(|| err(format!("unknown property input '{n}' (use T, P, h, s, u, v, d, x)"), a.value.span()))?;
                keys.push(k);
                nodes.push(self.expr(&a.value, t)?);
            }
            let trivial = matches!(out, Key::MolarMass | Key::Tcrit | Key::Pcrit);
            // property model: `Fluid::Ideal`, `Fluid::Real`, or from the number of inputs
            let (base, model) = props::split_model(&fluid);
            let ideal = match model {
                props::Model::Ideal => true,
                props::Model::Real => false,
                props::Model::Auto => keys.len() == 1 && out != Key::SurfTension,
            };
            if trivial {
                if !keys.is_empty() {
                    return Err(err(format!("{display}(Fluid) takes no state inputs"), span));
                }
            } else if ideal {
                // ideal gas: EES `enthalpy(CO2, T=T)`, or explicit `entropy(CO2::Ideal, T=T, P=P)`
                if keys.is_empty() {
                    return Err(err(format!("{display}({fluid}, ...): give the state inputs, e.g. T=..."), span));
                }
                let label = format!("{display}({base}, ideal gas)");
                let g = props::IdealGasCall::new(label, base, out, keys, Some(&us)).map_err(|e| err(e, span))?;
                return Ok(Some(ext(t, Arc::new(g), &nodes)));
            } else if keys.len() != 2 && !(out == Key::SurfTension && keys.len() == 1) {
                let hint = if model == props::Model::Real {
                    format!("{base}::Real is the real fluid, which needs two state inputs (use {base}::Ideal for the ideal gas)")
                } else {
                    "exactly two state inputs are required (one input, T, gives the ideal-gas value; Fluid::Ideal selects the ideal gas)".to_string()
                };
                return Err(err(format!("{display}: {hint}"), span));
            }
            if out == Key::SurfTension && keys.len() == 1 {
                keys.push(Key::Q);
                nodes.push(t.constant(0.0));
            }
            let label = format!("{display}({base})");
            let pc = PropCall::new(label, base, out, keys, Some(&us)).map_err(|e| err(e, span))?;
            if trivial {
                use crate::funcs::ExtFn;
                let v = pc.eval(&[]).map_err(|e| err(e, span))?;
                let n = t.constant(v);
                // molarmass(Fluid) is kg/kmol, Tcrit a temperature, ...
                self.const_dims.push((n, out.dim()));
                return Ok(Some(n));
            }
            return Ok(Some(ext(t, Arc::new(pc), &nodes)));
        }
        match name {
            "propssi" => {
                if args.len() != 6 {
                    return Err(err("PropsSI('Out', 'In1', v1, 'In2', v2, 'Fluid')", span));
                }
                let out = self.str_arg(&args[0])?;
                let n1 = self.str_arg(&args[1])?;
                let n2 = self.str_arg(&args[3])?;
                let fluid = self.str_arg(&args[5])?;
                self.check_fluid(&fluid, span)?;
                let v1 = self.expr(&args[2].value, t)?;
                let v2 = self.expr(&args[4].value, t)?;
                let label = format!("PropsSI({out},{fluid})");
                let keys = (Key::from_coolprop(&out), Key::from_input(&n1).or(Key::from_coolprop(&n1)), Key::from_input(&n2).or(Key::from_coolprop(&n2)));
                if let (Some(o), Some(a), Some(b)) = keys {
                    if let Ok(pc) = PropCall::new(label.clone(), &fluid, o, vec![a, b], None) {
                        return Ok(Some(ext(t, Arc::new(pc), &[v1, v2])));
                    }
                }
                let rc = RawCall { label, kind: RawKind::PropsSI { out, in1: n1, in2: n2, fluid: props::coolprop_fluid(&fluid) } };
                Ok(Some(ext(t, Arc::new(rc), &[v1, v2])))
            }
            "props1si" => {
                if args.len() != 2 {
                    return Err(err("Props1SI('Fluid', 'Out')", span));
                }
                let a = self.str_arg(&args[0])?;
                let b = self.str_arg(&args[1])?;
                // CoolProp accepts both argument orders
                let v =
                    props::backend::props1_si(&props::coolprop_fluid(&a), &b).or_else(|_| props::backend::props1_si(&props::coolprop_fluid(&b), &a)).map_err(|e| err(e, span))?;
                Ok(Some(t.constant(v)))
            }
            "hapropssi" => {
                if args.len() != 7 {
                    return Err(err("HAPropsSI('Out', 'In1', v1, 'In2', v2, 'In3', v3)", span));
                }
                let out = self.str_arg(&args[0])?;
                let n1 = self.str_arg(&args[1])?;
                let n2 = self.str_arg(&args[3])?;
                let n3 = self.str_arg(&args[5])?;
                let v1 = self.expr(&args[2].value, t)?;
                let v2 = self.expr(&args[4].value, t)?;
                let v3 = self.expr(&args[6].value, t)?;
                let rc = RawCall { label: format!("HAPropsSI({out})"), kind: RawKind::HAPropsSI { out, in1: n1, in2: n2, in3: n3 } };
                Ok(Some(ext(t, Arc::new(rc), &[v1, v2, v3])))
            }
            _ => Ok(None),
        }
    }
}
