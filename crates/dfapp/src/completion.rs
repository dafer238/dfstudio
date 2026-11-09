//! Context-aware completion for the equation editor.
//!
//! The text before the cursor decides what is offered and in which order:
//!
//! * `enthalpy(` → fluids: the ones the script already uses, then common
//!   working fluids, then every other fluid;
//! * `enthalpy(Water, ` → the state inputs (`P=`, `T=`, ...) not given yet;
//! * `enthalpy(Water, P=` → variables whose dimension is a pressure first,
//!   then variables of unknown dimension that look like one (`P_cond`), then
//!   the rest;
//! * `x = 10 [` → units, those of `x`'s dimension first;
//! * the start of a statement → variables, keywords and directives;
//! * anywhere else → variables, functions and constants.
//!
//! Within one context a candidate whose name starts with the typed prefix
//! comes before one where the prefix starts a word inside the name
//! (`cond` → `P_cond`), which comes before a fuzzy (subsequence) match.

use crate::analysis::Analysis;
use crate::editor::{Candidate, Kind, DIRECTIVES, KEYWORDS};
use dfcore::props::{property_function, Key, PROPERTY_FUNCTIONS};
use dfcore::units::{parse_unit, Dim};

/// Working fluids offered first in a property call, after those of the script.
pub const COMMON_FLUIDS: &[&str] = &[
    "Water",
    "Air",
    "CO2",
    "Nitrogen",
    "Oxygen",
    "Ammonia",
    "Methane",
    "Hydrogen",
    "Helium",
    "Argon",
    "R134a",
    "R245fa",
    "R1234yf",
    "R1234ze(E)",
    "R32",
    "R410A",
    "Propane",
    "Isobutane",
    "Toluene",
];

/// Fluids known under an EES-style name (liquids, heat-transfer fluids) that
/// CoolProp's list does not contain.
pub const EXTRA_FLUIDS: &[(&str, &str)] = &[
    ("Steam", "water, IAPWS-95"),
    ("Sodium", "liquid sodium"),
    ("NaK", "sodium-potassium"),
    ("Lead", "liquid lead"),
    ("LBE", "lead-bismuth eutectic"),
    ("SolarSalt", "60 NaNO3 - 40 KNO3"),
    ("Therminol66", "heat-transfer oil"),
    ("Therminol72", "heat-transfer oil"),
    ("TherminolVP1", "heat-transfer oil (Dowtherm A)"),
    ("DowthermJ", "heat-transfer fluid"),
    ("DowthermQ", "heat-transfer fluid"),
    ("Syltherm800", "silicone oil"),
    ("SylthermXLT", "silicone oil"),
];

/// Where the cursor is, as far as completion is concerned.
#[derive(Debug, Clone, PartialEq)]
pub enum Ctx {
    /// Inside `[ ]` after a number; dimension of the equation's left side.
    Unit(Option<Dim>),
    /// Typing a `$` directive.
    Directive,
    /// First argument of a property function.
    Fluid,
    /// A later argument of a property function, before its `=`: the inputs
    /// already given (lower case).
    PropInput { func: String, used: Vec<String> },
    /// The value of a named property input (`P=`).
    PropValue(Key),
    /// Start of a statement.
    Statement,
    /// Inside an expression.
    Expr,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '#' || c == '$'
}

/// Innermost unclosed call before char `start` on the same statement:
/// (lower-case function name, text of its arguments up to `start`).
fn enclosing_call(chars: &[char], start: usize) -> Option<(String, String)> {
    let mut depth = 0i32;
    let mut i = start.min(chars.len());
    while i > 0 {
        i -= 1;
        match chars[i] {
            '\n' => return None,
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    let mut j = i;
                    while j > 0 && is_ident(chars[j - 1]) {
                        j -= 1;
                    }
                    let name: String = chars[j..i].iter().collect::<String>().to_lowercase();
                    let args: String = chars[i + 1..start].iter().collect();
                    return Some((name, args));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Split call arguments at top-level `,` or `;` (decimal-comma scripts).
fn split_args(args: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut depth = 0i32;
    for c in args.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' | ';' if depth == 0 => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(c);
    }
    out
}

/// Functions whose first argument is a fluid and later ones named inputs.
pub fn is_fluid_function(name: &str) -> bool {
    property_function(name).is_some() || name == "t_sat" || name == "p_sat"
}

/// Completion context at the identifier that starts at char `start`.
pub fn context(text: &str, start: usize, analysis: Option<&Analysis>) -> Ctx {
    let chars: Vec<char> = text.chars().collect();
    let start = start.min(chars.len());
    let line_from = chars[..start].iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0);
    let before: String = chars[line_from..start].iter().collect();
    if chars.get(start) == Some(&'$') || before.ends_with('$') {
        return Ctx::Directive;
    }
    // unit: "12 [" with no "]" after it
    if let Some(open) = before.rfind('[') {
        let lead = before[..open].trim_end();
        if !before[open..].contains(']') && lead.chars().last().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            let lhs = before.split('=').next().unwrap_or("").trim();
            let dim = analysis.and_then(|a| {
                let v = a.model.var(&lhs.to_lowercase().replace(' ', ""))?;
                a.model.var_dim(v)
            });
            return Ctx::Unit(dim);
        }
    }
    if let Some((name, args)) = enclosing_call(&chars, start) {
        if is_fluid_function(&name) {
            let parts = split_args(&args);
            if parts.len() == 1 {
                return Ctx::Fluid;
            }
            let cur = parts.last().unwrap();
            if let Some((k, _)) = cur.split_once('=') {
                if let Some(key) = Key::from_input(k.trim()) {
                    return Ctx::PropValue(key);
                }
                return Ctx::Expr;
            }
            let used = parts[1..parts.len() - 1].iter().filter_map(|p| p.split_once('=').map(|(k, _)| k.trim().to_lowercase())).collect();
            return Ctx::PropInput { func: name, used };
        }
        return Ctx::Expr;
    }
    let t = before.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("call") {
        return Ctx::Statement;
    }
    Ctx::Expr
}

/// Whether typing `c` just before char `cur` should open the popup with an
/// empty prefix (a property call wants a fluid, an input or a value).
pub fn auto_trigger(text: &str, cur: usize) -> bool {
    let chars: Vec<char> = text.chars().take(cur).collect();
    let Some(&last) = chars.last() else { return false };
    if !matches!(last, '(' | ',' | ';' | '=' | ' ') {
        return false;
    }
    if last == ' ' && !matches!(chars.iter().rev().nth(1), Some(',' | ';')) {
        return false;
    }
    if last == '=' && matches!(chars.iter().rev().nth(1), Some('=' | '<' | '>')) {
        return false;
    }
    match enclosing_call(&chars, chars.len()) {
        Some((name, _)) => is_fluid_function(&name),
        None => false,
    }
}

/// How well `label` matches `prefix`: 0 starts with it, 1 a word inside
/// starts with it, 2 subsequence; `None` no match.
pub fn match_tier(label: &str, prefix: &str) -> Option<u8> {
    if prefix.is_empty() {
        return Some(0);
    }
    let l = label.to_lowercase();
    let p = prefix.to_lowercase();
    if l.starts_with(&p) {
        return Some(0);
    }
    // word boundaries: after '_' or a lower→upper case change
    let lc: Vec<char> = label.chars().collect();
    for i in 1..lc.len() {
        let boundary = lc[i - 1] == '_' || (lc[i - 1].is_lowercase() && lc[i].is_uppercase());
        if boundary && lc[i..].iter().collect::<String>().to_lowercase().starts_with(&p) {
            return Some(1);
        }
    }
    if p.chars().count() >= 2 {
        let mut it = l.chars();
        if p.chars().all(|c| it.any(|x| x == c)) {
            return Some(2);
        }
    }
    None
}

/// Fluids named as the first argument of a property call in the script, in
/// order of first use.
pub fn script_fluids(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    for i in 0..chars.len() {
        if chars[i] != '(' {
            continue;
        }
        let mut j = i;
        while j > 0 && is_ident(chars[j - 1]) {
            j -= 1;
        }
        let name: String = chars[j..i].iter().collect::<String>().to_lowercase();
        if !is_fluid_function(&name) {
            continue;
        }
        let arg: String = chars[i + 1..].iter().take_while(|c| !matches!(c, ',' | ';' | ')' | '\n')).collect();
        let f = arg.trim().trim_matches(|c| c == '\'' || c == '"').to_string();
        if !f.is_empty() && f.chars().all(|c| is_ident(c) || "():-".contains(c)) && !out.iter().any(|o| o.eq_ignore_ascii_case(&f)) {
            out.push(f);
        }
    }
    out
}

/// State inputs a property function accepts.
fn inputs_for(func: &str) -> &'static [(&'static str, &'static str)] {
    const ALL: &[(&str, &str)] = &[
        ("P", "pressure"),
        ("T", "temperature"),
        ("h", "specific enthalpy"),
        ("s", "specific entropy"),
        ("x", "vapour quality"),
        ("v", "specific volume"),
        ("d", "density"),
        ("u", "specific internal energy"),
    ];
    match func {
        "t_sat" => &[("P", "pressure")],
        "p_sat" | "surfacetension" => &[("T", "temperature")],
        _ => ALL,
    }
}

/// A scored candidate: lower sorts first.
struct Scored {
    tier: u8,
    prio: u8,
    c: Candidate,
}

/// Completion candidates for the identifier `prefix` that starts at char
/// `start`, best first.
pub fn candidates(text: &str, start: usize, prefix: &str, analysis: Option<&Analysis>, fluids: &[String]) -> Vec<Candidate> {
    let ctx = context(text, start, analysis);
    let mut out: Vec<Scored> = Vec::new();
    let mut push = |label: String, insert: String, kind: Kind, detail: String, prio: u8| {
        if let Some(tier) = match_tier(&label, prefix) {
            out.push(Scored { tier, prio, c: Candidate { label, insert, kind, detail } });
        }
    };
    match &ctx {
        Ctx::Directive => {
            let p = prefix.trim_start_matches('$');
            for (d, doc) in DIRECTIVES {
                if match_tier(&d[1..], p).is_some() {
                    out.push(Scored { tier: 0, prio: 0, c: Candidate { label: d.to_string(), insert: format!("{d} "), kind: Kind::Directive, detail: doc.to_string() } });
                }
            }
            return finish(out);
        }
        Ctx::Unit(dim) => {
            let mut seen = std::collections::HashSet::new();
            for q in crate::units_ui::QUANTITIES.iter().map(|q| q.dim).chain([Dim::NONE]) {
                for u in crate::units_ui::choices(q) {
                    if !seen.insert(u) {
                        continue;
                    }
                    let d = parse_unit(u).ok().map(|x| x.dim);
                    let prio = if dim.is_some() && d == *dim { 0 } else { 1 };
                    let detail = d.and_then(|d| d.name()).unwrap_or("").to_string();
                    push(u.to_string(), u.to_string(), Kind::Unit, detail, prio);
                }
            }
            for u in ["dK", "dC", "kg", "mm", "kN", "N", "L", "m^3", "m^2", "kW/K", "W/m^2-K", "rpm", "Hz"] {
                if seen.insert(u) {
                    let d = parse_unit(u).ok().map(|x| x.dim);
                    let prio = if dim.is_some() && d == *dim { 0 } else { 2 };
                    push(u.to_string(), u.to_string(), Kind::Unit, String::new(), prio);
                }
            }
            return finish(out);
        }
        Ctx::Fluid => {
            // fluids of the other lines (the current one is being typed)
            let chars: Vec<char> = text.chars().collect();
            let s0 = start.min(chars.len());
            let from = chars[..s0].iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0);
            let to = chars[s0..].iter().position(|c| *c == '\n').map(|p| s0 + p).unwrap_or(chars.len());
            let other: String = chars[..from].iter().chain(chars[to..].iter()).collect();
            let used = script_fluids(&other);
            let mut seen = std::collections::HashSet::new();
            for f in &used {
                if seen.insert(f.to_lowercase()) {
                    push(f.clone(), f.clone(), Kind::Fluid, "used in this model".into(), 0);
                }
            }
            for (n, f) in COMMON_FLUIDS.iter().enumerate() {
                if seen.insert(f.to_lowercase()) {
                    push(f.to_string(), f.to_string(), Kind::Fluid, "common fluid".into(), 10 + n as u8);
                }
            }
            for (f, d) in EXTRA_FLUIDS {
                if seen.insert(f.to_lowercase()) {
                    push(f.to_string(), f.to_string(), Kind::Fluid, d.to_string(), 100);
                }
            }
            for f in fluids {
                if seen.insert(f.to_lowercase()) {
                    push(f.clone(), f.clone(), Kind::Fluid, "CoolProp fluid".into(), 100);
                }
            }
            // other names of the same fluids (CarbonDioxide = CO2, H2O = Water, ...)
            for (c, aliases) in dfcore::props::FLUID_ALIASES {
                let main = if c.contains("::") { aliases[0] } else { c };
                for a in aliases.iter() {
                    if seen.insert(a.to_lowercase()) {
                        push(a.to_string(), a.to_string(), Kind::Fluid, format!("= {main}"), 100);
                    }
                }
            }
            return finish_alpha(out);
        }
        Ctx::PropInput { func, used } => {
            for (n, (k, d)) in inputs_for(func).iter().enumerate() {
                if !used.iter().any(|u| Key::from_input(u) == Key::from_input(k)) {
                    push(format!("{k}="), format!("{k}="), Kind::Keyword, d.to_string(), n as u8);
                }
            }
            return finish(out);
        }
        _ => {}
    }

    // expressions and statements
    let want: Option<Dim> = match &ctx {
        Ctx::PropValue(k) => Some(k.dim()),
        _ => None,
    };
    let stmt = ctx == Ctx::Statement;
    // priorities: 0 best fit for the context ... 6 unlikely
    if let Some(a) = analysis {
        let mut seen = std::collections::HashSet::new();
        for (i, v) in a.model.vars.iter().enumerate() {
            let (label, insert) = if v.index.is_empty() {
                (v.display.clone(), v.display.clone())
            } else {
                let base = v.display.split('[').next().unwrap_or("").to_string();
                (format!("{base}[]"), format!("{base}["))
            };
            if label.eq_ignore_ascii_case(prefix) || !seen.insert(label.to_lowercase()) {
                continue;
            }
            let dim = a.model.var_dim(i as u32);
            let prio = match want {
                Some(w) => match dim {
                    Some(d) if d == w => 0,
                    None if looks_like(&v.base, w) => 1,
                    None => 2,
                    Some(_) => 4,
                },
                None => 1,
            };
            let mut detail = dim.and_then(|d| d.name()).map(str::to_string).unwrap_or_default();
            if v.index.is_empty() {
                if let Some(sol) = &a.solution {
                    if sol.known[i] {
                        let (x, u) = crate::app::display_in(&Default::default(), a, i, sol.x[i]);
                        detail = format!("= {} {u}", crate::editor::fmt_num(x)).trim_end().to_string();
                    }
                }
            }
            push(label, insert, Kind::Variable, detail, prio);
        }
        for cell in &a.model.funcs {
            if let Some(src) = &cell.source {
                let sig = if src.is_procedure {
                    format!("Call {}({} : {})", src.display, src.params.join(", "), src.outputs.join(", "))
                } else {
                    format!("{}({})", src.display, src.params.join(", "))
                };
                push(src.display.clone(), format!("{}(", src.display), Kind::UserFunction, sig, if want.is_some() { 3 } else { 2 });
            }
        }
    }
    for (f, doc) in PROPERTY_FUNCTIONS {
        let ins = match *f {
            "propssi" => "PropsSI(".to_string(),
            "props1si" => "Props1SI(".to_string(),
            "hapropssi" => "HAPropsSI(".to_string(),
            _ => format!("{f}("),
        };
        let fits = want.is_some_and(|w| property_function(f).is_some_and(|k| k.dim() == w));
        push(ins.trim_end_matches('(').to_string(), ins, Kind::Property, doc.to_string(), if fits { 1 } else { 3 });
    }
    for (f, doc) in dfcore::compile::MATH_FUNCTIONS {
        push(f.to_string(), format!("{f}("), Kind::Function, doc.to_string(), 3);
    }
    for (c, doc) in dfcore::compile::BUILTIN_CONSTS {
        push(c.to_string(), c.to_string(), Kind::Constant, doc.to_string(), 4);
    }
    // keywords: only at the start of a statement, or the logical operators
    if prefix.chars().count() >= 2 || stmt {
        for k in KEYWORDS {
            let logical = matches!(*k, "and" | "or" | "not");
            if stmt || logical {
                push(k.to_string(), k.to_string(), Kind::Keyword, String::new(), if stmt { 2 } else { 5 });
            }
        }
    }
    if stmt {
        for (d, doc) in DIRECTIVES {
            push(d.to_string(), format!("{d} "), Kind::Directive, doc.to_string(), 5);
        }
    }
    let mut v = finish(out);
    // a fuzzy match on a short prefix is noise when better matches exist
    if v.len() > 50 {
        v.truncate(200);
    }
    v
}

/// Whether a variable of unknown dimension is probably of dimension `d`,
/// from its name (`P_cond`, `p2`, `T_hot`, `h_in`).
fn looks_like(base: &str, d: Dim) -> bool {
    let first = base.split('_').next().unwrap_or(base).trim_end_matches(|c: char| c.is_ascii_digit());
    let letters: &[&str] = match d {
        Dim::PRESSURE => &["p"],
        Dim::TEMP => &["t"],
        Dim::SPEC_ENERGY => &["h", "u"],
        Dim::SPEC_ENTROPY => &["s"],
        Dim::SPEC_VOLUME => &["v"],
        Dim::DENSITY => &["d", "rho"],
        Dim::NONE => &["x", "q"],
        _ => &[],
    };
    letters.contains(&first)
}

/// Sort by match, then priority, then name (lists in a given order).
fn finish_alpha(mut out: Vec<Scored>) -> Vec<Candidate> {
    out.sort_by(|a, b| (a.tier, a.prio).cmp(&(b.tier, b.prio)).then_with(|| a.c.label.to_lowercase().cmp(&b.c.label.to_lowercase())));
    out.into_iter().map(|s| s.c).collect()
}

/// Sort by match, then priority, then shorter names.
fn finish(mut out: Vec<Scored>) -> Vec<Candidate> {
    out.sort_by(|a, b| (a.tier, a.prio, a.c.label.len()).cmp(&(b.tier, b.prio, b.c.label.len())).then_with(|| a.c.label.to_lowercase().cmp(&b.c.label.to_lowercase())));
    out.into_iter().map(|s| s.c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};

    const SCRIPT: &str = "P_cond = 0.1 [bar]\nP_boil = 150 [bar]\nT_in = 550 [C]\nm_dot = 50 [kg/s]\nh_in = enthalpy(Water, P=P_boil, T=T_in)\nQ = m_dot*h_in\n";

    fn analyse(s: &str) -> crate::analysis::Analysis {
        run(Request { gen: 1, text: s.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() })
    }

    fn at_end(text: &str, a: &crate::analysis::Analysis) -> Vec<Candidate> {
        let cur = text.chars().count();
        let chars: Vec<char> = text.chars().collect();
        let mut s = cur;
        while s > 0 && (chars[s - 1].is_alphanumeric() || chars[s - 1] == '_') {
            s -= 1;
        }
        let prefix: String = chars[s..].iter().collect();
        candidates(text, s, &prefix, Some(a), &["Water".into(), "Acetone".into(), "R134a".into()])
    }

    #[test]
    fn contexts() {
        assert_eq!(context("h = enthalpy(", 13, None), Ctx::Fluid);
        assert_eq!(context("h = enthalpy(Wa", 13, None), Ctx::Fluid);
        assert_eq!(context("h = enthalpy(Air, ", 18, None), Ctx::PropInput { func: "enthalpy".into(), used: vec![] });
        assert_eq!(context("h = enthalpy(Air, P=p1, ", 24, None), Ctx::PropInput { func: "enthalpy".into(), used: vec!["p".into()] });
        assert_eq!(context("h = enthalpy(Air, P=", 20, None), Ctx::PropValue(Key::P));
        assert_eq!(context("h = enthalpy(Air; T=", 20, None), Ctx::PropValue(Key::T));
        assert_eq!(context("h = sin(", 8, None), Ctx::Expr);
        assert_eq!(context("  ", 2, None), Ctx::Statement);
        assert_eq!(context("x = 10 [", 8, None), Ctx::Unit(None));
        assert!(auto_trigger("h = enthalpy(", 13));
        assert!(auto_trigger("h = enthalpy(Air, P=", 20));
        assert!(auto_trigger("h = enthalpy(Air, ", 18));
        assert!(!auto_trigger("h = sin(", 8));
        assert!(!auto_trigger("x = ", 4));
    }

    #[test]
    fn fluids_first_in_property_calls() {
        let a = analyse(SCRIPT);
        let c = at_end(&format!("{SCRIPT}h2 = entropy("), &a);
        assert!(c.iter().all(|c| c.kind == Kind::Fluid));
        assert_eq!(c[0].label, "Water", "the script's fluid first: {:?}", &c[..3]);
        assert_eq!(c[1].label, "Air");
        assert!(c.iter().any(|c| c.label == "Acetone"));
        // typed prefix
        let c = at_end(&format!("{SCRIPT}h2 = entropy(R1"), &a);
        assert_eq!(c[0].label, "R134a");
    }

    #[test]
    fn inputs_then_values_by_dimension() {
        let a = analyse(SCRIPT);
        let c = at_end(&format!("{SCRIPT}h2 = entropy(Water, P=P_boil, "), &a);
        assert!(!c.iter().any(|c| c.label == "P="), "P already given");
        assert_eq!(c[0].label, "T=");
        let c = at_end(&format!("{SCRIPT}h2 = entropy(Water, P="), &a);
        let first: Vec<&str> = c.iter().take(2).map(|c| c.label.as_str()).collect();
        assert!(first.contains(&"P_cond") && first.contains(&"P_boil"), "{first:?}");
        let c = at_end(&format!("{SCRIPT}h2 = entropy(Water, P=P_boil, T="), &a);
        assert_eq!(c[0].label, "T_in");
        // prefix inside a name: "cond" finds P_cond
        let c = at_end(&format!("{SCRIPT}y = cond"), &a);
        assert!(c.iter().any(|c| c.label == "P_cond"));
    }

    #[test]
    fn units_of_the_left_side_first() {
        let a = analyse(SCRIPT);
        let c = at_end(&format!("{SCRIPT}P_cond = 2 ["), &a);
        assert_eq!(parse_unit(&c[0].label).unwrap().dim, Dim::PRESSURE, "{:?}", c[0]);
    }

    #[test]
    fn match_tiers() {
        assert_eq!(match_tier("P_cond", "p"), Some(0));
        assert_eq!(match_tier("P_cond", "cond"), Some(1));
        assert_eq!(match_tier("mDot", "dot"), Some(1));
        assert_eq!(match_tier("enthalpy", "eny"), Some(2));
        assert_eq!(match_tier("enthalpy", "z"), None);
        assert_eq!(script_fluids("a = enthalpy(Water, P=1, T=2)\nb = density(R134a; P=1; T=3)"), vec!["Water", "R134a"]);
    }
}
