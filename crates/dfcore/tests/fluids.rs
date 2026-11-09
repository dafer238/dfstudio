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
