//! Library window (*Insert* menu): a property-call builder and searchable
//! lists of the property functions, fluids, state inputs, math functions,
//! constants, keywords and units, each of which can be inserted at the
//! editor's cursor.

use crate::analysis::Analysis;
use dfcore::locale::Separators;
use dfcore::props::{self, Key};
use dfcore::units::{Dim, UnitSystem};
use eframe::egui::{self, RichText};

/// Sections of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Section {
    #[default]
    Builder,
    Properties,
    Fluids,
    Inputs,
    Math,
    Constants,
    Keywords,
    Units,
}

impl Section {
    pub const ALL: [Section; 8] = [Section::Builder, Section::Properties, Section::Fluids, Section::Inputs, Section::Math, Section::Constants, Section::Keywords, Section::Units];
    pub fn label(self) -> &'static str {
        match self {
            Section::Builder => "Property call builder",
            Section::Properties => "Property functions",
            Section::Fluids => "Fluids",
            Section::Inputs => "State inputs",
            Section::Math => "Math functions",
            Section::Constants => "Constants",
            Section::Keywords => "Keywords and directives",
            Section::Units => "Units",
        }
    }
}

/// How the fluid name is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Backend {
    /// CoolProp's default (Helmholtz equations of state) or DeltaF Studio's
    /// own liquids.
    #[default]
    Default,
    /// IAPWS-IF97 for water (`'IF97::Water'`).
    If97,
    /// CoolProp incompressible fluids and solutions (`'INCOMP::MEG-30%'`).
    Incomp,
    /// Mixture (`'CO2[0.9]&Argon[0.1]'`).
    Mixture,
    /// Humid air (`AirH2O`), EES style, through HAPropsSI.
    HumidAir,
}

impl Backend {
    const ALL: [Backend; 5] = [Backend::Default, Backend::If97, Backend::Incomp, Backend::Mixture, Backend::HumidAir];
    fn label(self) -> &'static str {
        match self {
            Backend::Default => "Pure and pseudo-pure fluids (HEOS)",
            Backend::If97 => "Water, IAPWS-IF97",
            Backend::Incomp => "Incompressible liquids and solutions",
            Backend::Mixture => "Mixture",
            Backend::HumidAir => "Humid air (AirH2O)",
        }
    }
}

/// How many state inputs a property function takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arity {
    /// Two state inputs (one with the ideal-gas option).
    Two,
    /// One input from the given list.
    One(&'static [&'static str]),
    /// A fluid constant.
    None,
}

/// Property functions of the builder: (name, description, arity, output dimension).
const PROPS: &[(&str, &str, Arity, Dim)] = &[
    ("enthalpy", "Specific enthalpy", Arity::Two, Dim::SPEC_ENERGY),
    ("entropy", "Specific entropy", Arity::Two, Dim::SPEC_ENTROPY),
    ("temperature", "Temperature", Arity::Two, Dim::TEMP),
    ("pressure", "Pressure", Arity::Two, Dim::PRESSURE),
    ("density", "Density", Arity::Two, Dim::DENSITY),
    ("volume", "Specific volume", Arity::Two, Dim::SPEC_VOLUME),
    ("quality", "Vapour quality (-1 single phase)", Arity::Two, Dim::NONE),
    ("intenergy", "Specific internal energy", Arity::Two, Dim::SPEC_ENERGY),
    ("cp", "Isobaric specific heat", Arity::Two, Dim::SPEC_ENTROPY),
    ("cv", "Isochoric specific heat", Arity::Two, Dim::SPEC_ENTROPY),
    ("viscosity", "Dynamic viscosity", Arity::Two, Dim::VISCOSITY),
    ("conductivity", "Thermal conductivity", Arity::Two, Dim::CONDUCTIVITY),
    ("soundspeed", "Speed of sound", Arity::Two, Dim::VELOCITY),
    ("prandtl", "Prandtl number", Arity::Two, Dim::NONE),
    ("compressibilityfactor", "Compressibility factor Z", Arity::Two, Dim::NONE),
    ("kinematicviscosity", "Kinematic viscosity", Arity::Two, Dim::DIFFUSIVITY),
    ("thermaldiffusivity", "Thermal diffusivity", Arity::Two, Dim::DIFFUSIVITY),
    ("isentropicexponent", "Isentropic exponent", Arity::Two, Dim::NONE),
    ("isothermalcompress", "Isothermal compressibility", Arity::Two, Dim::INV_PRESSURE),
    ("volexpcoef", "Volumetric expansion coefficient", Arity::Two, Dim::INV_TEMP),
    ("surfacetension", "Surface tension", Arity::One(&["T"]), Dim::SURFACE_TENSION),
    ("t_sat", "Saturation temperature", Arity::One(&["P"]), Dim::TEMP),
    ("p_sat", "Saturation pressure", Arity::One(&["T"]), Dim::PRESSURE),
    ("enthalpy_vaporization", "Latent heat of vaporization", Arity::One(&["T", "P"]), Dim::SPEC_ENERGY),
    ("molarmass", "Molar mass", Arity::None, Dim::MOLAR_MASS),
    ("t_crit", "Critical temperature", Arity::None, Dim::TEMP),
    ("p_crit", "Critical pressure", Arity::None, Dim::PRESSURE),
    ("v_crit", "Critical specific volume", Arity::None, Dim::SPEC_VOLUME),
    ("t_triple", "Triple-point temperature", Arity::None, Dim::TEMP),
    ("p_triple", "Triple-point pressure", Arity::None, Dim::PRESSURE),
    ("acentricfactor", "Acentric factor", Arity::None, Dim::NONE),
    ("normalboilingpt", "Normal boiling point", Arity::None, Dim::TEMP),
];

/// Humid-air functions (fluid `AirH2O`), three inputs including P.
const HA_PROPS: &[(&str, &str)] = &[
    ("enthalpy", "Enthalpy per kg dry air"),
    ("entropy", "Entropy per kg dry air"),
    ("volume", "Specific volume per kg dry air"),
    ("temperature", "Dry-bulb temperature"),
    ("cp", "Specific heat per kg dry air"),
    ("humrat", "Humidity ratio (kg water / kg dry air)"),
    ("relhum", "Relative humidity (0-1)"),
    ("wetbulb", "Wet-bulb temperature"),
    ("dewpoint", "Dew-point temperature"),
    ("viscosity", "Dynamic viscosity"),
    ("conductivity", "Thermal conductivity"),
];

/// State inputs: (name, meaning).
pub const STATE_INPUTS: &[(&str, &str)] = &[
    ("P", "pressure"),
    ("T", "temperature"),
    ("h", "specific enthalpy"),
    ("s", "specific entropy"),
    ("u", "specific internal energy"),
    ("v", "specific volume"),
    ("d", "density (also rho)"),
    ("x", "vapour quality (also q)"),
];

/// Humid-air inputs: (name, meaning).
pub const HA_INPUTS: &[(&str, &str)] = &[
    ("T", "dry-bulb temperature"),
    ("P", "total pressure"),
    ("w", "humidity ratio"),
    ("R", "relative humidity (0-1)"),
    ("B", "wet-bulb temperature"),
    ("D", "dew-point temperature"),
    ("h", "enthalpy per kg dry air"),
];

/// Keywords and directives: (name, description, snippet inserted).
pub const KEYWORDS: &[(&str, &str, &str)] = &[
    ("Duplicate", "Repeat equations for each index: Duplicate i = 1, N ... End", "Duplicate i = 1, N\n    \nEnd\n"),
    ("Function", "A function of its arguments, written with assignments", "Function f(x)\n    f := x\nEnd\n"),
    ("Procedure", "A procedure with inputs and outputs: Procedure p(a, b : c, d)", "Procedure p(a : b)\n    b := a\nEnd\n"),
    ("Call", "Call a procedure: Call p(x : y)", "Call p( : )"),
    ("If", "If cond Then ... ElseIf ... Else ... EndIf (in functions)", "If x > 0 Then\n    \nElse\n    \nEndIf\n"),
    ("Repeat", "Repeat ... Until cond (in functions)", "Repeat\n    \nUntil x > 0\n"),
    ("While", "While cond ... End (in functions)", "While x > 0\n    \nEnd\n"),
    ("For", "For i = a To b [Step s] ... End (in functions)", "For i = 1 To N\n    \nEnd\n"),
    ("Return", "Leave a function or procedure", "Return"),
    ("and", "Logical and", "and "),
    ("or", "Logical or", "or "),
    ("not", "Logical not", "not "),
    ("$UnitSystem", "Model unit system: Eng, SI, Imperial or custom (SI C bar kJ)", "$UnitSystem Eng\n"),
    ("$Guess", "Starting values: $Guess x = 1, T[3] = 300", "$Guess "),
    ("$Bounds", "Bounds: $Bounds x = 0 .. 1", "$Bounds "),
    ("$Units", "Display units: $Units T[*] = C, W_net = MW", "$Units "),
    ("$Control", "Control loop: $Control vary x to make y = target", "$Control vary  to make  = "),
];

/// What is known about a fluid, for the fluid table.
#[derive(Clone, Debug)]
pub struct FluidInfo {
    pub name: String,
    pub group: &'static str,
    /// (M kg/kmol, Tcrit K, pcrit Pa), when CoolProp gives them.
    pub data: Option<(f64, f64, f64)>,
    /// Other names of the same fluid (`CarbonDioxide`, `R744` for `CO2`).
    pub aliases: Vec<String>,
}

/// Other names of a fluid, without the name itself.
fn other_names(name: &str) -> Vec<String> {
    props::fluid_aliases(name).into_iter().filter(|a| !a.eq_ignore_ascii_case(name) && !a.contains("::")).collect()
}

/// Every fluid the property functions accept by name, grouped.
pub fn fluid_catalogue(coolprop: &[String]) -> Vec<FluidInfo> {
    let mut out = Vec::new();
    for f in coolprop {
        let m = props::backend::props1_si(f, "molar_mass").ok().map(|m| m * 1e3);
        let tc = props::backend::props1_si(f, "Tcrit").ok();
        let pc = props::backend::props1_si(f, "pcrit").ok();
        let data = match (m, tc, pc) {
            (Some(m), Some(t), Some(p)) => Some((m, t, p)),
            _ => None,
        };
        out.push(FluidInfo { name: f.clone(), group: "CoolProp (HEOS)", data, aliases: other_names(f) });
    }
    for (f, _) in crate::completion::EXTRA_FLUIDS {
        out.push(FluidInfo { name: f.to_string(), group: "Liquids and heat-transfer fluids", data: None, aliases: other_names(f) });
    }
    out.push(FluidInfo { name: "AirH2O".into(), group: "Humid air", data: None, aliases: props::HUMID_AIR_NAMES[1..].iter().map(|s| s.to_string()).collect() });
    out
}

/// Builder state.
#[derive(Clone, Debug)]
struct Builder {
    func: String,
    backend: Backend,
    fluid: String,
    mixture: String,
    ideal: bool,
    /// (input name, value expression)
    ins: Vec<(String, String)>,
    lhs: String,
    preview: Option<Result<String, String>>,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            func: "enthalpy".into(),
            backend: Backend::Default,
            fluid: "Water".into(),
            mixture: "CO2[0.9]&Argon[0.1]".into(),
            ideal: false,
            ins: vec![("P".into(), String::new()), ("T".into(), String::new())],
            lhs: String::new(),
            preview: None,
        }
    }
}

impl Builder {
    fn arity(&self) -> Arity {
        if self.backend == Backend::HumidAir {
            return Arity::Two;
        }
        PROPS.iter().find(|p| p.0 == self.func).map(|p| p.2).unwrap_or(Arity::Two)
    }

    /// Input names allowed for this function, fluid and option.
    fn allowed(&self) -> Vec<&'static str> {
        if self.backend == Backend::HumidAir {
            return HA_INPUTS.iter().map(|i| i.0).collect();
        }
        match self.arity() {
            Arity::None => vec![],
            Arity::One(k) => k.to_vec(),
            Arity::Two if self.ideal => match self.func.as_str() {
                "temperature" => vec!["h", "u"],
                "enthalpy" | "intenergy" | "cp" | "cv" => vec!["T"],
                // T and P, or inputs that fix them (h and u depend on T only)
                _ => vec!["T", "P", "h", "u", "s", "v", "d"],
            },
            Arity::Two => STATE_INPUTS.iter().map(|i| i.0).collect(),
        }
    }

    /// Number of inputs.
    fn n_inputs(&self) -> usize {
        if self.backend == Backend::HumidAir {
            return 3;
        }
        match self.arity() {
            Arity::None => 0,
            Arity::One(_) => 1,
            Arity::Two if self.ideal => match self.func.as_str() {
                "temperature" | "enthalpy" | "intenergy" | "cp" | "cv" => 1,
                _ => 2,
            },
            Arity::Two => 2,
        }
    }

    /// Make the inputs fit the function: their number and allowed names,
    /// keeping values already typed.
    fn fix_inputs(&mut self) {
        let allowed = self.allowed();
        let n = self.n_inputs();
        self.ins.truncate(n);
        while self.ins.len() < n {
            let next = allowed.iter().find(|a| !self.ins.iter().any(|(k, _)| k == *a)).copied().unwrap_or("T");
            self.ins.push((next.into(), String::new()));
        }
        for i in 0..self.ins.len() {
            if !allowed.contains(&self.ins[i].0.as_str()) {
                let next = allowed.iter().find(|a| !self.ins.iter().any(|(k, _)| k == *a)).copied().unwrap_or(allowed.first().copied().unwrap_or("T"));
                self.ins[i].0 = next.into();
            }
        }
        if self.backend == Backend::HumidAir && !self.ins.iter().any(|(k, _)| k == "P") {
            self.ins[1].0 = "P".into();
        }
    }

    fn fluid_text(&self) -> String {
        match self.backend {
            Backend::Default if self.ideal => format!("{}::Ideal", dfcore::props::split_model(&self.fluid).0),
            Backend::Default => self.fluid.clone(),
            Backend::If97 => "'IF97::Water'".into(),
            Backend::Incomp => format!("'INCOMP::{}'", self.fluid),
            Backend::Mixture => format!("'{}'", self.mixture),
            Backend::HumidAir => "AirH2O".into(),
        }
    }

    /// The call, e.g. `enthalpy(Water, P=P[1], T=T[1])`.
    fn call(&self, sep: &str) -> String {
        let mut s = format!("{}({}", self.func, self.fluid_text());
        for (k, v) in &self.ins {
            let v = if v.trim().is_empty() { "…" } else { v.trim() };
            s.push_str(&format!("{sep}{k}={v}"));
        }
        s.push(')');
        s
    }

    fn code(&self, sep: &str) -> String {
        let c = self.call(sep);
        if self.lhs.trim().is_empty() {
            c
        } else {
            format!("{} = {c}", self.lhs.trim())
        }
    }

    fn complete(&self) -> Result<(), String> {
        if self.backend == Backend::Default && self.fluid.trim().is_empty() {
            return Err("choose a fluid".into());
        }
        if let Some((k, _)) = self.ins.iter().find(|(_, v)| v.trim().is_empty()) {
            return Err(format!("give a value for {k}"));
        }
        let mut keys: Vec<&str> = self.ins.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        if keys.len() != self.ins.len() {
            return Err("the inputs must be different".into());
        }
        Ok(())
    }
}

/// Is the expression a number with an optional unit (`10`, `10 [bar]`)?
fn is_literal(v: &str) -> bool {
    let v = v.trim();
    let num = v.split('[').next().unwrap_or("").trim();
    num.replace(',', ".").parse::<f64>().is_ok() && v.find('[').is_none_or(|i| v[i..].ends_with(']'))
}

/// Evaluate a property call: alone when its inputs are numbers, else with
/// the model (`script`) so that its variables are known.
pub fn evaluate(call: &str, script: &str, us: &UnitSystem, standalone: bool) -> Result<String, String> {
    const NAME: &str = "zz_library_preview";
    let src = if standalone { format!("{NAME} = {call}\n") } else { format!("{script}\n{NAME} = {call}\n") };
    let opts = dfcore::CompileOptions { unit_system: Some(us.clone()), check_fluids: true };
    let m = dfcore::compile(&src, &opts);
    if let Some(d) = m.diagnostics.iter().find(|d| d.severity == dfcore::Severity::Error) {
        return Err(d.msg.clone());
    }
    let v = m.var(NAME).ok_or("no value")?;
    let (_, sol) = dfcore::solve(&m, &dfcore::SolveOptions::default());
    if !sol.known[v as usize] {
        return Err(if sol.message.is_empty() { "could not be evaluated (are its inputs known?)".into() } else { sol.message.clone() });
    }
    let x = sol.x[v as usize];
    Ok(match m.display_unit(v) {
        Some((mu, du)) => format!("{} {}", crate::editor::fmt_num(mu.convert(x, &du)), du.symbol),
        None => crate::editor::fmt_num(x),
    })
}

/// The library window.
#[derive(Default)]
pub struct Library {
    pub open: bool,
    pub section: Section,
    search: String,
    b: Builder,
    fluids: Option<Vec<FluidInfo>>,
    incomp: Option<(Vec<String>, Vec<String>)>,
    fluid_filter: String,
}

/// What the window asks the application to do.
pub enum Action {
    /// Insert this text at the editor's cursor.
    Insert(String),
}

impl Library {
    /// Open the window at a section.
    pub fn open_at(&mut self, s: Section) {
        self.open = true;
        self.section = s;
    }

    pub fn show(&mut self, ctx: &egui::Context, analysis: Option<&Analysis>, script: &str, us: &UnitSystem, coolprop: &[String]) -> Option<Action> {
        if !self.open {
            return None;
        }
        let mut act = None;
        let mut open = self.open;
        egui::Window::new("Library").open(&mut open).default_size([820.0, 560.0]).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("name or description").desired_width(260.0));
                if !self.search.is_empty() && ui.small_button("✕").clicked() {
                    self.search.clear();
                }
            });
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    for s in Section::ALL {
                        ui.selectable_value(&mut self.section, s, s.label());
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Double-click an entry, or press Insert, to put it at the editor's cursor.").small().weak());
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().id_salt("library_content").auto_shrink([false, false]).show(ui, |ui| {
                        act = match self.section {
                            Section::Builder => self.builder(ui, analysis, script, us, coolprop),
                            Section::Properties => self.properties(ui),
                            Section::Fluids => self.fluids(ui, coolprop),
                            Section::Inputs => self.inputs(ui, us),
                            Section::Math => list(ui, &self.search, dfcore::compile::MATH_FUNCTIONS.iter().map(|(n, d)| (n.to_string(), d.to_string(), format!("{n}(")))),
                            Section::Constants => list(ui, &self.search, dfcore::compile::BUILTIN_CONSTS.iter().map(|(n, d)| (n.to_string(), d.to_string(), n.to_string()))),
                            Section::Keywords => list(ui, &self.search, KEYWORDS.iter().map(|(n, d, s)| (n.to_string(), d.to_string(), s.to_string()))),
                            Section::Units => self.units(ui),
                        };
                    });
                });
            });
        });
        self.open = open;
        act
    }

    fn builder(&mut self, ui: &mut egui::Ui, analysis: Option<&Analysis>, script: &str, us: &UnitSystem, coolprop: &[String]) -> Option<Action> {
        let mut act = None;
        let sep = if dfcore::locale::detect(script) == Separators::Comma { "; " } else { ", " };
        let before = (self.b.func.clone(), self.b.backend, self.b.ideal);
        egui::Grid::new("builder").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Fluid model");
            egui::ComboBox::from_id_salt("lib_backend").selected_text(self.b.backend.label()).width(300.0).show_ui(ui, |ui| {
                for b in Backend::ALL {
                    ui.selectable_value(&mut self.b.backend, b, b.label());
                }
            });
            ui.end_row();

            ui.label("Fluid");
            match self.b.backend {
                Backend::Default => {
                    ui.vertical(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.b.fluid).desired_width(300.0).hint_text("Water, CO2, R134a …"));
                        let used = crate::completion::script_fluids(script);
                        let f = self.b.fluid.to_lowercase();
                        let mut picks: Vec<String> = used.clone();
                        picks.extend(crate::completion::COMMON_FLUIDS.iter().map(|s| s.to_string()));
                        picks.extend(crate::completion::EXTRA_FLUIDS.iter().map(|s| s.0.to_string()));
                        picks.extend(coolprop.iter().cloned());
                        let mut seen = std::collections::HashSet::new();
                        picks.retain(|p| seen.insert(p.to_lowercase()));
                        let exact = picks.iter().any(|p| p.eq_ignore_ascii_case(&self.b.fluid));
                        if !exact {
                            let hits: Vec<&String> = picks.iter().filter(|p| f.is_empty() || crate::completion::match_tier(p, &f).is_some()).take(12).collect();
                            ui.horizontal_wrapped(|ui| {
                                for h in hits {
                                    if ui.small_button(h).clicked() {
                                        self.b.fluid = h.clone();
                                    }
                                }
                            });
                        } else if !used.is_empty() {
                            ui.label(RichText::new(format!("in this model: {}", used.join(", "))).small().weak());
                        }
                    });
                }
                Backend::If97 => {
                    ui.label("Water (IAPWS-IF97, the industrial formulation; faster, slightly less accurate than IAPWS-95)");
                }
                Backend::Incomp => {
                    let (pure, sol) = self.incomp.get_or_insert_with(props::backend::incompressible_list).clone();
                    egui::ComboBox::from_id_salt("lib_incomp").selected_text(&self.b.fluid).width(300.0).show_ui(ui, |ui| {
                        if pure.is_empty() && sol.is_empty() {
                            ui.label("CoolProp is not available in this build");
                        }
                        ui.label(RichText::new("Pure").strong());
                        for f in &pure {
                            ui.selectable_value(&mut self.b.fluid, f.clone(), f);
                        }
                        ui.label(RichText::new("Solutions (add the fraction, e.g. MEG-30%)").strong());
                        for f in &sol {
                            ui.selectable_value(&mut self.b.fluid, format!("{f}-30%"), f);
                        }
                    });
                }
                Backend::Mixture => {
                    ui.vertical(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.b.mixture).desired_width(300.0));
                        ui.label(RichText::new("Components with mole fractions, joined by &: CO2[0.9]&Argon[0.1]").small().weak());
                    });
                }
                Backend::HumidAir => {
                    ui.label("AirH2O: per kg dry air (ASHRAE RP-1485, CoolProp HAPropsSI)");
                }
            }
            ui.end_row();

            ui.label("Property");
            let funcs: Vec<(&str, &str)> = if self.b.backend == Backend::HumidAir { HA_PROPS.to_vec() } else { PROPS.iter().map(|p| (p.0, p.1)).collect() };
            if !funcs.iter().any(|f| f.0 == self.b.func) {
                self.b.func = funcs[0].0.into();
            }
            let cur = funcs.iter().find(|f| f.0 == self.b.func).map(|f| format!("{}  ({})", f.1, f.0)).unwrap_or_default();
            egui::ComboBox::from_id_salt("lib_func").selected_text(cur).width(300.0).show_ui(ui, |ui| {
                for (n, d) in &funcs {
                    ui.selectable_value(&mut self.b.func, n.to_string(), format!("{d}  ({n})"));
                }
            });
            ui.end_row();

            let can_ideal = self.b.backend == Backend::Default
                && matches!(
                    self.b.func.as_str(),
                    "enthalpy" | "entropy" | "intenergy" | "cp" | "cv" | "temperature" | "pressure" | "density" | "volume" | "soundspeed" | "compressibilityfactor"
                );
            if can_ideal {
                ui.label("");
                ui.checkbox(&mut self.b.ideal, "Ideal gas (Fluid::Ideal)").on_hover_text(
                    "Writes Fluid::Ideal: the ideal-gas model, P v = R T with the fluid's own cp0(T). enthalpy, intenergy, cp, cv from T; entropy, density, ... from T and P. \
                     Without it, one input selects the ideal gas and two the real fluid; Fluid::Real forces the real fluid.",
                );
                ui.end_row();
            } else {
                self.b.ideal = false;
            }
        });
        if before != (self.b.func.clone(), self.b.backend, self.b.ideal) {
            self.b.preview = None;
        }
        self.b.fix_inputs();
        let allowed = self.b.allowed();
        let ha = self.b.backend == Backend::HumidAir;
        if !self.b.ins.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("State inputs").strong());
            egui::Grid::new("builder_inputs").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                for i in 0..self.b.ins.len() {
                    let (k, v) = &mut self.b.ins[i];
                    let meaning = |k: &str| (if ha { HA_INPUTS } else { STATE_INPUTS }).iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or("");
                    egui::ComboBox::from_id_salt(("lib_in", i)).selected_text(format!("{k} — {}", meaning(k))).width(220.0).show_ui(ui, |ui| {
                        for a in &allowed {
                            ui.selectable_value(k, a.to_string(), format!("{a} — {}", meaning(a)));
                        }
                    });
                    ui.add_sized([200.0, 20.0], egui::TextEdit::singleline(v).hint_text("variable, number or expression"));
                    // variables of the right quantity
                    let dim = input_dim(k, ha);
                    let vars = matching_vars(analysis, dim);
                    egui::ComboBox::from_id_salt(("lib_var", i)).selected_text("variables…").width(110.0).show_ui(ui, |ui| {
                        if vars.is_empty() {
                            ui.label(RichText::new("no variable of this quantity yet").weak());
                        }
                        for (name, val) in &vars {
                            if ui.selectable_label(false, format!("{name}  {val}")).clicked() {
                                *v = name.clone();
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Result variable");
            ui.add(egui::TextEdit::singleline(&mut self.b.lhs).desired_width(160.0).hint_text("optional, e.g. h[1]"));
        });
        ui.add_space(8.0);
        let code = self.b.code(sep);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(&code).monospace().size(15.0));
        });
        let ok = self.b.complete();
        ui.horizontal(|ui| {
            if ui.add_enabled(ok.is_ok(), egui::Button::new("Insert at cursor")).clicked() {
                let line = if self.b.lhs.trim().is_empty() { code.clone() } else { format!("{code}\n") };
                act = Some(Action::Insert(line));
            }
            if ui.add_enabled(ok.is_ok(), egui::Button::new("Copy")).clicked() {
                ui.ctx().copy_text(code.clone());
            }
            let standalone = self.b.ins.iter().all(|(_, v)| is_literal(v));
            let tip = if standalone { "Evaluate with the numbers given" } else { "Evaluate with the model's current values (solves the model)" };
            if ui.add_enabled(ok.is_ok(), egui::Button::new("Evaluate")).on_hover_text(tip).clicked() {
                self.b.preview = Some(evaluate(&self.b.call(sep), script, us, standalone));
            }
            if let Err(e) = &ok {
                ui.label(RichText::new(e).weak());
            }
        });
        match &self.b.preview {
            Some(Ok(v)) => {
                ui.label(RichText::new(format!("= {v}")).monospace().strong());
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            None => {}
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Values are in the model unit system unless they carry a unit: P=10 [bar].").small().weak());
        act
    }

    fn properties(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut rows: Vec<(String, String, String)> = props::PROPERTY_FUNCTIONS
            .iter()
            .map(|(n, d)| {
                let ins = match *n {
                    "propssi" => "PropsSI(".to_string(),
                    "props1si" => "Props1SI(".to_string(),
                    "hapropssi" => "HAPropsSI(".to_string(),
                    _ => format!("{n}("),
                };
                (ins.trim_end_matches('(').to_string(), d.to_string(), ins)
            })
            .collect();
        rows.push(("enthalpy_vaporization".into(), "Latent heat  enthalpy_vaporization(Fluid, T=) or (Fluid, P=)".into(), "enthalpy_vaporization(".into()));
        for (n, d) in HA_PROPS {
            if matches!(*n, "humrat" | "relhum" | "wetbulb" | "dewpoint") {
                rows.push((n.to_string(), format!("{d}  {n}(AirH2O, T=, P=, R=)"), format!("{n}(AirH2O, ")));
            }
        }
        let mut act = list(ui, &self.search, rows.into_iter());
        ui.add_space(8.0);
        if ui.button("Open the property call builder").clicked() {
            self.section = Section::Builder;
            act = None;
        }
        act
    }

    fn fluids(&mut self, ui: &mut egui::Ui, coolprop: &[String]) -> Option<Action> {
        let mut act = None;
        let fl = self.fluids.get_or_insert_with(|| fluid_catalogue(coolprop));
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.fluid_filter);
        });
        let q = format!("{} {}", self.search, self.fluid_filter).trim().to_lowercase();
        let mut use_in_builder: Option<String> = None;
        for group in ["CoolProp (HEOS)", "Liquids and heat-transfer fluids", "Humid air"] {
            let hit = |f: &FluidInfo| q.is_empty() || f.name.to_lowercase().contains(&q) || f.aliases.iter().any(|a| a.to_lowercase().contains(&q));
            let rows: Vec<&FluidInfo> = fl.iter().filter(|f| f.group == group && hit(f)).collect();
            if rows.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{group} ({})", rows.len())).default_open(true).show(ui, |ui| {
                egui::Grid::new(("fluids", group)).striped(true).num_columns(6).spacing([14.0, 2.0]).show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Also");
                    ui.strong("M [kg/kmol]");
                    ui.strong("T_crit [°C]");
                    ui.strong("p_crit [bar]");
                    ui.label("");
                    ui.end_row();
                    for f in rows {
                        if ui.add(egui::Label::new(RichText::new(&f.name).monospace()).sense(egui::Sense::click())).double_clicked() {
                            act = Some(Action::Insert(f.name.clone()));
                        }
                        let also = f.aliases.join(", ");
                        let short = if also.chars().count() > 40 { format!("{}…", also.chars().take(39).collect::<String>()) } else { also.clone() };
                        let r = ui.label(RichText::new(short).small().weak());
                        if !also.is_empty() {
                            r.on_hover_text(format!("Same fluid: {also}"));
                        }
                        match f.data {
                            Some((m, t, p)) => {
                                ui.label(format!("{m:.3}"));
                                ui.label(format!("{:.2}", t - 273.15));
                                ui.label(format!("{:.3}", p / 1e5));
                            }
                            None => {
                                ui.label("");
                                ui.label("");
                                ui.label("");
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Insert").clicked() {
                                act = Some(Action::Insert(f.name.clone()));
                            }
                            if f.group != "Humid air" && ui.small_button("Builder").on_hover_text("Use this fluid in the property call builder").clicked() {
                                use_in_builder = Some(f.name.clone());
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Names are not case-sensitive and every alias is the same fluid (CO2 = CarbonDioxide = R744, Water = H2O = Steam). Other forms: 'IF97::Water', 'INCOMP::MEG-30%' (incompressible), 'CO2[0.9]&Argon[0.1]' (mixture), see the builder.").small().weak());
        if let Some(f) = use_in_builder {
            self.b.backend = Backend::Default;
            self.b.fluid = f;
            self.section = Section::Builder;
        }
        act
    }

    fn inputs(&mut self, ui: &mut egui::Ui, us: &UnitSystem) -> Option<Action> {
        let mut act = None;
        ui.label(RichText::new("Named state inputs of the property functions").strong());
        egui::Grid::new("lib_inputs").striped(true).num_columns(4).spacing([14.0, 2.0]).show(ui, |ui| {
            ui.strong("Input");
            ui.strong("Meaning");
            ui.strong("Unit in this model");
            ui.label("");
            ui.end_row();
            for (k, d) in STATE_INPUTS {
                let unit = Key::from_input(k).map(|k| us.unit_for(k.dim()).symbol).unwrap_or_default();
                ui.monospace(format!("{k}="));
                ui.label(*d);
                ui.label(unit);
                if ui.small_button("Insert").clicked() {
                    act = Some(Action::Insert(format!("{k}=")));
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("Humid air (AirH2O) inputs; give P and two more").strong());
        egui::Grid::new("lib_ha_inputs").striped(true).num_columns(3).spacing([14.0, 2.0]).show(ui, |ui| {
            for (k, d) in HA_INPUTS {
                ui.monospace(format!("{k}="));
                ui.label(*d);
                if ui.small_button("Insert").clicked() {
                    act = Some(Action::Insert(format!("{k}=")));
                }
                ui.end_row();
            }
        });
        act
    }

    fn units(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut act = None;
        let q = self.search.to_lowercase();
        egui::Grid::new("lib_units").striped(true).num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
            for qn in crate::units_ui::QUANTITIES {
                let units: Vec<&str> =
                    crate::units_ui::choices(qn.dim).into_iter().filter(|u| q.is_empty() || u.to_lowercase().contains(&q) || qn.label.to_lowercase().contains(&q)).collect();
                if units.is_empty() {
                    continue;
                }
                ui.label(qn.label);
                ui.horizontal_wrapped(|ui| {
                    for u in units {
                        if ui.small_button(u).on_hover_text(format!("Insert [{u}]")).clicked() {
                            act = Some(Action::Insert(format!("[{u}]")));
                        }
                    }
                });
                ui.end_row();
            }
            ui.label("Temperature difference");
            ui.horizontal(|ui| {
                for u in ["dK", "dC"] {
                    if ui.small_button(u).clicked() {
                        act = Some(Action::Insert(format!("[{u}]")));
                    }
                }
            });
            ui.end_row();
        });
        ui.label(RichText::new("Compound units are written with / and - (or *): kJ/kg-K, W/m^2-K.").small().weak());
        act
    }
}

/// Dimension of a builder input.
fn input_dim(k: &str, ha: bool) -> Option<Dim> {
    if ha {
        return props::HaCall::input(k).map(|x| x.1);
    }
    Key::from_input(k).map(|k| k.dim())
}

/// (display name, value) of the variables of dimension `dim`.
fn matching_vars(a: Option<&Analysis>, dim: Option<Dim>) -> Vec<(String, String)> {
    let (Some(a), Some(dim)) = (a, dim) else { return vec![] };
    let mut v: Vec<(String, String)> = (0..a.model.vars.len())
        .filter(|i| a.model.var_dim(*i as u32) == Some(dim))
        .map(|i| {
            let val = match &a.solution {
                Some(s) if s.known[i] => {
                    let (x, u) = crate::app::display_in(&Default::default(), a, i, s.x[i]);
                    format!("= {} {u}", crate::editor::fmt_num(x))
                }
                _ => String::new(),
            };
            (a.model.vars[i].display.clone(), val)
        })
        .collect();
    v.sort_by(|x, y| crate::app::natord(&x.0, &y.0));
    v
}

/// A searchable list of (name, description, snippet); returns the snippet
/// to insert.
fn list(ui: &mut egui::Ui, search: &str, rows: impl Iterator<Item = (String, String, String)>) -> Option<Action> {
    let q = search.to_lowercase();
    let mut act = None;
    egui::Grid::new(ui.next_auto_id()).striped(true).num_columns(3).spacing([14.0, 3.0]).show(ui, |ui| {
        for (n, d, snip) in rows {
            if !q.is_empty() && !n.to_lowercase().contains(&q) && !d.to_lowercase().contains(&q) {
                continue;
            }
            let r = ui.add(egui::Label::new(RichText::new(&n).monospace().strong()).sense(egui::Sense::click()));
            if r.double_clicked() {
                act = Some(Action::Insert(snip.clone()));
            }
            ui.label(&d);
            if ui.small_button("Insert").clicked() {
                act = Some(Action::Insert(snip.clone()));
            }
            ui.end_row();
        }
    });
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_code() {
        let mut b = Builder::default();
        b.ins[0].1 = "P[1]".into();
        b.ins[1].1 = "T[1]".into();
        b.lhs = "h[1]".into();
        assert_eq!(b.code(", "), "h[1] = enthalpy(Water, P=P[1], T=T[1])");
        assert_eq!(b.code("; "), "h[1] = enthalpy(Water; P=P[1]; T=T[1])");
        b.func = "t_sat".into();
        b.fix_inputs();
        assert_eq!(b.ins, vec![("P".to_string(), "P[1]".to_string())]);
        b.func = "molarmass".into();
        b.fix_inputs();
        assert!(b.ins.is_empty());
        assert_eq!(b.call(", "), "molarmass(Water)");
        b.func = "enthalpy".into();
        b.ideal = true;
        b.fix_inputs();
        assert_eq!(b.ins.len(), 1);
        assert_eq!(b.ins[0].0, "T");
        assert!(b.call(", ").starts_with("enthalpy(Water::Ideal, T="));
        b.func = "entropy".into();
        b.fix_inputs();
        assert_eq!(b.ins.len(), 2);
        b.func = "enthalpy".into();
        b.backend = Backend::HumidAir;
        b.ideal = false;
        b.fix_inputs();
        assert_eq!(b.ins.len(), 3);
        assert!(b.ins.iter().any(|(k, _)| k == "P"));
        assert!(b.call(", ").starts_with("enthalpy(AirH2O, "));
        b.backend = Backend::If97;
        b.func = "density".into();
        b.fix_inputs();
        assert!(b.call(", ").starts_with("density('IF97::Water', "));
    }

    #[test]
    fn literals() {
        assert!(is_literal("10"));
        assert!(is_literal(" 10 [bar] "));
        assert!(is_literal("1,5"));
        assert!(!is_literal("P[1]"));
        assert!(!is_literal("2*x"));
    }

    #[test]
    fn evaluates_a_call() {
        let us = UnitSystem::engineering();
        // saturated vapour at 1 atm: h_g = 2675.6 kJ/kg (IAPWS-95)
        let v = evaluate("enthalpy(Water, P=1.01325 [bar], x=1)", "", &us, true).unwrap();
        let x: f64 = v.split_whitespace().next().unwrap().parse().unwrap();
        assert!((x - 2675.6).abs() < 0.5, "{v}");
        assert!(v.ends_with("kJ/kg"));
        // with the model's variables
        let v = evaluate("temperature(Water, P=p1, x=0)", "p1 = 1.01325 [bar]", &us, false).unwrap();
        assert!(v.starts_with("100") || v.starts_with("99.9"), "{v}");
        assert!(evaluate("enthalpy(NoSuchFluid, P=1, T=20)", "", &us, true).is_err());
    }
}
