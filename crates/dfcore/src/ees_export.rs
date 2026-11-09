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
//! * `For` and `While` (EES has neither) become a guarded `Repeat ... Until`,
//!   `ElseIf` a nested `If` with its own `EndIf`, `not` is resolved by
//!   flipping comparisons, and the operands of `and` / `or` are put in
//!   parentheses (EES reads them left to right, without precedence);
//! * `PropsSI`, `Props1SI` and `HAPropsSI` become the EES property and
//!   `AirH2O` functions, their SI arguments converted to model units
//!   (`T + 273.15`, `P*1e5` and a trailing `/1000` simplify away);
//!   `atan2` becomes a helper function written at the top;
//! * functions and procedures are moved before the first equation, where
//!   EES reads them;
//! * whatever cannot be translated (`$Guess`, `$Bounds`, `$Units`,
//!   `$Control`, CoolProp-only fluids and functions, ...) is commented, and
//!   listed with its line in a header block.
//!
//! The translation is textual on the parsed spans, so the layout and the
//! comments of the script are kept.

use crate::ast::{Arg, BinOp, CmpOp, Expr, FStmt, FuncDef, Stmt};
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
    ("HeavyWater", "DeuteriumOxide", None),
    ("1-Butene", "Butene", None),
];

/// Real fluids and mixtures of the EES fluid list (Fluid Property
/// Information, EES 10), in lowercase: EES names are case-insensitive. A
/// pure fluid with the same CoolProp name keeps it; any other is reported.
const EES_REAL: &[&str] = &[
    "acetone",
    "ammonia",
    "argon",
    "benzene",
    "butene",
    "carbondioxide",
    "carbonmonoxide",
    "carbonylsulfide",
    "cis-2-butene",
    "cyclohexane",
    "cyclopentane",
    "d4",
    "d5",
    "deuterium",
    "deuteriumoxide",
    "diethylether",
    "dimethylcarbonate",
    "dimethylether",
    "ethane",
    "ethanol",
    "ethylbenzene",
    "ethylene",
    "fluorine",
    "helium",
    "hfe7000",
    "hfe7100",
    "hfe7200",
    "hfe7500",
    "hfo1336mzz(z)",
    "hydrogen",
    "hydrogenchloride",
    "hydrogensulfide",
    "isobutane",
    "isobutene",
    "isohexane",
    "isopentane",
    "krypton",
    "mdm",
    "md2m",
    "md4m",
    "mm",
    "m-xylene",
    "methane",
    "methanol",
    "n-butane",
    "n-decane",
    "n-dodecane",
    "n-heptane",
    "n-hexane",
    "n-nonane",
    "n-octane",
    "n-pentane",
    "n-undecane",
    "neon",
    "neopentane",
    "nitrogen",
    "nitrousoxide",
    "novec649",
    "o-xylene",
    "orthohydrogen",
    "oxygen",
    "parahydrogen",
    "propane",
    "p-xylene",
    "propylene",
    "ses36",
    "steam",
    "steam_iapws",
    "steam_nbs",
    "sulfurdioxide",
    "sulfurhexafluoride",
    "toluene",
    "trans-2-butene",
    "water",
    "xenon",
    "r11",
    "r12",
    "r13",
    "r13i1",
    "r14",
    "r22",
    "r23",
    "r32",
    "r40",
    "r41",
    "r113",
    "r114",
    "r115",
    "r116",
    "r123",
    "r124",
    "r125",
    "r134a",
    "r141b",
    "r142b",
    "r143a",
    "r143m",
    "r152a",
    "r161",
    "r218",
    "r227ea",
    "r236ea",
    "r236fa",
    "r245fa",
    "r290",
    "r365mfc",
    "r600",
    "r600a",
    "r717",
    "r718",
    "r744",
    "rc318",
    "r1216",
    "r1224yd(z)",
    "r1225ye(z)",
    "r1233zd(e)",
    "r1234yf",
    "r1234ze(e)",
    "r1234ze(z)",
    "r1243zf",
    "re245cb2",
    "re245fa2",
    "air_ha",
    "nh3h2o",
    "r404a",
    "r407c",
    "r410a",
    "r423a",
    "r448a",
    "r449a",
    "r450a",
    "r452a",
    "r452b",
    "r454b",
    "r454c",
    "r500",
    "r502",
    "r507a",
    "r508b",
    "r513a",
    "r514a",
    "r515a",
];

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
    ("kinematicviscosity", "KinematicViscosity"),
    ("thermaldiffusivity", "ThermalDiffusivity"),
    ("isentropicexponent", "IsentropicExponent"),
    ("isothermalcompress", "IsothermalCompress"),
    ("volexpcoef", "VolExpCoef"),
    ("t_triple", "T_triple"),
    ("acentricfactor", "AcentricFactor"),
    ("v_crit", "V_crit"),
    ("normalboilingpt", "NormalBoilingPt"),
];

/// Humid-air functions (fluid `AirH2O`), unchanged.
const HUMID_FUNCS: &[&str] = &["humrat", "relhum", "wetbulb", "dewpoint"];

/// Built-in functions of DeltaF Studio that EES may not have under the same
/// name or meaning.
const UNSURE_FUNCS: &[(&str, &str)] = &[("p_triple", "p_triple(Fluid) is not available; write P_sat(Fluid, T=T_triple(Fluid))")];

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
    /// CoolProp calls written as EES functions: (call start, EES call in
    /// model units, factor, offset of the model unit to SI)
    si_calls: Vec<(usize, String, f64, f64)>,
    /// start of the CoolProp calls whose SI conversion stays in the text
    si_raw: Vec<usize>,
    atan2_used: bool,
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
        let (s, t) = self.ext(e);
        self.render(s, t)
    }

    /// Span of an expression including the parentheses that group it
    /// (the parser leaves them out): `(a + b)*c` → the whole text.
    fn ext(&self, e: &Expr) -> (usize, usize) {
        let (mut s, mut t) = match e {
            Expr::Bin(_, a, b, _) | Expr::Cmp(_, a, b, _) => (self.ext(a).0, self.ext(b).1),
            Expr::Neg(a, sp) | Expr::Not(a, sp) => (sp.start, self.ext(a).1.max(sp.end)),
            _ => (e.span().start, e.span().end),
        };
        loop {
            let before = self.src[..s].trim_end();
            let after = self.src[t..].trim_start();
            if !(before.ends_with('(') && after.starts_with(')')) {
                break;
            }
            let open = before.len() - 1;
            // `f(x)`: the parentheses of a call, not a group
            let call = self.src[..open].chars().next_back().is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#'));
            let mut depth = 0i32;
            let balanced = self.src[s..t].chars().all(|c| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth >= 0
            }) && depth == 0;
            if call || !balanced {
                break;
            }
            s = open;
            t = self.src.len() - after.len() + 1;
        }
        (s, t)
    }

    /// Text of an expression in a condition: and/or operands always in
    /// parentheses (EES evaluates and/or from left to right, without
    /// precedence), `not` resolved (EES has no `not`), `=` / `<>` as in EES.
    fn cond_text(&self, e: &Expr) -> String {
        match e {
            Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b, _) => {
                let w = if *op == BinOp::And { "and" } else { "or" };
                format!("({}) {w} ({})", self.cond_text(a), self.cond_text(b))
            }
            Expr::Cmp(op, a, b, _) => format!("{} {} {}", self.expr_text(a), cmp_text(*op), self.expr_text(b)),
            Expr::Not(a, _) => self.negate(a),
            _ => format!("{} <> 0", self.expr_text(e)),
        }
    }

    /// Text of `not e` without `not`: comparisons flipped, De Morgan.
    fn negate(&self, e: &Expr) -> String {
        match e {
            Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b, _) => {
                let w = if *op == BinOp::And { "or" } else { "and" };
                format!("({}) {w} ({})", self.negate(a), self.negate(b))
            }
            Expr::Cmp(op, a, b, _) => {
                let n = match op {
                    CmpOp::Lt => CmpOp::Ge,
                    CmpOp::Le => CmpOp::Gt,
                    CmpOp::Gt => CmpOp::Le,
                    CmpOp::Ge => CmpOp::Lt,
                    CmpOp::Eq => CmpOp::Ne,
                    CmpOp::Ne => CmpOp::Eq,
                };
                format!("{} {} {}", self.expr_text(a), cmp_text(n), self.expr_text(b))
            }
            Expr::Not(a, _) => self.cond_text(a),
            _ => format!("{} = 0", self.expr_text(e)),
        }
    }

    /// Indentation of the line holding `pos`.
    fn indent_at(&self, pos: usize) -> String {
        let ls = self.src[..pos].rfind('\n').map(|k| k + 1).unwrap_or(0);
        self.src[ls..pos].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
    }

    /// Replace the closing keyword(s) `kw_start..end` of a block, on their
    /// own line when they are, by `lines` (each already indented).
    fn close_block(&mut self, kw_start: usize, end: usize, lines: &str) {
        let ls = self.src[..kw_start].rfind('\n').map(|k| k + 1).unwrap_or(0);
        if self.src[ls..kw_start].trim().is_empty() {
            self.replace(ls, end, lines.to_string());
        } else {
            self.replace(kw_start, end, format!("\n{lines}"));
        }
    }

    /// `If`, with its `ElseIf` / `Else If` chain: EES has no ElseIf, so each
    /// one becomes `Else` + a nested `If` with its own `EndIf`.
    fn if_chain(&mut self, s: &FStmt) {
        let FStmt::If { cond, then, els, span } = s else { return };
        let multi = self.src[span.start..span.end].contains('\n');
        let ind = self.indent_at(span.start);
        let mut cur: (&Expr, &Vec<FStmt>, &Vec<FStmt>) = (cond, then, els);
        let mut levels = 0;
        loop {
            let (c, t, e) = cur;
            self.expr(c, span.start);
            self.paren(c);
            self.fstmts(t);
            match e.as_slice() {
                [FStmt::If { cond: c2, then: t2, els: e2, span: s2 }] if multi && s2.end == span.end => {
                    let at = s2.start;
                    let pad = ind.clone();
                    if self.src[at..].len() >= 6 && self.src[at..at + 6].eq_ignore_ascii_case("elseif") {
                        self.replace(at, at + 6, format!("Else\n{pad}If"));
                    } else {
                        // `Else If`: the If on its own line
                        let k = self.src[..at].trim_end().len();
                        self.replace(k, at, format!("\n{pad}"));
                    }
                    levels += 1;
                    cur = (c2, t2, e2);
                }
                _ => {
                    self.fstmts(e);
                    break;
                }
            }
        }
        if !multi {
            return;
        }
        // closing keyword: EndIf, End If or End → EndIf, once per level
        let text = &self.src[span.start..span.end];
        let low = text.to_ascii_lowercase();
        let kw = if low.ends_with("endif") {
            span.end - 5
        } else if low.ends_with("if") && low[..low.len() - 2].trim_end().ends_with("end") {
            span.start + low[..low.len() - 2].trim_end().len() - 3
        } else if low.ends_with("end") {
            span.end - 3
        } else {
            return;
        };
        let lines = vec![format!("{ind}EndIf"); levels + 1].join("\n");
        self.close_block(kw, span.end, &lines);
    }

    /// `While c ... End` → `If (c) Then Repeat ... Until (not c) EndIf`.
    fn while_loop(&mut self, cond: &Expr, body: &[FStmt], span: crate::lexer::Span) {
        self.expr(cond, span.start);
        self.fstmts(body);
        let ind = self.indent_at(span.start);
        let (_, mut hend) = self.ext(cond);
        let rest = &self.src[hend..];
        let r = rest.trim_start_matches([' ', '\t']);
        if r.len() >= 2 && r[..2].eq_ignore_ascii_case("do") && !r[2..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            hend += rest.len() - r.len() + 2;
        }
        let head = format!("If ({}) Then\n{ind}Repeat", self.cond_text(cond));
        self.replace(span.start, hend, head);
        let until = format!("{ind}Until ({})\n{ind}EndIf", self.negate(cond));
        self.close_block(span.end - 3, span.end, &until);
    }

    /// `For i = a To b [Step s] ... End` → `i := a`, then a guarded
    /// Repeat-Until that adds the step at the end of each pass.
    fn for_loop(&mut self, var: &str, from: &Expr, to: &Expr, step: Option<&Expr>, body: &[FStmt], span: crate::lexer::Span) {
        for e in [Some(from), Some(to), step].into_iter().flatten() {
            self.expr(e, span.start);
        }
        self.fstmts(body);
        let ind = self.indent_at(span.start);
        let inner = format!("{ind}  ");
        // the loop variable as written
        let head = self.src[span.start + 3..].trim_start();
        let v: String = head.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let mut assigned = Vec::new();
        assigned_vars(body, &mut assigned);
        if assigned.iter().any(|a| a == var) {
            self.limit(span.start, span.start, format!("For loop: the body changes the loop variable {v}; DeltaF Studio counts with its own counter, the translation with {v}"));
        }
        let fixed = |e: &Expr| {
            let mut used = Vec::new();
            used_vars(e, &mut used);
            !used.iter().any(|u| assigned.contains(u) || u == var)
        };
        let mut pre = Vec::new();
        let atom = |t: String| if t.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '[' | ']')) { t } else { format!("({t})") };
        let to_t = if fixed(to) {
            atom(self.expr_text(to))
        } else {
            pre.push(format!("{v}_end := {}", self.expr_text(to)));
            format!("{v}_end")
        };
        let lit = |e: &Expr| match e {
            Expr::Num { value, unit: None, .. } => Some(*value),
            Expr::Neg(a, _) => match a.as_ref() {
                Expr::Num { value, unit: None, .. } => Some(-value),
                _ => None,
            },
            _ => None,
        };
        let (st, sign) = match step {
            None => ("1".to_string(), Some(true)),
            Some(s) => match lit(s) {
                Some(x) => (num(x.abs()), Some(x > 0.0)),
                None if fixed(s) => (atom(self.expr_text(s)), None),
                None => {
                    pre.push(format!("{v}_step := {}", self.expr_text(s)));
                    (format!("{v}_step"), None)
                }
            },
        };
        let (guard, until, incr) = match sign {
            Some(true) => (format!("{v} <= {to_t}"), format!("{v} > {to_t}"), format!("{v} := {v} + {st}")),
            Some(false) => (format!("{v} >= {to_t}"), format!("{v} < {to_t}"), format!("{v} := {v} - {st}")),
            // either sign: past the end when (i - b)*s > 0
            None => (format!("({v} - {to_t})*{st} <= 0"), format!("({v} - {to_t})*{st} > 0"), format!("{v} := {v} + {st}")),
        };
        let hend = self.ext(step.unwrap_or(to)).1;
        let mut head = String::new();
        for p in &pre {
            head.push_str(&format!("{p}\n{ind}"));
        }
        head.push_str(&format!("{v} := {}\n{ind}If ({guard}) Then\n{ind}Repeat", self.expr_text(from)));
        self.replace(span.start, hend, head);
        self.close_block(span.end - 3, span.end, &format!("{inner}{incr}\n{ind}Until ({until})\n{ind}EndIf"));
    }

    /// `PropsSI`, `Props1SI` and `HAPropsSI` (SI units) as the property
    /// function of EES in model units: (text, dimension of the result).
    fn coolprop_call(&mut self, name: &str, args: &[Arg], start: usize, stmt_end: usize) -> Result<(String, Dim), String> {
        let lit = |a: &Arg| match &a.value {
            Expr::Str { value, .. } if a.name.is_none() => Some(value.clone()),
            _ => None,
        };
        fn wrap(t: String) -> String {
            if t.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '[' | ']')) {
                t
            } else {
                format!("({t})")
            }
        }
        // an argument in SI, written in model units
        let input = |c: &Self, e: &Expr, d: Dim| -> String {
            let u = c.us.unit_for(d);
            let text = c.expr_text(e);
            match e {
                Expr::Num { unit: Some(_), .. } => return text, // `300 [K]`: already in model units
                Expr::Num { value, unit: None, .. } => return num(u.from_si(*value)),
                _ => {}
            }
            let close = |x: f64, y: f64| (x - y).abs() <= 1e-9 * y.abs().max(1e-300);
            let num_of = |x: &Expr| match x {
                Expr::Num { value, unit: None, .. } => Some(*value),
                _ => None,
            };
            if u.offset == 0.0 {
                if close(u.factor, 1.0) {
                    return text;
                }
                if let Expr::Bin(BinOp::Mul, a, b, _) = e {
                    if num_of(b).is_some_and(|k| close(k, u.factor)) {
                        return c.expr_text(a);
                    }
                    if num_of(a).is_some_and(|k| close(k, u.factor)) {
                        return c.expr_text(b);
                    }
                }
                return format!("{}/{}", wrap(text), num(u.factor));
            }
            if close(u.factor, 1.0) {
                if let Expr::Bin(BinOp::Add, a, b, _) = e {
                    if num_of(b).is_some_and(|k| close(k, u.offset)) {
                        return c.expr_text(a);
                    }
                    if num_of(a).is_some_and(|k| close(k, u.offset)) {
                        return c.expr_text(b);
                    }
                }
                return format!("{} - {}", wrap(text), num(u.offset));
            }
            format!("ConvertTemp(K, {}, {text})", u.symbol.trim())
        };
        let strip = |f: &str| f.strip_prefix("HEOS::").unwrap_or(f).to_string();
        match name {
            "hapropssi" => {
                if args.len() != 7 {
                    return Err("HAPropsSI needs an output and three inputs".into());
                }
                let keys: Option<Vec<String>> = [0, 1, 3, 5].iter().map(|&k| lit(&args[k])).collect();
                let Some(keys) = keys else { return Err("HAPropsSI with keys that are not text constants".into()) };
                let Some((f, ees)) = ha_output(&keys[0]) else { return Err(format!("HAPropsSI output '{}' has no AirH2O function", keys[0])) };
                let (_, dim) = props::HaCall::output(f).expect("listed");
                let mut t = format!("{ees}(AirH2O");
                for (k, a) in [(1, 2), (2, 4), (3, 6)] {
                    let Some(n) = ha_input(&keys[k]) else { return Err(format!("HAPropsSI input '{}' is not an input of the AirH2O functions", keys[k])) };
                    let (_, d) = props::HaCall::input(n).expect("listed");
                    t.push_str(&format!(", {n}={}", input(self, &args[a].value, d)));
                }
                t.push(')');
                Ok((t, dim))
            }
            "propssi" => {
                if args.len() != 6 {
                    return Err("PropsSI with one input: use the property functions (T_crit, P_crit, MolarMass ...)".into());
                }
                let keys: Option<Vec<String>> = [0, 1, 3, 5].iter().map(|&k| lit(&args[k])).collect();
                let Some(keys) = keys else { return Err("PropsSI with keys or fluid that are not text constants".into()) };
                let out = Key::from_coolprop(&keys[0]).ok_or_else(|| format!("PropsSI output '{}' is not a property function", keys[0]))?;
                let ees = ees_function(out).ok_or_else(|| format!("PropsSI output '{}' has no property function", keys[0]))?;
                let (fluid, note) = Self::fluid_name(&strip(&keys[3]), false);
                if let Some(n) = note {
                    self.limit(start, stmt_end, n);
                }
                let mut t = format!("{ees}({fluid}");
                for (k, a) in [(1, 2), (2, 4)] {
                    let key = Key::from_coolprop(&keys[k]).filter(|k| matches!(k, Key::T | Key::P | Key::H | Key::S | Key::U | Key::D | Key::Q));
                    let Some(key) = key else { return Err(format!("PropsSI input '{}' is not a state input of the property functions", keys[k])) };
                    let v = input(self, &args[a].value, key.dim());
                    t.push_str(&match key {
                        Key::D => format!(", v=1/({v})"),
                        Key::Q => format!(", x={v}"),
                        Key::T | Key::P => format!(", {}={v}", key.coolprop()),
                        _ => format!(", {}={v}", key.coolprop()[..1].to_ascii_lowercase()),
                    });
                }
                t.push(')');
                if out == Key::Q {
                    self.quality_used = true;
                }
                Ok((t, out.dim()))
            }
            _ => {
                // Props1SI(fluid, output), either order
                let (Some(a), Some(b)) = (args.first().and_then(lit), args.get(1).and_then(lit)) else {
                    return Err("Props1SI with arguments that are not text constants".into());
                };
                let ((out, ees), fl) =
                    match (Key::from_coolprop(&a).and_then(|k| ees_function(k).map(|e| (k, e))), Key::from_coolprop(&b).and_then(|k| ees_function(k).map(|e| (k, e)))) {
                        (_, Some(o)) => (o, a),
                        (Some(o), None) => (o, b),
                        _ => return Err(format!("Props1SI('{a}', '{b}') has no property function")),
                    };
                if !matches!(out, Key::Tcrit | Key::Pcrit | Key::MolarMass) {
                    return Err(format!("Props1SI output '{ees}' has no property function"));
                }
                let (fluid, note) = Self::fluid_name(&strip(&fl), false);
                if let Some(n) = note {
                    self.limit(start, stmt_end, n);
                }
                Ok((format!("{ees}({fluid})"), out.dim()))
            }
        }
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
            Expr::Bin(op, a, b, _) => {
                self.expr(a, stmt_end);
                self.expr(b, stmt_end);
                // `HAPropsSI(...)/1000`, `PropsSI('T', ...) - 273.15`: the
                // conversion to model units cancels the one written
                let Expr::Call { span: cs, .. } = a.as_ref() else { return };
                let Expr::Num { value: k, unit: None, .. } = b.as_ref() else { return };
                let Some((_, core, f, o)) = self.si_calls.iter().find(|c| c.0 == cs.start).cloned() else { return };
                let close = |x: f64, y: f64| (x - y).abs() <= 1e-9 * y.abs();
                let cancels = match op {
                    BinOp::Div => o == 0.0 && close(*k, f),
                    BinOp::Mul => o == 0.0 && close(*k * f, 1.0),
                    BinOp::Sub => close(f, 1.0) && o != 0.0 && close(*k, o),
                    _ => false,
                };
                if cancels {
                    self.si_raw.retain(|p| *p != cs.start);
                    let (s, t) = self.ext(e);
                    self.replace(s, t, core);
                }
            }
            Expr::Cmp(_, a, b, _) => {
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
            // `humrat(HumidAir, ...)`: EES knows the name AirH2O only
            if let Some(Expr::Var { display, idx, span, .. }) = args.first().map(|a| &a.value) {
                if idx.is_empty() && props::is_humid_air(props::split_model(display).0) {
                    self.replace(span.start, span.end, "AirH2O".into());
                }
            }
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
            "atan2" if args.len() == 2 => {
                self.atan2_used = true;
                let t = format!("atan2_df({}, {})", self.expr_text(&args[0].value), self.expr_text(&args[1].value));
                self.replace(start, end, t);
            }
            "propssi" | "props1si" | "hapropssi" => match self.coolprop_call(name, args, start, stmt_end) {
                Ok((core, dim)) => {
                    // the call returns SI: model units → SI around the EES function
                    let u = self.us.unit_for(dim);
                    let t = if u.offset != 0.0 && (u.factor - 1.0).abs() < 1e-12 {
                        format!("({core} + {})", num(u.offset))
                    } else if u.offset != 0.0 {
                        format!("ConvertTemp({}, K, {core})", u.symbol.trim())
                    } else if (u.factor - 1.0).abs() < 1e-12 {
                        core.clone()
                    } else {
                        format!("({core}*{})", num(u.factor))
                    };
                    if t != core {
                        self.si_raw.push(start);
                    }
                    self.si_calls.push((start, core, u.factor, u.offset));
                    self.replace(start, end, t);
                }
                Err(msg) => self.limit(start, stmt_end, format!("{msg}; write it with the property functions")),
            },
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
        if props::is_humid_air(base) {
            return ("AirH2O".into(), None);
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
        // refrigerants and other pure fluids: same name when EES has it
        if !EES_REAL.contains(&cp.to_ascii_lowercase().as_str()) {
            return (cp, Some(format!("fluid {base} is not available; replace it")));
        }
        let note = ideal.then(|| format!("no ideal-gas model of {cp} under a formula name; the real fluid is used, so the results differ"));
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
        let trivial = out.is_none_or(|k| k.is_fluid_constant());
        let humid = props::is_humid_air(props::split_model(&written).0);
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
                } else if trailing_comment.is_none()
                    && self.notes.len() == n0
                    && !matches!(rhs, Expr::Num { unit: Some(_), .. })
                    && !self.si_raw.iter().any(|p| (span.start..span.end).contains(p))
                {
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
        let (s, t) = self.ext(cond);
        let text = format!("({})", self.cond_text(cond));
        self.replace(s, t, text);
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
                FStmt::If { .. } => self.if_chain(s),
                FStmt::Repeat { body, until, span } => {
                    self.fstmts(body);
                    self.expr(until, span.end);
                    self.paren(until);
                }
                FStmt::While { cond, body, span } => self.while_loop(cond, body, *span),
                FStmt::For { var, from, to, step, body, span } => self.for_loop(var, from, to, step.as_ref(), body, *span),
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
    let mut c = Ctx {
        src,
        us: model.unit_system.clone(),
        edits: Vec::new(),
        notes: Vec::new(),
        limitations: Vec::new(),
        model: Some(&model),
        quality_used: false,
        si_calls: Vec::new(),
        si_raw: Vec::new(),
        atan2_used: false,
    };
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
    // EES reads functions and procedures only at the top, before the equations
    let first_eq = pr
        .program
        .stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Equation { span, .. } | Stmt::Duplicate { span, .. } | Stmt::Call { span, .. } => Some(span.start),
            Stmt::Directive { .. } => None,
        })
        .min();
    let mut top = String::new();
    if c.atan2_used {
        top.push_str(&atan2_helper(model.unit_system.degrees));
        top.push('\n');
    }
    for f in &pr.program.funcs {
        if first_eq.is_some_and(|q| f.span.start > q) {
            let ls = src[..f.span.start].rfind('\n').map(|k| k + 1).unwrap_or(0);
            let start = if src[ls..f.span.start].trim().is_empty() { ls } else { f.span.start };
            let le = src[f.span.end..].find('\n').map(|k| f.span.end + k + 1).unwrap_or(src.len());
            top.push_str(c.render(start, le).trim_end());
            top.push_str("\n\n");
            c.replace(start, le, String::new());
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
    head.push_str(&top);
    let mut text = head + body.trim_start_matches('\n');
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Export { text, limitations: c.limitations }
}

/// EES text of a comparison operator.
fn cmp_text(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
        CmpOp::Eq => "=",
        CmpOp::Ne => "<>",
    }
}

/// EES property function that returns a CoolProp output.
fn ees_function(k: Key) -> Option<&'static str> {
    PROP_FUNCS.iter().find(|(df, _)| props::property_function(df) == Some(k)).map(|p| p.1)
}

/// AirH2O function for a `HAPropsSI` output key: (DeltaF Studio name, EES name).
fn ha_output(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key.to_ascii_lowercase().as_str() {
        "h" | "hda" | "enthalpy" => ("enthalpy", "Enthalpy"),
        "s" | "sda" | "entropy" => ("entropy", "Entropy"),
        "v" | "vda" => ("volume", "Volume"),
        "t" | "tdb" | "t_db" => ("temperature", "Temperature"),
        "c" | "cp" => ("cp", "Cp"),
        "w" | "omega" | "humrat" => ("humrat", "HumRat"),
        "r" | "rh" | "relhum" => ("relhum", "RelHum"),
        "b" | "twb" | "t_wb" | "wetbulb" => ("wetbulb", "WetBulb"),
        "d" | "tdp" | "t_dp" | "dewpoint" => ("dewpoint", "DewPoint"),
        "m" | "visc" | "mu" => ("viscosity", "Viscosity"),
        "k" | "conductivity" => ("conductivity", "Conductivity"),
        _ => return None,
    })
}

/// AirH2O input name for a `HAPropsSI` input key.
fn ha_input(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "t" | "tdb" | "t_db" => "T",
        "p" => "P",
        "w" | "omega" | "humrat" => "w",
        "r" | "rh" | "relhum" => "R",
        "b" | "twb" | "t_wb" | "wetbulb" => "B",
        "d" | "tdp" | "t_dp" | "dewpoint" => "D",
        "h" | "hda" | "enthalpy" => "h",
        _ => return None,
    })
}

/// Variables a function body assigns (loop variables and call outputs too).
fn assigned_vars(body: &[FStmt], out: &mut Vec<String>) {
    for s in body {
        match s {
            FStmt::Assign { target, .. } => out.push(target.clone()),
            FStmt::If { then, els, .. } => {
                assigned_vars(then, out);
                assigned_vars(els, out);
            }
            FStmt::Repeat { body, .. } | FStmt::While { body, .. } => assigned_vars(body, out),
            FStmt::For { var, body, .. } => {
                out.push(var.clone());
                assigned_vars(body, out);
            }
            FStmt::Call { outputs, .. } => out.extend(outputs.iter().map(|o| o.0.clone())),
            FStmt::Return { .. } => {}
        }
    }
}

/// Variables an expression reads.
fn used_vars(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Var { name, idx, .. } => {
            out.push(name.clone());
            idx.iter().for_each(|i| used_vars(i, out));
        }
        Expr::Neg(a, _) | Expr::Not(a, _) => used_vars(a, out),
        Expr::Bin(_, a, b, _) | Expr::Cmp(_, a, b, _) => {
            used_vars(a, out);
            used_vars(b, out);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| used_vars(&a.value, out)),
        Expr::Num { .. } | Expr::Str { .. } => {}
    }
}

/// Four-quadrant arc tangent for EES, which has none: written once at the
/// top of the export when the script uses `atan2`. In radians, as `atan2`
/// of DeltaF Studio in either angle mode (`deg`: EES `arctan` in degrees).
fn atan2_helper(deg: bool) -> String {
    let a = if deg { "arctan(y/x)*pi/180" } else { "arctan(y/x)" };
    format!(
        "Function atan2_df(y, x)\n  {{four-quadrant arc tangent in radians: atan2(y, x) of DeltaF Studio}}\n  atan2_df := 0\n  If (x <> 0) Then a := {a}\n  \
         If (x > 0) Then atan2_df := a\n  If (x < 0) and (y >= 0) Then atan2_df := a + pi\n  If (x < 0) and (y < 0) Then atan2_df := a - pi\n  \
         If (x = 0) and (y > 0) Then atan2_df := pi/2\n  If (x = 0) and (y < 0) Then atan2_df := -pi/2\nEnd\n"
    )
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
        assert!(t.contains("w = (Enthalpy(Water, T=26.85, P=1)*1000)"), "{t}");
        assert!(t.contains("line 5 of the original"), "{t}");
        let e = ex("h = enthalpy(D2O, T=20, P=1)\nz = atan2(1, 2)\nc = ceil(2.3) + step(1)\nph = PropsSI('Phase', 'T', 300, 'P', 1e5, 'Water')\n");
        assert!(e.text.contains("Enthalpy(DeuteriumOxide,"), "{}", e.text);
        assert!(e.text.contains("z = atan2_df(1, 2)") && e.text.contains("\nFunction atan2_df(y, x)\n"), "{}", e.text);
        assert!(e.limitations.iter().any(|l| l.0 == 4 && l.1.contains("Phase")), "{:?}", e.limitations);
        assert_eq!(e.limitations.iter().filter(|l| l.0 > 0).count(), 1, "{:?}", e.limitations);
        let e = ex("T1 = 20; T2 = 40\nh = enthalpy(Water, T=(T1 + T2)/2, P=1)\n");
        assert!(e.text.contains("h = Enthalpy(Water, T=(T1 + T2)/2, P=1)"), "{}", e.text);
        let e = ex("a = enthalpy(R1234yf, T=20, P=1)\nb = enthalpy(Novec649, T=20, P=1)\nc = enthalpy(MD3M, T=20, P=1)\n");
        assert_eq!(e.limitations.iter().filter(|l| l.0 > 0).map(|l| l.0).collect::<Vec<_>>(), [3], "{:?}", e.limitations);
    }

    #[test]
    fn functions_and_decimal_comma() {
        let e = ex("Function f(a; b)\n  y = a*1,5 + b\n  If y > 1 Then f := y Else f := 0\nEnd\nx = f(1; 2)\n");
        let t = &e.text;
        assert!(t.contains("Function f(a, b)\n  y := a*1.5 + b\n  If (y > 1) Then f := y Else f := 0\nEnd"), "{t}");
        assert!(t.contains("x = f(1, 2)"), "{t}");
    }

    #[test]
    fn coolprop_calls_become_property_functions() {
        let e = ex("P[1] = 1.2; T[1] = 25; RH = 0.5\n\
                    w = HAPropsSI('W', 'T', T[1] + 273.15, 'P', P[1]*1e5, 'R', RH)\n\
                    h = HAPropsSI('H', 'T', T[1] + 273.15, 'P', P[1]*1e5, 'R', RH)/1000\n\
                    Td = HAPropsSI('Tdp', 'T', T[1] + 273.15, 'P', 101325, 'R', RH) - 273.15\n\
                    hw = PropsSI('H', 'T', T[1] + 273.15, 'Q', 0, 'Water')\n\
                    Tc = Props1SI('CO2', 'Tcrit')\n\
                    k = 2/HAPropsSI('V', 'T', T[1] + 273.15, 'P', P[1]*1e5, 'W', w)\n");
        let t = &e.text;
        assert!(t.contains("\nw = HumRat(AirH2O, T=T[1], P=P[1], R=RH)\n"), "{t}");
        assert!(t.contains("\nh = Enthalpy(AirH2O, T=T[1], P=P[1], R=RH)\n"), "{t}");
        assert!(t.contains("\nTd = DewPoint(AirH2O, T=T[1], P=1.01325, R=RH)\n"), "{t}");
        assert!(t.contains("\nhw = (Enthalpy(Water, T=T[1], x=0)*1000)\n"), "{t}");
        assert!(t.contains("\nTc = (T_crit(CarbonDioxide) + 273.15)\n"), "{t}");
        assert!(t.contains("\nk = 2/Volume(AirH2O, T=T[1], P=P[1], w=w)"), "{t}");
        assert!(e.limitations.iter().all(|l| l.0 == 0), "{:?}", e.limitations);
        // aliases of humid air are written AirH2O
        let e = ex("w = humrat(HumidAir, T=30, P=1, R=0.6)\nh = enthalpy(Moist_Air, T=30, P=1, w=w)\n");
        assert!(e.text.contains("w = humrat(AirH2O, T=30, P=1, R=0.6)") && e.text.contains("h = Enthalpy(AirH2O, T=30, P=1, w=w)"), "{}", e.text);
        // the conversion cancels only what it should
        let e = ex("h = 2*HAPropsSI('H', 'T', 300, 'P', 1e5, 'R', 0.5)/1000\n");
        assert!(e.text.contains("h = 2*(Enthalpy(AirH2O, T=26.85, P=1, R=0.5)*1000)/1000"), "{}", e.text);
    }

    #[test]
    fn loops_and_conditions_become_ees() {
        let src = concat!(
            "y = f(3)\nFunction f(N)\n  s = 0\n  For i = 1 To N\n    s = s + i\n  End\n  For j = N To 1 Step -2\n    s = s + j\n  End\n",
            "  k = 0\n  While k < 3 and not (s = 2) Do\n    k = k + 1\n  End\n",
            "  If s = 1 Then\n    f := 1\n  ElseIf s > 9 or s < 0 and k > 1 Then\n    f := 2\n  Else\n    f := s\n  End If\nEnd\n"
        );
        let e = ex(src);
        let t = &e.text;
        // functions go first in EES
        assert!(t.find("Function f(N)").unwrap() < t.find("y = f(3)").unwrap(), "{t}");
        assert!(t.contains("  i := 1\n  If (i <= N) Then\n  Repeat\n    s := s + i\n    i := i + 1\n  Until (i > N)\n  EndIf\n"), "{t}");
        assert!(t.contains("  j := N\n  If (j >= 1) Then\n  Repeat\n    s := s + j\n    j := j - 2\n  Until (j < 1)\n  EndIf\n"), "{t}");
        assert!(t.contains("  If ((k < 3) and (s <> 2)) Then\n  Repeat\n    k := k + 1\n  Until ((k >= 3) or (s = 2))\n  EndIf\n"), "{t}");
        // and binds tighter than or here, not in EES: the parentheses say so
        assert!(t.contains("  If (s = 1) Then\n    f := 1\n  Else\n  If ((s > 9) or ((s < 0) and (k > 1))) Then\n    f := 2\n  Else\n    f := s\n  EndIf\n  EndIf\nEnd"), "{t}");
        assert!(!t.contains("For") && !t.contains("While") && !t.contains("ElseIf") && !t.contains("not"), "{t}");
        assert!(e.limitations.is_empty(), "{:?}", e.limitations);
        // the same results here
        let m = crate::compile(src, &crate::CompileOptions::default());
        assert!(!m.has_errors());
        // a bound the body changes is kept in a variable
        let e = ex("Function g(N)\n  g := 0\n  For i = 1 To N Step N/2\n    N = N - 1\n    g := g + 1\n  End\nEnd\nz = g(4)\n");
        assert!(e.text.contains("i_end := N\n  i_step := N/2\n  i := 1\n  If ((i - i_end)*i_step <= 0) Then"), "{}", e.text);
    }

    #[test]
    fn new_functions_keep_their_ees_names() {
        let e = ex("k = isentropicexponent(Steam, T=600, P=100)\nnu = kinematicviscosity(Air, T=20, P=1)\nTb = normalboilingpt(Water)\n\
                    pt = p_triple(Water)\ny = erf(0.5) + bessel_k0(1) + average(1, 2)\n");
        let t = &e.text;
        assert!(t.contains("k = IsentropicExponent(Water, T=600, P=100)"), "{t}");
        assert!(t.contains("nu = KinematicViscosity(Air_ha, T=20, P=1)"), "{t}");
        assert!(t.contains("Tb = NormalBoilingPt(Water)"), "{t}");
        assert!(t.contains("y = erf(0.5) + bessel_k0(1) + average(1, 2)"), "{t}");
        assert_eq!(e.limitations.iter().filter(|l| l.0 > 0).map(|l| l.0).collect::<Vec<_>>(), [4], "{:?}", e.limitations);
    }

    #[test]
    fn equation_unit_becomes_a_comment() {
        let e = ex("a = 2\nb = 3\na = b - c [bar]\n");
        assert!(e.text.contains("a = b - c   {unit of the variable this equation computes: bar}"), "{}", e.text);
    }
}
