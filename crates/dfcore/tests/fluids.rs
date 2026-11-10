//! Fluid names: aliases, case, mixtures and backend prefixes.

use dfcore::props::{backend, coolprop_fluid, fluid_aliases, FLUID_ALIASES};
use dfcore::{compile, solve, CompileOptions, SolveOptions};

fn values(src: &str, names: &[&str]) -> Vec<f64> {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}\n{src}", s.message);
    names.iter().map(|n| s.x[m.var(n).unwrap_or_else(|| panic!("{n}")) as usize]).collect()
}

/// Every entry of the alias table is a fluid CoolProp (or DeltaF Studio's
/// liquids) accepts, and no alias belongs to two fluids.
#[test]
fn alias_table_is_valid() {
    let mut seen = std::collections::HashMap::new();
    for (c, aliases) in FLUID_ALIASES {
        backend::check_fluid(c).unwrap_or_else(|e| panic!("{c}: {e}"));
        for a in aliases.iter().chain([c]) {
            if let Some(prev) = seen.insert(a.to_lowercase(), *c) {
                assert_eq!(prev, *c, "alias {a} of both {prev} and {c}");
            }
            assert_eq!(coolprop_fluid(a), *c, "{a}");
            assert_eq!(coolprop_fluid(&a.to_uppercase()), *c, "{a}");
            assert_eq!(coolprop_fluid(&a.to_lowercase()), *c, "{a}");
        }
    }
}

#[test]
fn names_in_any_case() {
    assert_eq!(coolprop_fluid("CarbonDioxide"), "CO2");
    assert_eq!(coolprop_fluid("r744"), "CO2");
    assert_eq!(coolprop_fluid("Steam"), "Water");
    assert_eq!(coolprop_fluid("h2o"), "Water");
    assert_eq!(coolprop_fluid("ToLuEnE"), "Toluene");
    assert_eq!(coolprop_fluid("r134A"), "R134a");
    assert_eq!(coolprop_fluid("propane"), "n-Propane");
    // backends and mixtures: the fluid part is mapped, the rest is kept
    assert_eq!(coolprop_fluid("IF97::Steam"), "IF97::Water");
    assert_eq!(coolprop_fluid("INCOMP::MEG-30%"), "INCOMP::MEG-30%");
    assert_eq!(coolprop_fluid("CarbonDioxide[0.9]&Ar[0.1]"), "CO2[0.9]&Argon[0.1]");
    // unknown names are passed through for CoolProp to report
    assert_eq!(coolprop_fluid("NoSuchFluid"), "NoSuchFluid");
    let a = fluid_aliases("co2");
    assert_eq!(a[0], "CO2");
    assert!(a.iter().any(|x| x == "CarbonDioxide") && a.iter().any(|x| x == "R744"), "{a:?}");
}

/// All aliases give the same state, for the real fluid, the ideal gas
/// (one input) and a mixture.
#[test]
fn aliases_give_the_same_properties() {
    let v = values(
        "h1 = enthalpy(CO2, P=100, T=50)\nh2 = enthalpy(CarbonDioxide, P=100, T=50)\nh3 = enthalpy(r744, P=100, T=50)\n\
         s1 = entropy(Water, P=10, T=300)\ns2 = entropy(H2O, P=10, T=300)\ns3 = entropy(Steam, P=10, T=300)\n\
         g1 = enthalpy(CO2, T=500)\ng2 = enthalpy(CarbonDioxide, T=500)\n\
         m1 = density('CO2[0.9]&Argon[0.1]', P=100, T=50)\nm2 = density('CarbonDioxide[0.9]&Ar[0.1]', P=100, T=50)\n\
         p1 = density(Propane, P=10, T=20)\np2 = density(C3H8, P=10, T=20)\np3 = density(R290, P=10, T=20)",
        &["h1", "h2", "h3", "s1", "s2", "s3", "g1", "g2", "m1", "m2", "p1", "p2", "p3"],
    );
    for (a, b) in [(0, 1), (0, 2), (3, 4), (3, 5), (6, 7), (8, 9), (10, 11), (10, 12)] {
        assert_eq!(v[a], v[b], "{a} {b}: {v:?}");
    }
}

fn errors(src: &str) -> Vec<String> {
    let m = compile(src, &CompileOptions::default());
    m.diagnostics.iter().filter(|d| d.severity == dfcore::Severity::Error).map(|d| d.msg.clone()).collect()
}

fn close(name: &str, a: f64, b: f64, rel: f64) {
    assert!((a - b).abs() <= rel * b.abs().max(1e-12), "{name}: {a} vs {b}");
}

/// `Fluid::Ideal` / `Fluid::Real`, and the model from the number of inputs.
#[test]
fn model_suffix_selects_the_backend() {
    let v = values(
        "T = 200; P = 1.01325\n\
         h_auto1 = enthalpy(CO2, T=T)\nh_ig = enthalpy(CO2::Ideal, T=T)\nh_ig2 = enthalpy(CarbonDioxide::ideal, T=T, P=50)\n\
         h_auto2 = enthalpy(CO2, T=T, P=50)\nh_real = enthalpy(CarbonDioxide::Real, T=T, P=50)\n\
         s_auto1 = entropy(CO2, T=T)\ns_ig = entropy(CO2::Ideal, T=T, P=P)\ns_ig10 = entropy(CO2::Ideal, T=T, P=10*P)\n\
         d_ig = density(CO2::Ideal, T=T, P=P)\nv_ig = volume(CO2::Ideal, T=T, P=P)\nz_ig = compressibilityfactor(CO2::Ideal, T=T, P=50)\n\
         T_ps = temperature(CO2::Ideal, P=10*P, s=s_ig10)\nP_ts = pressure(CO2::Ideal, T=T, s=s_ig10)\nT_h = temperature(CO2::Ideal, h=h_ig)\n\
         T_pv = temperature(CO2::Ideal, P=P, v=v_ig)\nP_tv = pressure(CO2::Ideal, T=T, d=d_ig)\nT_hs = temperature(CO2::Ideal, h=h_ig, s=s_ig10)\n\
         cp_ig = cp(CO2::Ideal, T=T, P=50); cv_ig = cv(CO2::Ideal, T=T); c_ig = soundspeed(CO2::Ideal, T=T, P=P)\n\
         d_low_real = density(CO2, T=T, P=1e-4); d_low_ig = density(CO2::Ideal, T=T, P=1e-4)\n\
         M = molarmass(CO2::Ideal)",
        &[
            "h_auto1",
            "h_ig",
            "h_ig2",
            "h_auto2",
            "h_real",
            "s_auto1",
            "s_ig",
            "s_ig10",
            "d_ig",
            "v_ig",
            "z_ig",
            "T_ps",
            "P_ts",
            "T_h",
            "T_pv",
            "P_tv",
            "T_hs",
            "cp_ig",
            "cv_ig",
            "c_ig",
            "d_low_real",
            "d_low_ig",
            "M",
        ],
    );
    let [h_auto1, h_ig, h_ig2, h_auto2, h_real, s_auto1, s_ig, s_ig10, d_ig, v_ig, z_ig, t_ps, p_ts, t_h, t_pv, p_tv, t_hs, cp_ig, cv_ig, c_ig, d_lr, d_li, m] = v[..] else {
        unreachable!()
    };
    // one input: ideal gas; explicit ::Ideal ignores P for h; two inputs: real fluid
    assert_eq!(h_auto1, h_ig);
    assert_eq!(h_ig, h_ig2);
    assert_eq!(h_auto2, h_real);
    assert!((h_auto2 - h_ig).abs() > 100.0, "real and ideal references differ: {h_auto2} {h_ig}");
    // s from T alone is at 1 atm; s(T, 10 P) = s(T, P) - R ln 10
    let r = 8.314462618 / m; // kJ/kg-K, M in kg/kmol
    close("s 1 atm", s_ig, s_auto1, 1e-12);
    close("s ln P", s_ig - s_ig10, r * 10f64.ln(), 1e-9);
    // P v = R T, Z = 1
    let tk = 200.0 + 273.15;
    close("rho", d_ig, 101.325 / (r * tk), 1e-9);
    close("v", v_ig * d_ig, 1.0, 1e-12);
    assert_eq!(z_ig, 1.0);
    // inverse calls give the state back
    close("T(P,s)", t_ps + 273.15, tk, 1e-8);
    close("P(T,s)", p_ts, 10.0 * 1.01325, 1e-8);
    close("T(h)", t_h + 273.15, tk, 1e-8);
    close("T(P,v)", t_pv + 273.15, tk, 1e-9);
    close("P(T,d)", p_tv, 1.01325, 1e-9);
    close("T(h,s)", t_hs + 273.15, tk, 1e-8);
    close("cp - cv = R", cp_ig - cv_ig, r, 1e-9);
    close("sound", c_ig, (cp_ig / cv_ig * r * 1e3 * tk).sqrt(), 1e-9);
    // at very low pressure the real fluid is an ideal gas
    close("low-pressure limit", d_li, d_lr, 1e-5);
}

#[test]
fn model_suffix_errors() {
    let has = |src: &str, what: &str| {
        let e = errors(src);
        assert!(e.iter().any(|m| m.contains(what)), "{src}: {e:?}");
    };
    has("h = enthalpy(CO2::Real, T=200)", "needs two state inputs");
    has("mu = viscosity(CO2::Ideal, T=200, P=1)", "use the real fluid");
    has("h = enthalpy(CO2::Ideal, T=200, h=10)", "ideal-gas inputs");
    has("T = t_sat(CO2::Ideal, P=10)", "no saturation");
    has("x = CO2::Ideal", "only allowed as the fluid");
    has("h = enthalpy('CO2&Argon::Ideal', T=200)", "mole fraction");
    has("h = enthalpy('Nitrogen[0.9]&SulfurDioxide[0.1]', T=500, P=100)", "::Ideal");
    assert!(errors("h = enthalpy('CO2[0.5]&Argon[0.5]::Ideal', T=200)").is_empty());
    assert!(errors("T = t_sat(CO2::Real, P=10)").is_empty());
}
