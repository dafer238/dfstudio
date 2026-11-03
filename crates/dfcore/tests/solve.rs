use dfcore::{compile, solve, CompileOptions, SolveOptions, VarStatus};

fn run(src: &str) -> (dfcore::Model, dfcore::Solution) {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    (m, s)
}

fn val(m: &dfcore::Model, s: &dfcore::Solution, n: &str) -> f64 {
    s.x[m.var(n).unwrap() as usize]
}

#[test]
fn nonlinear_block() {
    let (m, s) = run("x^2 + y^2 = 25\nx - y = 1\n$Guess x = 3, y = 2");
    assert!(s.converged, "{}", s.message);
    assert!((val(&m, &s, "x") - 4.0).abs() < 1e-9);
    assert!((val(&m, &s, "y") - 3.0).abs() < 1e-9);
}

#[test]
fn duplicate_sum_and_functions() {
    let src = r#"
N = 5
Duplicate i = 1, N
  x[i] = i^2
End
S = Sum(x[i], i=1, N)
Function clip(a, lo, hi)
  If a < lo Then
    clip := lo
  ElseIf a > hi Then
    clip := hi
  Else
    clip := a
  EndIf
End
c = clip(S, 0, 50)
Procedure split(m, f : m1, m2)
  m1 := f*m
  m2 := m - m1
End
Call split(10, 0.3 : a, b)
"#;
    let (m, s) = run(src);
    assert!(s.converged, "{}", s.message);
    assert_eq!(val(&m, &s, "S"), 55.0);
    assert_eq!(val(&m, &s, "c"), 50.0);
    assert!((val(&m, &s, "a") - 3.0).abs() < 1e-12);
    assert!((val(&m, &s, "b") - 7.0).abs() < 1e-12);
}

#[test]
fn structural_feedback() {
    let m = compile("a = 1\nb + c = a\nd = 2\nd = 3", &CompileOptions::default());
    let plan = dfcore::Plan::new(&m, &[]);
    let st = &plan.structure;
    assert_eq!(st.var_status[m.var("a").unwrap() as usize], VarStatus::Determined);
    assert_eq!(st.var_status[m.var("b").unwrap() as usize], VarStatus::UnderDetermined);
    assert_eq!(st.var_status[m.var("d").unwrap() as usize], VarStatus::OverDetermined);
    assert_eq!(st.dof, 1);
}

#[test]
fn units_literals() {
    let (m, s) = run("$UnitSystem SI\nT = 25 [C]\nP = 2 [bar]\nh = 1 [kJ/kg]");
    assert!((val(&m, &s, "T") - 298.15).abs() < 1e-12);
    assert!((val(&m, &s, "P") - 2e5).abs() < 1e-9);
    assert!((val(&m, &s, "h") - 1e3).abs() < 1e-12);
}

#[cfg(feature = "coolprop")]
#[test]
fn rankine_cycle_coolprop() {
    let src = include_str!("../../../examples/rankine.dfs");
    let (m, s) = run(src);
    assert!(s.converged, "{}", s.message);
    // Saturated liquid at 0.1 bar: h = 191.81 kJ/kg (IAPWS-95 via CoolProp)
    assert!((val(&m, &s, "h[1]") - 191.806).abs() < 1e-2);
    let eta = val(&m, &s, "eta_th");
    assert!(eta > 0.36 && eta < 0.38, "eta = {eta}");
}

#[cfg(feature = "coolprop")]
#[test]
fn implicit_property_inversion() {
    // T from h at fixed P solved implicitly through enthalpy(): Newton on a property call
    let (m, s) = run("P = 100 [bar]\nh = 3000\nh = enthalpy(Water, P=P, T=T)\n$Guess T = 300");
    assert!(s.converged, "{}", s.message);
    let t = val(&m, &s, "T");
    let (m2, s2) = run("P = 100 [bar]\nh = 3000\nT = temperature(Water, P=P, h=h)");
    assert!((t - val(&m2, &s2, "T")).abs() < 1e-6);
}

#[test]
fn convert_and_converttemp() {
    let (m, s) = run("$UnitSystem SI C kPa kJ\nP = 10*convert(bar, kPa)\nf = convert(kJ/kg-K, J/kg-K)\nT_K = converttemp(C, K, 25)\nT_F = converttemp(C, F, 100)");
    assert!(s.converged, "{}", s.message);
    assert!((val(&m, &s, "P") - 1000.0).abs() < 1e-9);
    assert!((val(&m, &s, "f") - 1000.0).abs() < 1e-9);
    assert!((val(&m, &s, "T_K") - 298.15).abs() < 1e-9);
    assert!((val(&m, &s, "T_F") - 212.0).abs() < 1e-9);
    let bad = compile("x = convert(kJ, kg)", &CompileOptions::default());
    assert!(bad.has_errors());
}

#[cfg(feature = "coolprop")]
#[test]
fn humid_air_liquids_and_latent_heat() {
    // ASHRAE psychrometric chart, 30 C, 1 atm, 50 % RH: w = 0.0134, Twb = 22.0 C
    let (m, s) = run("T = 30; P = 1.01325\nw = humrat(AirH2O, T=T, P=P, R=0.5)\nTwb = wetbulb(AirH2O, T=T, P=P, w=w)\nphi = relhum(AirH2O, T=T, P=P, B=Twb)\n\
         hfg = enthalpy_vaporization(Water, T=100)\n\
         h1 = enthalpy(Sodium, T=400, P=2); h2 = h1 + 100; T2 = temperature(Sodium, P=2, h=h2)\n\
         rho_pb = density(Lead, T=426.85, P=1)\nT_salt = temperature(SolarSalt, P=1, h=enthalpy(SolarSalt, T=400, P=1))");
    assert!(s.converged, "{}", s.message);
    assert!((val(&m, &s, "w") - 0.01337).abs() < 1e-4);
    assert!((val(&m, &s, "Twb") - 22.0).abs() < 0.05);
    assert!((val(&m, &s, "phi") - 0.5).abs() < 1e-6);
    assert!((val(&m, &s, "hfg") - 2256.4).abs() < 0.5);
    // sodium cp ~ 1.28 kJ/kg-K at 400 C: +100 kJ/kg is about +78 K
    assert!((val(&m, &s, "T2") - 478.5).abs() < 0.5);
    assert!((val(&m, &s, "rho_pb") - 10545.35).abs() < 0.01);
    assert!((val(&m, &s, "T_salt") - 400.0).abs() < 1e-6);
}

#[cfg(feature = "coolprop")]
#[test]
fn ideal_gas_from_one_input() {
    // NIST-JANAF (Chase 1998), CO2: H(500 K) - H(298.15 K) = 8.305 kJ/mol,
    // S(500 K, 1 bar) = 234.901 J/mol-K; M = 44.0095 g/mol
    let src = "$UnitSystem SI K kPa kJ mass\n\
        h0 = enthalpy(CO2, T=298.15)\n\
        h1 = enthalpy(CO2, T=500)\n\
        s1 = entropy(CO2, T=500)\n\
        u1 = intenergy(CO2, T=500)\n\
        cp1 = cp(CO2, T=500)\n\
        cv1 = cv(CO2, T=500)\n\
        T2 = temperature(CO2, h=h1)\n\
        hn = enthalpy(N2, T=298.15)\n";
    let m = dfcore::compile(src, &Default::default());
    assert!(!m.diagnostics.iter().any(|d| d.severity == dfcore::Severity::Error), "{:?}", m.diagnostics);
    let (_, sol) = dfcore::solve(&m, &Default::default());
    assert!(sol.converged, "{}", sol.message);
    let v = |n: &str| sol.x[m.vars.iter().position(|v| v.name.eq_ignore_ascii_case(n)).unwrap()];
    let mm = 44.0095;
    // CoolProp molar mass of CO2 differs from 44.0095 in the 5th digit
    assert!((v("h0") - (-393.522e3 / mm)).abs() < 0.1, "formation reference {}", v("h0"));
    assert!(((v("h1") - v("h0")) - 8.305e3 / mm).abs() < 0.5, "dh {}", v("h1") - v("h0"));
    let s_1atm = (234.901 - 8.314462618 * (1.01325f64).ln()) / mm;
    assert!((v("s1") - s_1atm).abs() < 0.005, "s {} vs {}", v("s1"), s_1atm);
    let r = 8.314462618 / mm;
    assert!((v("u1") - (v("h1") - r * 500.0)).abs() < 0.01);
    assert!((v("cp1") - v("cv1") - r).abs() < 1e-4);
    assert!((v("T2") - 500.0).abs() < 1e-6);
    assert!(v("hn").abs() < 0.01, "elements: h = 0 at 298.15 K");
}

#[test]
fn unit_after_an_equation_sets_the_computed_variable() {
    // d is computed by the second equation; its unit is bar (model: kPa)
    let src = "$UnitSystem SI C kPa kJ\nb = 500; c = 200; e = 100\na = b - c/d + e [bar]\na = 500\nT[1] = 2\nT = 3\nx = T [m]\ny = T[1]\n";
    let m = dfcore::compile(src, &Default::default());
    assert!(!m.diagnostics.iter().any(|d| d.severity == dfcore::Severity::Error), "{:?}", m.diagnostics);
    let d = m.vars.iter().position(|v| v.name == "d").unwrap();
    assert_eq!(m.vars[d].unit_hint.as_deref(), Some("bar"));
    let a = m.vars.iter().position(|v| v.name == "a").unwrap();
    assert_eq!(m.vars[a].unit_hint, None, "the unit goes to the computed variable, not the left side");
    // with a space and a unit, `T [m]` is a unit; `T[1]` stays an index
    let x = m.vars.iter().position(|v| v.name == "x").unwrap();
    assert_eq!(m.vars[x].unit_hint.as_deref(), Some("m"));
    let (_, sol) = dfcore::solve(&m, &Default::default());
    assert!(sol.converged);
    assert!((sol.x[d] - 2.0).abs() < 1e-6, "value in model units: 500 = 500 - 200/d + 100, got {}", sol.x[d]);
    // EES names with `|`
    let m = dfcore::compile("h|o_f = 3\n", &Default::default());
    assert!(m.diagnostics.is_empty(), "{:?}", m.diagnostics);
}

#[cfg(feature = "coolprop")]
#[test]
fn temperature_and_enthalpy_fix_a_state() {
    // CoolProp has no T-h flash; the pressure is searched. A compressed
    // liquid (EES PhD_ST state 6: P = 2.863 bar, T = 35.74 C) is recovered.
    let val = |src: &str, name: &str| {
        let m = dfcore::compile(src, &Default::default());
        let (_, sol) = dfcore::solve(&m, &Default::default());
        assert!(sol.converged, "{src}");
        sol.x[m.vars.iter().position(|v| v.name == name).unwrap()]
    };
    let liq = "$UnitSystem SI C bar kJ\nT = 35.74\nh = enthalpy(Water, P=2.863, T=T)\np = pressure(Water, T=T, h=h)\ns = entropy(Water, T=T, h=h)\n";
    assert!((val(liq, "p") - 2.863).abs() < 1e-6);
    assert!((val(liq, "s") - 0.5150557).abs() < 1e-6);
    // wet and superheated states
    let wet = "$UnitSystem SI C bar kJ\nx = quality(Water, T=35.74, h=1000)\n";
    assert!((val(wet, "x") - 0.351912).abs() < 1e-5);
    let vap = "$UnitSystem SI C bar kJ\nh = enthalpy(Water, T=200, P=2)\np = pressure(Water, T=200, h=h)\n";
    assert!((val(vap, "p") - 2.0).abs() < 1e-6);
    // supercritical, and T-u
    let sc = "$UnitSystem SI C bar kJ\nu = intenergy(CO2, T=50, P=200)\np = pressure(CO2, T=50, u=u)\n";
    assert!((val(sc, "p") - 200.0).abs() < 1e-5);
    // no state: a clear message
    let m = dfcore::compile("$UnitSystem SI C bar kJ\ns = entropy(Water, T=35.74, h=149.5)\n", &Default::default());
    let (_, sol) = dfcore::solve(&m, &Default::default());
    assert!(!sol.converged);
}
