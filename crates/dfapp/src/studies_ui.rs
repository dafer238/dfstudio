//! Parametric tables, goal seek and optimisation tabs.

use crate::analysis::Analysis;
use crate::app::natord;
use crate::editor::fmt_num;
use dfcore::studies::{goal_seek_multi, optimize, parametric, GoalMultiResult, OptMethod, OptResult, OptSpec};
use dfcore::units::{parse_unit, Unit};
use dfcore::{Model, SolveOptions};
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Instant;

/// Unit conversions between display and model units for one variable.
pub fn conv(model: &Model, display_units: &HashMap<String, String>, v: u32) -> (Unit, Unit) {
    let name = &model.vars[v as usize].name;
    if let Some(u) = display_units.get(name) {
        if let Ok(du) = parse_unit(u) {
            return (model.unit_system.unit_for(du.dim), du);
        }
    }
    model.display_unit(v).unwrap_or_else(|| (Unit::none(), Unit::none()))
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParamTable {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// input cells as typed (display units)
    pub cells: Vec<Vec<String>>,
    #[serde(skip)]
    pub results: Vec<Option<Vec<f64>>>,
    #[serde(skip)]
    pub converged: Vec<bool>,
    pub fill_from: Vec<String>,
    pub fill_to: Vec<String>,
    /// Points per input for a full-factorial grid.
    #[serde(default)]
    pub grid_n: usize,
}

impl ParamTable {
    pub fn rows(&self) -> usize {
        self.cells.len()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct OptForm {
    pub objective: String,
    pub maximize: bool,
    pub method: OptMethod,
    pub vars: Vec<(String, String, String)>,
    pub max_evals: usize,
}

impl Default for OptForm {
    fn default() -> Self {
        OptForm { objective: String::new(), maximize: true, method: OptMethod::NelderMead, vars: Vec::new(), max_evals: 300 }
    }
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoalForm {
    // single-row form of earlier versions (migrated into `rows`)
    pub vary: String,
    pub target: String,
    pub value: String,
    #[serde(default)]
    pub rows: Vec<GoalRow>,
}

/// One goal-seek condition: `target = value` or, when `matching`,
/// `target = other` (parameter matching), by changing `vary`.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoalRow {
    pub target: String,
    pub matching: bool,
    pub value: String,
    pub other: String,
    pub vary: String,
}

enum JobResult {
    Param(Result<(Vec<Vec<f64>>, Vec<bool>), String>, f64),
    /// result and the names of the varied variables
    Goal(Result<GoalMultiResult, String>, Vec<String>, f64),
    Opt(Result<OptResult, String>, f64),
}

#[derive(Default)]
pub struct StudiesUi {
    rx: Option<Receiver<JobResult>>,
    pub running: Option<&'static str>,
    pub message: String,
    /// varied variables and their values (model units)
    pub goal_result: Option<Vec<(String, f64)>>,
    /// values the user asked to write into the script: (variable, model value)
    pub apply_request: Option<Vec<(String, f64)>>,
    pub opt_result: Option<OptResult>,
    pub opt_units: Vec<String>,
    new_rows: usize,
}

fn var_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, model: &Model, value: &mut String, filter: impl Fn(usize) -> bool) {
    let mut names: Vec<(usize, &str)> = model.vars.iter().enumerate().filter(|(i, _)| filter(*i)).map(|(i, v)| (i, v.display.as_str())).collect();
    names.sort_by(|a, b| natord(&a.1.to_lowercase(), &b.1.to_lowercase()));
    egui::ComboBox::from_id_salt(id).selected_text(if value.is_empty() { "select…" } else { value.as_str() }).height(320.0).show_ui(ui, |ui| {
        let fid = ui.scope_id().with("var_filter");
        let mut f: String = ui.data_mut(|d| d.get_temp(fid).unwrap_or_default());
        let r = ui.add(egui::TextEdit::singleline(&mut f).hint_text("type to filter").desired_width(160.0));
        if !r.has_focus() && f.is_empty() {
            r.request_focus();
        }
        let fl = f.to_lowercase();
        let mut first: Option<&str> = None;
        for (_, n) in names.iter().filter(|(_, n)| fl.is_empty() || n.to_lowercase().contains(&fl)).take(500) {
            first.get_or_insert(n);
            ui.selectable_value(value, n.to_string(), *n);
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(n) = first {
                *value = n.to_string();
                ui.close();
            }
        }
        ui.data_mut(|d| d.insert_temp(fid, f));
    });
}

impl StudiesUi {
    pub fn poll(&mut self, table: &mut ParamTable) {
        let Some(rx) = &self.rx else { return };
        if let Ok(r) = rx.try_recv() {
            self.running = None;
            self.rx = None;
            match r {
                JobResult::Param(Ok((vals, conv)), ms) => {
                    let n_ok = conv.iter().filter(|c| **c).count();
                    self.message = format!("{} rows solved ({} converged) in {:.0} ms", conv.len(), n_ok, ms);
                    table.results = vals.into_iter().map(Some).collect();
                    table.converged = conv;
                }
                JobResult::Param(Err(e), _) => self.message = format!("Parametric table: {e}"),
                JobResult::Goal(Ok(g), names, ms) => {
                    self.message = format!("Goal seek converged ({}, {:.0} ms)", g.method, ms);
                    self.goal_result = Some(names.into_iter().zip(g.values).collect());
                }
                JobResult::Goal(Err(e), _, _) => self.message = format!("Goal seek: {e}"),
                JobResult::Opt(Ok(o), ms) => {
                    self.message = format!("{} ({:.0} ms)", o.message, ms);
                    self.opt_result = Some(o);
                }
                JobResult::Opt(Err(e), _) => self.message = format!("Optimisation: {e}"),
            }
        }
    }

    // ------------------------------------------------------------ parametric
    pub fn parametric_tab(&mut self, ui: &mut egui::Ui, a: &Arc<Analysis>, t: &mut ParamTable, du: &HashMap<String, String>) {
        let m = &a.model;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Parametric table").heading());
            ui.label(
                RichText::new("Input columns are fixed per row (their `x = constant` equations are disabled); output columns are computed. Rows are solved in parallel.").weak(),
            );
        });
        ui.horizontal(|ui| {
            let mut add_in = String::new();
            ui.label("Add input");
            var_combo(ui, "add_in", m, &mut add_in, |_| true);
            if !add_in.is_empty() && !t.inputs.contains(&add_in) {
                t.inputs.push(add_in);
                for r in &mut t.cells {
                    r.push(String::new());
                }
                t.fill_from.push(String::new());
                t.fill_to.push(String::new());
            }
            let mut add_out = String::new();
            ui.label("Add output");
            var_combo(ui, "add_out", m, &mut add_out, |_| true);
            if !add_out.is_empty() && !t.outputs.contains(&add_out) {
                t.outputs.push(add_out);
            }
            ui.separator();
            if self.new_rows == 0 {
                self.new_rows = 10;
            }
            ui.add(egui::DragValue::new(&mut self.new_rows).range(1..=10000).prefix("rows "));
            if ui.button("Set rows").clicked() {
                t.cells.resize(self.new_rows, vec![String::new(); t.inputs.len()]);
                t.results.clear();
            }
            if ui.button("Clear").clicked() {
                *t = ParamTable::default();
            }
        });
        if !t.inputs.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Fill input");
                t.fill_from.resize(t.inputs.len(), String::new());
                t.fill_to.resize(t.inputs.len(), String::new());
                for k in 0..t.inputs.len() {
                    ui.label(RichText::new(&t.inputs[k]).monospace());
                    ui.add(egui::TextEdit::singleline(&mut t.fill_from[k]).desired_width(60.0).hint_text("from"));
                    ui.add(egui::TextEdit::singleline(&mut t.fill_to[k]).desired_width(60.0).hint_text("to"));
                    if ui.small_button("linear").clicked() {
                        if let (Ok(a0), Ok(b0)) = (t.fill_from[k].trim().parse::<f64>(), t.fill_to[k].trim().parse::<f64>()) {
                            let n = t.cells.len().max(2);
                            if t.cells.len() < 2 {
                                t.cells.resize(n, vec![String::new(); t.inputs.len()]);
                            }
                            for (i, r) in t.cells.iter_mut().enumerate() {
                                r[k] = fmt_num(a0 + (b0 - a0) * i as f64 / (n - 1) as f64);
                            }
                        }
                    }
                    ui.separator();
                }
                if t.inputs.len() >= 2 {
                    if t.grid_n < 2 {
                        t.grid_n = 10;
                    }
                    ui.add(egui::DragValue::new(&mut t.grid_n).range(2..=100).prefix("n = "));
                    let total = t.grid_n.saturating_pow(t.inputs.len() as u32);
                    let ranges: Vec<Option<(f64, f64)>> =
                        (0..t.inputs.len()).map(|k| t.fill_from[k].trim().parse::<f64>().ok().zip(t.fill_to[k].trim().parse::<f64>().ok())).collect();
                    let ok = total <= 20_000 && ranges.iter().all(|r| r.is_some());
                    if ui
                        .add_enabled(ok, egui::Button::new("full grid"))
                        .on_hover_text(format!("Full-factorial table: every combination of n values per input ({total} rows); needs every from/to range"))
                        .clicked()
                    {
                        let n = t.grid_n;
                        let k = t.inputs.len();
                        t.cells = (0..total)
                            .map(|row| {
                                let mut r = row;
                                let mut cells = vec![String::new(); k];
                                // last input varies fastest
                                for j in (0..k).rev() {
                                    let (a0, b0) = ranges[j].unwrap();
                                    cells[j] = fmt_num(a0 + (b0 - a0) * (r % n) as f64 / (n - 1) as f64);
                                    r /= n;
                                }
                                cells
                            })
                            .collect();
                        t.results.clear();
                        t.converged.clear();
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            let can = !t.inputs.is_empty() && t.rows() > 0 && self.running.is_none();
            if ui.add_enabled(can, egui::Button::new(RichText::new("Run table").strong())).clicked() {
                self.run_param(a, t, du);
            }
            if self.running == Some("param") {
                ui.spinner();
            }
            if ui.button("Copy (TSV)").clicked() {
                let mut s = t.inputs.iter().chain(&t.outputs).cloned().collect::<Vec<_>>().join("\t") + "\n";
                for (i, r) in t.cells.iter().enumerate() {
                    let mut row = r.clone();
                    if let Some(Some(v)) = t.results.get(i) {
                        row.extend(v.iter().map(|x| x.to_string()));
                    }
                    s += &(row.join("\t") + "\n");
                }
                ui.ctx().copy_text(s);
            }
            ui.label(&self.message);
        });
        ui.separator();
        // header units
        let unit_of = |name: &str| m.var(name).map(|v| conv(m, du, v).1.symbol).unwrap_or_default();
        let mut remove_in: Option<usize> = None;
        let mut remove_out: Option<usize> = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut tb = TableBuilder::new(ui).striped(true).column(Column::exact(44.0));
            for _ in 0..t.inputs.len() + t.outputs.len() {
                tb = tb.column(Column::initial(110.0).at_least(60.0).resizable(true));
            }
            tb.header(40.0, |mut h| {
                h.col(|ui| {
                    ui.strong("Run");
                });
                for (k, n) in t.inputs.iter().enumerate() {
                    h.col(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(n).strong().color(Color32::from_rgb(120, 170, 255)));
                                if ui.small_button("x").clicked() {
                                    remove_in = Some(k);
                                }
                            });
                            ui.label(RichText::new(unit_of(n)).small().weak());
                        });
                    });
                }
                for (k, n) in t.outputs.iter().enumerate() {
                    h.col(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(n);
                                if ui.small_button("x").clicked() {
                                    remove_out = Some(k);
                                }
                            });
                            ui.label(RichText::new(unit_of(n)).small().weak());
                        });
                    });
                }
            })
            .body(|body| {
                let nin = t.inputs.len();
                body.rows(22.0, t.cells.len(), |mut row| {
                    let i = row.index();
                    let ok = t.converged.get(i).copied();
                    row.col(|ui| {
                        let c = match ok {
                            Some(true) => Color32::from_rgb(90, 180, 90),
                            Some(false) => Color32::from_rgb(220, 80, 80),
                            None => ui.visuals().weak_text_color(),
                        };
                        ui.label(RichText::new((i + 1).to_string()).color(c));
                    });
                    for k in 0..nin {
                        row.col(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut t.cells[i][k]).desired_width(f32::INFINITY));
                        });
                    }
                    for k in 0..t.outputs.len() {
                        row.col(|ui| {
                            if let Some(Some(v)) = t.results.get(i) {
                                ui.label(RichText::new(fmt_num(v[k])).monospace());
                            }
                        });
                    }
                });
            });
        });
        if let Some(k) = remove_in {
            t.inputs.remove(k);
            for r in &mut t.cells {
                if k < r.len() {
                    r.remove(k);
                }
            }
            t.results.clear();
        }
        if let Some(k) = remove_out {
            t.outputs.remove(k);
            t.results.clear();
        }
    }

    fn run_param(&mut self, a: &Arc<Analysis>, t: &ParamTable, du: &HashMap<String, String>) {
        let m = a.model.clone();
        let mut inputs = Vec::new();
        for n in &t.inputs {
            match m.var(n) {
                Some(v) => inputs.push(v),
                None => {
                    self.message = format!("unknown variable {n}");
                    return;
                }
            }
        }
        let outputs: Vec<Option<u32>> = t.outputs.iter().map(|n| m.var(n)).collect();
        let mut rows = Vec::new();
        for (i, r) in t.cells.iter().enumerate() {
            let mut vals = Vec::new();
            for (k, c) in r.iter().enumerate() {
                let Ok(v) = c.trim().parse::<f64>() else {
                    self.message = format!("row {}: '{}' is not a number", i + 1, c);
                    return;
                };
                let (mu, d) = conv(&m, du, inputs[k]);
                vals.push(d.convert(v, &mu));
            }
            rows.push(vals);
        }
        let out_conv: Vec<Option<(Unit, Unit)>> = outputs.iter().map(|o| o.map(|v| conv(&m, du, v))).collect();
        let warm = a.solution.as_ref().map(|s| s.x.clone());
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.running = Some("param");
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let r = parametric(&m, &inputs, &rows, &SolveOptions::default(), warm.as_deref()).map(|sols| {
                let vals = sols
                    .iter()
                    .map(|s| {
                        outputs
                            .iter()
                            .zip(&out_conv)
                            .map(|(o, c)| match (o, c) {
                                (Some(v), Some((mu, du))) if s.known[*v as usize] => mu.convert(s.x[*v as usize], du),
                                _ => f64::NAN,
                            })
                            .collect()
                    })
                    .collect();
                (vals, sols.iter().map(|s| s.converged).collect())
            });
            let _ = tx.send(JobResult::Param(r, t0.elapsed().as_secs_f64() * 1e3));
        });
    }

    // ------------------------------------------------------------ goal seek + optimisation
    pub fn optimise_tab(&mut self, ui: &mut egui::Ui, a: &Arc<Analysis>, g: &mut GoalForm, o: &mut OptForm, du: &HashMap<String, String>, script: &str) {
        let m = a.model.clone();
        ui.label(&self.message);
        ui.separator();
        ui.heading("Goal seek and parameter matching");
        ui.label(
            RichText::new("Change variables (normally set by `x = constant`) until each condition holds: a variable reaches a value (Excel-like goal seek) or equals another variable (parameter matching). Use as many conditions as variables to change.")
                .weak(),
        );
        if g.rows.is_empty() {
            g.rows.push(GoalRow { target: std::mem::take(&mut g.target), value: std::mem::take(&mut g.value), vary: std::mem::take(&mut g.vary), ..Default::default() });
        }
        let mut remove = None;
        let nrows = g.rows.len();
        for (k, r) in g.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label("Make");
                var_combo(ui, ("gs_target", k), &m, &mut r.target, |_| true);
                egui::ComboBox::from_id_salt(("gs_mode", k)).width(70.0).selected_text(if r.matching { "equal to" } else { "=" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.matching, false, "= value");
                    ui.selectable_value(&mut r.matching, true, "equal to variable");
                });
                if r.matching {
                    var_combo(ui, ("gs_other", k), &m, &mut r.other, |_| true);
                } else {
                    ui.add(egui::TextEdit::singleline(&mut r.value).desired_width(80.0));
                    if let Some(v) = m.var(&r.target) {
                        ui.label(conv(&m, du, v).1.symbol);
                    }
                }
                ui.label("by changing");
                var_combo(ui, ("gs_vary", k), &m, &mut r.vary, |_| true);
                if nrows > 1 && ui.small_button("remove").clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            g.rows.remove(k);
        }
        ui.horizontal(|ui| {
            if ui.button("Add condition").on_hover_text("Solve several conditions together (n variables changed for n conditions)").clicked() {
                g.rows.push(GoalRow::default());
            }
            let valid =
                |r: &GoalRow| m.var(&r.target).is_some() && m.var(&r.vary).is_some() && if r.matching { m.var(&r.other).is_some() } else { r.value.trim().parse::<f64>().is_ok() };
            let can = self.running.is_none() && g.rows.iter().all(valid);
            if ui.add_enabled(can, egui::Button::new(RichText::new("Run").strong())).clicked() {
                // matching conditions become `__match_k = (y1) - (y2)` driven to zero
                let mut src = script.to_string();
                for (k, r) in g.rows.iter().enumerate().filter(|(_, r)| r.matching) {
                    src.push_str(&format!("\n__match_{k} = ({}) - ({})\n", r.target, r.other));
                }
                let m2 = Arc::new(dfcore::compile(&src, &dfcore::CompileOptions { unit_system: Some(m.unit_system.clone()), check_fluids: false }));
                let mut targets = Vec::new();
                for (k, r) in g.rows.iter().enumerate() {
                    if r.matching {
                        targets.push((m2.var(&format!("__match_{k}")).unwrap(), 0.0));
                    } else {
                        let tv = m.var(&r.target).unwrap();
                        let (mu, d) = conv(&m, du, tv);
                        targets.push((m2.var(&r.target).unwrap(), d.convert(r.value.trim().parse::<f64>().unwrap(), &mu)));
                    }
                }
                let vary: Vec<u32> = g.rows.iter().map(|r| m2.var(&r.vary).unwrap()).collect();
                let names: Vec<String> = g.rows.iter().map(|r| r.vary.clone()).collect();
                // warm start mapped by name (the augmented model has extra variables)
                let warm = a.solution.as_ref().map(|s| m2.vars.iter().map(|v| m.var(&v.name).map(|i| s.x[i as usize]).unwrap_or(f64::NAN)).collect::<Vec<f64>>());
                let (tx, rx) = channel();
                self.rx = Some(rx);
                self.running = Some("goal");
                std::thread::spawn(move || {
                    let t0 = Instant::now();
                    let r = goal_seek_multi(&m2, &vary, &targets, &SolveOptions::default(), warm.as_deref());
                    let _ = tx.send(JobResult::Goal(r, names, t0.elapsed().as_secs_f64() * 1e3));
                });
                self.goal_result = None;
            }
            if self.running == Some("goal") {
                ui.spinner();
            }
        });
        if let Some(res) = &self.goal_result {
            ui.horizontal_wrapped(|ui| {
                for (n, x) in res {
                    if let Some(v) = m.var(n) {
                        let (mu, d) = conv(&m, du, v);
                        let unit = if d.symbol == "-" { "" } else { d.symbol.as_str() };
                        ui.label(RichText::new(format!("{n} = {} {unit}", fmt_num(mu.convert(*x, &d))).trim_end()).strong().color(Color32::from_rgb(90, 180, 90)));
                    }
                }
                if ui.button("Apply to script").on_hover_text("Overwrite the `x = value` equations of the changed variables with these results").clicked() {
                    self.apply_request = Some(res.clone());
                }
            });
        }
        ui.add_space(12.0);
        ui.separator();
        ui.heading("Optimisation");
        ui.horizontal(|ui| {
            ui.radio_value(&mut o.maximize, true, "Maximise");
            ui.radio_value(&mut o.maximize, false, "Minimise");
            var_combo(ui, "opt_obj", &m, &mut o.objective, |_| true);
            ui.label("Method");
            egui::ComboBox::from_id_salt("opt_method")
                .selected_text(match o.method {
                    OptMethod::Brent => "Brent (1 variable)",
                    OptMethod::NelderMead => "Nelder-Mead (local)",
                    OptMethod::DifferentialEvolution => "Differential evolution (global)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut o.method, OptMethod::Brent, "Brent (1 variable)");
                    ui.selectable_value(&mut o.method, OptMethod::NelderMead, "Nelder-Mead (local)");
                    ui.selectable_value(&mut o.method, OptMethod::DifferentialEvolution, "Differential evolution (global)");
                });
            ui.add(egui::DragValue::new(&mut o.max_evals).range(10..=100000).prefix("max evals "));
        });
        ui.label("Decision variables and bounds (display units):");
        let mut remove = None;
        for (k, (n, lo, hi)) in o.vars.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                var_combo(ui, ("opt_var", k), &m, n, |_| true);
                ui.add(egui::TextEdit::singleline(lo).desired_width(70.0).hint_text("lower"));
                ui.label("…");
                ui.add(egui::TextEdit::singleline(hi).desired_width(70.0).hint_text("upper"));
                if let Some(v) = m.var(n) {
                    ui.label(conv(&m, du, v).1.symbol);
                }
                if ui.small_button("remove").clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            o.vars.remove(k);
        }
        ui.horizontal(|ui| {
            if ui.button("Add variable").clicked() {
                o.vars.push(Default::default());
            }
            let can = self.running.is_none() && m.var(&o.objective).is_some() && !o.vars.is_empty();
            if ui.add_enabled(can, egui::Button::new(RichText::new("Optimise").strong())).clicked() {
                let mut vars = Vec::new();
                for (n, lo, hi) in &o.vars {
                    let (Some(v), Ok(l), Ok(h)) = (m.var(n), lo.trim().parse::<f64>(), hi.trim().parse::<f64>()) else {
                        self.message = format!("check variable '{n}' and its bounds");
                        return;
                    };
                    let (mu, d) = conv(&m, du, v);
                    vars.push((v, d.convert(l, &mu), d.convert(h, &mu)));
                }
                let spec = OptSpec { objective: m.var(&o.objective).unwrap(), maximize: o.maximize, vars, method: o.method, max_evals: o.max_evals, tol: 1e-8 };
                self.opt_units = o.vars.iter().map(|(n, ..)| n.clone()).collect();
                let warm = a.solution.as_ref().map(|s| s.x.clone());
                let (tx, rx) = channel();
                self.rx = Some(rx);
                self.running = Some("opt");
                let m2 = m.clone();
                std::thread::spawn(move || {
                    let t0 = Instant::now();
                    let r = optimize(&m2, &spec, &SolveOptions::default(), warm.as_deref());
                    let _ = tx.send(JobResult::Opt(r, t0.elapsed().as_secs_f64() * 1e3));
                });
                self.opt_result = None;
            }
            if self.running == Some("opt") {
                ui.spinner();
            }
        });
        if let Some(r) = &self.opt_result {
            ui.add_space(6.0);
            if let Some(ov) = m.var(&o.objective) {
                let (mu, d) = conv(&m, du, ov);
                ui.label(RichText::new(format!("{} = {} {}", o.objective, fmt_num(mu.convert(r.objective, &d)), d.symbol)).strong().color(Color32::from_rgb(90, 180, 90)));
            }
            for (k, n) in self.opt_units.iter().enumerate() {
                if let (Some(v), Some(b)) = (m.var(n), r.best.get(k)) {
                    let (mu, d) = conv(&m, du, v);
                    ui.label(format!("{n} = {} {}", fmt_num(mu.convert(*b, &d)), d.symbol));
                }
            }
            if ui.button("Apply to script").on_hover_text("Overwrite the `x = value` equations of the decision variables with the optimum").clicked() {
                self.apply_request = Some(self.opt_units.iter().cloned().zip(r.best.iter().copied()).collect());
            }
            let pts: Vec<[f64; 2]> = r.history.iter().enumerate().filter(|(_, v)| v.is_finite()).map(|(i, v)| [i as f64 + 1.0, *v]).collect();
            egui_plot::Plot::new("opt_hist").height(220.0).x_axis_label("evaluation").y_axis_label("best objective (model units)").show(ui, |p| {
                p.line(egui_plot::Line::new("best", pts));
            });
        }
    }
}

/// Number with ~9 significant digits, trailing zeros removed (enough to
/// reproduce goal-seek and optimisation results to their tolerance).
pub fn fmt_full(x: f64) -> String {
    if x == 0.0 || !x.is_finite() {
        return format!("{x}");
    }
    let a = x.abs();
    if !(1e-4..1e9).contains(&a) {
        return format!("{x:.8e}");
    }
    let d = (8 - a.log10().floor() as i32).clamp(0, 12) as usize;
    let s = format!("{x:.d$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Overwrite the `x = constant` equation of each variable in the script with
/// a new value (model units). A unit literal in the original equation
/// (`P[2] = 150 [bar]`) is kept and the value converted to it. Returns the
/// rewritten lines; fails without changing the text if any variable has no
/// constant equation or the script changed since it was analysed.
pub fn apply_values(text: &mut String, model: &Model, vals: &[(u32, f64)]) -> Result<Vec<String>, String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for &(v, x) in vals {
        let name = &model.vars[v as usize].display;
        let Some(&e) = dfcore::studies::constant_eqs_of(model, v).first() else {
            return Err(format!("{name} is not set by an `{name} = value` equation in the script, so there is nothing to overwrite"));
        };
        let eq = &model.eqs[e as usize];
        let src = text.get(eq.span.start..eq.span.end).ok_or("the script changed since the last analysis; solve again first")?;
        if !src.contains('=') || src.trim() != eq.text.trim() {
            return Err("the script changed since the last analysis; solve again first".into());
        }
        // keep the unit of a literal such as `150 [bar]`
        let rhs_src = src.split_once('=').map(|x| x.1).unwrap_or("");
        let unit = rhs_src.rfind('[').and_then(|i| rhs_src[i + 1..].find(']').map(|j| rhs_src[i + 1..i + 1 + j].trim().to_string()));
        let rhs = match unit.as_deref().and_then(|u| parse_unit(u).ok().map(|pu| (u.to_string(), pu))) {
            Some((u, pu)) if !src.trim_end().ends_with(name.as_str()) => {
                let mu = model.unit_system.unit_for(pu.dim);
                format!("{} [{u}]", fmt_full(mu.convert(x, &pu)))
            }
            _ => fmt_full(x),
        };
        // keep the left-hand side as written when it is the variable itself
        let lhs = src.split('=').next().unwrap_or("").trim();
        let new = if dfcore::parser::canon(lhs).replace(' ', "") == model.vars[v as usize].name.replace(' ', "") { format!("{lhs} = {rhs}") } else { format!("{name} = {rhs}") };
        edits.push((eq.span.start, eq.span.end, new));
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut lines = Vec::new();
    for (a, b, new) in edits {
        text.replace_range(a..b, &new);
        lines.push(new);
    }
    lines.reverse();
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_keeps_units_and_layout() {
        let src = "P[1] = 0.1 [bar];  x[1] = 0\nP[2] = 150 [bar]\nT[3] = 550 [C]\neta = 0.85\ny = 2*eta + P[2] + T[3]\n";
        let m = dfcore::compile(src, &dfcore::CompileOptions::default());
        let mut t = src.to_string();
        let p2 = m.var("P[2]").unwrap();
        let t3 = m.var("T[3]").unwrap();
        let eta = m.var("eta").unwrap();
        let x1 = m.var("x[1]").unwrap();
        // model units are the engineering system (bar, C)
        let lines = apply_values(&mut t, &m, &[(p2, 123.456789012), (t3, 600.0), (eta, 0.9), (x1, 0.5)]).unwrap();
        assert_eq!(t, "P[1] = 0.1 [bar];  x[1] = 0.5\nP[2] = 123.456789 [bar]\nT[3] = 600 [C]\neta = 0.9\ny = 2*eta + P[2] + T[3]\n");
        assert_eq!(lines.len(), 4);
        let y = m.var("y").unwrap();
        assert!(apply_values(&mut t, &m, &[(y, 1.0)]).is_err());
        assert_eq!(fmt_full(0.123456789123), "0.123456789");
    }
}
