//! EES language features: string variables, $Constant, $Include, lookup
//! tables, integral, arrays in functions, modules, heating values,
//! Call Warning and the molar unit system.

use dfcore::compile::Files;
use dfcore::{compile, solve, CompileOptions, SolveOptions};
use std::collections::HashMap;

fn opts(files: &[(&str, &str)]) -> CompileOptions {
    let map: HashMap<String, String> = files.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    CompileOptions { files: Files::new(move |n| map.get(n).cloned()), ..Default::default() }
}

fn run_with(src: &str, o: &CompileOptions) -> (dfcore::Model, dfcore::Solution) {
    let m = compile(src, o);
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}", s.message);
    (m, s)
}

fn val(m: &dfcore::Model, s: &dfcore::Solution, n: &str) -> f64 {
    s.x[m.var(n).unwrap_or_else(|| panic!("no variable {n}")) as usize]
}

const COPPER: &str = "T [K],k [W/m-K]\n200,413\n250,406\n300,401\n400,393\n500,386\n600,379\n";

#[test]
fn include_constants_and_string_variables() {
    let o = opts(&[("lib.dfs", "Function dbl(x)\n  dbl := 2*x\nEnd\nzz = 4\n")]);
    let src = "$Include lib.dfs\n$Constant L0# = 3 [m]\n$Constant G#$ = 'two'\nN$ = G#$\ny = dbl(L0#)\nz = zz + 1\n";
    let (m, s) = run_with(src, &o);
    assert_eq!(val(&m, &s, "y"), 6.0);
    assert_eq!(val(&m, &s, "z"), 5.0);
    assert_eq!(m.strings.get("n$").or(m.strings.get("N$")).map(String::as_str), Some("two"));
    // a missing include file is an error at the directive
    let bad = compile("$Include nothere.dfs\nx = 1\n", &o);
    assert!(bad.has_errors());
    // built-in constants are not redefined
    assert!(compile("$Constant g# = 9.7\nx = g#\n", &o).has_errors());
}

#[test]
fn lookup_tables_and_interpolation() {
    let o = opts(&[("copper.csv", COPPER), ("eps.csv", "0,300,400\n0.5,0.1,0.2\n1.0,0.3,0.5\n")]);
    let src = "$UnitSystem SI K kPa kJ mass\n\
        k1 = interpolate1('copper', 'T', 'k', T=350)\n\
        T2 = interpolate1('copper', T, k, k=390)\n\
        r = lookup('copper', 2.5, 'k')\n\
        r2 = lookup('copper', 3, 2)\n\
        e = interpolate2dm('eps', 350, 0.75)\n";
    let (m, s) = run_with(src, &o);
    assert!((val(&m, &s, "k1") - 397.0).abs() < 1e-9);
    assert!((val(&m, &s, "T2") - (400.0 + 100.0 * 3.0 / 7.0)).abs() < 1e-9);
    assert!((val(&m, &s, "r") - 403.5).abs() < 1e-9 && val(&m, &s, "r2") == 401.0);
    assert!((val(&m, &s, "e") - 0.275).abs() < 1e-12);
    // the T column is in kelvin: a model in C gets the converted values
    let (m, s) = run_with("$UnitSystem SI C kPa kJ mass\nk = interpolate1('copper', 'T', 'k', T=76.85)\n", &o);
    assert!((val(&m, &s, "k") - 397.0).abs() < 1e-6);
}

#[test]
fn integrals() {
    let (m, s) = run_with("a = integral(x^2, x, 0, 3)\nb = integral(exp(-x^2), x, 0, 1)\nc = integral(x*k, x, 0, u)\nu = 2\nk = 3\n", &CompileOptions::default());
    assert!((val(&m, &s, "a") - 9.0).abs() < 1e-12);
    assert!((val(&m, &s, "b") - 0.746_824_132_812_427).abs() < 1e-12);
    assert!((val(&m, &s, "c") - 6.0).abs() < 1e-12);
}

#[test]
fn arrays_in_functions_and_modules() {
    let src = "Function pinch(N)\n  T[0] := 10\n  i := 0\n  Repeat\n    i := i + 1\n    T[i] := 10 + i^2\n  Until (i >= N)\n  \
               m := 0\n  i := 0\n  Repeat\n    i := i + 1\n    m := max(m, T[i] - T[i-1])\n  Until (i >= N)\n  pinch := m\nEnd\n\
               Module sq(x : y)\n  y = q^2\n  q = x + 1\nEnd\n\
               p = pinch(4)\nCall sq(2 : y1)\nCall sq(3 : y2)\n";
    let (m, s) = run_with(src, &CompileOptions::default());
    assert_eq!(val(&m, &s, "p"), 7.0);
    assert_eq!(val(&m, &s, "y1"), 9.0);
    assert_eq!(val(&m, &s, "y2"), 16.0);
    // each use of the module has its own local variables
    assert!(m.var("sq1.q").is_some() || m.var("sq1.Q").is_some(), "{:?}", m.var_index.keys().collect::<Vec<_>>());
}

#[test]
fn call_warning_reaches_the_message() {
    let src = "Function f(x)\n  If x > 1 Then Call Warning('x = XXXF2 is high', x)\n  f := 2*x\nEnd\ny = f(3)\n";
    let (m, s) = run_with(src, &CompileOptions::default());
    assert_eq!(val(&m, &s, "y"), 6.0);
    assert!(s.message.contains("Warning: x = 3.00 is high"), "{}", s.message);
}

#[cfg(feature = "coolprop")]
#[test]
fn heating_values_and_molar_units() {
    let (m, s) = run_with("$UnitSystem SI K kPa kJ mass\na = lhv(CH4)\nb = hhv(Methane)\nc = lhv('Methane[0.9]&Nitrogen[0.1]')\n", &CompileOptions::default());
    assert!((val(&m, &s, "a") - 50_009.0).abs() < 20.0 && (val(&m, &s, "b") - 55_496.0).abs() < 20.0);
    // per kg of mixture: 0.9*802.3 MJ/kmol over 0.9*16.04 + 0.1*28.01 kg/kmol
    assert!((val(&m, &s, "c") - 0.9 * 802_301.0 / (0.9 * 16.043 + 0.1 * 28.013)).abs() < 5.0);
    let src = "$UnitSystem SI MOLAR K kPa kJ\nT = 300\nP = 100\n\
        h = enthalpy(Nitrogen, T=T, P=P); v = volume(Nitrogen, T=T, P=P); rho = density(Nitrogen, T=T, P=P)\n\
        cpm = cp(Nitrogen, T=T, P=P); T2 = temperature(Nitrogen, h=h + 1000, P=P)\n\
        hm = enthalpy(Nitrogen, T=T, P=P)/molarmass(Nitrogen)\nl = lhv(Methane)\n";
    let (m, s) = run_with(src, &CompileOptions::default());
    let v = |n: &str| val(&m, &s, n);
    // ideal-gas molar volume R T / P within Z of nitrogen
    assert!((v("v") * v("P") / (8.314462618 * v("T")) - 1.0).abs() < 1e-3);
    assert!((v("rho") * v("v") - 1.0).abs() < 1e-12);
    assert!((v("cpm") - 29.17).abs() < 0.03);
    assert!((v("T2") - v("T") - 1000.0 / v("cpm")).abs() < 0.1);
    assert!((v("l") - 802_301.0).abs() < 1.0);
    // per kg again when divided by the molar mass
    let (mm, sm) = run_with("$UnitSystem SI K kPa kJ mass\nh = enthalpy(Nitrogen, T=300, P=100)\n", &CompileOptions::default());
    assert!((v("hm") - val(&mm, &sm, "h")).abs() < 1e-9);
}

#[cfg(feature = "coolprop")]
#[test]
fn hot_gas_mixtures() {
    // natural gas above the methane T_max (625 K): CoolProp's own P-h and P-s
    // flashes fail there; the fallback search in T finds the state
    let ng = "'Methane[0.9]&Ethane[0.05]&n-Propane[0.02]&Nitrogen[0.02]&CO2[0.01]'";
    let src = format!(
        "$UnitSystem SI C bar kJ mass\nT1 = 800\nP1 = 1.01325\nh1 = enthalpy({ng}, T=T1, P=P1)\ns1 = entropy({ng}, T=T1, P=P1)\n\
         T2 = temperature({ng}, P=P1, h=h1)\nT3 = temperature({ng}, P=P1, s=s1)\nh4 = enthalpy({ng}, P=20, s=s1)\nT4 = temperature({ng}, P=20, h=h4)\n"
    );
    let (m, s) = run_with(&src, &CompileOptions::default());
    let v = |n: &str| val(&m, &s, n);
    assert!((v("T2") - 800.0).abs() < 1e-8 && (v("T3") - 800.0).abs() < 1e-8, "{} {}", v("T2"), v("T3"));
    assert!(v("T4") > 1000.0 && v("T4") < 1300.0, "{}", v("T4"));
    // ideal-gas mixture of flue gas: within 0.1 % of the real mixture at
    // 800 C and 1 atm; SO2, which has no interaction parameters with N2 in
    // CoolProp, works in the ideal-gas mixture
    let flue = "Nitrogen[0.75]&CO2[0.08]&Water[0.10]&Oxygen[0.06]&Argon[0.01]";
    let src = format!(
        "$UnitSystem SI C bar kJ mass\nT = 800\nP = 1.01325\ncp_r = cp('{flue}', T=T, P=P)\ncp_i = cp('{flue}::Ideal', T=T, P=P)\n\
         dh_r = enthalpy('{flue}', T=T, P=P) - enthalpy('{flue}', T=300, P=P)\ndh_i = enthalpy('{flue}::Ideal', T=T, P=P) - enthalpy('{flue}::Ideal', T=300, P=P)\n\
         d_r = density('{flue}', T=T, P=P)\nd_i = density('{flue}::Ideal', T=T, P=P)\n\
         h_s = enthalpy('Nitrogen[0.75]&CO2[0.08]&Water[0.10]&Oxygen[0.06]&SulfurDioxide[0.01]::Ideal', T=T, P=P)\n"
    );
    let (m, s) = run_with(&src, &CompileOptions::default());
    let v = |n: &str| val(&m, &s, n);
    for (a, b) in [("cp_r", "cp_i"), ("dh_r", "dh_i"), ("d_r", "d_i")] {
        assert!((v(a) / v(b) - 1.0).abs() < 1e-3, "{a} = {}, {b} = {}", v(a), v(b));
    }
    assert!(v("h_s").is_finite());
}

#[cfg(feature = "coolprop")]
#[test]
fn adiabatic_flame_temperature_of_methane() {
    // CH4 + 2 (O2 + 3.76 N2) -> CO2 + 2 H2O + 7.52 N2, frozen composition
    // (no dissociation): about 2330 K (Turns, An Introduction to
    // Combustion, ex. 2.5, estimates 2318 K with a constant cp)
    let src = "$UnitSystem SI K kPa kJ molar\n\
        h_R = enthalpy(Methane, T=298.15) + 2*enthalpy(Oxygen, T=298.15) + 7.52*enthalpy(Nitrogen, T=298.15)\n\
        h_R = 10.52*enthalpy('CO2[1]&Water[2]&Nitrogen[7.52]::Ideal', T=T_ad)\n$Guess T_ad = 2000\n\
        q = h_R - 10.52*enthalpy('CO2[1]&Water[2]&Nitrogen[7.52]::Ideal', T=298.15)\n";
    let (m, s) = run_with(src, &CompileOptions::default());
    assert!((val(&m, &s, "T_ad") - 2328.0).abs() < 3.0, "{}", val(&m, &s, "T_ad"));
    // the heat of reaction is the LHV
    assert!((val(&m, &s, "q") - 802_301.0).abs() < 1.0);
}
