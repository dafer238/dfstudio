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
    let (m, s) = run("T = 30; P = 1.01325\nw = humrat(AirH2O, T=T, P=P, R=0.5)\nTwb = wetbulb(HumidAir, T=T, P=P, w=w)\nphi = relhum(moist_air, T=T, P=P, B=Twb)\n\
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

/// Without guesses, temperatures start at 25 °C and pressures at 1 bar
/// (not 1 K / 1 Pa), so an inverse property calculation converges in SI.
#[test]
fn automatic_start_values_si() {
    let src = "$UnitSystem SI\nh = 400000 [J/kg]\nP = 2 [bar]\nh = enthalpy(Water, T=T, P=P)";
    let (m, s) = run(src);
    assert!(s.converged, "{}", s.message);
    let t = val(&m, &s, "T");
    let h = dfcore::props::backend::props("Water", dfcore::props::Key::H, &[dfcore::props::Key::T, dfcore::props::Key::P], &[t, 2e5], None).unwrap();
    assert!((h - 4e5).abs() < 1e-3, "T = {t} K, h = {h}");
    // automatic bounds: T is a temperature input of a property call
    let v = m.var("T").unwrap();
    assert!(m.auto_bounds.contains(&v));
    assert!(m.bounds[&v].0 > 0.0);
}

/// A variable used as a quality input is bounded to 0..1 and starts at 0.5.
#[test]
fn quality_bounds_and_start() {
    let src = "$UnitSystem Eng\nP = 1 [bar]\nh = 1500\nh = enthalpy(Water, P=P, x=q)";
    let (m, s) = run(src);
    assert!(s.converged, "{}", s.message);
    let v = m.var("q").unwrap();
    assert_eq!(m.bounds[&v], (0.0, 1.0));
    let q = val(&m, &s, "q");
    assert!(q > 0.0 && q < 1.0, "q = {q}");
}

/// Guesses win over the previous solution, which wins over automatic values.
#[test]
fn start_value_precedence() {
    use dfcore::solver::{start_values, StartSource};
    let m = compile("x^2 = 4\ny^2 = 9\nz^2 = 16\n$Guess x = -1", &CompileOptions::default());
    let n = m.vars.len();
    let mut warm = vec![f64::NAN; n];
    warm[m.var("x").unwrap() as usize] = 5.0;
    warm[m.var("y").unwrap() as usize] = -2.0;
    let (x, src) = start_values(&m, &SolveOptions::default(), Some(&warm));
    let i = |n: &str| m.var(n).unwrap() as usize;
    assert_eq!((x[i("x")], src[i("x")]), (-1.0, StartSource::Guess));
    assert_eq!((x[i("y")], src[i("y")]), (-2.0, StartSource::Previous));
    assert_eq!((x[i("z")], src[i("z")]), (1.0, StartSource::Automatic));
}

#[test]
fn special_functions() {
    // reference values: Abramowitz & Stegun (1972), tables 7.1, 6.1, 9.1, 9.8
    let (m, s) = run("a = erf(0.5); b = erfc(0.5); g = gamma_(4.5); f = factorial(5); f0 = factorial(-2)\n\
         j0 = bessel_j0(1); j1 = bessel_j1(1); y0 = bessel_y0(1); y1 = bessel_y1(1)\n\
         i0 = bessel_i0(1); i1 = bessel_i1(1); k0 = bessel_k0(1); k1 = bessel_k1(1); i5 = bessel_i0(5); k5 = bessel_k1(5)\n\
         av = average(1, 2, 6)\nerf(x) = 0.5\nbessel_i1(w) = 1");
    assert!(s.converged, "{}", s.message);
    let near = |n: &str, v: f64, tol: f64| assert!((val(&m, &s, n) - v).abs() <= tol * v.abs(), "{n} = {} (expected {v})", val(&m, &s, n));
    near("a", 0.5204998778, 1e-9);
    near("b", 0.4795001222, 1e-9);
    near("g", 11.6317283966, 1e-9);
    near("f", 120.0, 0.0);
    near("f0", 1.0, 0.0);
    near("j0", 0.7651976866, 1e-9);
    near("j1", 0.4400505857, 1e-9);
    near("y0", 0.0882569642, 1e-8);
    near("y1", -0.7812128213, 1e-9);
    // modified Bessel functions: polynomial approximations, ~2e-7
    near("i0", 1.2660658778, 3e-7);
    near("i1", 0.5651591040, 3e-7);
    near("k0", 0.4210244382, 3e-7);
    near("k1", 0.6019072302, 3e-7);
    near("i5", 27.239871823, 3e-7);
    near("k5", 0.0040446134, 3e-7);
    near("av", 3.0, 0.0);
    // inverse through the derivatives: erf^-1(0.5), I1^-1(1)
    near("x", 0.4769362762, 1e-8);
    near("w", 1.5183471012, 1e-6);
}

#[test]
fn call_error_stops_with_the_message() {
    let (_, s) = run("Function f(x)\n  If x < 0 Then Call Error('x must be positive, got XXXF2', x)\n  f := sqrt(x)\nEnd\na = f(-3)");
    assert!(!s.converged);
    assert!(s.message.contains("x must be positive, got -3.00"), "{}", s.message);
    assert_eq!(dfcore::funcs::error_message("T = XXXE2 K, too low", Some(1234.5)), "T = 1.23e3 K, too low");
    assert_eq!(dfcore::funcs::error_message("n = XXX", Some(3.0)), "n = 3");
}

#[cfg(feature = "coolprop")]
#[test]
fn more_thermophysical_functions() {
    // EES manual examples: steam at 870 K, 35 MPa: k = 1.331 (cp/cv = 1.615),
    // kappa = 3.466e-5 1/kPa; water: T_triple = 273.16 K, p_triple = 611.655 Pa,
    // omega = 0.3443, T_nbp = 373.124 K (IAPWS-95)
    let (m, s) = run("$UnitSystem SI K kPa kJ mass\n\
         k = isentropicexponent(Steam, T=870, P=35000); kappa = isothermalcompress(Steam, T=870, P=35000)\n\
         r = cp(Steam, T=870, P=35000)/cv(Steam, T=870, P=35000)\n\
         nu = kinematicviscosity(Water, T=300, P=101.325); mu = viscosity(Water, T=300, P=101.325); rho = density(Water, T=300, P=101.325)\n\
         al = thermaldiffusivity(Water, T=300, P=101.325); kw = conductivity(Water, T=300, P=101.325); c = cp(Water, T=300, P=101.325)\n\
         beta = volexpcoef(Water, T=300, P=101.325)\n\
         Tt = t_triple(Water); pt = p_triple(Water); om = acentricfactor(Water); Tb = normalboilingpt(Water); vc = v_crit(Water)\n\
         kig = isentropicexponent(Air, T=300); big = volexpcoef(Air::Ideal, T=300, P=100); kapig = isothermalcompress(Air::Ideal, T=300, P=100)\n\
         nu_oil = kinematicviscosity(Therminol66, T=400, P=200); b_oil = volexpcoef(Therminol66, T=400, P=200)");
    assert!(s.converged, "{}", s.message);
    let v = |n: &str| val(&m, &s, n);
    assert!((v("k") - 1.331).abs() < 1e-3 && (v("r") - 1.615).abs() < 1e-3);
    assert!((v("kappa") - 3.466e-5).abs() < 1e-8);
    assert!((v("nu") - v("mu") / v("rho")).abs() < 1e-15);
    assert!((v("al") - v("kw") / (v("rho") * v("c") * 1e3)).abs() < 1e-15);
    // liquid water at 300 K: beta = 2.76e-4 1/K (IAPWS-95)
    assert!((v("beta") - 2.76e-4).abs() < 2e-6);
    assert!((v("Tt") - 273.16).abs() < 1e-9 && (v("pt") - 0.611655).abs() < 1e-5);
    assert!((v("om") - 0.3443).abs() < 1e-4 && (v("Tb") - 373.124).abs() < 1e-3);
    assert!((1.0 / v("vc") - 322.0).abs() < 1e-6);
    assert!((v("kig") - 1.4).abs() < 2e-3);
    assert!((v("big") - 1.0 / 300.0).abs() < 1e-12 && (v("kapig") - 0.01).abs() < 1e-12);
    assert!(v("nu_oil") > 0.0 && v("b_oil") > 1e-4 && v("b_oil") < 2e-3);
}
