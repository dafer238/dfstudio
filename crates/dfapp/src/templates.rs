//! Built-in cycle templates: a flowsheet (cycle diagram) and the matching
//! equations, compiled into the program so they are always available.

use crate::app::Settings;
use crate::cycle::{Comp, CycleDiagram, Kind, Kpi, Link};
use crate::plots::{Diagram, PlotDef, PlotKind, Series, Source, StateMap};
use crate::studies_ui::{GoalRow, ParamTable};
use dfcore::studies::OptMethod;

pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    /// Equations whose state numbers match the diagram streams.
    pub script: &'static str,
    pub diagram: fn() -> CycleDiagram,
    /// Studies and plots of the example project: a parametric table, a goal
    /// seek or an optimisation, and plots (written by `--write-examples`).
    pub studies: fn(&mut Settings),
}

pub const ALL: [Template; 4] = [
    Template {
        name: "Simple Rankine",
        about: "Pump, boiler, turbine and condenser; water / steam, states 1 to 4",
        script: include_str!("../../../examples/rankine.dfs"),
        diagram: rankine,
        studies: rankine_studies,
    },
    Template {
        name: "Simple Brayton",
        about: "Open gas-turbine cycle: compressor, combustor (heater), turbine and generator; air, states 1 to 4",
        script: include_str!("../../../examples/brayton.dfs"),
        diagram: brayton,
        studies: brayton_studies,
    },
    Template {
        name: "Recuperated Brayton",
        about: "Gas turbine with a recuperator: compressor, recuperator, combustor and turbine; air, states 1 to 6",
        script: include_str!("../../../examples/brayton_recuperated.dfs"),
        diagram: brayton_recuperated,
        studies: recuperated_studies,
    },
    Template {
        name: "Refrigeration",
        about: "Vapour-compression refrigeration: compressor, condenser, expansion valve and evaporator; R134a, states 1 to 4",
        script: include_str!("../../../examples/refrigeration.dfs"),
        diagram: refrigeration,
        studies: refrigeration_studies,
    },
];

const WATER: [u8; 3] = [0, 60, 220];
const STEAM: [u8; 3] = [230, 120, 0];
const AIR: [u8; 3] = [0, 150, 70];
const GAS: [u8; 3] = [220, 0, 0];
const WORKING: [u8; 3] = [200, 0, 200];

struct Build {
    d: CycleDiagram,
}

impl Build {
    fn new(title: &str) -> Build {
        Build { d: CycleDiagram { title: title.into(), ..Default::default() } }
    }
    fn comp(&mut self, kind: Kind, pos: [f32; 2], label: &str, vars: &[&str]) -> u32 {
        let id = self.d.next_id;
        self.d.next_id += 1;
        self.d.comps.push(Comp {
            id,
            kind,
            pos,
            rot: 0,
            flip: false,
            label: label.into(),
            vars: vars.iter().map(|s| s.to_string()).collect(),
            shaft: Vec::new(),
            params: Default::default(),
            anchors: Vec::new(),
        });
        id
    }
    /// Stream from outlet port `from` to inlet port `to` with state `state`.
    fn link(&mut self, from: (u32, usize), to: (u32, usize), state: i64, color: [u8; 3]) {
        let id = self.d.next_id;
        self.d.next_id += 1;
        self.d.links.push(Link { id, from, to, state: Some(state), color: Some(color), fluid: String::new(), label_t: None, label_off: None });
    }
    fn kpis(&mut self, k: &[(&str, &str)]) {
        self.d.kpis = k.iter().map(|(l, v)| Kpi { label: l.to_string(), var: v.to_string() }).collect();
    }
}

/// Simple Rankine cycle (states as in `examples/rankine.dfs`).
pub fn rankine() -> CycleDiagram {
    let mut b = Build::new("Simple Rankine cycle");
    let pump = b.comp(Kind::Pump, [100.0, 300.0], "Pump", &["W_p"]);
    // pumping upwards into the boiler
    if let Some(c) = b.d.comps.iter_mut().find(|c| c.id == pump) {
        c.rot = 3;
    }
    let boil = b.comp(Kind::Heater, [100.0, 120.0], "Boiler", &["Q_in"]);
    let turb = b.comp(Kind::Turbine, [320.0, 100.0], "Turbine", &["W_t"]);
    let cond = b.comp(Kind::Cooler, [400.0, 300.0], "Condenser", &[]);
    b.link((cond, 1), (pump, 0), 1, WATER);
    b.link((pump, 1), (boil, 0), 2, WATER);
    b.link((boil, 1), (turb, 0), 3, STEAM);
    b.link((turb, 1), (cond, 0), 4, STEAM);
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into(), "m".into()];
    b.kpis(&[("Thermal efficiency", "eta_th"), ("Turbine power", "W_t"), ("Pump power", "W_p"), ("Heat input", "Q_in")]);
    b.d
}

/// Simple open Brayton cycle (states as in `examples/brayton.dfs`).
pub fn brayton() -> CycleDiagram {
    let mut b = Build::new("Simple Brayton cycle");
    let src = b.comp(Kind::Source, [40.0, 300.0], "Air in", &[]);
    let comp = b.comp(Kind::Compressor, [160.0, 250.0], "Compressor", &["W_c"]);
    let heat = b.comp(Kind::Heater, [300.0, 130.0], "Combustor", &["Q_in"]);
    let turb = b.comp(Kind::Turbine, [440.0, 250.0], "Turbine", &["W_t"]);
    let sink = b.comp(Kind::Sink, [600.0, 330.0], "Exhaust", &[]);
    let gen = b.comp(Kind::Generator, [580.0, 250.0], "Generator", &["W_net"]);
    if let Some(g) = b.d.comps.iter_mut().find(|c| c.id == gen) {
        g.shaft = vec![turb, comp];
    }
    b.link((src, 0), (comp, 0), 1, AIR);
    b.link((comp, 1), (heat, 0), 2, AIR);
    b.link((heat, 1), (turb, 0), 3, GAS);
    b.link((turb, 1), (sink, 0), 4, GAS);
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into(), "m".into()];
    b.kpis(&[("Thermal efficiency", "eta_th"), ("Net power", "W_net"), ("Back-work ratio", "bwr"), ("Heat input", "Q_in")]);
    b.d
}

/// Recuperated Brayton cycle (states as in `examples/brayton_recuperated.dfs`).
pub fn brayton_recuperated() -> CycleDiagram {
    let mut b = Build::new("Recuperated Brayton cycle");
    let src = b.comp(Kind::Source, [40.0, 350.0], "Air in", &[]);
    let comp = b.comp(Kind::Compressor, [160.0, 300.0], "Compressor", &["W_c"]);
    let rec = b.comp(Kind::HeatExchanger, [330.0, 120.0], "Recuperator", &["Q_rec"]);
    let heat = b.comp(Kind::Heater, [480.0, 180.0], "Combustor", &["Q_in"]);
    let turb = b.comp(Kind::Turbine, [580.0, 300.0], "Turbine", &["W_t"]);
    let sink = b.comp(Kind::Sink, [190.0, 110.0], "Exhaust", &[]);
    let gen = b.comp(Kind::Generator, [720.0, 300.0], "Generator", &["W_net"]);
    for c in b.d.comps.iter_mut() {
        if c.id == gen {
            c.shaft = vec![turb, comp];
        }
        // recuperator: hot side right to left, cold side left to right
        if c.id == rec || c.id == sink {
            c.flip = true;
        }
        // combustor: in at the top, out at the bottom
        if c.id == heat {
            c.rot = 2;
        }
    }
    b.link((src, 0), (comp, 0), 1, AIR);
    b.link((comp, 1), (rec, 2), 2, AIR);
    b.link((rec, 3), (heat, 0), 3, AIR);
    b.link((heat, 1), (turb, 0), 4, GAS);
    b.link((turb, 1), (rec, 0), 5, GAS);
    b.link((rec, 1), (sink, 0), 6, GAS);
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into(), "m".into()];
    b.kpis(&[("Thermal efficiency", "eta_th"), ("Net power", "W_net"), ("Recuperator duty", "Q_rec"), ("Heat input", "Q_in")]);
    b.d
}

/// Vapour-compression refrigeration cycle (states as in
/// `examples/refrigeration.dfs`).
pub fn refrigeration() -> CycleDiagram {
    let mut b = Build::new("Vapour-compression refrigeration cycle");
    let evap = b.comp(Kind::Heater, [80.0, 200.0], "Evaporator", &["Q_evap"]);
    let comp = b.comp(Kind::Compressor, [240.0, 80.0], "Compressor", &["W_c"]);
    let cond = b.comp(Kind::Cooler, [420.0, 200.0], "Condenser", &["Q_cond"]);
    let valve = b.comp(Kind::Valve, [250.0, 330.0], "Expansion valve", &[]);
    if let Some(v) = b.d.comps.iter_mut().find(|c| c.id == valve) {
        v.flip = true; // flow from right to left
    }
    b.link((evap, 1), (comp, 0), 1, WORKING);
    b.link((comp, 1), (cond, 0), 2, WORKING);
    b.link((cond, 1), (valve, 0), 3, WORKING);
    b.link((valve, 1), (evap, 0), 4, WORKING);
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into(), "m".into()];
    b.kpis(&[("COP", "COP"), ("Cooling capacity", "Q_evap"), ("Compressor power", "W_c"), ("Condenser heat", "Q_cond")]);
    b.d
}

// ------------------------------------------------------------------ example studies

/// Parametric table over the grid of `inputs` (values as typed, display
/// units), every combination in row order.
fn table(inputs: &[(&str, &[f64])], outputs: &[&str]) -> ParamTable {
    let mut cells: Vec<Vec<String>> = vec![Vec::new()];
    for (_, vals) in inputs {
        cells = cells.iter().flat_map(|row| vals.iter().map(move |v| row.iter().cloned().chain(std::iter::once(format!("{v}"))).collect())).collect();
    }
    ParamTable {
        inputs: inputs.iter().map(|(n, _)| n.to_string()).collect(),
        outputs: outputs.iter().map(|s| s.to_string()).collect(),
        fill_from: inputs.iter().map(|(_, v)| format!("{}", v[0])).collect(),
        fill_to: inputs.iter().map(|(_, v)| format!("{}", v[v.len() - 1])).collect(),
        grid_n: inputs.first().map(|(_, v)| v.len()).unwrap_or(0),
        cells,
        ..Default::default()
    }
}

fn steps(from: f64, to: f64, step: f64) -> Vec<f64> {
    let n = ((to - from) / step).round() as usize;
    (0..=n).map(|k| ((from + k as f64 * step) * 1e6).round() / 1e6).collect()
}

fn property_plot(name: &str, fluid: &str, diagram: Diagram, order: &str) -> PlotDef {
    let mut d = PlotDef::new(name.into(), PlotKind::Property);
    d.fluid = fluid.into();
    d.diagram = diagram;
    (d.log_x, d.log_y) = diagram.default_log();
    d.map = StateMap { t: "t".into(), p: "p".into(), h: "h".into(), s: "s".into(), v: String::new() };
    d.order = order.into();
    d
}

/// XY plot of the parametric table; series names ending in `>` go on the
/// right axis.
fn xy_plot(name: &str, x: &str, ys: &[&str]) -> PlotDef {
    let mut d = PlotDef::new(name.into(), PlotKind::XY);
    d.source = Source::Parametric;
    d.x = x.into();
    for (k, y) in ys.iter().enumerate() {
        let mut s = Series::new(y.trim_end_matches('>'), k);
        s.y2 = y.ends_with('>');
        d.series.push(s);
    }
    d.title = name.into();
    d
}

fn goal(target: &str, value: &str, vary: &str) -> crate::studies_ui::GoalForm {
    crate::studies_ui::GoalForm {
        rows: vec![GoalRow { target: target.into(), matching: false, value: value.into(), other: String::new(), vary: vary.into() }],
        ..Default::default()
    }
}

fn rankine_studies(s: &mut Settings) {
    s.tables = vec![table(&[("P[2]", &steps(50.0, 200.0, 25.0)), ("T[3]", &steps(450.0, 600.0, 25.0))], &["eta_th", "W_net", "x[4]"])];
    s.goal = goal("x[4]", "0.9", "P[2]");
    let mut surf = PlotDef::new("Efficiency surface".into(), PlotKind::Surface);
    surf.source = Source::Parametric;
    surf.x = "P[2]".into();
    surf.y3 = "T[3]".into();
    surf.z3 = "eta_th".into();
    surf.title = "Thermal efficiency over boiler pressure and temperature".into();
    s.plots.plots = vec![property_plot("T-s diagram", "Water", Diagram::Ts, ""), surf, xy_plot("Exhaust quality", "P[2]", &["x[4]"])];
}

fn brayton_studies(s: &mut Settings) {
    s.tables = vec![table(&[("r_p", &steps(4.0, 30.0, 2.0))], &["eta_th", "w_spec", "T[4]"])];
    s.opt =
        crate::studies_ui::OptForm { objective: "w_spec".into(), maximize: true, method: OptMethod::Brent, vars: vec![("r_p".into(), "4".into(), "30".into())], max_evals: 300 };
    s.plots.plots = vec![property_plot("T-s diagram", "Air", Diagram::Ts, "1,2,3,4"), xy_plot("Efficiency and specific work", "r_p", &["eta_th", "w_spec>"])];
}

fn recuperated_studies(s: &mut Settings) {
    s.tables = vec![table(&[("r_p", &steps(2.0, 10.0, 0.5))], &["eta_th", "w_spec", "T[5]", "Q_rec"])];
    s.goal = goal("eta_th", "0.38", "eps");
    s.opt =
        crate::studies_ui::OptForm { objective: "eta_th".into(), maximize: true, method: OptMethod::Brent, vars: vec![("r_p".into(), "2".into(), "10".into())], max_evals: 300 };
    s.plots.plots = vec![property_plot("T-s diagram", "Air", Diagram::Ts, "1,2,3,4,5,6"), xy_plot("Efficiency and specific work", "r_p", &["eta_th", "w_spec>"])];
}

fn refrigeration_studies(s: &mut Settings) {
    s.tables = vec![table(&[("T_evap", &steps(-30.0, 5.0, 5.0))], &["COP", "W_c", "m_dot"])];
    s.goal = goal("COP", "4", "T_cond");
    s.plots.plots = vec![property_plot("P-h diagram", "R134a", Diagram::Ph, ""), xy_plot("COP and compressor power", "T_evap", &["COP", "W_c>"])];
}

/// Add template `t` to `d`. An empty diagram becomes the template (keeping
/// the view); otherwise the template is placed to the right of the drawing
/// with new ids, and its state numbers follow the highest one in use.
/// Returns the state number added to the template's states.
pub fn insert(d: &mut CycleDiagram, t: &CycleDiagram) -> i64 {
    if d.comps.is_empty() && d.links.is_empty() {
        let (offset, zoom) = (d.offset, d.zoom);
        *d = t.clone();
        d.offset = offset;
        d.zoom = zoom;
        return 0;
    }
    let base = d.comps.iter().map(|c| c.id).chain(d.links.iter().map(|l| l.id)).max().unwrap_or(0).max(d.next_id);
    let shift = d.links.iter().filter_map(|l| l.state).max().unwrap_or(0);
    let bounds = |c: &[Comp]| c.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), c| ([lo[0].min(c.pos[0]), lo[1].min(c.pos[1])], [hi[0].max(c.pos[0]), hi[1].max(c.pos[1])]));
    let (dlo, dhi) = bounds(&d.comps);
    let (tlo, _) = bounds(&t.comps);
    let (dx, dy) = if d.comps.is_empty() { (0.0, 0.0) } else { (dhi[0] + 160.0 - tlo[0], dlo[1] - tlo[1]) };
    let id = |i: u32| i + base;
    for c in &t.comps {
        let mut c = c.clone();
        c.id = id(c.id);
        c.pos = [((c.pos[0] + dx) / 10.0).round() * 10.0, ((c.pos[1] + dy) / 10.0).round() * 10.0];
        c.shaft = c.shaft.iter().map(|s| id(*s)).collect();
        d.comps.push(c);
    }
    for l in &t.links {
        let mut l = l.clone();
        l.id = id(l.id);
        l.from.0 = id(l.from.0);
        l.to.0 = id(l.to.0);
        l.state = l.state.map(|s| s + shift);
        d.links.push(l);
    }
    for k in &t.kpis {
        if !d.kpis.iter().any(|x| x.var.eq_ignore_ascii_case(&k.var)) {
            d.kpis.push(k.clone());
        }
    }
    for v in &t.state_vars {
        if !d.state_vars.iter().any(|x| x.eq_ignore_ascii_case(v)) {
            d.state_vars.push(v.clone());
        }
    }
    d.next_id = d.comps.iter().map(|c| c.id).chain(d.links.iter().map(|l| l.id)).max().unwrap_or(0) + 1;
    shift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};
    use dfcore::SolveOptions;
    use std::collections::HashMap;

    #[test]
    fn templates_solve_and_label_every_stream() {
        for t in &ALL {
            let d = (t.diagram)();
            let a = run(Request { gen: 1, text: t.script.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() });
            let sol = a.solution.as_ref().expect("solved");
            assert!(sol.converged, "{}", t.name);
            let du = HashMap::new();
            let inp = crate::cycle::CycleInputs { analysis: &a, display_units: &du, script: t.script };
            for l in &d.links {
                let lines = inp.state_lines(&d, l.state.unwrap());
                assert_eq!(lines.len(), 4, "{}: state {:?} {lines:?}", t.name, l.state);
            }
            for k in &d.kpis {
                assert!(a.model.var(&k.var).is_some(), "{}: KPI {}", t.name, k.var);
            }
        }
    }

    /// A component shows the same tabs whether or not it can be computed
    /// (a tab without data says what it needs), so the selected tab is kept
    /// when moving between components of a kind.
    #[test]
    fn component_tabs_do_not_depend_on_data() {
        let tabs = |r: &crate::components::Report| -> Vec<String> {
            r.sections.iter().map(|s| s.title.clone()).chain(r.charts.iter().map(|c| if c.key.is_empty() { c.title.clone() } else { c.key.into() })).collect()
        };
        for t in &ALL {
            let d = (t.diagram)();
            let du = HashMap::new();
            let solved =
                run(Request { gen: 1, text: t.script.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() });
            let unsolved =
                run(Request { gen: 1, text: t.script.into(), opts: Default::default(), solve: false, solve_opts: Default::default(), warm: None, init: Default::default() });
            assert!(unsolved.solution.is_none());
            let (a, b) = (
                crate::cycle::CycleInputs { analysis: &solved, display_units: &du, script: t.script },
                crate::cycle::CycleInputs { analysis: &unsolved, display_units: &du, script: t.script },
            );
            let mut by_kind: Vec<(crate::cycle::Kind, Vec<String>)> = Vec::new();
            for c in &d.comps {
                let diag = crate::components::PathDiagram::default();
                let (ra, rb) = (crate::components::report(&d, &a, c.id, diag), crate::components::report(&d, &b, c.id, diag));
                assert_eq!(tabs(&ra), tabs(&rb), "{}: {}", t.name, c.label);
                assert!(rb.sections.iter().all(|s| !s.missing.is_empty() || s.title == "Shaft" || !s.rows.is_empty()), "{}: {}", t.name, c.label);
                match by_kind.iter().find(|k| k.0 == c.kind) {
                    Some(prev) => assert_eq!(prev.1, tabs(&ra), "{}: {} differs from another {:?}", t.name, c.label, c.kind),
                    None => by_kind.push((c.kind, tabs(&ra))),
                }
            }
        }
    }

    /// The example studies run: every parametric row converges, the goal
    /// seek reaches its target and the optimisation finds an interior point.
    #[test]
    fn example_studies_run() {
        use dfcore::studies::{goal_seek, optimize, OptSpec};
        for t in &ALL {
            let mut s = Settings::default();
            (t.studies)(&mut s);
            let a = run(Request { gen: 1, text: t.script.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() });
            let m = &a.model;
            let du = HashMap::new();
            let table = &mut s.tables[0];
            table.solve_now(&a, &du).unwrap();
            assert!(table.rows() >= 8 && table.converged.iter().all(|c| *c), "{}: parametric", t.name);
            let table = &s.tables[0];
            for p in &s.plots.plots {
                for n in std::iter::once(&p.x).chain(p.series.iter().map(|x| &x.y)).chain([&p.y3, &p.z3]).filter(|n| !n.is_empty()) {
                    assert!(table.inputs.contains(n) || table.outputs.contains(n), "{}: plot {} uses {n}", t.name, p.name);
                }
            }
            let warm = a.solution.as_ref().map(|x| x.x.clone());
            for g in &s.goal.rows {
                let (vary, target) = (m.var(&g.vary).unwrap(), m.var(&g.target).unwrap());
                let value: f64 = g.value.parse().unwrap();
                let r = goal_seek(m, vary, target, value, &SolveOptions::default(), warm.as_deref()).unwrap();
                assert!(r.solution.converged, "{}: goal seek", t.name);
                assert!((r.solution.x[target as usize] - value).abs() < 1e-6 * value.abs().max(1.0), "{}: goal seek", t.name);
            }
            if !s.opt.objective.is_empty() {
                let vars: Vec<(u32, f64, f64)> = s.opt.vars.iter().map(|(n, lo, hi)| (m.var(n).unwrap(), lo.parse().unwrap(), hi.parse().unwrap())).collect();
                let spec = OptSpec {
                    objective: m.var(&s.opt.objective).unwrap(),
                    maximize: s.opt.maximize,
                    vars: vars.clone(),
                    method: s.opt.method,
                    max_evals: s.opt.max_evals,
                    tol: 1e-6,
                };
                let r = optimize(m, &spec, &SolveOptions::default(), warm.as_deref()).unwrap();
                assert!(r.best[0] > vars[0].1 + 0.1 && r.best[0] < vars[0].2 - 0.1, "{}: optimum {:?} at a bound", t.name, r.best);
            }
        }
    }

    #[test]
    fn insert_beside_an_existing_drawing() {
        let mut d = CycleDiagram::default();
        assert_eq!(insert(&mut d, &rankine()), 0);
        assert_eq!(d.comps.len(), 4);
        let shift = insert(&mut d, &brayton());
        assert_eq!(shift, 4);
        assert_eq!(d.comps.len(), 10);
        insert(&mut d, &refrigeration());
        assert_eq!(d.links.iter().filter_map(|l| l.state).max(), Some(12));
        d.comps.truncate(10);
        d.links.truncate(8);
        let states: Vec<i64> = d.links.iter().filter_map(|l| l.state).collect();
        assert_eq!(states, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        // ids are unique and every link and shaft points at a component
        let mut ids: Vec<u32> = d.comps.iter().map(|c| c.id).chain(d.links.iter().map(|l| l.id)).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), d.comps.len() + d.links.len());
        for l in &d.links {
            assert!(d.comps.iter().any(|c| c.id == l.from.0) && d.comps.iter().any(|c| c.id == l.to.0));
        }
        let g = d.comps.iter().find(|c| c.kind == Kind::Generator).unwrap();
        assert!(g.shaft.iter().all(|s| d.comps.iter().any(|c| c.id == *s && c.kind != Kind::Generator)));
        assert!(d.comps.iter().skip(4).all(|c| c.pos[0] > 400.0));
        assert!(d.kpis.iter().any(|k| k.var == "bwr"));
    }
}
