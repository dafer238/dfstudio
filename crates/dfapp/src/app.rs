//! Main window: menus, tabs, variables and solution tables.

use crate::analysis::{Analysis, Request, Worker};
use crate::cycle::{CycleDiagram, CycleInputs, CycleUi};
use crate::deps::{DepAction, Deps, GraphState};
use crate::editor::{fmt_num, status_color, Editor, EditorContext, Focus};
use crate::plots::{PlotInputs, PlotsState, PlotsUi};
use crate::studies_ui::{GoalForm, OptForm, ParamTable, StudiesUi};
use crate::units_ui::{choices, DisplayPreset};
use dfcore::dffile::{Artifact, DfFile, Meta};
use dfcore::locale::Separators;
use dfcore::units::{parse_unit, Unit};
use dfcore::{CompileOptions, Severity, SolveOptions, UnitSystem, VarStatus};
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EXAMPLE: &str = include_str!("../../../examples/rankine.dfs");
const LANGUAGE_DOC: &str = include_str!("../../../docs/LANGUAGE.md");

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    Equations,
    Solution,
    Arrays,
    Parametric,
    Optimize,
    Plots,
    Cycle,
    Components,
}

/// What a variable is in the model, for the Variables panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Set by a constant (`x = 5 [bar]`) or fixed by a study.
    Input,
    Calculated,
    Pending,
    Over,
    Unused,
}

impl Role {
    fn of(a: &Analysis, v: usize) -> Role {
        let st = &a.plan.structure;
        match st.var_status[v] {
            VarStatus::Fixed => Role::Input,
            VarStatus::UnderDetermined => Role::Pending,
            VarStatus::OverDetermined => Role::Over,
            VarStatus::Unused => Role::Unused,
            VarStatus::Determined => match st.var_block[v] {
                Some(b) => {
                    let bl = &st.blocks[b as usize];
                    let own = bl.eqs.len() == 1 && a.model.eqs[bl.eqs[0] as usize].vars.iter().all(|&x| x as usize == v);
                    if own {
                        Role::Input
                    } else {
                        Role::Calculated
                    }
                }
                None => Role::Calculated,
            },
        }
    }
    fn tag(self) -> &'static str {
        match self {
            Role::Input => "input",
            Role::Calculated => "calc",
            Role::Pending => "pending",
            Role::Over => "over",
            Role::Unused => "unused",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoleFilter {
    All,
    Inputs,
    Calculated,
    Problems,
}

/// Sub-views of the Equations tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EqView {
    Editor,
    Dependencies,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Settings {
    pub text: String,
    pub path: Option<PathBuf>,
    pub unit_system: UnitSystem,
    pub display_units: HashMap<String, String>,
    pub live_solve: bool,
    pub font_size: f32,
    #[serde(default)]
    pub table: ParamTable,
    #[serde(default)]
    pub goal: GoalForm,
    #[serde(default)]
    pub opt: OptForm,
    #[serde(default)]
    pub plots: PlotsState,
    #[serde(default)]
    pub cycle: CycleDiagram,
    #[serde(default)]
    pub format: crate::format::FormatOptions,
    /// Format the script before every save.
    #[serde(default = "yes")]
    pub format_on_save: bool,
    /// Format the script when the editor loses focus.
    #[serde(default = "yes")]
    pub format_on_blur: bool,
    /// Revision of the defaults the stored settings were written with; older
    /// settings are moved to the current defaults once.
    #[serde(default)]
    pub defaults_rev: u32,
}

fn yes() -> bool {
    true
}

/// Current revision of the defaults (1: format on save and on focus change,
/// indent 4, comment column 60).
const DEFAULTS_REV: u32 = 1;

impl Settings {
    fn upgrade(&mut self) {
        if self.defaults_rev < 1 {
            self.format_on_save = true;
            self.format_on_blur = true;
            self.format = Default::default();
        }
        self.defaults_rev = DEFAULTS_REV;
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            text: EXAMPLE.to_string(),
            path: None,
            unit_system: UnitSystem::engineering(),
            display_units: HashMap::new(),
            live_solve: true,
            font_size: 14.0,
            table: ParamTable::default(),
            goal: GoalForm::default(),
            opt: OptForm::default(),
            plots: PlotsState::default(),
            cycle: CycleDiagram::default(),
            format: Default::default(),
            format_on_save: true,
            format_on_blur: true,
            defaults_rev: DEFAULTS_REV,
        }
    }
}

pub struct DfApp {
    pub s: Settings,
    dirty: bool,
    worker: Worker,
    pub analysis: Option<Arc<Analysis>>,
    gen: u64,
    last_edit: Option<Instant>,
    tab: Tab,
    editor: Editor,
    var_filter: String,
    sol_filter: String,
    show_diag: bool,
    show_help: bool,
    status: String,
    studies: StudiesUi,
    plots: PlotsUi,
    cycle: CycleUi,
    /// Files attached to the project (saved inside the .df file).
    files: Vec<Artifact>,
    meta: Meta,
    show_files: bool,
    /// Variable under study (canonical name): highlighted in the editor and
    /// analysed in the Dependencies view.
    focus: Option<String>,
    eq_view: EqView,
    deps: Option<Deps>,
    graph: GraphState,
    role_filter: RoleFilter,
    /// Save automatically (a few seconds after the last change) to the
    /// current path. Off for opened files, on for a project started from
    /// scratch once it has been saved for the first time.
    auto_save: bool,
    /// Project started from scratch and never saved.
    fresh: bool,
    /// Hash of the project state at the last save or load.
    saved_hash: u64,
    /// Last time the project state differed from the saved one.
    changed_at: Option<Instant>,
    last_check: Option<Instant>,
    ctx: egui::Context,
}

impl DfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<String>) -> Self {
        // SVG icons (components library, menus)
        egui_extras::install_image_loaders(&cc.egui_ctx);
        // a little more air between rows and widgets than egui's defaults
        cc.egui_ctx.all_styles_mut(|st| {
            st.spacing.item_spacing = egui::vec2(8.0, 6.0);
            st.spacing.button_padding = egui::vec2(6.0, 3.0);
            st.spacing.interact_size.y = 22.0;
            st.spacing.menu_margin = egui::Margin::same(8);
            st.spacing.window_margin = egui::Margin::same(10);
        });
        let mut s: Settings = cc.storage.and_then(|st| eframe::get_value(st, "dfstudio")).unwrap_or_default();
        s.upgrade();
        let status = String::from("Ready");
        let mut editor = Editor::new();
        editor.font_size = s.font_size;
        editor.fluids = {
            let mut f = dfcore::props::backend::fluid_list();
            f.sort_by_key(|a| a.to_lowercase());
            f
        };
        let mut app = DfApp {
            s,
            dirty: false,
            worker: Worker::new(cc.egui_ctx.clone()),
            analysis: None,
            gen: 0,
            last_edit: None,
            tab: Tab::Equations,
            editor,
            var_filter: String::new(),
            sol_filter: String::new(),
            show_diag: true,
            show_help: false,
            status,
            studies: StudiesUi::default(),
            plots: PlotsUi::default(),
            cycle: CycleUi::default(),
            files: Vec::new(),
            meta: Meta::default(),
            show_files: false,
            focus: None,
            eq_view: EqView::Editor,
            deps: None,
            graph: GraphState::default(),
            role_filter: RoleFilter::All,
            auto_save: false,
            fresh: false,
            saved_hash: 0,
            changed_at: None,
            last_check: None,
            ctx: cc.egui_ctx.clone(),
        };
        match file {
            Some(f) => app.load_path(PathBuf::from(f)),
            None => {
                // a restored session without a file is work from scratch
                app.fresh = app.s.path.is_none();
                app.saved_hash = app.state_hash();
                app.submit(app.s.live_solve)
            }
        }
        app
    }

    fn submit(&mut self, solve: bool) {
        self.gen += 1;
        let warm = self.analysis.as_ref().and_then(|a| {
            let sol = a.solution.as_ref()?;
            Some(Arc::new(a.model.vars.iter().enumerate().filter(|(i, _)| sol.known[*i]).map(|(i, v)| (v.name.clone(), sol.x[i])).collect::<HashMap<_, _>>()))
        });
        self.worker.submit(Request {
            gen: self.gen,
            text: self.s.text.clone(),
            opts: CompileOptions { unit_system: Some(self.s.unit_system.clone()), check_fluids: true },
            solve,
            solve_opts: SolveOptions::default(),
            warm,
        });
    }

    /// Hash of everything a save writes (script, settings, studies, plots, diagram).
    fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&self.s).unwrap_or_default().hash(&mut h);
        self.files.len().hash(&mut h);
        h.finish()
    }

    /// Auto save: once a second, compare the project state with the saved
    /// one; save when it has been unchanged for 3 s after an edit.
    fn auto_save_tick(&mut self, ctx: &egui::Context) {
        if !self.auto_save || self.s.path.is_none() {
            return;
        }
        let now = Instant::now();
        if self.last_check.is_some_and(|t| now - t < Duration::from_secs(1)) {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.last_check = Some(now);
        let h = self.state_hash();
        if h == self.saved_hash {
            self.changed_at = None;
            return;
        }
        match self.changed_at {
            None => self.changed_at = Some(now),
            Some(t) if now - t >= Duration::from_secs(3) && self.last_edit.is_none() => {
                self.save(false);
                self.status = format!("Auto-saved {}", self.status.trim_start_matches("Saved "));
                self.changed_at = None;
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    /// Format the script (an undo point is recorded first).
    fn format_script(&mut self) {
        let f = crate::format::format(&self.s.text, &self.s.format);
        if f != self.s.text {
            let ctx = self.ctx.clone();
            self.editor.checkpoint(&ctx, &self.s.text);
            self.s.text = f;
            self.dirty = true;
            self.last_edit = Some(Instant::now());
        }
    }

    /// Convert the script to the other decimal / list separators (an undo
    /// point is recorded first).
    fn set_separators(&mut self, to: Separators) {
        let f = dfcore::locale::convert(&self.s.text, to);
        if f != self.s.text {
            let ctx = self.ctx.clone();
            self.editor.checkpoint(&ctx, &self.s.text);
            self.s.text = f;
            self.dirty = true;
            self.last_edit = Some(Instant::now());
            self.status = match to {
                Separators::Point => "Converted to decimal points and ',' separators".into(),
                Separators::Comma => "Converted to decimal commas and ';' separators".into(),
            };
        }
    }

    fn undo_redo(&mut self, redo: bool) {
        if matches!(self.tab, Tab::Cycle | Tab::Components) {
            if self.cycle.undo_redo(&mut self.s.cycle, redo) {
                self.dirty = true;
            }
            return;
        }
        let ctx = self.ctx.clone();
        if self.editor.history(&ctx, &mut self.s.text, redo) {
            self.dirty = true;
            self.last_edit = Some(Instant::now());
        }
    }

    fn new_project(&mut self) {
        self.s.text.clear();
        self.cycle.clear_history();
        self.s.path = None;
        self.files.clear();
        self.meta = Meta::default();
        self.dirty = false;
        self.fresh = true;
        self.auto_save = false;
        self.saved_hash = self.state_hash();
        self.submit(false);
    }

    fn title(&self) -> String {
        let name = self.s.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        format!("DF Studio — {}{}", name, if self.dirty { " *" } else { "" })
    }

    // ------------------------------------------------------------ files
    fn open(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("DF Studio project or script", &["df", "dfs", "ees", "txt"])
            .add_filter("DF Studio project", &["df"])
            .add_filter("Equation scripts", &["dfs", "ees", "txt"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load_path(p);
        }
    }

    /// Pick a model in the examples folder (next to the executable, in the
    /// source tree or in the working directory); without one, load the
    /// built-in Rankine example.
    fn load_example(&mut self) {
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            let exe = exe.canonicalize().unwrap_or(exe);
            if let Some(dir) = exe.parent() {
                // installed: next to the executable (Windows), in the app
                // bundle's Resources (macOS) or in share/ (AppImage)
                for sub in ["examples", "../Resources/examples", "../share/dfstudio/examples"] {
                    cands.push(dir.join(sub));
                }
            }
            for a in exe.ancestors().skip(2).take(3) {
                cands.push(a.join("examples"));
            }
        }
        cands.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples"));
        if let Ok(cwd) = std::env::current_dir() {
            cands.push(cwd.join("examples"));
        }
        match cands.into_iter().find(|d| d.is_dir()) {
            Some(dir) => {
                let dir = dir.canonicalize().unwrap_or(dir);
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Load example")
                    .set_directory(&dir)
                    .add_filter("DF Studio project or script", &["df", "dfs", "ees", "txt"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    self.load_path(p);
                }
            }
            None => {
                self.s.text = EXAMPLE.into();
                self.s.path = None;
                self.status = "No examples folder found: loaded the built-in Rankine example".into();
                self.submit(self.s.live_solve);
            }
        }
    }

    /// Open a `.df` project (recognised by its signature) or a text script.
    pub fn load_path(&mut self, p: PathBuf) {
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", p.display());
                return;
            }
        };
        if dfcore::dffile::is_df(&bytes) || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("df")) {
            match DfFile::from_bytes(&bytes) {
                Ok(f) => {
                    self.meta = f.meta.clone();
                    let n_unknown = f.unknown.len();
                    self.files = crate::project::apply_df(f, &mut self.s);
                    self.status = format!(
                        "Opened project {} ({} attached file{}){}",
                        p.display(),
                        self.files.len(),
                        if self.files.len() == 1 { "" } else { "s" },
                        if n_unknown > 0 { format!(" · {n_unknown} section(s) from a newer version kept") } else { String::new() }
                    );
                }
                Err(e) => {
                    self.status = format!("Cannot open {}: {e}", p.display());
                    return;
                }
            }
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ees")) {
            // EES model: binary (equations + Variable Information) or text
            match dfcore::ees::read(&bytes) {
                Ok(e) => {
                    self.s.text = e.to_script();
                    self.files.clear();
                    self.meta = Meta::default();
                    self.status = if e.version.is_empty() {
                        format!("Opened EES text file {}", p.display())
                    } else {
                        format!(
                            "Imported EES {} file {}: equations, and units, bounds and last solution (as guesses) from the Variable Information; EES tables, plots and diagrams are not imported. Save as .df or .dfs.",
                            e.version.trim_start_matches('X'),
                            p.display()
                        )
                    };
                }
                Err(err) => {
                    self.status = format!("Cannot open {}: {err}", p.display());
                    return;
                }
            }
        } else {
            self.s.text = String::from_utf8_lossy(&bytes).to_string();
            self.files.clear();
            self.meta = Meta::default();
            self.status = format!("Opened {}", p.display());
        }
        self.s.path = Some(p);
        self.dirty = false;
        self.analysis = None;
        self.cycle.clear_history();
        // opened files are not auto-saved unless asked (File > Auto save)
        self.auto_save = false;
        self.fresh = false;
        self.saved_hash = self.state_hash();
        self.submit(self.s.live_solve);
    }

    fn save(&mut self, as_new: bool) {
        let is_script = |p: &PathBuf| p.extension().is_some_and(|e| e == "dfs" || e == "txt");
        let is_df = |p: &PathBuf| p.extension().is_some_and(|e| e == "df");
        let path = match (&self.s.path, as_new) {
            (Some(p), false) if is_df(p) || is_script(p) => Some(p.clone()),
            _ => {
                let stem = self.s.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
                rfd::FileDialog::new()
                    .add_filter("DF Studio project (equations, settings, results, attached files)", &["df"])
                    .add_filter("Equation script only (text)", &["dfs"])
                    .set_file_name(format!("{stem}.df"))
                    .save_file()
            }
        };
        let Some(mut p) = path else { return };
        if p.extension().is_none() {
            p.set_extension("df");
        }
        if self.s.format_on_save {
            self.format_script();
        }
        let r = if is_script(&p) {
            std::fs::write(&p, &self.s.text).map_err(|e| e.to_string())
        } else {
            let mut f = crate::project::to_df(&self.s, self.analysis.as_deref(), &self.files, &self.meta);
            let r = f.save(&p);
            self.meta = f.meta;
            r
        };
        match r {
            Ok(()) => {
                self.status = format!("Saved {}", p.display());
                self.s.path = Some(p);
                self.dirty = false;
                if self.fresh {
                    // first save of a project started from scratch
                    self.fresh = false;
                    self.auto_save = true;
                }
                self.saved_hash = self.state_hash();
            }
            Err(e) => self.status = format!("Cannot save: {e}"),
        }
    }

    fn files_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_files;
        egui::Window::new("Project files").open(&mut open).default_size([520.0, 300.0]).show(ctx, |ui| {
            ui.label(RichText::new("Attached files are stored inside the .df project (data tables, images, references) and travel with it to other computers.").small().weak());
            ui.horizontal(|ui| {
                if ui.button("Add files…").clicked() {
                    if let Some(ps) = rfd::FileDialog::new().pick_files() {
                        for p in ps {
                            match std::fs::read(&p) {
                                Ok(data) => {
                                    let name = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                                    self.files.retain(|f| f.name != name);
                                    self.files.push(Artifact { name, data });
                                    self.dirty = true;
                                }
                                Err(e) => self.status = format!("Cannot read {}: {e}", p.display()),
                            }
                        }
                    }
                }
                ui.label(format!("{} file(s), {}", self.files.len(), human_size(self.files.iter().map(|f| f.data.len()).sum())));
            });
            ui.separator();
            let mut remove = None;
            egui::Grid::new("files").striped(true).num_columns(3).show(ui, |ui| {
                for (i, f) in self.files.iter().enumerate() {
                    ui.label(&f.name);
                    ui.label(human_size(f.data.len()));
                    ui.horizontal(|ui| {
                        if ui.small_button("extract…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_file_name(&f.name).save_file() {
                                self.status = match std::fs::write(&p, &f.data) {
                                    Ok(()) => format!("Extracted {}", p.display()),
                                    Err(e) => format!("Cannot write {}: {e}", p.display()),
                                };
                            }
                        }
                        if ui.small_button("remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                self.files.remove(i);
                self.dirty = true;
            }
        });
        self.show_files = open;
    }

    // ------------------------------------------------------------ units
    /// (value in display unit, unit symbol) for variable `i`.
    fn display(&self, a: &Analysis, i: usize, x: f64) -> (f64, String) {
        display_in(&self.s.display_units, a, i, x)
    }

    fn focus_id(&self, a: &Analysis) -> Option<u32> {
        self.focus.as_ref().and_then(|n| a.model.var(n))
    }

    /// Lines of the script that mention the focused variable.
    fn focus_hl(&self, a: &Analysis) -> Option<Focus> {
        let v = self.focus_id(a)?;
        let vi = &a.model.vars[v as usize];
        let bytes = self.s.text.as_bytes();
        let line_of = |b: usize, from: u32, start: usize| from + bytes[start.min(bytes.len())..b.min(bytes.len())].iter().filter(|c| **c == b'\n').count() as u32;
        let mut lines = std::collections::HashSet::new();
        lines.insert(vi.line);
        for eq in &a.model.eqs {
            if eq.vars.contains(&v) {
                let last = line_of(eq.span.end, eq.span.line, eq.span.start);
                lines.extend(eq.span.line..=last);
            }
        }
        Some(Focus { base: vi.base.clone(), index: if vi.index.len() == 1 { Some(vi.index[0]) } else { None }, lines })
    }

    /// Focus a variable (or clear the focus).
    fn set_focus(&mut self, a: &Analysis, v: Option<u32>) {
        self.focus = v.map(|v| a.model.vars[v as usize].name.clone());
    }

    fn apply_preset(&mut self, p: DisplayPreset) {
        let Some(a) = self.analysis.clone() else {
            return;
        };
        for (i, v) in a.model.vars.iter().enumerate() {
            if let Some(d) = a.model.var_dim(i as u32) {
                if d.is_none() {
                    continue;
                }
                let u: Unit = p.unit_for(d, &a.model.unit_system);
                self.s.display_units.insert(v.name.clone(), u.symbol);
            }
        }
    }

    fn unit_combo(&mut self, ui: &mut egui::Ui, a: &Analysis, i: usize, current: &str) {
        let Some(dim) = a.model.var_dim(i as u32) else {
            ui.label(current);
            return;
        };
        let mut opts = choices(dim);
        if !opts.iter().any(|o| *o == current) && !current.is_empty() {
            opts.insert(0, Box::leak(current.to_string().into_boxed_str()));
        }
        if opts.len() <= 1 {
            ui.label(current);
            return;
        }
        let mut sel = current.to_string();
        egui::ComboBox::from_id_salt(("unit", i)).selected_text(current).width(90.0).show_ui(ui, |ui| {
            for o in &opts {
                ui.selectable_value(&mut sel, o.to_string(), *o);
            }
        });
        if sel != current {
            self.s.display_units.insert(a.model.vars[i].name.clone(), sel);
        }
    }

    // ------------------------------------------------------------ panels
    fn variables_panel(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.analysis.clone() else {
            ui.label("Analysing…");
            return;
        };
        let st = &a.plan.structure;
        let dark = ui.visuals().dark_mode;
        ui.horizontal_wrapped(|ui| {
            let chip = |ui: &mut egui::Ui, s: VarStatus, label: &str, n: usize| {
                dot(ui, status_color(s, dark));
                ui.label(RichText::new(format!("{label} {n}")).color(status_color(s, dark)));
            };
            chip(ui, VarStatus::Determined, "determined", st.count(VarStatus::Determined));
            chip(ui, VarStatus::UnderDetermined, "pending", st.count(VarStatus::UnderDetermined));
            chip(ui, VarStatus::OverDetermined, "over-specified", st.count(VarStatus::OverDetermined));
        });
        ui.label(format!("{} equations · {} variables · {} degree(s) of freedom open", a.model.eqs.len(), a.model.vars.len(), st.dof));
        let roles: Vec<Role> = (0..a.model.vars.len()).map(|i| Role::of(&a, i)).collect();
        let n_in = roles.iter().filter(|r| **r == Role::Input).count();
        let n_calc = roles.iter().filter(|r| **r == Role::Calculated).count();
        let n_bad = roles.iter().filter(|r| matches!(r, Role::Pending | Role::Over)).count();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.role_filter, RoleFilter::All, "All");
            ui.selectable_value(&mut self.role_filter, RoleFilter::Inputs, format!("Inputs {n_in}"))
                .on_hover_text("Set directly by a constant in the script (x = 5 [bar]) or fixed by a study");
            ui.selectable_value(&mut self.role_filter, RoleFilter::Calculated, format!("Calculated {n_calc}")).on_hover_text("Computed from the equations");
            ui.selectable_value(&mut self.role_filter, RoleFilter::Problems, format!("Pending/over {n_bad}"))
                .on_hover_text("Not determined (open degrees of freedom) or over-specified");
        });
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.var_filter);
        });
        ui.horizontal(|ui| {
            ui.label("Display units");
            egui::ComboBox::from_id_salt("preset").selected_text("apply preset…").show_ui(ui, |ui| {
                for p in DisplayPreset::ALL {
                    if ui.selectable_label(false, p.label()).clicked() {
                        self.apply_preset(p);
                    }
                }
                if ui.selectable_label(false, "Reset to script units").clicked() {
                    self.s.display_units.clear();
                }
            });
        });
        ui.separator();
        // pending first, then over, then the rest
        let f = self.var_filter.to_lowercase();
        let rf = self.role_filter;
        let keep = |r: Role| match rf {
            RoleFilter::All => true,
            RoleFilter::Inputs => r == Role::Input,
            RoleFilter::Calculated => r == Role::Calculated,
            RoleFilter::Problems => matches!(r, Role::Pending | Role::Over),
        };
        let mut idx: Vec<usize> = (0..a.model.vars.len()).filter(|i| (f.is_empty() || a.model.vars[*i].name.contains(&f)) && keep(roles[*i])).collect();
        let rank = |s: VarStatus| match s {
            VarStatus::UnderDetermined => 0,
            VarStatus::OverDetermined => 1,
            _ => 2,
        };
        idx.sort_by(|x, y| rank(st.var_status[*x]).cmp(&rank(st.var_status[*y])).then(natord(&a.model.vars[*x].name, &a.model.vars[*y].name)));
        self.var_table(ui, &a, &idx, "vars_side");
    }

    fn var_table(&mut self, ui: &mut egui::Ui, a: &Arc<Analysis>, idx: &[usize], salt: &str) {
        let dark = ui.visuals().dark_mode;
        let st = &a.plan.structure;
        let mut jump: Option<u32> = None;
        let mut pick: Option<Option<u32>> = None;
        let focus = self.focus_id(a);
        ui.push_id(salt, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .column(Column::exact(14.0))
                .column(Column::exact(48.0))
                .column(Column::initial(130.0).at_least(60.0).clip(true))
                .column(Column::initial(120.0).at_least(60.0))
                .column(Column::remainder().at_least(70.0))
                .header(20.0, |mut h| {
                    h.col(|_| {});
                    h.col(|ui| {
                        ui.strong("Role").on_hover_text("input: set by a constant or a study · calc: computed by the equations");
                    });
                    h.col(|ui| {
                        ui.strong("Variable");
                    });
                    h.col(|ui| {
                        ui.strong("Value");
                    });
                    h.col(|ui| {
                        ui.strong("Unit");
                    });
                })
                .body(|body| {
                    body.rows(22.0, idx.len(), |mut row| {
                        let i = idx[row.index()];
                        let v = &a.model.vars[i];
                        let s = st.var_status[i];
                        row.col(|ui| {
                            dot(ui, status_color(s, dark)).on_hover_text(format!("{s:?}"));
                        });
                        let role = Role::of(a, i);
                        row.col(|ui| {
                            let t = RichText::new(role.tag()).small();
                            ui.label(match role {
                                Role::Input => t.strong(),
                                Role::Calculated => t.weak(),
                                _ => t.color(status_color(s, dark)),
                            });
                        });
                        row.col(|ui| {
                            let sel = focus == Some(i as u32);
                            if ui
                                .selectable_label(sel, RichText::new(&v.display).monospace())
                                .on_hover_text(format!("line {} · {:?}\nclick: highlight its equations and study its dependencies (click again to clear)", v.line, s))
                                .clicked()
                            {
                                pick = Some(if sel { None } else { Some(i as u32) });
                                if !sel {
                                    jump = Some(crate::deps::def_line(a, i as u32));
                                }
                            }
                        });
                        let (val, unit) = match &a.solution {
                            Some(sol) if sol.known[i] => {
                                let (d, u) = self.display(a, i, sol.x[i]);
                                (fmt_num(d), u)
                            }
                            _ => ("—".to_string(), self.display(a, i, 0.0).1),
                        };
                        row.col(|ui| {
                            ui.label(RichText::new(val).monospace());
                        });
                        row.col(|ui| {
                            self.unit_combo(ui, a, i, &unit);
                        });
                    });
                });
        });
        if let Some(p) = pick {
            self.set_focus(a, p);
            self.tab = Tab::Equations;
        }
        if let Some(l) = jump {
            if self.eq_view == EqView::Editor {
                self.editor.jump_to_line = Some(l);
            }
        }
    }

    fn solution_tab(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.analysis.clone() else {
            return;
        };
        let Some(sol) = &a.solution else {
            ui.label("No solution yet — press Solve (F2) or enable live solve.");
            return;
        };
        let col = if sol.converged { Color32::from_rgb(80, 170, 90) } else { Color32::from_rgb(220, 120, 40) };
        ui.label(RichText::new(&sol.message).color(col).strong());
        ui.label(format!(
            "{} blocks ({} explicit, largest {}), {:.2} ms solve, {:.2} ms compile",
            sol.blocks.len(),
            sol.blocks.iter().filter(|b| b.explicit).count(),
            sol.blocks.iter().map(|b| b.size).max().unwrap_or(0),
            sol.elapsed_ms,
            a.compile_ms
        ));
        for l in &a.control_log {
            ui.label(RichText::new(format!("Control loops: {l}")).weak());
        }
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.sol_filter);
            if ui.button("Copy table (TSV)").clicked() {
                let mut s = String::from("variable\tvalue\tunit\n");
                for (i, v) in a.model.vars.iter().enumerate() {
                    let (d, u) = self.display(&a, i, sol.x[i]);
                    s += &format!("{}\t{}\t{}\n", v.display, d, u);
                }
                ui.ctx().copy_text(s);
                self.status = "Solution copied to clipboard".into();
            }
        });
        ui.separator();
        let f = self.sol_filter.to_lowercase();
        let mut idx: Vec<usize> = (0..a.model.vars.len()).filter(|i| f.is_empty() || a.model.vars[*i].name.contains(&f)).collect();
        idx.sort_by(|x, y| natord(&a.model.vars[*x].name, &a.model.vars[*y].name));
        self.var_table(ui, &a, &idx, "vars_solution");
    }

    fn arrays_tab(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.analysis.clone() else {
            return;
        };
        let m = &a.model;
        let mut bases: Vec<String> = Vec::new();
        let mut cell: HashMap<(String, i64), usize> = HashMap::new();
        let mut idxs: Vec<i64> = Vec::new();
        for (i, v) in m.vars.iter().enumerate() {
            if v.index.len() == 1 {
                if !bases.contains(&v.base) {
                    bases.push(v.base.clone());
                }
                cell.insert((v.base.clone(), v.index[0]), i);
                idxs.push(v.index[0]);
            }
        }
        idxs.sort_unstable();
        idxs.dedup();
        if bases.is_empty() {
            ui.label("No arrays in this model.");
            return;
        }
        let disp: HashMap<String, String> = m.vars.iter().filter(|v| v.index.len() == 1).map(|v| (v.base.clone(), v.display.split('[').next().unwrap_or("").to_string())).collect();
        // export: the table as shown (display units), full precision
        let du = &self.s.display_units;
        let table = |sep: char| -> String {
            let q = |t: &str| {
                if sep == ',' && (t.contains(',') || t.contains('"')) {
                    format!("\"{}\"", t.replace('"', "\"\""))
                } else {
                    t.to_string()
                }
            };
            let mut out = String::from("i");
            for b in &bases {
                let unit = idxs.iter().find_map(|k| cell.get(&(b.clone(), *k))).map(|&i| display_in(du, &a, i, 0.0).1).unwrap_or_default();
                let h = if unit.is_empty() { disp[b].clone() } else { format!("{} [{unit}]", disp[b]) };
                out.push(sep);
                out += &q(&h);
            }
            out.push('\n');
            for k in &idxs {
                out += &k.to_string();
                for b in &bases {
                    out.push(sep);
                    if let (Some(&i), Some(sol)) = (cell.get(&(b.clone(), *k)), &a.solution) {
                        if sol.known[i] {
                            out += &format!("{}", display_in(du, &a, i, sol.x[i]).0);
                        }
                    }
                }
                out.push('\n');
            }
            out
        };
        let path = self.s.path.clone();
        let mut status: Option<String> = None;
        ui.horizontal(|ui| {
            if ui.button("Copy table (TSV)").on_hover_text("Paste into Excel or a text editor").clicked() {
                ui.ctx().copy_text(table('\t'));
                status = Some("Arrays table copied to clipboard".into());
            }
            if ui.button("Export CSV…").clicked() {
                let stem = path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "arrays".into());
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{stem}_arrays.csv")).save_file() {
                    status = Some(match std::fs::write(&p, table(',')) {
                        Ok(()) => format!("Arrays table written to {}", p.display()),
                        Err(e) => format!("Cannot write {}: {e}", p.display()),
                    });
                }
            }
            ui.label(RichText::new(format!("{} arrays × {} indices, values in display units", bases.len(), idxs.len())).weak());
        });
        if let Some(st) = status {
            self.status = st;
        }
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut tb = TableBuilder::new(ui).striped(true).column(Column::exact(50.0));
            for _ in &bases {
                tb = tb.column(Column::initial(105.0).at_least(60.0).resizable(true));
            }
            tb.header(36.0, |mut h| {
                h.col(|ui| {
                    ui.strong("i");
                });
                for b in &bases {
                    // unit of the first element
                    let unit = idxs.iter().find_map(|k| cell.get(&(b.clone(), *k))).map(|&i| self.display(&a, i, 0.0).1).unwrap_or_default();
                    h.col(|ui| {
                        ui.vertical(|ui| {
                            ui.strong(&disp[b]);
                            ui.label(RichText::new(unit).small().weak());
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, idxs.len(), |mut row| {
                    let k = idxs[row.index()];
                    row.col(|ui| {
                        ui.label(k.to_string());
                    });
                    for b in &bases {
                        row.col(|ui| {
                            if let Some(&i) = cell.get(&(b.clone(), k)) {
                                let txt = match &a.solution {
                                    Some(sol) if sol.known[i] => fmt_num(self.display(&a, i, sol.x[i]).0),
                                    _ => "—".into(),
                                };
                                ui.label(RichText::new(txt).monospace());
                            }
                        });
                    }
                });
            });
        });
    }

    /// Dependencies sub-tab: how the focused variable is computed, and the
    /// dependency graph (of the focus, or of the whole model).
    fn deps_view(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.analysis.clone() else {
            ui.label("Analysing…");
            return;
        };
        if !self.deps.as_ref().is_some_and(|d| d.is_for(&a)) {
            self.deps = Some(Deps::new(&a));
        }
        let d = self.deps.take().unwrap();
        let focus = self.focus_id(&a);
        let m = &a.model;
        let mut act: Option<DepAction> = None;
        ui.horizontal(|ui| {
            ui.label("Variable");
            let sel = focus.map(|v| m.vars[v as usize].display.clone()).unwrap_or_else(|| "All variables".into());
            egui::ComboBox::from_id_salt("dep_focus").selected_text(sel).height(400.0).show_ui(ui, |ui| {
                if ui.selectable_label(focus.is_none(), "All variables").clicked() {
                    act = Some(DepAction::Focus(None));
                }
                let mut idx: Vec<usize> = (0..m.vars.len()).collect();
                idx.sort_by(|x, y| natord(&m.vars[*x].name, &m.vars[*y].name));
                for i in idx {
                    if ui.selectable_label(focus == Some(i as u32), RichText::new(&m.vars[i].display).monospace()).clicked() {
                        act = Some(DepAction::Focus(Some(i as u32)));
                    }
                }
            });
            ui.separator();
            ui.add_enabled(focus.is_some(), egui::Checkbox::new(&mut self.graph.show_all, "Whole model"));
            ui.label("Depth");
            ui.add(egui::DragValue::new(&mut self.graph.depth).range(0..=50).custom_formatter(|v, _| if v == 0.0 { "all".into() } else { format!("{v}") }))
                .on_hover_text("Distance up and down from the focus (0 = all)");
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut self.graph.zoom, 0.2..=3.0).show_value(false)).on_hover_text("Ctrl + scroll on the graph also zooms");
            ui.separator();
            let leg = |ui: &mut egui::Ui, c: Color32, t: &str| {
                dot(ui, c);
                ui.label(RichText::new(t).small());
            };
            leg(ui, Color32::from_rgb(250, 200, 60), "focus / coupled");
            leg(ui, Color32::from_rgb(70, 140, 230), "needed by it");
            leg(ui, Color32::from_rgb(235, 140, 40), "computed from it");
        });
        ui.label(
            RichText::new("Edge u -> v: the block that computes v reads u. Boxes group variables solved simultaneously (one Newton block). Click a node to focus it, double-click to open its equation.")
                .small()
                .weak(),
        );
        ui.separator();
        let du = &self.s.display_units;
        let value = |v: u32| -> String {
            match &a.solution {
                Some(sol) if sol.known[v as usize] => {
                    let (x, u) = display_in(du, &a, v as usize, sol.x[v as usize]);
                    format!("{} {}", fmt_num(x), u)
                }
                _ => "—".into(),
            }
        };
        if let Some(v) = focus {
            egui::Panel::left("dep_info").resizable(true).default_size(420.0).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(x) = crate::deps::info(ui, &a, &d, v, &value) {
                        act = Some(x);
                    }
                });
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(x) = crate::deps::graph(ui, &a, &d, focus, &mut self.graph, &value) {
                act = Some(x);
            }
        });
        self.deps = Some(d);
        match act {
            Some(DepAction::Focus(v)) => self.set_focus(&a, v),
            Some(DepAction::Jump(l)) => {
                self.eq_view = EqView::Editor;
                self.editor.jump_to_line = Some(l);
            }
            None => {}
        }
    }

    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.analysis.clone() else {
            return;
        };
        egui::ScrollArea::vertical().max_height(120.0).auto_shrink([false, true]).show(ui, |ui| {
            let mut any = false;
            for d in &a.model.diagnostics {
                any = true;
                let (c, tag) = match d.severity {
                    Severity::Error => (Color32::from_rgb(230, 80, 80), "error"),
                    Severity::Warning => (Color32::from_rgb(230, 170, 60), "warning"),
                    Severity::Info => (Color32::GRAY, "info"),
                };
                if ui.link(RichText::new(format!("line {}: {tag}: {}", d.span.line, d.msg)).color(c)).clicked() {
                    self.tab = Tab::Equations;
                    self.editor.jump_to_line = Some(d.span.line);
                }
            }
            if let Some(sol) = &a.solution {
                if !sol.converged {
                    any = true;
                    ui.label(RichText::new(&sol.message).color(Color32::from_rgb(230, 130, 50)));
                }
            }
            for e in &a.plan.structure.redundant {
                any = true;
                let eq = &a.model.eqs[*e as usize];
                let res = a.solution.as_ref().map(|s| s.residuals[*e as usize]).unwrap_or(f64::NAN);
                let what = if res.is_finite() && res.abs() < 1e-6 { "redundant (consistent)" } else { "over-specifies the system" };
                if ui
                    .link(
                        RichText::new(format!("line {}: `{}` {what}: remove it or free one of its variables", eq.span.line, eq.label.trim())).color(Color32::from_rgb(230, 80, 80)),
                    )
                    .clicked()
                {
                    self.tab = Tab::Equations;
                    self.editor.jump_to_line = Some(eq.span.line);
                }
            }
            for e in &a.model.unit_conflicts {
                any = true;
                let eq = &a.model.eqs[*e as usize];
                if ui.link(RichText::new(format!("line {}: unit check: terms with different dimensions in `{}`", eq.span.line, eq.text.trim())).color(Color32::GRAY)).clicked() {
                    self.tab = Tab::Equations;
                    self.editor.jump_to_line = Some(eq.span.line);
                }
            }
            if !any {
                ui.label(RichText::new("No problems").weak());
            }
        });
    }
}

impl eframe::App for DfApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.s.font_size = self.editor.font_size;
        eframe::set_value(storage, "dfstudio", &self.s);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(a) = self.worker.poll() {
            if a.gen == self.gen || a.explicit_solve || self.analysis.is_none() {
                if a.explicit_solve {
                    if let Some(sol) = &a.solution {
                        self.status = sol.message.clone();
                    }
                }
                self.analysis = Some(Arc::new(a));
            }
        }
        self.studies.poll(&mut self.s.table);
        // debounce live analysis
        if let Some(t) = self.last_edit {
            let wait = Duration::from_millis(120);
            if t.elapsed() >= wait {
                self.last_edit = None;
                self.submit(self.s.live_solve);
            } else {
                ctx.request_repaint_after(wait - t.elapsed());
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        // shortcuts
        let (f2, ctrl_s, ctrl_o, ctrl_n, fmt_key) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::F2),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                i.consume_key(egui::Modifiers::SHIFT | egui::Modifiers::ALT, egui::Key::F),
            )
        });
        if fmt_key {
            self.format_script();
        }
        if f2 {
            self.submit(true);
            self.status = "Solving…".into();
        }
        if ctrl_s {
            self.save(false);
        }
        if ctrl_o {
            self.open();
        }
        if ctrl_n {
            self.new_project();
        }
        self.auto_save_tick(&ctx);

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if crate::icons::menu_item(ui, "new", "New", "Ctrl+N").clicked() {
                        self.new_project();
                    }
                    if crate::icons::menu_item(ui, "open", "Open…", "Ctrl+O").clicked() {
                        self.open();
                    }
                    if crate::icons::menu_item(ui, "save", "Save", "Ctrl+S").clicked() {
                        self.save(false);
                    }
                    if crate::icons::menu_item(ui, "save_as", "Save as…", "").clicked() {
                        self.save(true);
                    }
                    ui.checkbox(&mut self.auto_save, "Auto save")
                        .on_hover_text("Save to the current file 3 s after each change. Off for opened files; on for a new project after its first Save as.");
                    if crate::icons::menu_item(ui, "files", &format!("Project files… ({})", self.files.len()), "").clicked() {
                        self.show_files = true;
                    }
                    ui.separator();
                    if crate::icons::menu_item(ui, "example", "Load example…", "").on_hover_text("Open a model from the examples folder").clicked() {
                        self.load_example();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if crate::icons::menu_item(ui, "undo", "Undo", "Ctrl+Z").clicked() {
                        self.undo_redo(false);
                    }
                    if crate::icons::menu_item(ui, "redo", "Redo", "Ctrl+Y").clicked() {
                        self.undo_redo(true);
                    }
                    ui.separator();
                    if crate::icons::menu_item(ui, "find", "Find…", "Ctrl+F").clicked() {
                        self.tab = Tab::Equations;
                        self.eq_view = EqView::Editor;
                        self.editor.open_find(false, &self.s.text);
                    }
                    if crate::icons::menu_item(ui, "replace", "Replace…", "Ctrl+H").clicked() {
                        self.tab = Tab::Equations;
                        self.eq_view = EqView::Editor;
                        self.editor.open_find(true, &self.s.text);
                    }
                    ui.separator();
                    if crate::icons::menu_item(ui, "format", "Format script", "Shift+Alt+F")
                        .on_hover_text("Indent blocks, align trailing comments, normalise spacing (the equations do not change)")
                        .clicked()
                    {
                        self.format_script();
                    }
                    ui.checkbox(&mut self.s.format_on_save, "Format on save");
                    ui.checkbox(&mut self.s.format_on_blur, "Format when the editor loses focus");
                    ui.horizontal(|ui| {
                        ui.label("Comment column");
                        ui.add(egui::DragValue::new(&mut self.s.format.comment_col).range(16..=160));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Indent");
                        ui.add(egui::DragValue::new(&mut self.s.format.indent).range(1..=8).suffix(" spaces"));
                    });
                    ui.separator();
                    ui.label(RichText::new("Decimal and list separators").strong())
                        .on_hover_text("Detected from the script. Choosing the other one converts every number and argument list (comments are kept).");
                    let cur = dfcore::locale::detect(&self.s.text);
                    for (sep, name) in [(Separators::Point, "1.5 and f(a, b)  (US / UK)"), (Separators::Comma, "1,5 and f(a; b)  (Spanish / European)")] {
                        if ui.radio(cur == sep, name).clicked() && cur != sep {
                            self.set_separators(sep);
                        }
                    }
                });
                ui.menu_button("Solve", |ui| {
                    if crate::icons::menu_item(ui, "solve", "Solve", "F2").clicked() {
                        self.submit(true);
                    }
                    ui.checkbox(&mut self.s.live_solve, "Live solve while typing");
                });
                ui.menu_button("Units", |ui| {
                    ui.label(RichText::new("Model unit system").strong());
                    ui.label(RichText::new("(used to interpret the equations; $UnitSystem in the script overrides it)").small().weak());
                    for us in UnitSystem::presets() {
                        let sel = self.s.unit_system.name == us.name;
                        if ui.radio(sel, &us.name).clicked() {
                            self.s.unit_system = us;
                            self.submit(self.s.live_solve);
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Display units preset").strong());
                    for p in DisplayPreset::ALL {
                        if crate::icons::menu_item(ui, "units", p.label(), "").clicked() {
                            self.apply_preset(p);
                        }
                    }
                    if crate::icons::menu_item(ui, "reset", "Reset display units", "").clicked() {
                        self.s.display_units.clear();
                    }
                });
                ui.menu_button("View", |ui| {
                    if crate::icons::menu_item(ui, "font_larger", "Larger font", "").clicked() {
                        self.editor.font_size = (self.editor.font_size + 1.0).min(28.0);
                    }
                    if crate::icons::menu_item(ui, "font_smaller", "Smaller font", "").clicked() {
                        self.editor.font_size = (self.editor.font_size - 1.0).max(9.0);
                    }
                    ui.checkbox(&mut self.show_diag, "Problems panel");
                    ui.separator();
                    ui.label("Theme");
                    let cur = ui.ctx().options(|o| o.theme_preference);
                    ui.horizontal(|ui| {
                        for (t, name, icon) in
                            [(egui::ThemePreference::Light, "Light", "sun"), (egui::ThemePreference::Dark, "Dark", "moon"), (egui::ThemePreference::System, "System", "monitor")]
                        {
                            if ui.add(egui::Button::selectable(cur == t, (crate::icons::image(ui, icon, 14.0), name))).clicked() {
                                ui.ctx().set_theme(t);
                            }
                        }
                    });
                });
                ui.menu_button("Help", |ui| {
                    if crate::icons::menu_item(ui, "book", "Language reference", "").clicked() {
                        self.show_help = true;
                    }
                    ui.label("Ctrl+Space: completion · F2: solve");
                    ui.separator();
                    for (icon, text, url) in [
                        ("book", "Documentation", "https://github.com/dafer238/dfstudio/tree/main/docs"),
                        ("bug", "Report a problem", "https://github.com/dafer238/dfstudio/issues"),
                        ("download", "Releases", "https://github.com/dafer238/dfstudio/releases"),
                    ] {
                        if crate::icons::menu_item(ui, icon, text, "").on_hover_text(url).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                    ui.separator();
                    ui.label(format!("DF Studio {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Free software, GNU AGPL-3.0. Properties: CoolProp.");
                });
            });
        });

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Solve").strong()).on_hover_text("F2").clicked() {
                    self.submit(true);
                    self.status = "Solving…".into();
                }
                ui.separator();
                for (t, label) in [
                    (Tab::Equations, "Equations"),
                    (Tab::Solution, "Solution"),
                    (Tab::Arrays, "Arrays"),
                    (Tab::Parametric, "Parametric"),
                    (Tab::Optimize, "Goal seek / Optimise"),
                    (Tab::Plots, "Plots"),
                    (Tab::Cycle, "Cycle diagram"),
                    (Tab::Components, "Components"),
                ] {
                    ui.selectable_value(&mut self.tab, t, label);
                }
                ui.separator();
                if let Some(a) = &self.analysis {
                    ui.label(RichText::new(format!("Units: {}", a.model.unit_system.name)).weak());
                }
                if self.worker.busy {
                    ui.spinner();
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.auto_save && self.s.path.is_some() {
                    ui.label(RichText::new("auto save").small().color(Color32::from_rgb(80, 170, 90))).on_hover_text("File > Auto save");
                }
                ui.label(&self.status);
                if let Some(a) = &self.analysis {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let sol = a.solution.as_ref().map(|s| format!(" · solve {:.1} ms", s.elapsed_ms)).unwrap_or_default();
                        ui.label(RichText::new(format!("compile {:.1} ms · analysis {:.1} ms{}", a.compile_ms, a.analyse_ms, sol)).weak());
                    });
                }
            });
        });
        if self.show_diag {
            egui::Panel::bottom("problems").resizable(true).show(ui, |ui| {
                ui.strong("Problems");
                self.diagnostics(ui);
            });
        }

        if self.tab == Tab::Equations {
            egui::Panel::right("variables").resizable(true).default_size(420.0).show(ui, |ui| {
                ui.heading("Variables");
                self.variables_panel(ui);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Equations => {
                let an = self.analysis.clone();
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.eq_view, EqView::Editor, "Editor");
                    ui.selectable_value(&mut self.eq_view, EqView::Dependencies, "Dependencies");
                    if let Some(a) = &an {
                        if let Some(v) = self.focus_id(a) {
                            ui.separator();
                            ui.label("Focus:");
                            ui.label(RichText::new(&a.model.vars[v as usize].display).monospace().strong());
                            if ui.small_button("clear").on_hover_text("clear the focus (Esc)").clicked() {
                                self.focus = None;
                            }
                        }
                    }
                });
                if self.focus.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.editor.completion.is_none() {
                    self.focus = None;
                }
                match self.eq_view {
                    EqView::Editor => {
                        let fh = an.as_deref().and_then(|a| self.focus_hl(a));
                        let ectx = EditorContext { analysis: an.as_deref(), focus: fh.as_ref() };
                        if self.editor.show(ui, &mut self.s.text, &ectx) {
                            self.dirty = true;
                            self.last_edit = Some(Instant::now());
                            ctx.request_repaint_after(Duration::from_millis(130));
                        }
                        if self.editor.lost_focus && self.s.format_on_blur {
                            self.format_script();
                        }
                    }
                    EqView::Dependencies => self.deps_view(ui),
                }
            }
            Tab::Solution => self.solution_tab(ui),
            Tab::Arrays => self.arrays_tab(ui),
            Tab::Parametric => {
                if let Some(a) = self.analysis.clone() {
                    self.studies.parametric_tab(ui, &a, &mut self.s.table, &self.s.display_units);
                }
            }
            Tab::Optimize => {
                if let Some(a) = self.analysis.clone() {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.studies.optimise_tab(ui, &a, &mut self.s.goal, &mut self.s.opt, &self.s.display_units, &self.s.text);
                    });
                }
            }
            Tab::Plots => {
                if let Some(a) = self.analysis.clone() {
                    let inp = PlotInputs { analysis: &a, table: &self.s.table, studies: &self.studies, display_units: &self.s.display_units, script: &self.s.text };
                    self.plots.show(ui, &mut self.s.plots, &inp);
                }
            }
            Tab::Cycle => {
                if let Some(a) = self.analysis.clone() {
                    let inp = CycleInputs { analysis: &a, display_units: &self.s.display_units, script: &self.s.text };
                    self.cycle.show(ui, &mut self.s.cycle, &inp);
                }
                if let Some(t) = self.cycle.script_request.take() {
                    let ctx = self.ctx.clone();
                    self.editor.checkpoint(&ctx, &self.s.text);
                    self.s.text = t;
                    self.dirty = true;
                    self.last_edit = Some(Instant::now());
                }
            }
            Tab::Components => {
                if let Some(a) = self.analysis.clone() {
                    let inp = CycleInputs { analysis: &a, display_units: &self.s.display_units, script: &self.s.text };
                    self.cycle.components_tab(ui, &mut self.s.cycle, &inp);
                }
            }
        });
        if let Some(req) = self.studies.apply_request.take() {
            if let Some(a) = self.analysis.clone() {
                let vals: Vec<(u32, f64)> = req.iter().filter_map(|(n, x)| a.model.var(n).map(|v| (v, *x))).collect();
                match crate::studies_ui::apply_values(&mut self.s.text, &a.model, &vals) {
                    Ok(lines) => {
                        self.status = format!("Script updated: {}", lines.join("; "));
                        self.studies.message = self.status.clone();
                        self.dirty = true;
                        self.submit(true);
                    }
                    Err(e) => self.studies.message = format!("Apply: {e}"),
                }
            }
        }
        if self.studies.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        if self.show_files {
            self.files_window(&ctx);
        }
        // drag and drop a project or script onto the window
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())) {
            self.load_path(p);
        }

        if self.show_help {
            egui::Window::new("Language reference").open(&mut self.show_help).default_size([640.0, 560.0]).show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::markdown::show(ui, LANGUAGE_DOC);
                });
            });
        }
    }
}

/// (value in display unit, unit symbol) for variable `i`.
pub fn display_in(units: &HashMap<String, String>, a: &Analysis, i: usize, x: f64) -> (f64, String) {
    let m = &a.model;
    if let Some(u) = units.get(&m.vars[i].name) {
        if let Ok(du) = parse_unit(u) {
            let mu = m.unit_system.unit_for(du.dim);
            return (mu.convert(x, &du), u.clone());
        }
    }
    match m.display_unit(i as u32) {
        Some((mu, du)) => (mu.convert(x, &du), du.symbol),
        None => (x, String::new()),
    }
}

pub fn human_size(n: usize) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1 << 20 => format!("{:.1} KiB", n as f64 / 1024.0),
        n => format!("{:.1} MiB", n as f64 / (1u64 << 20) as f64),
    }
}

/// Small filled status dot (painted, independent of font glyphs).
pub fn dot(ui: &mut egui::Ui, c: Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, c);
    resp
}

/// Natural ordering so that t[2] < t[10].
pub fn natord(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| {
        let mut out: Vec<(String, i64)> = Vec::new();
        let mut txt = String::new();
        let mut num = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                if !num.is_empty() {
                    out.push((std::mem::take(&mut txt), num.parse().unwrap_or(0)));
                    num.clear();
                }
                txt.push(c);
            }
        }
        out.push((txt, num.parse().unwrap_or(-1)));
        out
    };
    key(a).cmp(&key(b))
}
