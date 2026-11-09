//! The simplified (EES-compatible) export keeps the model: the exported
//! script, read back by DeltaF Studio, gives the same solution.

use dfcore::{compile, solve, CompileOptions, SolveOptions};

fn solution(src: &str) -> std::collections::HashMap<String, f64> {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}\n{src}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}\n{src}", s.message);
    m.vars.iter().enumerate().map(|(i, v)| (v.name.clone(), s.x[i])).collect()
}

fn same_solution(src: &str) {
    let ex = dfcore::ees_export::export(src, &Default::default());
    let a = solution(src);
    // in the exported script a formula name is the ideal gas whatever the
    // number of inputs (EES convention); say so for DeltaF Studio
    let mut back = ex.text.clone();
    for f in ["CO2", "H2O", "Air", "N2", "O2", "Ar", "H2", "He", "CO", "CH4"] {
        back = back.replace(&format!("({f},"), &format!("({f}::Ideal,"));
    }
    let b = solution(&back);
    for (k, v) in &a {
        let w = b.get(k).unwrap_or_else(|| panic!("{k} missing after export\n{}", ex.text));
        assert!((v - w).abs() <= 1e-7 * v.abs().max(1.0), "{k}: {v} vs {w}\n{}", ex.text);
    }
}

#[test]
fn examples_survive_the_export() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "dfs") {
            same_solution(&std::fs::read_to_string(&p).unwrap());
            n += 1;
        }
    }
    assert!(n >= 4);
}

#[test]
fn models_and_units_survive_the_export() {
    same_solution(
        "T[1] = 32 [C]; P[1] = 7.7 [MPa]\nP[2] = 25 [MPa]; eta_c = 0.89\nh[1] = enthalpy(CarbonDioxide, T=T[1], P=P[1])\ns[1] = entropy(CO2::Real, T=T[1], P=P[1])\n\
         h_s[2] = enthalpy(CO2, P=P[2], s=s[1])\neta_c = (h_s[2] - h[1])/(h[2] - h[1])\nh_ig = enthalpy(CO2, T=T[1])\nh_ig2 = enthalpy(CO2::Ideal, T=T[1], P=P[1])\n\
         s_ig = entropy(CO2::Ideal, T=T[1], P=P[1]); s_ig0 = entropy(CO2, T=500 [K])\nrho = density(Steam, T=200, P=10)\nv = volume(Water, T=200, d=rho)\n\
         dT = 5 [dK]; y = ifelse(T[1] >= 32, 1, 2) + pow(2, 3) + 2**2\nh_a = enthalpy(Air, T=T[1], P=1)\n",
    );
}
