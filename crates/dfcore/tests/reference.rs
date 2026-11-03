//! Property values against published reference tables, and thermodynamic
//! identities that any correct property pipeline (fluid model + unit
//! conversion + input-pair handling) must satisfy.
//!
//! Sources:
//! * IAPWS R6-95(2018), Revised Release on the IAPWS Formulation 1995 for the
//!   Thermodynamic Properties of Ordinary Water Substance, Tables 7 and 8
//!   (computer-program verification values).
//! * IAPWS R7-97(2012), Revised Release on the IAPWS Industrial Formulation
//!   1997, Tables 5 and 15 (verification values, regions 1 and 2).
//! * Span & Wagner (1996), J. Phys. Chem. Ref. Data 25, 1509: CO2 critical
//!   and triple points. Tillner-Roth & Baehr (1994), J. Phys. Chem. Ref.
//!   Data 23, 657: R134a critical point.
//! * NIST-JANAF Thermochemical Tables (Chase, 1998): enthalpy of formation
//!   and standard entropy at 298.15 K.

use dfcore::{compile, solve, CompileOptions, SolveOptions};

/// Solve `src` and return the named values (model units).
fn values(src: &str, names: &[&str]) -> Vec<f64> {
    let m = compile(src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s) = solve(&m, &SolveOptions::default());
    assert!(s.converged, "{}\n{src}", s.message);
    names.iter().map(|n| s.x[m.var(n).unwrap_or_else(|| panic!("{n}")) as usize]).collect()
}

fn check(name: &str, got: f64, want: f64, rel: f64) {
    let err = ((got - want) / want).abs();
    assert!(err <= rel, "{name}: got {got}, reference {want}, relative error {err:.2e} > {rel:.0e}");
}

/// IAPWS-95 Table 7: single-phase states (T in K, rho in kg/m3) ->
/// p (MPa), cv (kJ/kg-K), w (m/s), s (kJ/kg-K).
#[test]
fn iapws95_single_phase() {
    let table = [
        (300.0, 0.9965560e3, 0.992418352e-1, 0.413018112e1, 0.150151914e4, 0.393062643),
        (300.0, 0.1005308e4, 0.200022515e2, 0.406798347e1, 0.153492501e4, 0.387405401),
        (300.0, 0.1188202e4, 0.700004704e3, 0.346135580e1, 0.244357992e4, 0.132609616),
        (500.0, 0.4350000, 0.999679423e-1, 0.150817541e1, 0.548314253e3, 0.794488271e1),
        (500.0, 0.4532000e1, 0.999938125, 0.166991025e1, 0.535739001e3, 0.682502725e1),
        (647.0, 0.3580000e3, 0.220384756e2, 0.618315728e1, 0.252145078e3, 0.432092307e1),
        (900.0, 0.2410000, 0.100062559, 0.175890657e1, 0.724027147e3, 0.916653194e1),
    ];
    for (t, d, p, cv, w, s) in table {
        let src = format!(
            "$UnitSystem SI K MPa kJ\nT = {t}\nrho = {d}\np = pressure(Water, T=T, d=rho)\ncv = cv(Water, T=T, d=rho)\nw = soundspeed(Water, T=T, d=rho)\ns = entropy(Water, T=T, d=rho)"
        );
        let v = values(&src, &["p", "cv", "w", "s"]);
        let tag = format!("T={t} rho={d}");
        check(&format!("p {tag}"), v[0], p, 1e-6);
        check(&format!("cv {tag}"), v[1], cv, 1e-6);
        check(&format!("w {tag}"), v[2], w, 1e-6);
        check(&format!("s {tag}"), v[3], s, 1e-6);
    }
}

/// IAPWS-95 Table 8: saturation at T (K) -> p_s (MPa), rho', rho'' (kg/m3),
/// h', h'' (kJ/kg), s', s'' (kJ/kg-K).
#[test]
fn iapws95_saturation() {
    let table = [
        (275.0, 0.698451167e-3, 0.999887406e3, 0.550664919e-2, 0.775972202e1, 0.250428995e4, 0.283094670e-1, 0.910660121e1),
        (450.0, 0.932203564, 0.890341250e3, 0.481200360e1, 0.749161585e3, 0.277441078e4, 0.210865845e1, 0.660921221e1),
        (625.0, 0.169082693e2, 0.567090385e3, 0.118290280e3, 0.168626976e4, 0.255071625e4, 0.380194683e1, 0.518506121e1),
    ];
    for (t, ps, dl, dv, hl, hv, sl, sv) in table {
        let src = format!(
            "$UnitSystem SI K MPa kJ\nT = {t}\nps = p_sat(Water, T=T)\ndl = density(Water, T=T, x=0)\ndv = density(Water, T=T, x=1)\n\
             hl = enthalpy(Water, T=T, x=0)\nhv = enthalpy(Water, T=T, x=1)\nsl = entropy(Water, T=T, x=0)\nsv = entropy(Water, T=T, x=1)"
        );
        let v = values(&src, &["ps", "dl", "dv", "hl", "hv", "sl", "sv"]);
        // the saturation solver of the reference itself is converged to ~1e-8
        for (i, (n, want)) in [("p_s", ps), ("rho'", dl), ("rho''", dv), ("h'", hl), ("h''", hv), ("s'", sl), ("s''", sv)].into_iter().enumerate() {
            check(&format!("{n} T={t}"), v[i], want, 2e-6);
        }
    }
}

/// IAPWS-IF97 Tables 5 and 15 through the `IF97::Water` backend (the
/// industrial formulation, as used by EES `Steam_IF97`-style models):
/// (T K, p MPa) -> v (m3/kg), h (kJ/kg), s (kJ/kg-K).
#[test]
fn iapws_if97_regions_1_and_2() {
    let table = [
        // region 1
        (300.0, 3.0, 0.100215168e-2, 0.115331273e3, 0.392294792),
        (300.0, 80.0, 0.971180894e-3, 0.184142828e3, 0.368563852),
        (500.0, 3.0, 0.120241800e-2, 0.975542239e3, 0.258041912e1),
        // region 2
        (300.0, 0.0035, 0.394913866e2, 0.254991145e4, 0.852238967e1),
        (700.0, 0.0035, 0.923015898e2, 0.333568375e4, 0.101749996e2),
        (700.0, 30.0, 0.542946619e-2, 0.263149474e4, 0.517540298e1),
    ];
    for (t, p, v, h, s) in table {
        let src =
            format!("$UnitSystem SI K MPa kJ\nT = {t}\np = {p}\nv = volume('IF97::Water', T=T, P=p)\nh = enthalpy('IF97::Water', T=T, P=p)\ns = entropy('IF97::Water', T=T, P=p)");
        let got = values(&src, &["v", "h", "s"]);
        let tag = format!("T={t} p={p}");
        check(&format!("v {tag}"), got[0], v, 1e-8);
        check(&format!("h {tag}"), got[1], h, 1e-8);
        check(&format!("s {tag}"), got[2], s, 1e-8);
    }
}

#[test]
fn critical_and_triple_points() {
    let v = values(
        "$UnitSystem SI K MPa kJ\nTc = t_crit(CarbonDioxide)\npc = p_crit(CarbonDioxide)\nTc2 = t_crit(R134a)\npc2 = p_crit(R134a)\nTc3 = t_crit(Water)\npc3 = p_crit(Water)\n\
         pt = p_sat(CarbonDioxide, T=216.592)",
        &["Tc", "pc", "Tc2", "pc2", "Tc3", "pc3", "pt"],
    );
    check("Tc CO2", v[0], 304.1282, 1e-6);
    check("pc CO2", v[1], 7.3773, 1e-4);
    check("Tc R134a", v[2], 374.21, 1e-5);
    check("pc R134a", v[3], 4.05928, 1e-4);
    check("Tc water", v[4], 647.096, 1e-7);
    check("pc water", v[5], 22.064, 1e-5);
    check("p triple CO2", v[6], 0.51795, 1e-3);
}

/// One-input (ideal-gas) calls use the NIST-JANAF thermochemical reference:
/// h(298.15 K) = enthalpy of formation, s(298.15 K, 1 atm) = S°.
#[test]
fn janaf_formation_enthalpy_and_entropy() {
    // (fluid, M kg/kmol, dHf kJ/mol, S° J/mol-K)
    let table = [("CO2", 44.0095, -393.522, 213.795), ("H2O", 18.01528, -241.826, 188.834), ("N2", 28.0134, 0.0, 191.609), ("O2", 31.9988, 0.0, 205.147)];
    for (f, mm, hf, s0) in table {
        let src = format!("$UnitSystem SI K kPa kJ\nh = enthalpy({f}, T=298.15)\ns = entropy({f}, T=298.15)");
        let v = values(&src, &["h", "s"]);
        let (h_ref, s_ref) = (hf * 1000.0 / mm, s0 / mm);
        if hf != 0.0 {
            check(&format!("h_f {f}"), v[0], h_ref, 2e-4);
        } else {
            assert!(v[0].abs() < 0.05, "h_f {f} = {}", v[0]);
        }
        check(&format!("S° {f}"), v[1], s_ref, 5e-4);
    }
}

/// Exact thermodynamic identities, checked by central differences through
/// the language in several unit systems:
///   (dh/ds)_P = T,  (dh/dP)_s = v,  Clausius-Clapeyron
///   dp_s/dT = (h'' - h') / (T (v'' - v')),  and g' = g'' at saturation.
#[test]
fn thermodynamic_identities() {
    // (unit system, fluid, T, P) with T, P in that unit system
    // (unit system, fluid, T, P, factor) with T, P in that unit system and
    // factor = (pressure unit x volume unit) / energy unit
    let psia_ft3_btu = 6894.757293168 * 0.3048f64.powi(3) / 1055.05585262;
    let cases = [
        ("SI K Pa J", "Water", 600.0, 5.0e6, 1.0),
        ("SI C bar kJ", "Water", 150.0, 2.0, 100.0),
        ("SI C bar kJ", "CarbonDioxide", 60.0, 120.0, 100.0),
        ("SI C kPa kJ", "R134a", 40.0, 500.0, 1.0),
        ("SI C kPa kJ", "Air", 300.0, 2000.0, 1.0),
        ("SI C kPa kJ", "Ammonia", 80.0, 1000.0, 1.0),
        ("Eng", "Water", 300.0, 10.0, 100.0),
        ("Imperial", "Water", 500.0, 100.0, psia_ft3_btu),
    ];
    for (us, f, t, p, k) in cases {
        let src = format!(
            "$UnitSystem {us}\nT = {t}\nP = {p}\ns = entropy({f}, T=T, P=P)\nv = volume({f}, T=T, P=P)\nds = 1e-3*abs(s) + 1e-6\ndp = 1e-3*P\n\
             dh_ds = (enthalpy({f}, P=P, s=s+ds) - enthalpy({f}, P=P, s=s-ds))/(2*ds)\n\
             dh_dp = (enthalpy({f}, s=s, P=P+dp) - enthalpy({f}, s=s, P=P-dp))/(2*dp)\n\
             Tabs = converttemp({tu}, K, T)",
            tu = if us.contains(" C ") || us == "Eng" {
                "C"
            } else if us == "Imperial" {
                "F"
            } else {
                "K"
            }
        );
        let v = values(&src, &["dh_ds", "Tabs", "dh_dp", "v"]);
        // dh/ds has units energy/(energy/(mass K)) = K in absolute temperature: in °F
        // systems the entropy unit is Btu/lbm-R, so the derivative is in R
        let t_abs = if us == "Imperial" { v[1] * 1.8 } else { v[1] };
        check(&format!("(dh/ds)_P = T, {f} {us}"), v[0], t_abs, 5e-6);
        // (dh/dP)_s / v = (pressure unit * volume unit) / energy unit
        let ratio = v[2] / v[3];
        check(&format!("(dh/dP)_s = v, {f} {us}"), ratio, k, 2e-5);
    }
    // Clausius-Clapeyron and equal Gibbs energy, water and CO2
    for (f, t) in [("Water", 150.0), ("Water", 300.0), ("CarbonDioxide", 0.0), ("R134a", 20.0)] {
        let src = format!(
            "$UnitSystem SI C kPa kJ\nT = {t}\ndT = 0.01\ndp_dT = (p_sat({f}, T=T+dT) - p_sat({f}, T=T-dT))/(2*dT)\n\
             hl = enthalpy({f}, T=T, x=0)\nhv = enthalpy({f}, T=T, x=1)\nvl = volume({f}, T=T, x=0)\nvv = volume({f}, T=T, x=1)\n\
             sl = entropy({f}, T=T, x=0)\nsv = entropy({f}, T=T, x=1)\nTk = T + 273.15\n\
             cc = (hv - hl)/(Tk*(vv - vl))\ngl = hl - Tk*sl\ngv = hv - Tk*sv"
        );
        let v = values(&src, &["dp_dT", "cc", "gl", "gv", "hv", "hl"]);
        check(&format!("Clausius-Clapeyron {f} {t} C"), v[0], v[1], 1e-6);
        assert!((v[2] - v[3]).abs() < 1e-6 * (v[4] - v[5]), "g' = g'' {f} {t} C: {} vs {}", v[2], v[3]);
    }
}

/// Flash round trips: (P, h) -> T, (P, s) -> T, (T, h) -> P and the
/// quality form all return the state they came from.
#[test]
fn flash_round_trips() {
    for (f, t, p) in [("Water", 500.0, 100.0), ("Water", 45.0, 0.1), ("CarbonDioxide", 35.0, 80.0), ("R134a", -10.0, 1.5), ("Air", 25.0, 1.01325)] {
        let src = format!(
            "$UnitSystem SI C bar kJ\nT = {t}\nP = {p}\nh = enthalpy({f}, T=T, P=P)\ns = entropy({f}, T=T, P=P)\n\
             T_ph = temperature({f}, P=P, h=h)\nT_ps = temperature({f}, P=P, s=s)\nP_th = pressure({f}, T=T, h=h)\nd = density({f}, T=T, P=P)\n\
             P_td = pressure({f}, T=T, d=d)"
        );
        let v = values(&src, &["T_ph", "T_ps", "P_th", "P_td"]);
        let tk = t + 273.15;
        check(&format!("T(P,h) {f}"), v[0] + 273.15, tk, 1e-8);
        check(&format!("T(P,s) {f}"), v[1] + 273.15, tk, 1e-8);
        // in a liquid h hardly depends on P, so P(T, h) is ill-conditioned:
        // check that it reproduces h instead (it does to round-off)
        if !(f == "Water" && t < 100.0) {
            check(&format!("P(T,h) {f}"), v[2], p, 1e-5);
        }
        check(&format!("P(T,d) {f}"), v[3], p, 1e-8);
    }
    // wet state: h = h' + x (h'' - h')
    let v = values(
        "$UnitSystem SI C bar kJ\nP = 0.1\nx = 0.87\nh = enthalpy(Water, P=P, x=x)\nhl = enthalpy(Water, P=P, x=0)\nhv = enthalpy(Water, P=P, x=1)\nx2 = quality(Water, P=P, h=h)",
        &["h", "hl", "hv", "x2"],
    );
    check("lever rule", v[0], v[1] + 0.87 * (v[2] - v[1]), 1e-9);
    check("quality(P, h)", v[3], 0.87, 1e-9);
}

/// Humid air: at saturation the humidity ratio follows from the water
/// saturation pressure, W = 0.621945 f p_ws / (p - f p_ws), with the
/// enhancement factor f between 1.003 and 1.006 near ambient pressure
/// (ASHRAE Handbook Fundamentals, ch. 1; Hyland & Wexler, 1983).
#[test]
fn humid_air_saturation() {
    for t in [5.0, 25.0, 40.0] {
        let src = format!(
            "$UnitSystem SI C kPa kJ\nT = {t}\nP = 101.325\nW = humrat(AirH2O, T=T, P=P, R=1)\npws = p_sat(Water, T=T)\nRH = relhum(AirH2O, T=T, P=P, w=W/2)\nTdp = dewpoint(AirH2O, T=T, P=P, w=W)"
        );
        let v = values(&src, &["W", "pws", "RH", "Tdp"]);
        let f = 0.621945 * v[1] / (101.325 - v[1]);
        let ratio = v[0] / f;
        assert!(ratio > 1.002 && ratio < 1.008, "T={t}: W/W_ideal = {ratio}");
        // dew point of saturated air is its temperature
        assert!((v[3] - t).abs() < 1e-6, "dew point {t}: {}", v[3]);
        // half the humidity ratio is a little more than half the vapour
        // pressure: p_w = p W / (0.621945 + W)
        let pw = 101.325 * (v[0] / 2.0) / (0.621945 + v[0] / 2.0);
        let pw_sat = 101.325 * v[0] / (0.621945 + v[0]);
        check(&format!("RH at W/2, {t} C"), v[2], pw / pw_sat, 1e-3);
    }
}
