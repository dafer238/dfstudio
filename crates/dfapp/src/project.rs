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
    /// Display preset and per-quantity units (absent in files before 0.2.2).
    unit_prefs: crate::units_ui::UnitPrefs,
    live_solve: Option<bool>,
    /// First parametric table, as written before multiple tables (kept so
    /// older versions still open the file).
    table: ParamTable,
    /// All parametric tables (absent in files before 0.3.0).
    tables: Vec<ParamTable>,
    table_sel: usize,
    goal: GoalForm,
    opt: OptForm,
    plots: PlotsState,
    cycle: crate::cycle::CycleDiagram,
    init: std::collections::BTreeMap<String, crate::analysis::InitEntry>,
}

const PARAM_TABLE: &str = "Parametric";

/// Name of the results table of parametric table `k` in the file
/// ("Parametric", "Parametric 2", ...).
fn param_table_name(k: usize) -> String {
    if k == 0 {
        PARAM_TABLE.into()
    } else {
        format!("{PARAM_TABLE} {}", k + 1)
    }
}

/// Build a project file from the current state.
pub fn to_df(s: &Settings, analysis: Option<&Analysis>, files: &[Artifact], meta: &Meta) -> DfFile {
    let st = ProjectState {
        unit_system: Some(s.unit_system.clone()),
        display_units: s.display_units.clone(),
        unit_prefs: s.unit_prefs.clone(),
        live_solve: Some(s.live_solve),
        table: s.tables.first().cloned().unwrap_or_default(),
        tables: s.tables.clone(),
        table_sel: s.table_sel,
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
    // parametric results as numeric tables (inputs, outputs, converged flag)
    let du = analysis.map(|a| s.units_for(&a.model)).unwrap_or_default();
    let unit = |n: &str| analysis.and_then(|a| a.model.var(n).map(|v| crate::studies_ui::conv(&a.model, &du, v).1.symbol)).unwrap_or_default();
    for (k, t) in s.tables.iter().enumerate() {
        if t.rows() == 0 || t.results.iter().all(|r| r.is_none()) {
            continue;
        }
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
        f.tables.push(Table { name: param_table_name(k), columns, rows: t.rows(), data });
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
    s.unit_prefs = st.unit_prefs;
    if let Some(l) = st.live_solve {
        s.live_solve = l;
    }
    s.tables = if st.tables.is_empty() { vec![st.table] } else { st.tables };
    s.table_sel = st.table_sel.min(s.tables.len() - 1);
    s.goal = st.goal;
    s.opt = st.opt;
    s.plots = st.plots;
    s.cycle = st.cycle;
    s.cycle.migrate();
    s.init = st.init;
    // restore parametric results so plots show without re-running the tables
    for (k, t) in s.tables.iter_mut().enumerate() {
        let name = param_table_name(k);
        let Some(tab) = f.tables.iter().find(|x| x.name == name) else {
            continue;
        };
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

    /// Files written before per-quantity units (no `unit_prefs`) keep their
    /// per-variable units and get no preset.
    #[test]
    fn old_project_state_loads() {
        let old = r#"{"display_units":{"m_dot":"kg/h","T[1]":"K"},"live_solve":true}"#;
        let mut f = DfFile::new("x = 1".to_string());
        f.project = Some(old.into());
        let mut s = Settings::default();
        apply_df(f, &mut s);
        assert_eq!(s.display_units.get("m_dot").map(String::as_str), Some("kg/h"));
        assert!(s.unit_prefs.is_empty());
    }

    /// Files written before multiple parametric tables (a single `table`)
    /// open with that table as the only one, results included.
    #[test]
    fn single_table_project_loads() {
        let old = r#"{"table":{"inputs":["x"],"outputs":["y"],"cells":[["1"],["2"]],"fill_from":[""],"fill_to":[""]}}"#;
        let mut f = DfFile::new("y = 2*x\nx = 1".to_string());
        f.project = Some(old.into());
        f.tables.push(Table {
            name: PARAM_TABLE.into(),
            columns: vec![("x".into(), String::new()), ("y".into(), String::new()), ("converged".into(), String::new())],
            rows: 2,
            data: vec![1.0, 2.0, 1.0, 2.0, 4.0, 1.0],
        });
        let mut s = Settings::default();
        apply_df(f, &mut s);
        assert_eq!(s.tables.len(), 1);
        assert_eq!(s.tables[0].inputs, vec!["x".to_string()]);
        assert_eq!(s.tables[0].results, vec![Some(vec![2.0]), Some(vec![4.0])]);
    }

    /// Stored settings with the single `table` of older versions migrate to
    /// one table in `tables`.
    #[test]
    fn single_table_settings_migrate() {
        let mut t = ParamTable::default();
        t.inputs = vec!["x".into()];
        let mut v = serde_json::to_value(Settings::default()).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("tables");
        o.remove("table_sel");
        o.insert("table".into(), serde_json::to_value(&t).unwrap());
        let mut s: Settings = serde_json::from_value(v).unwrap();
        s.upgrade();
        assert_eq!(s.tables.len(), 1);
        assert_eq!(s.tables[0].inputs, t.inputs);
        assert!(serde_json::to_value(&s).unwrap().get("table").is_none());
    }

    #[test]
    fn project_round_trip() {
        let mut s = Settings::default();
        s.display_units.insert("t[3]".into(), "K".into());
        s.unit_prefs.set_preset(crate::units_ui::DisplayPreset::Engineering);
        s.unit_prefs.quantities.insert("mass_flow".into(), "t/h".into());
        let t = &mut s.tables[0];
        t.inputs = vec!["P[2]".into()];
        t.outputs = vec!["eta_th".into()];
        t.cells = vec![vec!["20".into()], vec!["40".into()]];
        t.results = vec![Some(vec![0.30]), Some(vec![0.33])];
        t.converged = vec![true, false];
        let mut t2 = s.tables[0].clone();
        t2.name = "second".into();
        t2.results = vec![Some(vec![0.25]), Some(vec![0.27])];
        s.tables.push(t2);
        s.table_sel = 1;
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
        assert_eq!(s2.unit_prefs, s.unit_prefs);
        assert_eq!(s2.tables.len(), 2);
        assert_eq!(s2.table_sel, 1);
        assert_eq!(s2.tables[0].results, s.tables[0].results);
        assert_eq!(s2.tables[0].converged, vec![true, false]);
        assert_eq!(s2.tables[1].name, "second");
        assert_eq!(s2.tables[1].results, s.tables[1].results);
        assert_eq!(files2, files);
        if let Ok(out) = std::env::var("DF_FILE_OUT") {
            std::fs::copy(&path, out).unwrap();
        }
    }
}
