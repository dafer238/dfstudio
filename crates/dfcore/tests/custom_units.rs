//! User-defined (custom) units: any token that is not a built-in unit is a
//! base unit of its own, with SI prefixes inferred from the other tokens of
//! the model.

use dfcore::parser::Severity;
use dfcore::units::{parse_unit, UnitSystem};
use dfcore::{compile, solve, CompileOptions, SolveOptions};

fn run(src: &str) -> (dfcore::Model, dfcore::Solution) {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}", s.message);
    (m, s)
}

/// Value of a variable in its display unit, and that unit.
fn shown(m: &dfcore::Model, s: &dfcore::Solution, n: &str) -> (f64, String) {
    let v = m.var(n).unwrap();
    let (mu, du) = m.display_unit(v).unwrap();
    (mu.convert(s.x[v as usize], &du), du.symbol)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn prefixes_follow_the_other_tokens() {
    let src = "C_inv = 12 [MEUR]\nc_om = 300 [kEUR/yr]\nOM = c_om*20 [yr]\nTotal = C_inv + OM [EUR]\n";
    let (m, s) = run(src);
    assert!(m.unit_conflicts.is_empty());
    let (x, u) = shown(&m, &s, "Total");
    assert_eq!(u, "EUR");
    assert!(close(x, 18e6), "{x}");
    // the model unit is the bare token: EUR, with time in s
    assert!(close(s.x[m.var("C_inv").unwrap() as usize], 12e6));
    let (x, u) = shown(&m, &s, "OM");
    assert_eq!(u, "EUR");
    assert!(close(x, 6e6), "{x}");
    // the info note names the base unit only
    let notes: Vec<_> = m.diagnostics.iter().filter(|d| d.severity == Severity::Info).map(|d| d.msg.clone()).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("'EUR'"));
    assert_eq!(m.undeclared_units, vec!["EUR".to_string()]);
}

#[test]
fn conversions_within_a_model() {
    let m = compile("a = 1 [EUR/yr]\nb = 1 [a/kb]\nc = 1 [b]\n$CustomUnits a b", &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    m.with_units(|| {
        let f = parse_unit("MEUR/yr").unwrap();
        let g = parse_unit("EUR/yr").unwrap();
        assert_eq!(f.dim, g.dim);
        assert!(close(f.convert(1.0, &g), 1e6));
        assert!(close(g.convert(1.0, &parse_unit("EUR/day").unwrap()), 1.0 / 365.0));
        // declared: a is not the ampere, b is a base unit, kb = 1000 b
        let ab = parse_unit("a/b").unwrap();
        let akb = parse_unit("a/kb").unwrap();
        assert_eq!(ab.dim, akb.dim);
        assert!(close(akb.convert(1.0, &ab), 1e-3));
        assert_ne!(parse_unit("a").unwrap().dim, parse_unit("A").unwrap().dim);
        // case matters: mEUR is milli
        assert!(close(parse_unit("mEUR").unwrap().factor, 1e-3));
    });
    assert!(m.undeclared_units.contains(&"EUR".to_string()));
    assert!(!m.undeclared_units.contains(&"a".to_string()));
}

#[test]
fn no_prefix_without_the_base_token() {
    // `pax` is not pico-`ax`, and `MEUR` alone is its own base unit
    let m = compile("n = 3 [pax]\nC = 2 [MEUR]", &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    m.with_units(|| {
        assert_eq!(parse_unit("pax").unwrap().factor, 1.0);
        assert_eq!(parse_unit("MEUR").unwrap().factor, 1.0);
        assert!(close(parse_unit("kpax").unwrap().factor, 1e3));
    });
    assert_eq!(m.undeclared_units, vec!["pax".to_string(), "MEUR".to_string()]);
    // two prefixed forms of the same token share it as base unit
    let m = compile("a = 3 [kEUR]\nb = 2 [MEUR]\nc = a + b", &CompileOptions::default());
    assert!(m.unit_conflicts.is_empty());
    m.with_units(|| assert!(close(parse_unit("MEUR").unwrap().convert(1.0, &parse_unit("kEUR").unwrap()), 1e3)));
    assert_eq!(m.undeclared_units, vec!["EUR".to_string()]);
    let m = compile("n = 3 [pax]\nC = 2 [MEUR]", &CompileOptions::default());
    assert_eq!(m.undeclared_units, vec!["pax".to_string(), "MEUR".to_string()]);
}

#[test]
fn mixed_with_physical_units() {
    // a price per kWh times an energy in the model unit (kJ) is in EUR
    let src = "c = 0.25 [EUR/kWh]\nP = 10 [kW]\nt = 2 [h]\nE = P*t\nCost = c*E\n";
    let (m, s) = run(src);
    assert!(m.unit_conflicts.is_empty(), "{:?}", m.unit_conflicts);
    let (x, u) = shown(&m, &s, "Cost");
    assert_eq!(u, "EUR");
    assert!(close(x, 5.0), "{x}");
    // the model unit of EUR/kWh in a kJ system is EUR/kJ
    let us = UnitSystem::engineering();
    let d = m.var_dim(m.var("c").unwrap()).unwrap();
    assert_eq!(us.unit_for(d).symbol, "EUR/kJ");
    // CO2 is a token (not CO squared); t-CO2 and kg-CO2 convert
    let m = compile("e = 0.2 [kg-CO2/kWh]", &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    m.with_units(|| {
        let a = parse_unit("t-CO2/MWh").unwrap();
        let b = parse_unit("kg-CO2/kWh").unwrap();
        assert_eq!(a.dim, b.dim);
        assert!(close(a.convert(1.0, &b), 1.0));
    });
}

#[test]
fn unit_check_flags_mixed_custom_dimensions() {
    let m = compile("a = 1 [EUR]\nb = 2 [EUR/yr]\nc = a + b", &CompileOptions::default());
    assert_eq!(m.unit_conflicts.len(), 1);
    let m = compile("a = 1 [EUR]\nb = 2 [USD]\nc = a + b", &CompileOptions::default());
    assert_eq!(m.unit_conflicts.len(), 1);
}

#[test]
fn typo_suggests_a_builtin_unit() {
    let m = compile("m = 3 [kgg]", &CompileOptions::default());
    assert!(!m.has_errors());
    let w: Vec<_> = m.diagnostics.iter().filter(|d| d.severity == Severity::Warning).collect();
    assert_eq!(w.len(), 1, "{:?}", m.diagnostics);
    assert!(w[0].msg.contains("did you mean 'kg'"), "{}", w[0].msg);
    // a declared unit is silent
    let m = compile("m = 3 [kgg]\n$CustomUnits kgg", &CompileOptions::default());
    assert!(m.diagnostics.is_empty(), "{:?}", m.diagnostics);
    assert!(m.undeclared_units.is_empty());
}

#[test]
fn comments_are_units_only_with_known_tokens() {
    // `{inlet}` is a comment, not a custom unit; `{MEUR}` names a known one
    let m = compile("x = 2 {inlet}\nC = 5 {MEUR}\nD = 1 [EUR]", &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    assert!(m.vars[m.var("x").unwrap() as usize].unit_hint.is_none());
    assert_eq!(m.vars[m.var("C").unwrap() as usize].unit_hint.as_deref(), Some("MEUR"));
}

#[test]
fn prefixed_builtin_units() {
    for (u, f) in [("Mt", 1e9), ("kt", 1e6), ("hPa", 100.0), ("kbar", 1e8), ("nm", 1e-9), ("mA", 1e-3), ("Mt3", 1e27)] {
        let p = parse_unit(u).unwrap_or_else(|e| panic!("{u}: {e}"));
        assert!(p.dim.has_custom() == false, "{u}");
        assert!(close(p.factor, f), "{u}: {}", p.factor);
    }
    // short tokens keep their custom reading
    for u in ["ct", "PS", "pt"] {
        assert!(parse_unit(u).unwrap().dim.has_custom(), "{u}");
    }
    // existing units are unchanged
    assert!(close(parse_unit("MPa").unwrap().factor, 1e6));
    assert!(close(parse_unit("m3").unwrap().factor, 1.0));
    assert_eq!(parse_unit("C").unwrap().offset, 273.15);
}
