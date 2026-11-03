//! Built-in cycle templates: a flowsheet (cycle diagram) and the matching
//! equations, compiled into the program so they are always available.

use crate::cycle::{Comp, CycleDiagram, Kind, Kpi, Link};

pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    /// Equations whose state numbers match the diagram streams.
    pub script: &'static str,
    pub diagram: fn() -> CycleDiagram,
}

pub const ALL: [Template; 3] = [
    Template {
        name: "Simple Rankine",
        about: "Pump, boiler, turbine and condenser; water / steam, states 1 to 4",
        script: include_str!("../../../examples/rankine.dfs"),
        diagram: rankine,
    },
    Template {
        name: "Simple Brayton",
        about: "Open gas-turbine cycle: compressor, combustor (heater), turbine and generator; air, states 1 to 4",
        script: include_str!("../../../examples/brayton.dfs"),
        diagram: brayton,
    },
    Template {
        name: "Refrigeration",
        about: "Vapour-compression refrigeration: compressor, condenser, expansion valve and evaporator; R134a, states 1 to 4",
        script: include_str!("../../../examples/refrigeration.dfs"),
        diagram: refrigeration,
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
    let boil = b.comp(Kind::Heater, [100.0, 120.0], "Boiler", &["Q_in"]);
    let turb = b.comp(Kind::Turbine, [320.0, 100.0], "Turbine", &["W_t"]);
    let cond = b.comp(Kind::Cooler, [400.0, 300.0], "Condenser", &[]);
    b.link((cond, 1), (pump, 0), 1, WATER);
    b.link((pump, 1), (boil, 0), 2, WATER);
    b.link((boil, 1), (turb, 0), 3, STEAM);
    b.link((turb, 1), (cond, 0), 4, STEAM);
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into()];
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
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into()];
    b.kpis(&[("Thermal efficiency", "eta_th"), ("Net power", "W_net"), ("Back-work ratio", "bwr"), ("Heat input", "Q_in")]);
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
    b.d.state_vars = vec!["t".into(), "p".into(), "h".into()];
    b.kpis(&[("COP", "COP"), ("Cooling capacity", "Q_evap"), ("Compressor power", "W_c"), ("Condenser heat", "Q_cond")]);
    b.d
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
    use std::collections::HashMap;

    #[test]
    fn templates_solve_and_label_every_stream() {
        for t in &ALL {
            let d = (t.diagram)();
            let a = run(Request { gen: 1, text: t.script.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None });
            let sol = a.solution.as_ref().expect("solved");
            assert!(sol.converged, "{}", t.name);
            let du = HashMap::new();
            let inp = crate::cycle::CycleInputs { analysis: &a, display_units: &du, script: t.script };
            for l in &d.links {
                let lines = inp.state_lines(&d, l.state.unwrap());
                assert_eq!(lines.len(), 3, "{}: state {:?} {lines:?}", t.name, l.state);
            }
            for k in &d.kpis {
                assert!(a.model.var(&k.var).is_some(), "{}: KPI {}", t.name, k.var);
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
