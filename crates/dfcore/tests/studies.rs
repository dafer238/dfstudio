#![cfg(feature = "coolprop")]
use dfcore::studies::*;
use dfcore::{compile, CompileOptions, SolveOptions};

const RANKINE: &str = include_str!("../../../examples/rankine.dfs");

#[test]
fn parametric_rows_in_parallel() {
    let m = compile(RANKINE, &CompileOptions::default());
    let p2 = m.var("P[2]").unwrap();
    let rows: Vec<Vec<f64>> = (0..40).map(|i| vec![20.0 + 5.0 * i as f64]).collect();
    let sols = parametric(&m, &[p2], &rows, &SolveOptions::default(), None).unwrap();
    assert_eq!(sols.len(), 40);
    assert!(sols.iter().all(|s| s.converged));
    let eta = m.var("eta_th").unwrap() as usize;
    // efficiency increases with boiler pressure in this range
    assert!(sols[39].x[eta] > sols[0].x[eta]);
}

#[test]
fn goal_seek_turbine_exit_quality() {
    let m = compile(RANKINE, &CompileOptions::default());
    let t3 = m.var("T[3]").unwrap();
    let x4 = m.var("x[4]").unwrap();
    let r = goal_seek(&m, t3, x4, 0.9, &SolveOptions::default(), None).unwrap();
    assert!((r.solution.x[x4 as usize] - 0.9).abs() < 1e-8, "{:?}", r.method);
    assert!(r.value > 550.0);
}

#[test]
fn optimise_pressure_for_net_power_density() {
    // maximise eta_th - 0.0005*P[2]: an interior optimum exists
    let src = format!("{RANKINE}\nJ = eta_th - 0.0005*P[2]\n");
    let m = compile(&src, &CompileOptions::default());
    let spec = OptSpec { objective: m.var("J").unwrap(), maximize: true, vars: vec![(m.var("P[2]").unwrap(), 5.0, 300.0)], method: OptMethod::Brent, max_evals: 200, tol: 1e-8 };
    let r = optimize(&m, &spec, &SolveOptions::default(), None).unwrap();
    let nm = optimize(&m, &OptSpec { method: OptMethod::NelderMead, ..spec.clone() }, &SolveOptions::default(), None).unwrap();
    let de = optimize(&m, &OptSpec { method: OptMethod::DifferentialEvolution, max_evals: 400, ..spec.clone() }, &SolveOptions::default(), None).unwrap();
    assert!(r.best[0] > 5.0 && r.best[0] < 300.0);
    assert!((nm.objective - r.objective).abs() < 1e-5, "{} vs {}", nm.objective, r.objective);
    assert!((de.objective - r.objective).abs() < 1e-4, "{} vs {}", de.objective, r.objective);
}

#[test]
fn control_loop_directive() {
    let src = format!("{RANKINE}\n$Control vary m_dot to make W_t = 100000\n");
    let m = compile(&src, &CompileOptions::default());
    assert!(!m.has_errors(), "{:?}", m.diagnostics);
    let (_, s, log) = solve_with_controls(&m, &SolveOptions::default(), None).unwrap();
    assert!(s.converged, "{} {:?}", s.message, log);
    assert!((s.x[m.var("W_t").unwrap() as usize] - 100000.0).abs() < 1e-4);
}

#[test]
fn goal_seek_two_targets() {
    // vary boiler pressure and turbine inlet temperature so that the cycle
    // efficiency and the turbine exit quality both reach given values
    let m = compile(RANKINE, &CompileOptions::default());
    let (p2, t3) = (m.var("P[2]").unwrap(), m.var("T[3]").unwrap());
    let (eta, x4) = (m.var("eta_th").unwrap(), m.var("x[4]").unwrap());
    let r = goal_seek_multi(&m, &[p2, t3], &[(eta, 0.36), (x4, 0.88)], &SolveOptions::default(), None).unwrap();
    let s = &r.solution;
    assert!(s.converged);
    assert!((s.x[eta as usize] - 0.36).abs() < 1e-7, "eta {} ({})", s.x[eta as usize], r.method);
    assert!((s.x[x4 as usize] - 0.88).abs() < 1e-7, "x4 {}", s.x[x4 as usize]);
    assert_eq!(r.values, vec![s.x[p2 as usize], s.x[t3 as usize]]);
    // single pair goes through the 1-D path
    let r1 = goal_seek_multi(&m, &[t3], &[(x4, 0.9)], &SolveOptions::default(), None).unwrap();
    assert!((r1.solution.x[x4 as usize] - 0.9).abs() < 1e-8);
    assert!(goal_seek_multi(&m, &[t3], &[(x4, 0.9), (eta, 0.3)], &SolveOptions::default(), None).is_err());
}

#[test]
fn parameter_matching_by_difference() {
    // the app turns "make W_t equal to W_target by changing m_dot" into
    // `__match_0 = (W_t) - (W_target)` driven to zero
    let src = format!("{RANKINE}\nW_target = 50000\n\n__match_0 = (W_t) - (W_target)\n");
    let m = compile(&src, &CompileOptions::default());
    let (md, mt) = (m.var("m_dot").unwrap(), m.var("__match_0").unwrap());
    let r = goal_seek_multi(&m, &[md], &[(mt, 0.0)], &SolveOptions::default(), None).unwrap();
    let wt = r.solution.x[m.var("W_t").unwrap() as usize];
    assert!((wt - 50000.0).abs() < 1e-4, "W_t = {wt} ({})", r.method);
    assert!((r.values[0] - 50.0 * 50000.0 / 60944.396).abs() < 1e-3, "m_dot = {}", r.values[0]);
}
