//! Catalogue of selectable display units per physical dimension, display
//! presets, and per-quantity unit choices.
//!
//! A value is shown in the first unit found in this order:
//!
//! 1. the variable's own display unit (Variables panel, Initial values);
//! 2. the unit chosen for its quantity (*Units > Units per quantity*);
//! 3. the unit of the last display preset applied (*Units > Display units preset*);
//! 4. the unit from the script (`$Units`, `{unit}` comment, `[unit]` after
//!    the equation) or, without one, the model unit system.

use dfcore::units::{parse_unit, Dim, Unit, UnitSystem};
use dfcore::Model;
use std::collections::{BTreeMap, HashMap};

pub fn choices(dim: Dim) -> Vec<&'static str> {
    let v: &[&str] = match dim {
        Dim::TEMP => &["C", "K", "F", "R"],
        Dim::PRESSURE => &["bar", "kPa", "MPa", "Pa", "atm", "psia", "mbar"],
        Dim::SPEC_ENERGY => &["kJ/kg", "J/kg", "MJ/kg", "Btu/lbm", "kWh/kg"],
        Dim::SPEC_ENTROPY => &["kJ/kg-K", "J/kg-K", "Btu/lbm-R"],
        Dim::MOLAR_ENERGY => &["kJ/kmol", "J/mol", "kJ/mol", "MJ/kmol", "Btu/lbmol"],
        Dim::MOLAR_ENTROPY => &["kJ/kmol-K", "J/mol-K", "Btu/lbmol-R"],
        Dim::ENERGY => &["kJ", "J", "MJ", "GJ", "kWh", "MWh", "Btu"],
        Dim::POWER => &["kW", "W", "MW", "GW", "hp", "Btu/h"],
        Dim::MASS_FLOW => &["kg/s", "kg/h", "t/h", "kg/min", "lbm/s", "lbm/h"],
        Dim::MOLAR_FLOW => &["kmol/s", "mol/s", "kmol/h", "lbmol/s", "lbmol/h"],
        Dim::DENSITY => &["kg/m^3", "g/cm^3", "lbm/ft^3"],
        Dim::SPEC_VOLUME => &["m^3/kg", "L/kg", "ft^3/lbm"],
        Dim::VELOCITY => &["m/s", "km/h", "ft/s"],
        Dim::VISCOSITY => &["Pa-s", "lbm/ft-s"],
        Dim::CONDUCTIVITY => &["W/m-K", "kW/m-K", "Btu/hr-ft-R"],
        Dim::MOLAR_MASS => &["kg/kmol", "g/mol", "kg/mol", "lbm/lbmol"],
        Dim::AMOUNT => &["kmol", "mol", "lbmol"],
        Dim::MASS => &["kg", "g", "t", "lbm"],
        Dim::LENGTH => &["m", "mm", "cm", "km", "in", "ft"],
        Dim::TIME => &["s", "min", "h", "day", "yr"],
        Dim::NONE => &["-", "%"],
        _ => &[],
    };
    v.iter().copied().filter(|u| parse_unit(u).map(|p| p.dim == dim).unwrap_or(false)).collect()
}

/// Display units offered for a variable of dimension `dim` in `model`: the
/// catalogue for physical quantities; for custom units (`EUR/yr`) every
/// compatible unit written in the script, the model unit, and their k, M and
/// G multiples (`kEUR/yr`, `MEUR/yr`).
pub fn choices_for(model: &Model, dim: Dim) -> Vec<String> {
    if !dim.has_custom() {
        return choices(dim).into_iter().map(str::to_string).collect();
    }
    model.with_units(|| {
        let mut seen: Vec<String> = Vec::new();
        let mut written: Vec<&str> = model.vars.iter().filter_map(|v| v.unit_hint.as_deref()).collect();
        written.extend(model.display_units.values().map(String::as_str));
        let mu = model.unit_system.unit_for(dim).symbol;
        let mut base = vec![mu];
        base.extend(written.into_iter().map(str::to_string));
        let fits = |u: &str| parse_unit(u).is_ok_and(|p| p.dim == dim);
        // a custom unit alone (no SI part): its root name and factor
        let custom_root = |t: &str| {
            parse_unit(t).ok().filter(|p| p.dim.si_part().is_none()).and_then(|p| {
                let mut it = p.dim.1.iter();
                match (it.next(), it.next()) {
                    (Some((id, 1)), None) => Some((dfcore::units::custom_name(id), p.factor)),
                    _ => None,
                }
            })
        };
        for u in base {
            let u = u.trim().to_string();
            if !fits(&u) {
                continue;
            }
            // `kEUR/yr` → `EUR/yr`, then its multiples
            let first = u.split(['/', '-', '*', ' ', '^']).next().unwrap_or("").to_string();
            let root = custom_root(&first).map(|(r, _)| format!("{r}{}", &u[first.len()..]));
            for v in [Some(u.clone()), root.clone()].into_iter().flatten() {
                if fits(&v) && !seen.contains(&v) {
                    seen.push(v);
                }
            }
            if let Some(r) = root {
                for p in ["k", "M", "G"] {
                    let v = format!("{p}{r}");
                    if fits(&v) && !seen.contains(&v) {
                        seen.push(v);
                    }
                }
            }
        }
        seen
    })
}

/// Declare custom units in a script: append them to its `$CustomUnits` line,
/// or add one after the leading directives (`$UnitSystem`, ...).
pub fn declare_custom_units(text: &str, names: &[String]) -> String {
    if names.is_empty() {
        return text.to_string();
    }
    let add = names.join(" ");
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if let Some(l) = lines.iter_mut().find(|l| l.trim_start().to_lowercase().starts_with("$customunits")) {
        let keep = l.trim_end_matches('\r').trim_end().to_string();
        *l = format!("{keep} {add}");
        return lines.join("\n");
    }
    let at = lines.iter().take_while(|l| l.trim_start().starts_with('$')).count();
    lines.insert(at, format!("$CustomUnits {add}"));
    lines.join("\n")
}

/// A physical quantity whose display unit can be chosen for all the
/// variables of that dimension at once.
pub struct Quantity {
    /// Stable key, stored in project files.
    pub key: &'static str,
    pub label: &'static str,
    pub dim: Dim,
    /// Shown on hover in the menu.
    pub note: &'static str,
}

const MASS_BASIS: &str = "Mass-specific (per kg). A per-mole value needs the molar mass of the fluid, which a variable does not carry, so it is a different quantity: define it in the script, e.g. h_m = h*molarmass(Water), and choose its unit under the molar quantity.";

pub const QUANTITIES: &[Quantity] = &[
    Quantity { key: "temperature", label: "Temperature", dim: Dim::TEMP, note: "" },
    Quantity { key: "pressure", label: "Pressure", dim: Dim::PRESSURE, note: "" },
    Quantity { key: "specific_energy", label: "Specific enthalpy / energy", dim: Dim::SPEC_ENERGY, note: MASS_BASIS },
    Quantity { key: "specific_entropy", label: "Specific entropy / heat capacity", dim: Dim::SPEC_ENTROPY, note: MASS_BASIS },
    Quantity {
        key: "molar_energy",
        label: "Molar enthalpy / energy",
        dim: Dim::MOLAR_ENERGY,
        note: "Variables whose dimension is energy per amount of substance, e.g. h_m = h*molarmass(Water) or a literal in [kJ/kmol].",
    },
    Quantity {
        key: "molar_entropy",
        label: "Molar entropy / heat capacity",
        dim: Dim::MOLAR_ENTROPY,
        note: "Variables whose dimension is entropy per amount of substance, e.g. R# or a literal in [kJ/kmol-K].",
    },
    Quantity { key: "energy", label: "Energy", dim: Dim::ENERGY, note: "" },
    Quantity { key: "power", label: "Power / heat rate", dim: Dim::POWER, note: "" },
    Quantity { key: "mass_flow", label: "Mass flow rate", dim: Dim::MASS_FLOW, note: "" },
    Quantity { key: "molar_flow", label: "Molar flow rate", dim: Dim::MOLAR_FLOW, note: "" },
    Quantity { key: "density", label: "Density", dim: Dim::DENSITY, note: "" },
    Quantity { key: "specific_volume", label: "Specific volume", dim: Dim::SPEC_VOLUME, note: "" },
    Quantity { key: "velocity", label: "Velocity", dim: Dim::VELOCITY, note: "" },
    Quantity { key: "viscosity", label: "Dynamic viscosity", dim: Dim::VISCOSITY, note: "" },
    Quantity { key: "conductivity", label: "Thermal conductivity", dim: Dim::CONDUCTIVITY, note: "" },
    Quantity { key: "molar_mass", label: "Molar mass", dim: Dim::MOLAR_MASS, note: "" },
    Quantity { key: "amount", label: "Amount of substance", dim: Dim::AMOUNT, note: "" },
    Quantity { key: "mass", label: "Mass", dim: Dim::MASS, note: "" },
    Quantity { key: "length", label: "Length", dim: Dim::LENGTH, note: "" },
    Quantity { key: "time", label: "Time", dim: Dim::TIME, note: "" },
];

/// Key in [`UnitPrefs::quantities`] of a quantity with custom units, e.g.
/// `custom:EUR/s` (the dimension in model base units).
pub fn custom_key(dim: Dim) -> String {
    format!("custom:{dim}")
}

/// The catalogue entry of a dimension.
pub fn quantity_of(dim: Dim) -> Option<&'static Quantity> {
    QUANTITIES.iter().find(|q| q.dim == dim)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum DisplayPreset {
    ModelUnits,
    SI,
    Engineering,
    Imperial,
}

impl DisplayPreset {
    pub const ALL: [DisplayPreset; 4] = [DisplayPreset::ModelUnits, DisplayPreset::SI, DisplayPreset::Engineering, DisplayPreset::Imperial];
    pub fn label(self) -> &'static str {
        match self {
            DisplayPreset::ModelUnits => "Model unit system",
            DisplayPreset::SI => "SI (K, Pa, J)",
            DisplayPreset::Engineering => "Engineering (C, bar, kJ)",
            DisplayPreset::Imperial => "Imperial (F, psia, Btu)",
        }
    }
    pub fn unit_for(self, dim: Dim, model: &UnitSystem) -> Unit {
        match self {
            DisplayPreset::ModelUnits => model.unit_for(dim),
            DisplayPreset::SI => Unit::si(dim),
            DisplayPreset::Engineering => {
                let mut u = UnitSystem::engineering().unit_for(dim);
                if dim == Dim::POWER {
                    u = parse_unit("kW").unwrap();
                }
                u
            }
            DisplayPreset::Imperial => UnitSystem::imperial().unit_for(dim),
        }
    }
}

/// Display units chosen for whole quantities: the last preset applied and
/// the units picked per quantity on top of it.
#[derive(Clone, Default, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UnitPrefs {
    /// Last display preset applied; `None`: units from the script.
    pub preset: Option<DisplayPreset>,
    /// Unit symbol per quantity, by [`Quantity::key`].
    pub quantities: BTreeMap<String, String>,
}

impl UnitPrefs {
    pub fn is_empty(&self) -> bool {
        self.preset.is_none() && self.quantities.is_empty()
    }

    /// Apply a preset: every quantity takes the preset's unit.
    pub fn set_preset(&mut self, p: DisplayPreset) {
        self.preset = Some(p);
        self.quantities.clear();
    }

    /// The unit picked for a quantity (not the preset's).
    pub fn chosen(&self, q: &Quantity) -> Option<&str> {
        self.quantities.get(q.key).map(String::as_str).filter(|u| parse_unit(u).is_ok_and(|p| p.dim == q.dim))
    }

    /// Display unit for every variable of dimension `dim`, if the quantity
    /// choices or the preset set one (dimensionless values are left alone).
    pub fn unit_for(&self, dim: Dim, model: &UnitSystem) -> Option<String> {
        if dim.is_none() {
            return None;
        }
        if dim.has_custom() {
            // presets do not know custom units: only a choice for the quantity applies
            return self.quantities.get(&custom_key(dim)).filter(|u| parse_unit(u).is_ok_and(|p| p.dim == dim)).cloned();
        }
        if let Some(u) = quantity_of(dim).and_then(|q| self.chosen(q)) {
            return Some(u.to_string());
        }
        let u = self.preset?.unit_for(dim, model);
        // compound units without a readable symbol keep the script's unit
        parse_unit(&u.symbol).ok().filter(|p| p.dim == dim).map(|_| u.symbol)
    }
}

/// Display unit of every variable: the per-variable choices over the
/// per-quantity ones (see the module documentation for the order).
pub fn effective_units(model: &Model, per_var: &HashMap<String, String>, prefs: &UnitPrefs) -> HashMap<String, String> {
    model.with_units(|| effective_units_in(model, per_var, prefs))
}

fn effective_units_in(model: &Model, per_var: &HashMap<String, String>, prefs: &UnitPrefs) -> HashMap<String, String> {
    if prefs.is_empty() {
        return per_var.clone();
    }
    let mut out = HashMap::with_capacity(model.vars.len());
    for (i, v) in model.vars.iter().enumerate() {
        if let Some(u) = model.var_dim(i as u32).and_then(|d| prefs.unit_for(d, &model.unit_system)) {
            out.insert(v.name.clone(), u);
        }
    }
    for (k, u) in per_var {
        out.insert(k.clone(), u.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};
    use crate::app::display_in;

    const SCRIPT: &str = "m_dot = 10 [kg/s]\nh = 2500 [kJ/kg]\nQ = m_dot*h\nM = molarmass(Water)\nh_m = h*M\nT = 25 [C]\nP = 2 [bar]\n$Units P = kPa\n";

    fn analyse(s: &str) -> crate::analysis::Analysis {
        run(Request { gen: 1, text: s.into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None, init: Default::default() })
    }

    fn shown(a: &crate::analysis::Analysis, units: &HashMap<String, String>, name: &str) -> (f64, String) {
        let i = a.model.var(name).unwrap() as usize;
        display_in(units, a, i, a.solution.as_ref().unwrap().x[i])
    }

    #[test]
    fn declare_line() {
        let n = vec!["EUR".to_string(), "pax".to_string()];
        assert_eq!(declare_custom_units("$UnitSystem SI\nx = 1 [EUR]", &n), "$UnitSystem SI\n$CustomUnits EUR pax\nx = 1 [EUR]");
        assert_eq!(declare_custom_units("x = 1\n$CustomUnits USD\n", &n), "x = 1\n$CustomUnits USD EUR pax\n");
    }

    /// Custom units: the dropdown offers the script's spellings and their
    /// multiples; a choice for the quantity applies to all its variables.
    #[test]
    fn custom_quantities() {
        let a = analyse("C = 12 [MEUR]\nc_om = 300 [kEUR/yr]\nOM = c_om*20 [yr]\nTotal = C + OM {EUR}\nn = 4 [pax]\n");
        assert!(a.solution.as_ref().unwrap().converged);
        let m = &a.model;
        let d = m.var_dim(m.var("Total").unwrap()).unwrap();
        let c = choices_for(m, d);
        for u in ["EUR", "MEUR", "kEUR", "GEUR"] {
            assert!(c.iter().any(|x| x == u), "{u} not in {c:?}");
        }
        let d2 = m.var_dim(m.var("c_om").unwrap()).unwrap();
        let c2 = choices_for(m, d2);
        assert!(c2.iter().any(|x| x == "kEUR/yr") && c2.iter().any(|x| x == "MEUR/yr"), "{c2:?}");
        // presets leave custom quantities alone, a quantity choice applies
        let mut prefs = UnitPrefs::default();
        prefs.set_preset(DisplayPreset::SI);
        assert_eq!(prefs.unit_for(d, &m.unit_system), None);
        prefs.quantities.insert(custom_key(d), "MEUR".into());
        let du = effective_units(m, &HashMap::new(), &prefs);
        let (x, u) = shown(&a, &du, "Total");
        assert_eq!(u, "MEUR");
        assert!((x - 18.0).abs() < 1e-9, "{x}");
        assert_eq!(shown(&a, &du, "n").1, "pax");
    }

    #[test]
    fn catalogue_units_parse_to_their_quantity() {
        for q in QUANTITIES {
            let c = choices(q.dim);
            assert!(c.len() >= 2, "{}: {c:?}", q.key);
            for p in DisplayPreset::ALL {
                let u = UnitPrefs { preset: Some(p), quantities: Default::default() }.unit_for(q.dim, &UnitSystem::engineering());
                assert!(u.is_some(), "{} has no unit in preset {p:?}", q.key);
            }
        }
    }

    #[test]
    fn preset_then_quantity_override() {
        let a = analyse(SCRIPT);
        assert!(a.solution.as_ref().unwrap().converged);
        let mut prefs = UnitPrefs::default();
        prefs.set_preset(DisplayPreset::Engineering);
        prefs.quantities.insert("mass_flow".into(), "t/h".into());
        let du = effective_units(&a.model, &HashMap::new(), &prefs);
        let (x, u) = shown(&a, &du, "m_dot");
        assert_eq!(u, "t/h");
        assert!((x - 36.0).abs() < 1e-9, "{x}");
        // the preset still applies to the other quantities, also over the script's {kPa}
        assert_eq!(shown(&a, &du, "P").1, "bar");
        assert_eq!(shown(&a, &du, "h").1, "kJ/kg");
        // a per-variable choice wins over the quantity
        let per_var = HashMap::from([("m_dot".to_string(), "kg/h".to_string())]);
        let du = effective_units(&a.model, &per_var, &prefs);
        assert_eq!(shown(&a, &du, "m_dot").1, "kg/h");
        // a new preset resets the quantity choices
        prefs.set_preset(DisplayPreset::SI);
        let du = effective_units(&a.model, &HashMap::new(), &prefs);
        assert_eq!(shown(&a, &du, "m_dot").1, "kg/s");
        assert_eq!(shown(&a, &du, "T").1, "K");
    }

    #[test]
    fn quantity_without_preset_keeps_script_units_elsewhere() {
        let a = analyse(SCRIPT);
        let prefs = UnitPrefs { preset: None, quantities: BTreeMap::from([("specific_energy".to_string(), "J/kg".to_string())]) };
        let du = effective_units(&a.model, &HashMap::new(), &prefs);
        let (x, u) = shown(&a, &du, "h");
        assert_eq!(u, "J/kg");
        assert!((x - 2.5e6).abs() < 1e-6);
        assert_eq!(shown(&a, &du, "P").1, "kPa");
        // an unparsable or mismatched stored unit is ignored
        let bad = UnitPrefs { preset: None, quantities: BTreeMap::from([("specific_energy".to_string(), "bar".to_string())]) };
        assert_eq!(shown(&a, &effective_units(&a.model, &HashMap::new(), &bad), "h").1, "kJ/kg");
    }

    /// kJ/kmol applies to molar variables, with the molar mass in kg/kmol.
    #[test]
    fn molar_quantities() {
        let a = analyse(SCRIPT);
        let sol = a.solution.as_ref().unwrap();
        assert!(sol.converged);
        let i = a.model.var("h_m").unwrap();
        assert_eq!(a.model.var_dim(i), Some(Dim::MOLAR_ENERGY));
        let m = sol.x[a.model.var("M").unwrap() as usize];
        assert!((m - 18.015268).abs() < 1e-3, "{m}");
        // in the model (Engineering) h_m is in kJ/kmol
        let (x, u) = shown(&a, &HashMap::new(), "h_m");
        assert_eq!(u, "kJ/kmol");
        assert!((x - 2500.0 * m).abs() < 1e-6, "{x}");
        let prefs = UnitPrefs { preset: Some(DisplayPreset::Engineering), quantities: BTreeMap::from([("molar_energy".to_string(), "J/mol".to_string())]) };
        let (x, u) = shown(&a, &effective_units(&a.model, &HashMap::new(), &prefs), "h_m");
        assert_eq!(u, "J/mol");
        assert!((x - 2500.0 * m).abs() < 1e-6, "{x}");
        // R# = 8.314 kJ/kmol-K, so R#/M is the specific gas constant in kJ/kg-K
        let b = analyse("R = R#/molarmass(Nitrogen)");
        let r = b.solution.as_ref().unwrap().x[b.model.var("R").unwrap() as usize];
        assert!((r - 0.29680).abs() < 1e-4, "{r}");
        assert_eq!(shown(&b, &HashMap::new(), "R").1, "kJ/kg-K");
    }
}
