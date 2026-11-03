//! Solver verification: models with closed-form solutions, a cycle checked
//! against an independent calculation that bypasses the equation language,
//! and invariance of the physical results under the unit system.

use dfcore::props::{backend, Key};
use dfcore::{compile, solve, CompileOptions, SolveOptions};

fn run(src: &str) -> (dfcore::Model, dfcore::Solution) {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}", s.message);
    // every equation is satisfied, not only the blocks the solver reports
    assert!(s.residuals.iter().all(|r| r.is_nan() || r.abs() < 1e-8), "residuals {:?}", s.residuals);
    (m, s)
}

fn val(m: &dfcore::Model, s: &dfcore::Solution, n: &str) -> f64 {
    s.x[m.var(n).unwrap_or_else(|| panic!("{n}")) as usize]
}

fn close(name: &str, got: f64, want: f64, rel: f64) {
    let e = ((got - want) / want).abs();
    assert!(e <= rel, "{name}: got {got}, expected {want} (relative error {e:.2e})");
}

/// Air-standard Brayton cycle with constant cp: eta = 1 - r^(-(k-1)/k)
/// exactly, and the net work has a closed form too. The equations are
/// written implicitly (T2 from the isentropic relation in log form, T4
/// from the turbine efficiency) so that the solver has to iterate.
#[test]
fn brayton_constant_cp_closed_form() {
    let src = r#"
cp = 1.005; k = 1.4; R = cp*(k-1)/k   // names are case-insensitive: R and r_p
T1 = 300; r_p = 12; T3 = 1400
eta_c = 0.86; eta_t = 0.9
ln(T2s/T1) = (R/cp)*ln(r_p)
eta_c = (T2s - T1)/(T2 - T1)
ln(T3/T4s) = (R/cp)*ln(r_p)
eta_t = (T3 - T4)/(T3 - T4s)
w_net = cp*(T3 - T4) - cp*(T2 - T1)
eta = w_net/(cp*(T3 - T2))
eta_ideal = 1 - T1/T2s
"#;
    let (m, s) = run(src);
    let (cp, k, t1, r, t3, ec, et) = (1.005f64, 1.4f64, 300.0, 12.0f64, 1400.0, 0.86, 0.9);
    let tau = r.powf((k - 1.0) / k);
    let t2 = t1 + t1 * (tau - 1.0) / ec;
    let t4 = t3 - et * t3 * (1.0 - 1.0 / tau);
    let w = cp * (t3 - t4) - cp * (t2 - t1);
    close("eta_ideal", val(&m, &s, "eta_ideal"), 1.0 - 1.0 / tau, 1e-12);
    close("T2", val(&m, &s, "T2"), t2, 1e-12);
    close("T4", val(&m, &s, "T4"), t4, 1e-12);
    close("w_net", val(&m, &s, "w_net"), w, 1e-12);
    close("eta", val(&m, &s, "eta"), w / (cp * (t3 - t2)), 1e-12);
}

/// Counter-flow heat exchanger written with the LMTD (outlet temperatures
/// unknown, a coupled nonlinear block) against the effectiveness-NTU closed
/// form eps = (1 - exp(-NTU(1-Cr))) / (1 - Cr exp(-NTU(1-Cr))).
#[test]
fn counterflow_lmtd_matches_eps_ntu() {
    let src = r#"
C_h = 4.2; C_c = 6.0; UA = 9.0
T_hi = 150; T_ci = 20
Q = C_h*(T_hi - T_ho)
Q = C_c*(T_co - T_ci)
Q = UA*(dT1 - dT2)/ln(dT1/dT2)
dT1 = T_hi - T_co
dT2 = T_ho - T_ci
$Guess T_ho = 80, T_co = 60
"#;
    let (m, s) = run(src);
    let (ch, cc, ua) = (4.2f64, 6.0f64, 9.0f64);
    let (cmin, cr) = (ch.min(cc), ch.min(cc) / ch.max(cc));
    let ntu = ua / cmin;
    let eps = (1.0 - (-ntu * (1.0 - cr)).exp()) / (1.0 - cr * (-ntu * (1.0 - cr)).exp());
    close("Q", val(&m, &s, "Q"), eps * cmin * (150.0 - 20.0), 1e-10);
}

/// Colebrook-White friction factor (implicit in f) against an independent
/// fixed-point solution, and Haaland's explicit fit within its 2 %.
#[test]
fn colebrook_friction_factor() {
    for (re, rr) in [(1e4, 1e-4), (1e5, 1e-3), (1e7, 1e-5)] {
        let src = format!("Re = {re}; rr = {rr}\n1/sqrt(f) = -2*log10(rr/3.7 + 2.51/(Re*sqrt(f)))\n$Guess f = 0.02");
        let (m, s) = run(&src);
        let mut f = 0.02f64;
        for _ in 0..200 {
            f = (-2.0 * (rr / 3.7 + 2.51 / (re * f.sqrt())).log10()).powi(-2);
        }
        close(&format!("f Re={re}"), val(&m, &s, "f"), f, 1e-10);
        let haaland = (-1.8 * ((rr / 3.7f64).powf(1.11) + 6.9 / re).log10()).powi(-2);
        close(&format!("Haaland Re={re}"), val(&m, &s, "f"), haaland, 0.02);
    }
}

/// The Rankine example against the same cycle computed directly with
/// CoolProp in SI, without the equation language, unit conversion or
/// solver: catches errors in any of the three.
#[test]
fn rankine_example_against_direct_calculation() {
    let (m, s) = run(include_str!("../../../examples/rankine.dfs"));
    let sp = |i: [Key; 2], v: [f64; 2], o: Key| backend::state_point("Water", i, v, &[o]).unwrap()[0];
    let (p1, p2, t3, ep, et, mdot) = (0.1e5, 150e5, 550.0 + 273.15, 0.85, 0.88, 50.0);
    let h1 = sp([Key::P, Key::Q], [p1, 0.0], Key::H);
    let s1 = sp([Key::P, Key::Q], [p1, 0.0], Key::S);
    let h2 = h1 + (sp([Key::P, Key::S], [p2, s1], Key::H) - h1) / ep;
    let h3 = sp([Key::P, Key::T], [p2, t3], Key::H);
    let s3 = sp([Key::P, Key::T], [p2, t3], Key::S);
    let h4 = h3 - et * (h3 - sp([Key::P, Key::S], [p1, s3], Key::H));
    let x4 = sp([Key::P, Key::H], [p1, h4], Key::Q);
    let t2 = sp([Key::P, Key::H], [p2, h2], Key::T);
    let eta = ((h3 - h4) - (h2 - h1)) / (h3 - h2);
    // model units: C, bar, kJ
    close("h[2]", val(&m, &s, "h[2]"), h2 / 1e3, 1e-9);
    close("T[2]", val(&m, &s, "T[2]") + 273.15, t2, 1e-9);
    close("h[4]", val(&m, &s, "h[4]"), h4 / 1e3, 1e-9);
    close("x[4]", val(&m, &s, "x[4]"), x4, 1e-8);
    close("W_t", val(&m, &s, "W_t"), mdot * (h3 - h4) / 1e3, 1e-9);
    close("eta_th", val(&m, &s, "eta_th"), eta, 1e-9);
    // first law of the cycle: W_t - W_p = Q_in - Q_out
    let q_out = mdot * (h4 - h1) / 1e3;
    close("energy balance", val(&m, &s, "W_t") - val(&m, &s, "W_p"), val(&m, &s, "Q_in") - q_out, 1e-10);
}

/// The same physical model in three unit systems gives the same physical
/// results: unit conversion of literals, property calls and outputs.
#[test]
fn results_do_not_depend_on_the_unit_system() {
    let model = |us: &str, p_hi: &str, t3: &str, p_lo: &str| {
        format!(
            "$UnitSystem {us}\nP1 = {p_lo}\nP2 = {p_hi}\nT3 = {t3}\nh1 = enthalpy(Water, P=P1, x=0)\ns1 = entropy(Water, P=P1, x=0)\n\
             h2 = enthalpy(Water, P=P2, s=s1)\nh3 = enthalpy(Water, P=P2, T=T3)\ns3 = entropy(Water, P=P2, T=T3)\n\
             h4 = enthalpy(Water, P=P1, s=s3)\neta = ((h3 - h4) - (h2 - h1))/(h3 - h2)\nT4 = temperature(Water, P=P1, h=h4)\nv3 = volume(Water, P=P2, T=T3)"
        )
    };
    // 100 bar, 500 C, 0.08 bar written in each system's own units
    let cases = [
        model("SI K Pa J", "1e7", "773.15", "8000"),
        model("Eng", "100", "500", "0.08"),
        model("SI C kPa kJ", "10000", "500", "8"),
        model("Imperial", "1450.377377", "932", "1.160302"),
    ];
    let res: Vec<(f64, f64)> = cases
        .iter()
        .map(|src| {
            let (m, s) = run(src);
            let us = m.unit_system.clone();
            let t4 = us.unit_for(dfcore::units::Dim::TEMP).to_si(val(&m, &s, "T4"));
            (val(&m, &s, "eta"), t4)
        })
        .collect();
    for (i, r) in res.iter().enumerate().skip(1) {
        // the Imperial inputs are rounded to 10 digits
        close(&format!("eta case {i}"), r.0, res[0].0, 1e-7);
        close(&format!("T4 case {i}"), r.1, res[0].1, 1e-7);
    }
}

/// converttemp returns a number in another scale: it must not be taken as a
/// temperature in model units (a display unit would convert it again).
#[test]
fn converttemp_result_is_not_a_model_temperature() {
    let (m, s) = run("$UnitSystem Eng\nT = 25 [C]\nTk = converttemp(C, K, T)\nr = Tk/(T + 273.15)");
    close("Tk", val(&m, &s, "Tk"), 298.15, 1e-12);
    let tk = m.var("Tk").unwrap() as usize;
    assert_ne!(m.var_dims[tk], Some(dfcore::units::Dim::TEMP));
    assert_eq!(m.var_dims[m.var("T").unwrap() as usize], Some(dfcore::units::Dim::TEMP));
}

/// The EES cross-check model solves completely, and its built-in balances close.
#[test]
fn ees_crosscheck_model() {
    let (m, s) = run(include_str!("../../../examples/ees_crosscheck.dfs"));
    for b in ["balance_A", "balance_B"] {
        assert!(val(&m, &s, b).abs() < 1e-6, "{b} = {}", val(&m, &s, b));
    }
    // the implicit Colebrook equation and the Repeat loop in a Function agree
    close("Colebrook", val(&m, &s, "f_fw"), val(&m, &s, "f_loop"), 1e-10);
    // heat-exchanger temperatures stay feasible
    assert!(val(&m, &s, "DT_min_rc") > 0.0 && val(&m, &s, "T_cw_out") < val(&m, &s, "T_sat_cond"));
}
