//! Mixture editor of the property-call builder: components with their
//! fractions on a mole, mass or volume basis, preset compositions (natural
//! gases, air, refrigerant blends), the products of combustion of a fuel
//! with excess air, and the heating values of the mixture. It writes the
//! CoolProp mixture string with mole fractions (`Methane[0.9]&Ethane[0.1]`),
//! with `::Ideal` for the ideal-gas mixture.

use dfcore::combustion;
use dfcore::mixtures::{self, Basis, PRESETS};
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
}

impl Default for MixtureEditor {
    fn default() -> Self {
        MixtureEditor { open: false, basis: Basis::Mole, rows: Vec::new(), ideal: false, lambda: 1.0, note: None, masses: HashMap::new(), checks: HashMap::new() }
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
        egui::Window::new("Mixture").open(&mut open).default_width(520.0).resizable(true).show(ctx, |ui| {
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
            match (&mole, &string) {
                (Some((_, x, m)), Some(s)) => {
                    let mm: f64 = x.iter().zip(m).map(|(x, m)| x * m).sum();
                    let mut info = format!("M = {mm:.4} g/mol");
                    if let Ok(c) = combustion::molar(s) {
                        if c.lhv > 0.0 {
                            info.push_str(&format!("    LHV = {:.3} MJ/kg ({:.1} kJ/mol)    HHV = {:.3} MJ/kg", c.lhv / c.m, c.lhv, c.hhv / c.m));
                        }
                    }
                    ui.label(info);
                    if !self.ideal {
                        let chk = self.checks.entry(s.clone()).or_insert_with(|| dfcore::props::backend::check_fluid(s)).clone();
                        if let Err(e) = chk {
                            let e = if e.contains("binary pair") {
                                format!("CoolProp has no interaction parameters for a pair of these components ({e}). Use the ideal-gas mixture.")
                            } else {
                                e
                            };
                            ui.colored_label(ui.visuals().warn_fg_color, e);
                        }
                    }
                }
                _ => {
                    if let Err(e) = self.mole() {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
            }
            // ---- combustion products
            egui::CollapsingHeader::new("Combustion products of this fuel").id_salt("mix_comb").show(ui, |ui| {
                ui.label(
                    RichText::new("Complete combustion with dry air: C to CO2, H to H2O, S to SO2, N to N2. λ = air / stoichiometric air (1 = stoichiometric).").small().weak(),
                );
                ui.horizontal(|ui| {
                    ui.label("Excess-air ratio λ");
                    ui.add(egui::DragValue::new(&mut self.lambda).range(1.0..=10.0).speed(0.01).max_decimals(3));
                });
                let fuel: Option<Vec<(String, f64)>> = mole.as_ref().map(|(n, x, _)| n.iter().cloned().zip(x.iter().copied()).collect());
                match fuel.as_ref().map(|f| mixtures::products(f, self.lambda)) {
                    Some(Ok(p)) => {
                        let o2 = p.comps.iter().find(|c| c.0 == "Oxygen").map_or(0.0, |c| c.1);
                        ui.label(format!(
                            "Air/fuel {:.3} kg/kg (stoichiometric {:.3}),  {:.3} kmol air and {:.3} kmol products per kmol fuel,  O2 in the products {:.2} % (wet)",
                            p.air_per_fuel_mass,
                            p.afr_stoich,
                            p.air_per_fuel_mol,
                            p.products_per_fuel_mol,
                            100.0 * o2
                        ));
                        if ui.button("Replace with the combustion products").on_hover_text("The products become the mixture, as an ideal-gas mixture").clicked() {
                            let (n, x): (Vec<String>, Vec<f64>) = p.comps.iter().cloned().unzip();
                            self.set_mole(n, x);
                            self.ideal = true;
                            self.note = Some(format!("combustion products at λ = {}", self.lambda));
                        }
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).weak());
                    }
                    None => {}
                }
            });
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
