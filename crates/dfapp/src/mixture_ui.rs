//! Mixture editor of the property-call builder: components with their
//! fractions on a mole, mass or volume basis, preset compositions (natural
//! gases, air, refrigerant blends), the products of combustion of a fuel
//! with excess air, and the heating values of the mixture. It writes the
//! CoolProp mixture string with mole fractions (`Methane[0.9]&Ethane[0.1]`),
//! with `::Ideal` for the ideal-gas mixture.

use dfcore::mixtures::{self, Basis, FlowUnit, PRESETS, V_NORMAL};
use eframe::egui::{self, RichText};
use std::collections::HashMap;

/// Common species, offered as quick-add buttons: (label, CoolProp name).
const QUICK: &[(&str, &str)] = &[
    ("CH4", "Methane"),
    ("C2H6", "Ethane"),
    ("C3H8", "n-Propane"),
    ("C4H10", "n-Butane"),
    ("H2", "Hydrogen"),
    ("CO", "CarbonMonoxide"),
    ("H2S", "HydrogenSulfide"),
    ("N2", "Nitrogen"),
    ("O2", "Oxygen"),
    ("CO2", "CO2"),
    ("H2O", "Water"),
    ("Ar", "Argon"),
    ("SO2", "SulfurDioxide"),
];

#[derive(Clone, Debug)]
struct Row {
    name: String,
    /// percent on the editor's basis
    value: f64,
}

pub struct MixtureEditor {
    pub open: bool,
    basis: Basis,
    rows: Vec<Row>,
    ideal: bool,
    lambda: f64,
    note: Option<String>,
    /// molar masses (g/mol) by name
    masses: HashMap<String, Result<f64, String>>,
    /// CoolProp check of the real mixture, by mixture string
    checks: HashMap<String, Result<(), String>>,
    /// reference state of the properties: °C, kPa
    t_ref: f64,
    p_ref: f64,
    /// properties at the reference state, by mixture, model and state
    cache: HashMap<String, Result<RefProps, String>>,
    flow: f64,
    flow_unit: FlowUnit,
    /// the flow is that of the flue gas (else of the fuel)
    flow_is_flue: bool,
    /// time base of the combustion table: seconds
    per: f64,
}

/// Properties of the mixture at the reference state.
#[derive(Clone, Debug)]
struct RefProps {
    /// kg/kmol
    m: f64,
    /// kg/m³
    rho: f64,
    /// kJ/kg-K
    cp: f64,
    /// the ideal-gas mixture was used (CoolProp's real one failed)
    fell_back: bool,
    /// LHV, HHV in kJ/kmol
    hv: Option<(f64, f64)>,
}

fn ref_props(s: &str, ideal: bool, fuel: &[(String, f64)], m: f64, t: f64, p: f64) -> Result<RefProps, String> {
    let (rho, cp, used) = mixtures::density_cp(s, ideal, t, p)?;
    let hv = mixtures::heating_values(fuel, t).ok().filter(|h| h.0 > 0.0).map(|(l, h)| (l * 1e3, h * 1e3));
    Ok(RefProps { m, rho, cp: cp / 1e3, fell_back: used && !ideal, hv })
}

/// A number with 5 significant digits, without exponent for usual sizes.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-4..1e9).contains(&a) {
        return format!("{v:.4e}");
    }
    let d = (4 - a.log10().floor() as i32).clamp(0, 8) as usize;
    format!("{v:.d$}")
}

impl Default for MixtureEditor {
    fn default() -> Self {
        MixtureEditor {
            open: false,
            basis: Basis::Mole,
            rows: Vec::new(),
            ideal: false,
            lambda: 1.0,
            note: None,
            masses: HashMap::new(),
            checks: HashMap::new(),
            t_ref: 15.0,
            p_ref: 101.325,
            cache: HashMap::new(),
            flow: 1000.0,
            flow_unit: FlowUnit::Nm3H,
            flow_is_flue: false,
            per: 3600.0,
        }
    }
}

fn basis_label(b: Basis) -> &'static str {
    match b {
        Basis::Mole => "mol %",
        Basis::Mass => "mass %",
        Basis::Volume => "vol %",
    }
}

impl MixtureEditor {
    /// Open the editor on a mixture string (mole fractions).
    pub fn load(&mut self, text: &str, ideal: bool) {
        self.basis = Basis::Mole;
        self.note = None;
        self.rows = match mixtures::parse(text) {
            Ok((c, id)) => {
                self.ideal = ideal || id;
                let t: f64 = c.iter().map(|c| c.1).sum();
                c.into_iter().map(|(n, x)| Row { name: n, value: if t > 0.0 { 100.0 * x / t } else { 0.0 } }).collect()
            }
            Err(e) => {
                self.note = Some(e);
                Vec::new()
            }
        };
        if self.rows.is_empty() {
            self.rows.push(Row { name: "Methane".into(), value: 100.0 });
        }
        self.open = true;
    }

    fn mass(&mut self, name: &str) -> Result<f64, String> {
        let key = name.trim().to_string();
        self.masses.entry(key.clone()).or_insert_with(|| mixtures::molar_mass(&key)).clone()
    }

    /// Molar masses of the rows, or the first error.
    fn row_masses(&mut self) -> Result<Vec<f64>, String> {
        let names: Vec<String> = self.rows.iter().map(|r| r.name.clone()).collect();
        names.iter().map(|n| if n.trim().is_empty() { Err("a component has no name".into()) } else { self.mass(n) }).collect()
    }

    /// Names (CoolProp) and mole fractions of the rows.
    fn mole(&mut self) -> Result<(Vec<String>, Vec<f64>, Vec<f64>), String> {
        let m = self.row_masses()?;
        let v: Vec<f64> = self.rows.iter().map(|r| r.value).collect();
        let x = mixtures::to_mole(&v, &m, self.basis)?;
        let names = self.rows.iter().map(|r| dfcore::props::coolprop_fluid(r.name.trim())).collect();
        Ok((names, x, m))
    }

    /// Set the rows from names and mole fractions, on the current basis.
    fn set_mole(&mut self, names: Vec<String>, x: Vec<f64>) {
        let m: Vec<f64> = names.iter().map(|n| self.mass(n).unwrap_or(f64::NAN)).collect();
        let v = mixtures::from_mole(&x, &m, self.basis);
        self.rows = names.into_iter().zip(v).map(|(name, v)| Row { name, value: 100.0 * v }).collect();
    }

    /// The window. Returns the mixture string (mole fractions, no quotes,
    /// no model suffix) and the ideal-gas option when the user applies it.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<(String, bool)> {
        if !self.open {
            return None;
        }
        let mut out = None;
        let mut open = self.open;
        egui::Window::new("Mixture").open(&mut open).default_width(640.0).default_height(700.0).vscroll(true).resizable(true).show(ctx, |ui| {
            // ---- basis and presets
            ui.horizontal(|ui| {
                ui.label("Fractions in");
                let before = self.basis;
                for b in [Basis::Mole, Basis::Mass, Basis::Volume] {
                    let r = ui.radio_value(&mut self.basis, b, basis_label(b));
                    if b == Basis::Volume {
                        r.on_hover_text(
                            "Volume fraction of a gas mixture. By Amagat's law the partial volume of an ideal gas is proportional to its amount, \
                             so vol % = mol %. Real gases differ slightly (under about 0.5 % for natural gas at standard conditions); \
                             for liquids the volume fraction is not the mole fraction: use mass % or mol %.",
                        );
                    }
                }
                if self.basis != before {
                    // keep the composition, change how it is shown
                    let new = self.basis;
                    self.basis = before;
                    if let Ok((n, x, _)) = self.mole() {
                        self.basis = new;
                        self.set_mole(n, x);
                    } else {
                        self.basis = new;
                    }
                }
                ui.separator();
                egui::ComboBox::from_id_salt("mix_preset").selected_text("Presets…").width(150.0).show_ui(ui, |ui| {
                    for p in PRESETS {
                        if ui.selectable_label(false, p.name).on_hover_text(p.note).clicked() {
                            self.basis = p.basis;
                            self.ideal = p.ideal;
                            self.rows = p.comps.iter().map(|(n, v)| Row { name: n.to_string(), value: *v }).collect();
                            self.note = Some(format!("{}: {}", p.name, p.note));
                        }
                    }
                });
            });
            ui.add_space(4.0);
            // ---- components
            let masses: Vec<Result<f64, String>> = {
                let names: Vec<String> = self.rows.iter().map(|r| r.name.clone()).collect();
                names.iter().map(|n| self.mass(n)).collect()
            };
            let mole = self.mole().ok();
            let mut remove = None;
            egui::Grid::new("mix_rows").num_columns(5).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                ui.label(RichText::new("Component").strong());
                ui.label(RichText::new(basis_label(self.basis)).strong());
                ui.label(RichText::new("M, g/mol").strong());
                ui.label(RichText::new(if self.basis == Basis::Mass { "mol %" } else { "mass %" }).strong());
                ui.label("");
                ui.end_row();
                let other: Option<Vec<f64>> = mole.as_ref().map(|(_, x, m)| mixtures::from_mole(x, m, if self.basis == Basis::Mass { Basis::Mole } else { Basis::Mass }));
                for (i, r) in self.rows.iter_mut().enumerate() {
                    ui.add_sized([170.0, 20.0], egui::TextEdit::singleline(&mut r.name).hint_text("Methane, CO2, R32 …"));
                    ui.add(egui::DragValue::new(&mut r.value).range(0.0..=100.0).speed(0.05).max_decimals(6));
                    match &masses[i] {
                        Ok(m) => ui.label(format!("{m:.3}")),
                        Err(_) => ui.colored_label(ui.visuals().error_fg_color, "unknown"),
                    };
                    match &other {
                        Some(o) => ui.label(RichText::new(format!("{:.4}", 100.0 * o[i])).weak()),
                        None => ui.label(""),
                    };
                    if ui.small_button("×").on_hover_text("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                self.rows.remove(i);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("+ Add").clicked() {
                    self.rows.push(Row { name: String::new(), value: 0.0 });
                }
                ui.label(RichText::new("quick:").weak());
                for (l, n) in QUICK {
                    if ui.small_button(*l).on_hover_text(*n).clicked() {
                        if !self.rows.iter().any(|r| dfcore::props::coolprop_fluid(&r.name).eq_ignore_ascii_case(n)) {
                            self.rows.push(Row { name: n.to_string(), value: 0.0 });
                        }
                    }
                }
            });
            let total: f64 = self.rows.iter().map(|r| r.value).sum();
            ui.horizontal(|ui| {
                let txt = format!("Total {total:.4} %");
                if (total - 100.0).abs() > 1e-6 {
                    ui.label(RichText::new(txt).color(ui.visuals().warn_fg_color)).on_hover_text("The fractions are normalised when the mixture is written");
                } else {
                    ui.label(txt);
                }
                if ui.small_button("Normalise to 100 %").clicked() && total > 0.0 {
                    for r in &mut self.rows {
                        r.value *= 100.0 / total;
                    }
                }
            });
            ui.add_space(4.0);
            ui.checkbox(&mut self.ideal, "Ideal-gas mixture (::Ideal)").on_hover_text(
                "Gibbs–Dalton ideal-gas mixture of the components' ideal-gas properties, with thermochemical reference states \
                 (h = enthalpy of formation at 25 C), so that reacting mixtures balance. Accurate for gases at low pressure and high \
                 temperature (flue gases, combustion), needs no binary interaction parameters (SO2), and never condenses: check the dew point yourself. \
                 Without it: CoolProp's real mixture (GERG-type equations of state), with condensation, where CoolProp has the parameters.",
            );
            // ---- what the mixture is
            ui.separator();
            let string = mole.as_ref().map(|(n, x, _)| mixtures::coolprop_string(n, x));
            let fuel: Option<Vec<(String, f64)>> = mole.as_ref().map(|(n, x, _)| n.iter().cloned().zip(x.iter().copied()).collect());
            let mut rp: Option<RefProps> = None;
            match (&mole, &string, &fuel) {
                (Some((_, x, m)), Some(s), Some(f)) => {
                    if !self.ideal {
                        let chk = self.checks.entry(s.clone()).or_insert_with(|| dfcore::props::backend::check_fluid(s)).clone();
                        if let Err(e) = chk {
                            let e = if e.contains("binary pair") {
                                format!("CoolProp has no interaction parameters for a pair of these components ({e}). The properties below are those of the ideal-gas mixture.")
                            } else {
                                e
                            };
                            ui.colored_label(ui.visuals().warn_fg_color, e);
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Properties at").strong());
                        ui.add(egui::DragValue::new(&mut self.t_ref).range(-100.0..=1500.0).speed(0.5).suffix(" °C"));
                        ui.label("and");
                        ui.add(egui::DragValue::new(&mut self.p_ref).range(0.1..=1e5).speed(0.5).suffix(" kPa"));
                        if ui.small_button("ISO").on_hover_text("15 °C and 101.325 kPa: the reference conditions of ISO 13443 and ISO 6976").clicked() {
                            self.t_ref = 15.0;
                            self.p_ref = 101.325;
                        }
                        if ui.small_button("25 °C").on_hover_text("25 °C and 101.325 kPa: thermochemical tables").clicked() {
                            self.t_ref = 25.0;
                            self.p_ref = 101.325;
                        }
                    });
                    let mm: f64 = x.iter().zip(m).map(|(x, m)| x * m).sum();
                    let key = format!("{s}|{}|{}|{}", self.ideal, self.t_ref, self.p_ref);
                    let r = self.cache.entry(key).or_insert_with(|| ref_props(s, self.ideal, f, mm, self.t_ref + 273.15, self.p_ref * 1e3)).clone();
                    match r {
                        Ok(r) => {
                            egui::Grid::new("mix_props").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                                ui.label("Molar mass M");
                                ui.label(num(r.m));
                                ui.label("kg/kmol");
                                ui.end_row();
                                ui.label("Density ρ");
                                ui.label(num(r.rho));
                                ui.label("kg/m³");
                                ui.end_row();
                                ui.label("Specific heat cp");
                                ui.label(num(r.cp));
                                ui.label("kJ/kg-K");
                                ui.end_row();
                                if let Some((l, h)) = r.hv {
                                    for (name, v) in [("Lower heating value LHV", l), ("Higher heating value HHV", h)] {
                                        ui.label(name);
                                        ui.label(num(v / r.m));
                                        ui.label(RichText::new(format!("kJ/kg     {} kJ/kmol     {} MJ/Nm³", num(v), num(v / V_NORMAL / 1e3))).weak());
                                        ui.end_row();
                                    }
                                }
                            });
                            if r.hv.is_some() {
                                ui.label(
                                    RichText::new(
                                        "Heating values with reactants and products at the reference temperature (Kirchhoff's law, as ISO 6976); the pressure does not enter.",
                                    )
                                    .small()
                                    .weak(),
                                );
                            }
                            if r.fell_back {
                                ui.label(RichText::new("CoolProp's real mixture has no value here: ρ and cp are those of the ideal-gas mixture.").small().weak());
                            }
                            rp = Some(r);
                        }
                        Err(e) => {
                            ui.colored_label(ui.visuals().error_fg_color, e);
                        }
                    }
                }
                _ => {
                    if let Err(e) = self.mole() {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
            }
            // ---- flow
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Flow").strong());
                ui.add(egui::DragValue::new(&mut self.flow).range(0.0..=1e12).speed(1.0).max_decimals(6));
                egui::ComboBox::from_id_salt("mix_flow_unit").selected_text(self.flow_unit.label()).width(90.0).show_ui(ui, |ui| {
                    for u in FlowUnit::ALL {
                        ui.selectable_value(&mut self.flow_unit, u, u.label());
                    }
                });
                ui.label("of");
                ui.radio_value(&mut self.flow_is_flue, false, "this mixture");
                ui.radio_value(&mut self.flow_is_flue, true, "its flue gas").on_hover_text("The flow of the combustion products below; the fuel flow follows from it");
            });
            ui.label(RichText::new("1 Nm³ = 44.61503 mol: ideal gas at 0 °C and 101.325 kPa, whatever the gas (DIN 1343); 1 Sm³: the same at 15 °C (ISO 13443).").small().weak());
            let products = fuel.as_ref().map(|f| mixtures::products(f, self.lambda));
            // flue gas: molar mass of the products
            let m_of = |comps: &[(String, f64)], masses: &mut dyn FnMut(&str) -> f64| comps.iter().map(|(n, y)| y * masses(n)).sum::<f64>();
            let mut masses = std::mem::take(&mut self.masses);
            let mut mass_of = |n: &str| masses.entry(n.to_string()).or_insert_with(|| mixtures::molar_mass(n)).clone().unwrap_or(f64::NAN);
            let n_fuel: Option<f64> = match (&rp, self.flow_is_flue, &products) {
                (Some(r), false, _) => Some(self.flow_unit.to_kmol_s(self.flow, r.m)),
                (_, true, Some(Ok(p))) => {
                    let mp = m_of(&p.comps, &mut mass_of);
                    Some(self.flow_unit.to_kmol_s(self.flow, mp) / p.products_per_fuel_mol)
                }
                _ => None,
            };
            if let (Some(n), Some(r)) = (n_fuel, &rp) {
                let mut t = format!(
                    "Fuel: {} kg/s   {} kg/h   {} kmol/h   {} Nm³/h   {} Nm³/day   {} Sm³/h",
                    num(n * r.m),
                    num(n * r.m * 3600.0),
                    num(n * 3600.0),
                    num(FlowUnit::Nm3H.from_kmol_s(n, r.m)),
                    num(FlowUnit::Nm3D.from_kmol_s(n, r.m)),
                    num(FlowUnit::Sm3H.from_kmol_s(n, r.m))
                );
                if let Some((l, _)) = r.hv {
                    t.push_str(&format!("   thermal input (LHV) {} MW", num(n * l / 1e3)));
                }
                ui.label(t);
            }
            // ---- combustion products
            egui::CollapsingHeader::new("Combustion products of this fuel").id_salt("mix_comb").default_open(true).show(ui, |ui| {
                ui.label(
                    RichText::new("Complete combustion with dry air: C to CO2, H to H2O, S to SO2, N to N2. λ = air / stoichiometric air (1 = stoichiometric).").small().weak(),
                );
                ui.horizontal(|ui| {
                    ui.label("Excess-air ratio λ");
                    ui.add(egui::DragValue::new(&mut self.lambda).range(1.0..=10.0).speed(0.01).max_decimals(3));
                    ui.separator();
                    ui.label("Flows per");
                    for (l, v) in [("s", 1.0), ("h", 3600.0), ("day", 86400.0)] {
                        ui.radio_value(&mut self.per, v, l);
                    }
                });
                match &products {
                    Some(Ok(p)) => {
                        let o2 = p.comps.iter().find(|c| c.0 == "Oxygen").map_or(0.0, |c| c.1);
                        let w = p.comps.iter().find(|c| c.0 == "Water").map_or(0.0, |c| c.1);
                        ui.label(format!(
                            "Air/fuel {} kg/kg (stoichiometric {}),  {} kmol air and {} kmol products per kmol fuel,  O2 {} % wet, {} % dry",
                            num(p.air_per_fuel_mass),
                            num(p.afr_stoich),
                            num(p.air_per_fuel_mol),
                            num(p.products_per_fuel_mol),
                            num(100.0 * o2),
                            num(100.0 * o2 / (1.0 - w))
                        ));
                        let mp = m_of(&p.comps, &mut mass_of);
                        let air: Vec<(String, f64)> = mixtures::dry_air().iter().map(|(n, x)| (n.to_string(), *x)).collect();
                        let ma = m_of(&air, &mut mass_of);
                        let tb = match self.per as i64 {
                            1 => "s",
                            3600 => "h",
                            _ => "day",
                        };
                        let k = self.per;
                        egui::Grid::new("mix_products").num_columns(7).spacing([14.0, 3.0]).striped(true).show(ui, |ui| {
                            for h in ["", "mol %", "mol % dry", "mass %", &format!("kmol/{tb}"), &format!("kg/{tb}"), &format!("Nm³/{tb}")] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            let flows = |ui: &mut egui::Ui, n: Option<f64>, m: f64| match n {
                                Some(n) => {
                                    ui.label(num(n * k));
                                    ui.label(num(n * m * k));
                                    ui.label(num(n * V_NORMAL * k));
                                }
                                None => {
                                    ui.label("");
                                    ui.label("");
                                    ui.label("");
                                }
                            };
                            if let Some(r) = &rp {
                                ui.label(RichText::new("Fuel").strong());
                                ui.label("");
                                ui.label("");
                                ui.label("");
                                flows(ui, n_fuel, r.m);
                                ui.end_row();
                            }
                            ui.label(RichText::new("Air").strong());
                            ui.label("");
                            ui.label("");
                            ui.label("");
                            flows(ui, n_fuel.map(|n| n * p.air_per_fuel_mol), ma);
                            ui.end_row();
                            for (name, y) in &p.comps {
                                let mi = mass_of(name);
                                ui.label(name);
                                ui.label(num(100.0 * y));
                                ui.label(if name == "Water" { "—".to_string() } else { num(100.0 * y / (1.0 - w)) });
                                ui.label(num(100.0 * y * mi / mp));
                                flows(ui, n_fuel.map(|n| n * p.products_per_fuel_mol * y), mi);
                                ui.end_row();
                            }
                            ui.label(RichText::new("Flue gas").strong());
                            ui.label("100");
                            ui.label("");
                            ui.label("100");
                            flows(ui, n_fuel.map(|n| n * p.products_per_fuel_mol), mp);
                            ui.end_row();
                        });
                        let pw = w * 101_325.0;
                        let dew = if pw > 0.0 {
                            dfcore::props::backend::props("Water", dfcore::props::Key::T, &[dfcore::props::Key::P, dfcore::props::Key::Q], &[pw, 0.0], None).ok()
                        } else {
                            None
                        };
                        let mut t = format!("Flue gas: M = {} kg/kmol", num(mp));
                        if let Some(d) = dew {
                            t.push_str(&format!(", water dew point {} °C at 101.325 kPa (H2O partial pressure {} kPa)", num(d - 273.15), num(pw / 1e3)));
                        }
                        ui.label(RichText::new(t).small().weak());
                        if ui.button("Replace with the combustion products").on_hover_text("The products become the mixture, as an ideal-gas mixture").clicked() {
                            let (n, x): (Vec<String>, Vec<f64>) = p.comps.iter().cloned().unzip();
                            self.set_mole(n, x);
                            self.ideal = true;
                            self.flow_is_flue = false;
                            self.note = Some(format!("combustion products at λ = {}", self.lambda));
                        }
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).weak());
                    }
                    None => {}
                }
            });
            for (k, v) in masses {
                self.masses.entry(k).or_insert(v);
            }
            if let Some(n) = &self.note {
                ui.label(RichText::new(n).small().weak());
            }
            ui.separator();
            if let Some(s) = &string {
                let shown = if self.ideal { format!("'{s}::Ideal'") } else { format!("'{s}'") };
                ui.label(RichText::new(shown).monospace());
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(string.is_some(), egui::Button::new("Apply")).clicked() {
                    out = string.clone().map(|s| (s, self.ideal));
                    self.open = false;
                }
                if ui.button("Cancel").clicked() {
                    self.open = false;
                }
            });
        });
        self.open = self.open && open;
        out
    }
}
