//! Catalogue of selectable display units per physical dimension.

use dfcore::units::{parse_unit, Dim, Unit, UnitSystem};

pub fn choices(dim: Dim) -> Vec<&'static str> {
    let v: &[&str] = match dim {
        Dim::TEMP => &["C", "K", "F", "R"],
        Dim::PRESSURE => &["bar", "kPa", "MPa", "Pa", "atm", "psia", "mbar"],
        Dim::SPEC_ENERGY => &["kJ/kg", "J/kg", "MJ/kg", "Btu/lbm", "kWh/kg"],
        Dim::SPEC_ENTROPY => &["kJ/kg-K", "J/kg-K", "Btu/lbm-R"],
        Dim::ENERGY => &["kJ", "J", "MJ", "GJ", "kWh", "MWh", "Btu"],
        Dim::POWER => &["kW", "W", "MW", "GW", "hp", "Btu/h"],
        Dim::MASS_FLOW => &["kg/s", "kg/h", "t/h", "lbm/s", "lbm/h"],
        Dim::DENSITY => &["kg/m^3", "g/cm^3", "lbm/ft^3"],
        Dim::SPEC_VOLUME => &["m^3/kg", "L/kg", "ft^3/lbm"],
        Dim::VELOCITY => &["m/s", "km/h", "ft/s"],
        Dim::VISCOSITY => &["Pa-s", "lbm/ft-s"],
        Dim::CONDUCTIVITY => &["W/m-K", "kW/m-K", "Btu/hr-ft-R"],
        Dim::MOLAR_MASS => &["kg/kmol", "g/mol", "kg/mol"],
        Dim::MASS => &["kg", "g", "t", "lbm"],
        Dim::LENGTH => &["m", "mm", "cm", "km", "in", "ft"],
        Dim::TIME => &["s", "min", "h", "day", "yr"],
        Dim::NONE => &["-", "%"],
        _ => &[],
    };
    v.iter().copied().filter(|u| parse_unit(u).map(|p| p.dim == dim).unwrap_or(false)).collect()
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
