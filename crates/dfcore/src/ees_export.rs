//! Export of a model as a simplified, EES-compatible script.
//!
//! The DeltaF Studio language is a superset of the EES equation language
//! (Klein & Nellis, 2012). The export rewrites a script into the subset EES
//! reads, so a model built here can be pasted into EES:
//!
//! * one equation per line (`a = 1; b = 2` is split);
//! * US separators (`1.5`, `f(a, b)`) whatever the script uses;
//! * a `$UnitSystem SI MASS DEG <P> <T> <E>` line for the model unit system;
//! * literals with a unit (`10 [bar]`) become the number in model units,
//!   because EES does not convert bracketed units; the original is kept in a
//!   comment;
//! * fluid names: EES selects the property model by name, so the ideal gas
//!   is written with the formula (`CO2`, `H2O`, `N2`, `Air`) and the real
//!   fluid with the full name (`CarbonDioxide`, `Water`, `Nitrogen`,
//!   `Air_ha`); the model comes from the `::Ideal` / `::Real` suffix or the
//!   number of inputs, as in DeltaF Studio;
//! * ideal-gas h, u, cp, cv take T only; an ideal-gas entropy from T alone
//!   gets `P = 1 atm` (DeltaF Studio's standard state);
//! * density inputs `d=` become `v=1/(...)`, `ifelse(a<b, x, y)` becomes
//!   `IF(a, b, x, y, y)`, `pow(a, b)` and `**` become `^`, and `=` in a
//!   function body becomes `:=` (`//` comments are kept: EES reads them);
//! * the unit of the variable each equation computes is added as a
//!   `{unit}` comment when the line has none;
//! * whatever cannot be translated (`$Guess`, `$Bounds`, `$Units`,
//!   `$Control`, CoolProp-only fluids and functions, ...) is commented, and
//!   listed with its line in a header block.
//!
//! The translation is textual on the parsed spans, so the layout and the
//! comments of the script are kept.

use crate::ast::{Arg, CmpOp, Expr, FStmt, FuncDef, Stmt};
use crate::lexer::{lex, Tok};
use crate::props::{self, Key, Model};
use crate::units::{parse_unit, Dim, EnergyUnit, PressUnit, TempUnit, UnitSystem};

/// EES names of a fluid: (real fluid, ideal gas). Keyed by the name
/// [`props::coolprop_fluid`] gives.
const EES_NAMES: &[(&str, &str, Option<&str>)] = &[
    ("Water", "Water", Some("H2O")),
    ("CO2", "CarbonDioxide", Some("CO2")),
    ("Air", "Air_ha", Some("Air")),
    ("Nitrogen", "Nitrogen", Some("N2")),
    ("Oxygen", "Oxygen", Some("O2")),
    ("Argon", "Argon", Some("Ar")),
    ("Hydrogen", "Hydrogen", Some("H2")),
    ("Helium", "Helium", Some("He")),
    ("Neon", "Neon", Some("Ne")),
    ("CarbonMonoxide", "CarbonMonoxide", Some("CO")),
    ("SulfurDioxide", "SulfurDioxide", Some("SO2")),
    ("NitrousOxide", "NitrousOxide", Some("N2O")),
    ("HydrogenSulfide", "HydrogenSulfide", Some("H2S")),
    ("Ammonia", "Ammonia", Some("NH3")),
    ("Methane", "Methane", Some("CH4")),
    ("Ethane", "Ethane", Some("C2H6")),
    ("n-Propane", "Propane", Some("C3H8")),
    ("n-Butane", "n-Butane", Some("C4H10")),
    ("n-Pentane", "n-Pentane", Some("C5H12")),
    ("n-Hexane", "n-Hexane", Some("C6H14")),
    ("n-Octane", "n-Octane", Some("C8H18")),
    ("Ethylene", "Ethylene", Some("C2H4")),
    ("Methanol", "Methanol", Some("CH3OH")),
    ("Ethanol", "Ethanol", Some("C2H5OH")),
    ("IsoButane", "Isobutane", None),
    ("Propylene", "Propylene", None),
    ("Krypton", "Krypton", None),
    ("Xenon", "Xenon", None),
    ("Toluene", "Toluene", None),
    ("Benzene", "Benzene", None),
    ("Acetone", "Acetone", None),
    ("CycloHexane", "Cyclohexane", None),
    ("n-Heptane", "n-Heptane", None),
    ("n-Decane", "n-Decane", None),
    ("Deuterium", "Deuterium", None),
    ("SulfurHexafluoride", "SulfurHexafluoride", None),
    ("Isopentane", "Isopentane", None),
];

/// Fluids EES does not have (checked in EES 10: "not a recognized substance").
const NOT_IN_EES: &[&str] = &["HeavyWater"];

/// Property functions: canonical DeltaF Studio name → EES name.
const PROP_FUNCS: &[(&str, &str)] = &[
    ("enthalpy", "Enthalpy"),
    ("entropy", "Entropy"),
    ("temperature", "Temperature"),
    ("pressure", "Pressure"),
    ("density", "Density"),
    ("volume", "Volume"),
    ("quality", "Quality"),
    ("intenergy", "IntEnergy"),
    ("cp", "Cp"),
    ("specheat", "Cp"),
    ("cv", "Cv"),
    ("viscosity", "Viscosity"),
    ("conductivity", "Conductivity"),
    ("soundspeed", "SoundSpeed"),
    ("prandtl", "Prandtl"),
    ("surfacetension", "SurfaceTension"),
    ("compressibilityfactor", "CompressibilityFactor"),
    ("molarmass", "MolarMass"),
    ("t_crit", "T_crit"),
    ("p_crit", "P_crit"),
    ("t_sat", "T_sat"),
    ("p_sat", "P_sat"),
    ("enthalpy_vaporization", "Enthalpy_vaporization"),
];

/// Humid-air functions (fluid `AirH2O`), unchanged.
const HUMID_FUNCS: &[&str] = &["humrat", "relhum", "wetbulb", "dewpoint"];

/// Built-in functions of DeltaF Studio that EES may not have under the same
/// name or meaning.
const UNSURE_FUNCS: &[(&str, &str)] = &[
    ("atan2", "atan2(y, x) is not available; write it with arctan(y/x) and IF on the sign of x"),
    ("product", "product(expr, i=1, N): check that the target program has it, or expand the product"),
    ("propssi", "PropsSI (CoolProp, SI units) is not available; write it with the property functions"),
    ("props1si", "Props1SI (CoolProp) is not available; write it with the property functions"),
    ("hapropssi", "HAPropsSI (CoolProp humid air) is not available; use the AirH2O functions"),
];

/// Result of an export.
#[derive(Debug, Clone, Default)]
pub struct Export {
    /// The script, ready to paste.
    pub text: String,
    /// What could not be translated: (line of the original script, message).
    pub limitations: Vec<(u32, String)>,
}

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

struct Ctx<'a> {
    src: &'a str,
    us: UnitSystem,
    edits: Vec<Edit>,
    /// (byte offset where the comment goes, comment text)
    notes: Vec<(usize, String)>,
    limitations: Vec<(u32, String)>,
    model: Option<&'a crate::Model>,
    quality_used: bool,
}

/// Number in a form both programs read.
fn num(v: f64) -> String {
    let a = v.abs();
    if v == 0.0 {
        "0".into()
    } else if (1e-4..1e15).contains(&a) {
        let s = format!("{v}");
        // shortest round trip, but not 0.30000000000000004
        let r = format!("{v:.12}").trim_end_matches('0').trim_end_matches('.').to_string();
        if r.parse::<f64>().ok().is_some_and(|x| ((x - v) / v).abs() < 1e-12) && r.len() < s.len() {
            r
        } else {
            s
        }
    } else {
        format!("{v:e}")
    }
}

/// EES `$UnitSystem` line of a unit system.
pub fn unit_system_line(us: &UnitSystem) -> String {
    let base = if us.energy == EnergyUnit::Btu { "ENG" } else { "SI" };
    let p = match us.pressure {
        PressUnit::Pa => "PA",
        PressUnit::KPa => "KPA",
        PressUnit::Bar => "BAR",
        PressUnit::MPa => "MPA",
        PressUnit::Atm => "ATM",
        PressUnit::Psia => "PSIA",
    };
    let t = match us.temperature {
        TempUnit::K => "K",
        TempUnit::C => "C",
        TempUnit::F => "F",
        TempUnit::R => "R",
    };
    let e = match us.energy {
        EnergyUnit::J => "J",
        EnergyUnit::KJ => "KJ",
        EnergyUnit::Btu => "BTU",
    };
    format!("$UnitSystem {base} MASS {} {p} {t} {e}", if us.degrees { "DEG" } else { "RAD" })
}

/// Text of a comment, safe inside `{...}`.
fn brace(s: &str) -> String {
    format!("{{{}}}", s.replace('{', "(").replace('}', ")"))
}

impl<'a> Ctx<'a> {
    fn line_of(&self, pos: usize) -> u32 {
        self.src[..pos.min(self.src.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
    }

    fn limit(&mut self, pos: usize, end: usize, msg: String) {
        let line = self.line_of(pos);
        if !self.limitations.iter().any(|(l, m)| *l == line && *m == msg) {
            self.limitations.push((line, msg.clone()));
            self.notes.push((end, format!("NOT TRANSLATED: {msg}")));
        }
    }

    /// Replace `start..end`, dropping edits inside it (already folded into `text`).
    fn replace(&mut self, start: usize, end: usize, text: String) {
        self.edits.retain(|e| !(e.start >= start && e.end <= end));
        self.edits.push(Edit { start, end, text });
    }

    /// Source text of `start..end` with the edits inside it applied.
    fn render(&self, start: usize, end: usize) -> String {
        let mut inside: Vec<&Edit> = self.edits.iter().filter(|e| e.start >= start && e.end <= end).collect();
        inside.sort_by_key(|e| (e.start, e.end));
        let mut out = String::new();
        let mut at = start;
        for e in inside {
            if e.start < at {
                continue; // nested in an edit already applied
            }
            out.push_str(&self.src[at..e.start]);
            out.push_str(&e.text);
            at = e.end;
        }
        out.push_str(&self.src[at..end]);
        out
    }

    fn expr_text(&self, e: &Expr) -> String {
        let s = e.span();
        self.render(s.start, s.end)
    }

    /// Visit an expression bottom-up and record its edits. `stmt_end` is
    /// where a note about this expression is written.
    fn expr(&mut self, e: &Expr, stmt_end: usize) {
        match e {
            Expr::Num { value, unit: Some(u), span } => match parse_unit(u) {
                Ok(un) => {
                    let target = if un.dim.is_none() { String::new() } else { self.us.unit_for(un.dim).symbol };
                    if !un.is_delta() && !target.is_empty() && un.symbol.trim().eq_ignore_ascii_case(&target) {
                        return; // already in model units: `[unit]` only labels it
                    }
                    let v = if un.dim.is_none() { un.to_si(*value) } else { un.convert(*value, &self.us.unit_for(un.dim)) };
                    let orig = self.src[span.start..span.end].trim().to_string();
                    let now = num(v);
                    let shown = if un.is_delta() {
                        let k = if matches!(self.us.temperature, TempUnit::F | TempUnit::R) { "R" } else { "K" };
                        format!("{k} (difference)")
                    } else {
                        target.clone()
                    };
                    self.notes.push((stmt_end, format!("{orig} = {now} {shown}").trim().to_string()));
                    // the bracket labels the number for unit checking; a temperature
                    // difference or a dimensionless factor gets none
                    let text = if un.is_delta() || target.is_empty() { now } else { format!("{now} [{target}]") };
                    self.replace(span.start, span.end, text);
                }
                Err(_) => {
                    let msg = format!("unknown unit [{u}]");
                    self.limit(span.start, stmt_end, msg);
                }
            },
            Expr::Num { .. } | Expr::Str { .. } => {}
            Expr::Var { idx, .. } => idx.iter().for_each(|i| self.expr(i, stmt_end)),
            Expr::Neg(a, _) | Expr::Not(a, _) => self.expr(a, stmt_end),
            Expr::Bin(_, a, b, _) | Expr::Cmp(_, a, b, _) => {
                self.expr(a, stmt_end);
                self.expr(b, stmt_end);
            }
            Expr::Call { name, display, args, span } => {
                for a in args {
                    self.expr(&a.value, stmt_end);
                }
                self.call(name, display, args, span.start, span.end, stmt_end);
            }
        }
    }

    fn call(&mut self, name: &str, display: &str, args: &[Arg], start: usize, end: usize, stmt_end: usize) {
        if let Some((_, ees)) = PROP_FUNCS.iter().find(|p| p.0 == name) {
            self.property(name, ees, args, start, end, stmt_end);
            return;
        }
        if HUMID_FUNCS.contains(&name) {
            return;
        }
        match name {
            "ifelse" if args.len() == 3 => {
                // IF(a, b, x_lt, x_eq, x_gt)
                if let Expr::Cmp(op, a, b, _) = &args[0].value {
                    let (x, y) = (self.expr_text(&args[1].value), self.expr_text(&args[2].value));
                    let (lt, eq, gt) = match op {
                        CmpOp::Lt => (&x, &y, &y),
                        CmpOp::Le => (&x, &x, &y),
                        CmpOp::Gt => (&y, &y, &x),
                        CmpOp::Ge => (&y, &x, &x),
                        CmpOp::Eq => (&y, &x, &y),
                        CmpOp::Ne => (&x, &y, &x),
                    };
                    let t = format!("IF({}, {}, {lt}, {eq}, {gt})", self.expr_text(a), self.expr_text(b));
                    self.replace(start, end, t);
                } else {
                    self.limit(start, stmt_end, format!("{display}(): the condition must be a comparison (a < b, ...) to become IF(a, b, x, y, z)"));
                }
            }
            "pow" if args.len() == 2 => {
                let t = format!("({})^({})", self.expr_text(&args[0].value), self.expr_text(&args[1].value));
                self.replace(start, end, t);
            }
            _ => {
                if let Some((_, msg)) = UNSURE_FUNCS.iter().find(|f| f.0 == name) {
                    self.limit(start, stmt_end, msg.to_string());
                }
            }
        }
    }

    /// EES name of a fluid for a property model, and a limitation if any.
    fn fluid_name(written: &str, ideal: bool) -> (String, Option<String>) {
        let (base, _) = props::split_model(written);
        let cp = props::coolprop_fluid(base);
        if base.eq_ignore_ascii_case("airh2o") {
            return ("AirH2O".into(), None);
        }
        if NOT_IN_EES.contains(&cp.as_str()) {
            return (cp.clone(), Some(format!("fluid {base} is not available; replace it")));
        }
        if let Some((_, real, ig)) = EES_NAMES.iter().find(|n| n.0 == cp) {
            return match (ideal, ig) {
                (false, _) => (real.to_string(), None),
                (true, Some(ig)) => (ig.to_string(), None),
                (true, None) => (real.to_string(), Some(format!("no ideal-gas model of {base} under a formula name; the real fluid {real} is used, so the results differ"))),
            };
        }
        // liquids and heat-transfer fluids: the EES-style name of the alias table
        if let Some((_, aliases)) = props::FLUID_ALIASES.iter().find(|a| a.0 == cp && cp.contains("::")) {
            let n = aliases[0].to_string();
            let note = cp.starts_with("LIQ::").then(|| format!("{n}: DeltaF Studio uses its own correlation for this liquid; check the fluid and its correlation"));
            return (n, note);
        }
        if let Some(rest) = cp.strip_prefix("IF97::") {
            return ("Water".into(), Some(format!("IAPWS-IF97 ({rest}) is not available; Water (IAPWS-95) is used, which differs slightly")));
        }
        if cp.contains("::") || cp.contains('&') || cp.contains('[') {
            return (format!("'{base}'"), Some(format!("fluid '{base}' (CoolProp backend or mixture) has no equivalent; replace it")));
        }
        let ideal_note = ideal.then(|| format!("no ideal-gas model of {cp}; the real fluid is used, so the results differ"));
        // refrigerants and other pure fluids: same name
        let is_refrigerant = cp.len() > 1 && cp.starts_with(['R', 'r']) && cp[1..].starts_with(|c: char| c.is_ascii_digit());
        let note = if is_refrigerant { ideal_note } else { ideal_note.or(Some(format!("check that the fluid {cp} exists under this name"))) };
        (cp, note)
    }

    fn property(&mut self, name: &str, ees: &str, args: &[Arg], start: usize, end: usize, stmt_end: usize) {
        let Some(first) = args.first() else { return };
        let written = match &first.value {
            Expr::Var { display, idx, .. } if idx.is_empty() && first.name.is_none() => display.clone(),
            Expr::Str { value, .. } => value.clone(),
            _ => return,
        };
        let (_, model) = props::split_model(&written);
        let out = props::property_function(name);
        let mut inputs: Vec<(String, Option<Key>, String)> =
            args[1..].iter().map(|a| (a.name.clone().unwrap_or_default(), a.name.as_deref().and_then(Key::from_input), self.expr_text(&a.value))).collect();
        let trivial = matches!(out, Some(Key::MolarMass | Key::Tcrit | Key::Pcrit));
        let humid = props::split_model(&written).0.eq_ignore_ascii_case("airh2o");
        let ideal = !humid
            && !trivial
            && match model {
                Model::Ideal => true,
                Model::Real => false,
                Model::Auto => inputs.len() == 1 && out.is_some() && out != Some(Key::SurfTension),
            };
        let (fluid, note) = Self::fluid_name(&written, ideal);
        if let Some(n) = note {
            self.limit(start, stmt_end, n);
        }
        if name == "quality" {
            self.quality_used = true;
        }
        if ideal {
            let has_t = inputs.iter().any(|i| i.1 == Some(Key::T));
            if matches!(out, Some(Key::H | Key::U | Key::Cp | Key::Cv)) && has_t {
                // a function of T only for an ideal gas
                inputs.retain(|i| i.1 == Some(Key::T));
            }
            if out == Some(Key::S) && inputs.len() == 1 && has_t {
                // standard state, as DeltaF Studio's one-input entropy
                let p = num(self.us.unit_for(Dim::PRESSURE).from_si(101_325.0));
                inputs.push(("P".into(), Some(Key::P), p));
            }
        }
        // EES state inputs: specific volume instead of density
        for i in inputs.iter_mut() {
            if i.1 == Some(Key::D) {
                *i = ("v".into(), Some(Key::V), format!("1/({})", i.2));
            }
        }
        let mut t = format!("{ees}({fluid}");
        for (n, _, v) in &inputs {
            t.push_str(&format!(", {n}={v}"));
        }
        t.push(')');
        self.replace(start, end, t);
    }

    /// Unit (model units) of the variable an equation's left side names.
    fn lhs_unit(&self, lhs: &Expr) -> Option<String> {
        let m = self.model?;
        let Expr::Var { name, idx, .. } = lhs else { return None };
        let v = if idx.is_empty() { m.var(name)? } else { m.vars.iter().position(|v| v.base == *name)? as u32 };
        let d = m.var_dim(v)?;
        if d.is_none() {
            return None;
        }
        Some(self.us.unit_for(d).symbol)
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Equation { lhs, rhs, span, trailing_comment, unit } => {
                let n0 = self.notes.len();
                self.expr(lhs, span.end);
                self.expr(rhs, span.end);
                if let Some(u) = unit {
                    // `a = b - c/d [bar]`: drop the unit, keep it as a comment
                    if let Some(k) = self.src[span.start..span.end].rfind('[') {
                        let at = span.start + k;
                        let cut = self.src[..at].trim_end().len();
                        self.replace(cut, span.end, String::new());
                        self.notes.push((span.end, format!("unit of the variable this equation computes: {u}")));
                    }
                } else if trailing_comment.is_none() && self.notes.len() == n0 && !matches!(rhs, Expr::Num { unit: Some(_), .. }) {
                    if let Some(u) = self.lhs_unit(lhs) {
                        self.notes.push((span.end, u));
                    }
                }
            }
            Stmt::Duplicate { from, to, body, span, .. } => {
                self.expr(from, span.start);
                self.expr(to, span.start);
                for b in body {
                    self.stmt(b);
                }
            }
            Stmt::Call { inputs, outputs, span, .. } => {
                for e in inputs.iter().chain(outputs) {
                    self.expr(e, span.end);
                }
            }
            Stmt::Directive { name, args, span } => {
                let line_end = self.src[span.start..].find('\n').map(|k| span.start + k).unwrap_or(self.src.len());
                let text = self.src[span.start..line_end].trim_end().to_string();
                if name == "unitsystem" {
                    self.replace(span.start, line_end, brace(&format!("{text}: see the $UnitSystem line at the top")));
                    return;
                }
                let why = match name.as_str() {
                    "guess" => "set these guess values in the Variable Information window",
                    "bounds" | "limits" => "set these limits in the Variable Information window",
                    "units" => "set these display units in the Variable Information window",
                    "control" => "control loop: solve it by hand (fix the target and free the varied variable)",
                    _ => "directive without an equivalent",
                };
                let _ = args;
                let written = text.split_whitespace().next().unwrap_or("$").to_string();
                self.limitations.push((self.line_of(span.start), format!("{written}: {why}")));
                self.replace(span.start, line_end, brace(&format!("NOT TRANSLATED ({why}): {text}")));
            }
        }
    }

    /// Conditions of If / While / Until in parentheses.
    fn paren(&mut self, cond: &Expr) {
        let sp = cond.span();
        let before = self.src[..sp.start].trim_end().ends_with('(');
        let after = self.src[sp.end..].trim_start().starts_with(')');
        if !(before && after) {
            let t = format!("({})", self.render(sp.start, sp.end));
            self.replace(sp.start, sp.end, t);
        }
    }

    fn fstmts(&mut self, body: &[FStmt]) {
        for s in body {
            match s {
                FStmt::Assign { idx, value, span, .. } => {
                    idx.iter().for_each(|e| self.expr(e, span.end));
                    self.expr(value, span.end);
                    // `x = ...` → `x := ...`
                    let text = &self.src[span.start..span.end];
                    let mut depth = 0;
                    for (k, c) in text.char_indices() {
                        match c {
                            '[' | '(' => depth += 1,
                            ']' | ')' => depth -= 1,
                            ':' if depth == 0 => break,
                            '=' if depth == 0 => {
                                let at = span.start + k;
                                self.edits.push(Edit { start: at, end: at + 1, text: ":=".into() });
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                FStmt::If { cond, then, els, span } => {
                    self.expr(cond, span.start);
                    self.paren(cond);
                    self.fstmts(then);
                    self.fstmts(els);
                }
                FStmt::Repeat { body, until, span } => {
                    self.fstmts(body);
                    self.expr(until, span.end);
                    self.paren(until);
                }
                FStmt::While { cond, body, span } => {
                    self.expr(cond, span.start);
                    self.paren(cond);
                    self.fstmts(body);
                }
                FStmt::For { from, to, step, body, span, .. } => {
                    self.expr(from, span.start);
                    self.expr(to, span.start);
                    if let Some(s) = step {
                        self.expr(s, span.start);
                    }
                    self.fstmts(body);
                }
                FStmt::Call { inputs, outputs, span, .. } => {
                    inputs.iter().chain(outputs.iter().flat_map(|o| o.1.iter())).for_each(|e| self.expr(e, span.end));
                }
                FStmt::Return { .. } => {}
            }
        }
    }

    fn func(&mut self, f: &FuncDef) {
        self.fstmts(&f.body);
    }
}

/// Export `src` (any separator form) for the unit system `us` (the one of
/// the application; a `$UnitSystem` line in the script wins, as in a solve).
pub fn export(src: &str, us: &UnitSystem) -> Export {
    let norm = crate::locale::normalize(src);
    let src: &str = &norm;
    let opts = crate::CompileOptions { unit_system: Some(us.clone()), check_fluids: false };
    let model = crate::compile(src, &opts);
    let pr = crate::parser::parse(src);
    let mut c = Ctx { src, us: model.unit_system.clone(), edits: Vec::new(), notes: Vec::new(), limitations: Vec::new(), model: Some(&model), quality_used: false };
    for f in &pr.program.funcs {
        c.func(f);
    }
    for s in &pr.program.stmts {
        c.stmt(s);
    }
    let lexed = lex(src);
    // `**` → `^`
    for t in &lexed.tokens {
        if t.tok == Tok::Caret && &src[t.span.start..t.span.end] == "**" {
            c.edits.push(Edit { start: t.span.start, end: t.span.end, text: "^".into() });
        }
    }
    // notes: one `{...}` comment per statement, after it
    let mut by_pos: Vec<(usize, Vec<String>)> = Vec::new();
    for (p, n) in std::mem::take(&mut c.notes) {
        match by_pos.iter_mut().find(|(q, _)| *q == p) {
            Some((_, v)) => {
                if !v.contains(&n) {
                    v.push(n)
                }
            }
            None => by_pos.push((p, vec![n])),
        }
    }
    for (p, v) in by_pos {
        c.edits.push(Edit { start: p, end: p, text: format!("   {}", brace(&v.join("; "))) });
    }
    // `;` between equations → new line, with the indentation of the line
    let mut depth = 0i32;
    for t in &lexed.tokens {
        match t.tok {
            Tok::LParen | Tok::LBracket => depth += 1,
            Tok::RParen | Tok::RBracket => depth -= 1,
            Tok::Semicolon if depth == 0 => {
                let ls = src[..t.span.start].rfind('\n').map(|k| k + 1).unwrap_or(0);
                let indent: String = src[ls..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let mut e = t.span.end;
                while src[e..].starts_with([' ', '\t']) {
                    e += 1;
                }
                // drop a trailing `;` at the end of a line
                let nl = if src[e..].starts_with(['\n', '\r']) || e >= src.len() { String::new() } else { format!("\n{indent}") };
                c.edits.push(Edit { start: t.span.start, end: e, text: nl });
            }
            _ => {}
        }
    }
    let body = c.render(0, src.len());
    if c.quality_used {
        c.limitations.push((0, "Quality: outside the two-phase region the target program returns -100 (subcooled) or 100 (superheated), DeltaF Studio returns -1".into()));
    }
    if model.has_errors() {
        c.limitations.push((0, "the original script has errors; the export may be incomplete".into()));
    }
    c.limitations.sort_by_key(|l| l.0);
    let mut head = String::new();
    head.push_str("\"Simplified script exported from DeltaF Studio\n\n");
    head.push_str(&format!(" One equation per line. Unit system: {}. Literals with a unit were\n", model.unit_system.name));
    head.push_str(" converted to these units (the original is in a comment on the line).\n");
    head.push_str(" Absolute enthalpies and entropies of real fluids depend on the reference\n");
    head.push_str(" state of each property library: compare differences.");
    if c.limitations.is_empty() {
        head.push_str("\n\n Everything was translated.\"\n\n");
    } else {
        head.push_str("\n\n Not translated (marked NOT TRANSLATED on the line):\n");
        for (l, m) in &c.limitations {
            let m = m.replace('"', "'");
            if *l == 0 {
                head.push_str(&format!("  - {m}\n"));
            } else {
                head.push_str(&format!("  - line {l} of the original: {m}\n"));
            }
        }
        head.push_str("\"\n\n");
    }
    head.push_str(&unit_system_line(&model.unit_system));
    head.push_str("\n\n");
    let mut text = head + body.trim_start_matches('\n');
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Export { text, limitations: c.limitations }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(src: &str) -> Export {
        export(src, &UnitSystem::engineering())
    }

    #[test]
    fn one_equation_per_line_and_units() {
        let e = ex("P = 10 [MPa]; T = 500 [K]\n  a = 1; b = 2 // two\nT2 = 25 [C]; dT = 5 [dK]\n");
        assert!(e.text.contains("$UnitSystem SI MASS RAD BAR C KJ"), "{}", e.text);
        assert!(e.text.contains("\nP = 100 [bar]   {10 [MPa] = 100 bar"), "{}", e.text);
        assert!(e.text.contains("\nT = 226.85 [C]   {500 [K] = 226.85 C"), "{}", e.text);
        assert!(e.text.contains("\nT2 = 25 [C]\ndT = 5   {5 [dK] = 5 K (difference)}"), "{}", e.text);
        assert!(e.text.contains("\n  a = 1\n  b = 2 // two"), "{}", e.text);
        assert!(!e.text.contains(';'), "{}", e.text);
    }

    #[test]
    fn fluids_follow_the_model() {
        let e = ex("T = 200; P = 1.01325\nh1 = enthalpy(CO2, T=T)\nh2 = enthalpy(CO2, T=T, P=P)\ns1 = entropy(CarbonDioxide::Ideal, T=T, P=P)\n\
                    h3 = enthalpy(H2O::Ideal, T=T, P=P)\ns0 = entropy(CO2, T=T)\nrho = density(Steam, T=T, d=P)\nh4 = enthalpy(Air, T=T, P=P)\n");
        let t = &e.text;
        assert!(t.contains("h1 = Enthalpy(CO2, T=T)"), "{t}");
        assert!(t.contains("h2 = Enthalpy(CarbonDioxide, T=T, P=P)"), "{t}");
        assert!(t.contains("s1 = Entropy(CO2, T=T, P=P)"), "{t}");
        assert!(t.contains("h3 = Enthalpy(H2O, T=T)"), "{t}");
        assert!(t.contains("s0 = Entropy(CO2, T=T, P=1.01325)"), "{t}");
        assert!(t.contains("rho = Density(Water, T=T, v=1/(P))"), "{t}");
        assert!(t.contains("h4 = Enthalpy(Air_ha, T=T, P=P)"), "{t}");
        // units of the computed variables as comments
        assert!(t.contains("h1 = Enthalpy(CO2, T=T)   {kJ/kg}"), "{t}");
        assert!(e.limitations.is_empty(), "{:?}", e.limitations);
    }

    #[test]
    fn limitations_are_listed() {
        let e = ex("$Guess x = 3\nx^2 = 9\ny = ifelse(x > 1, 2, 3)\nz = pow(x, 2) + x**2\nh = enthalpy('INCOMP::MEG-30%', T=20, P=1)\nq = quality(R134a, P=2, h=300)\nw = PropsSI('H','T',300,'P',1e5,'Water')\n");
        let t = &e.text;
        assert!(t.contains("y = IF(x, 1, 3, 3, 2)"), "{t}");
        assert!(t.contains("z = (x)^(2) + x^2"), "{t}");
        assert!(t.contains("{NOT TRANSLATED (set these guess values"), "{t}");
        assert!(e.limitations.iter().any(|l| l.0 == 1 && l.1.contains("$Guess")), "{:?}", e.limitations);
        assert!(e.limitations.iter().any(|l| l.0 == 5 && l.1.contains("MEG")), "{:?}", e.limitations);
        assert!(e.limitations.iter().any(|l| l.1.contains("Quality")), "{:?}", e.limitations);
        assert!(e.limitations.iter().any(|l| l.0 == 7 && l.1.contains("PropsSI")), "{:?}", e.limitations);
        assert!(t.contains("line 5 of the original"), "{t}");
        let e = ex("h = enthalpy(D2O, T=20, P=1)\nz = atan2(1, 2)\nc = ceil(2.3) + step(1)\n");
        assert!(e.limitations.iter().any(|l| l.0 == 1 && l.1.contains("HeavyWater is not available") || l.1.contains("D2O is not available")), "{:?}", e.limitations);
        assert!(e.limitations.iter().any(|l| l.0 == 2 && l.1.contains("atan2")), "{:?}", e.limitations);
        assert_eq!(e.limitations.iter().filter(|l| l.0 > 0).count(), 2, "{:?}", e.limitations);
    }

    #[test]
    fn functions_and_decimal_comma() {
        let e = ex("Function f(a; b)\n  y = a*1,5 + b\n  If y > 1 Then f := y Else f := 0\nEnd\nx = f(1; 2)\n");
        let t = &e.text;
        assert!(t.contains("Function f(a, b)\n  y := a*1.5 + b\n  If (y > 1) Then f := y Else f := 0\nEnd"), "{t}");
        assert!(t.contains("x = f(1, 2)"), "{t}");
    }

    #[test]
    fn equation_unit_becomes_a_comment() {
        let e = ex("a = 2\nb = 3\na = b - c [bar]\n");
        assert!(e.text.contains("a = b - c   {unit of the variable this equation computes: bar}"), "{}", e.text);
    }
}
