//! Mapping between the application state and `.df` project files.

use crate::analysis::Analysis;
use crate::app::Settings;
use crate::plots::PlotsState;
use crate::studies_ui::{GoalForm, OptForm, ParamTable};
use dfcore::dffile::{Artifact, DfFile, Meta, Results, Table};
use dfcore::UnitSystem;
use std::collections::HashMap;

/// Application state stored in the `PROJ` section (JSON). Every field has a
/// default so that files written by older versions keep opening.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct ProjectState {
    unit_system: Option<UnitSystem>,
    display_units: HashMap<String, String>,
    live_solve: Option<bool>,
    table: ParamTable,
    goal: GoalForm,
    opt: OptForm,
    plots: PlotsState,
    cycle: crate::cycle::CycleDiagram,
    init: std::collections::BTreeMap<String, crate::analysis::InitEntry>,
}

const PARAM_TABLE: &str = "Parametric";

/// Build a project file from the current state.
pub fn to_df(s: &Settings, analysis: Option<&Analysis>, files: &[Artifact], meta: &Meta) -> DfFile {
    let st = ProjectState {
        unit_system: Some(s.unit_system.clone()),
        display_units: s.display_units.clone(),
        live_solve: Some(s.live_solve),
        table: s.table.clone(),
        goal: s.goal.clone(),
        opt: s.opt.clone(),
        plots: s.plots.clone(),
        cycle: s.cycle.clone(),
        init: s.init.clone(),
    };
    let mut f = DfFile::new(s.text.clone());
    f.meta = meta.clone();
    f.project = serde_json::to_string_pretty(&st).ok();
    if let Some(a) = analysis {
        if let Some(sol) = &a.solution {
            f.results = Some(Results::from_solution(&a.model, sol));
        }
    }
    // parametric results as a numeric table (inputs, outputs, converged flag)
    let t = &s.table;
    if t.rows() > 0 && t.results.iter().any(|r| r.is_some()) {
        let unit = |n: &str| analysis.and_then(|a| a.model.var(n).map(|v| crate::studies_ui::conv(&a.model, &s.display_units, v).1.symbol)).unwrap_or_default();
        let mut columns: Vec<(String, String)> = t.inputs.iter().chain(&t.outputs).map(|n| (n.clone(), unit(n))).collect();
        columns.push(("converged".into(), String::new()));
        let mut data = Vec::with_capacity(t.rows() * columns.len());
        for r in 0..t.rows() {
            for k in 0..t.inputs.len() {
                data.push(t.cells[r].get(k).and_then(|c| c.trim().parse().ok()).unwrap_or(f64::NAN));
            }
            for j in 0..t.outputs.len() {
                data.push(t.results.get(r).and_then(|x| x.as_ref()).and_then(|x| x.get(j).copied()).unwrap_or(f64::NAN));
            }
            data.push(if t.converged.get(r).copied().unwrap_or(false) { 1.0 } else { 0.0 });
        }
        f.tables.push(Table { name: PARAM_TABLE.into(), columns, rows: t.rows(), data });
    }
    f.files = files.to_vec();
    f
}

/// Apply a project file to the application state. Returns the attached files.
pub fn apply_df(f: DfFile, s: &mut Settings) -> Vec<Artifact> {
    s.text = f.script;
    let st: ProjectState = f.project.as_deref().and_then(|p| serde_json::from_str(p).ok()).unwrap_or_default();
    if let Some(u) = st.unit_system {
        s.unit_system = u;
    }
    s.display_units = st.display_units;
    if let Some(l) = st.live_solve {
        s.live_solve = l;
    }
    s.table = st.table;
    s.goal = st.goal;
    s.opt = st.opt;
    s.plots = st.plots;
    s.cycle = st.cycle;
    s.cycle.migrate();
    s.init = st.init;
    // restore parametric results so plots show without re-running the table
    if let Some(tab) = f.tables.iter().find(|t| t.name == PARAM_TABLE) {
        let t = &mut s.table;
        let ncol = tab.columns.len();
        let col = |name: &str| tab.columns.iter().position(|c| c.0 == name);
        let out_idx: Vec<Option<usize>> = t.outputs.iter().map(|o| col(o)).collect();
        let conv_idx = col("converged");
        if tab.rows == t.rows() {
            t.results = (0..tab.rows).map(|r| Some(out_idx.iter().map(|j| j.map(|j| tab.data[r * ncol + j]).unwrap_or(f64::NAN)).collect())).collect();
            t.converged = (0..tab.rows).map(|r| conv_idx.map(|j| tab.data[r * ncol + j] != 0.0).unwrap_or(true)).collect();
        }
    }
    f.files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};

    #[test]
    fn project_round_trip() {
        let mut s = Settings::default();
        s.display_units.insert("t[3]".into(), "K".into());
        s.table.inputs = vec!["P[2]".into()];
        s.table.outputs = vec!["eta_th".into()];
        s.table.cells = vec![vec!["20".into()], vec!["40".into()]];
        s.table.results = vec![Some(vec![0.30]), Some(vec![0.33])];
        s.table.converged = vec![true, false];
        let a = run(Request { gen: 1, text: s.text.clone(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() });
        let files = vec![Artifact { name: "notes.txt".into(), data: b"hello".to_vec() }];
        let mut f = to_df(&s, Some(&a), &files, &Meta::default());
        let dir = std::env::temp_dir().join("dfstudio-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rankine.df");
        f.save(&path).unwrap();
        let g = DfFile::load(&path).unwrap();
        let r = g.results.as_ref().unwrap();
        assert!((r.get("eta_th").unwrap() - 0.3706).abs() < 1e-3);
        assert_eq!(r.units[r.names.iter().position(|n| n == "h[3]").unwrap()], "kJ/kg");
        let mut s2 = Settings { text: String::new(), ..Settings::default() };
        let files2 = apply_df(g, &mut s2);
        assert_eq!(s2.text, s.text);
        assert_eq!(s2.display_units.get("t[3]").map(String::as_str), Some("K"));
        assert_eq!(s2.table.results, s.table.results);
        assert_eq!(s2.table.converged, vec![true, false]);
        assert_eq!(files2, files);
        if let Ok(out) = std::env::var("DF_FILE_OUT") {
            std::fs::copy(&path, out).unwrap();
        }
    }
}
