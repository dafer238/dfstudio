//! Thermophysical properties through CoolProp (Bell, Wronski, Quoilin &
//! Lemort, *Ind. Eng. Chem. Res.* 53(6):2498-2508, 2014,
//! doi:10.1021/ie4033999).
//!
//! The low-level `AbstractState` interface is used instead of `PropsSI`:
//! one state object per (thread, fluid) is created once and reused, which
//! avoids parsing strings and re-building the equation of state on every
//! call (see the CoolProp documentation, "Low Level Interface"). Gradients
//! are taken from CoolProp's analytic partial derivatives of the Helmholtz
//! equation of state where they exist and from finite differences otherwise.

use crate::funcs::{fd_grad, ExtFn};
use crate::units::{Dim, Unit, UnitSystem};

/// Thermodynamic property keys understood by DeltaF Studio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    T,
    P,
    H,
    S,
    U,
    D,
    V,
    Q,
    Cp,
    Cv,
    Visc,
    Cond,
    Sound,
    Prandtl,
    SurfTension,
    MolarMass,
    Tcrit,
    Pcrit,
    Z,
    Phase,
    /// Kinematic viscosity mu/rho
    KinVisc,
    /// Thermal diffusivity k/(rho cp)
    Diffusivity,
    /// Isentropic exponent -(v/P)(dP/dv)_s (cp/cv for an ideal gas)
    IsenExp,
    /// Isothermal compressibility -(1/v)(dv/dP)_T
    IsoComp,
    /// Volumetric expansion coefficient (1/v)(dv/dT)_P
    ExpCoef,
    Ttriple,
    Ptriple,
    Acentric,
    /// Critical specific volume
    Vcrit,
}

impl Key {
    /// A constant of the fluid (no state inputs): `t_crit(Fluid)`, ...
    pub fn is_fluid_constant(self) -> bool {
        matches!(self, Key::MolarMass | Key::Tcrit | Key::Pcrit | Key::Ttriple | Key::Ptriple | Key::Acentric | Key::Vcrit)
    }
    pub fn dim(self) -> Dim {
        match self {
            Key::T | Key::Tcrit | Key::Ttriple => Dim::TEMP,
            Key::P | Key::Pcrit | Key::Ptriple => Dim::PRESSURE,
            Key::H | Key::U => Dim::SPEC_ENERGY,
            Key::S | Key::Cp | Key::Cv => Dim::SPEC_ENTROPY,
            Key::D => Dim::DENSITY,
            Key::V | Key::Vcrit => Dim::SPEC_VOLUME,
            Key::KinVisc | Key::Diffusivity => Dim::DIFFUSIVITY,
            Key::IsoComp => Dim::INV_PRESSURE,
            Key::ExpCoef => Dim::INV_TEMP,
            Key::IsenExp | Key::Acentric => Dim::NONE,
            Key::Visc => Dim::VISCOSITY,
            Key::Cond => Dim::CONDUCTIVITY,
            Key::Sound => Dim::VELOCITY,
            Key::SurfTension => Dim::SURFACE_TENSION,
            Key::MolarMass => Dim::MOLAR_MASS,
            Key::Q | Key::Prandtl | Key::Z | Key::Phase => Dim::NONE,
        }
    }
    /// Dimension per amount of substance (EES molar unit system): h, u, s,
    /// cp, cv, v and density per mol; the other properties as `dim`.
    pub fn molar_dim(self) -> Dim {
        match self {
            Key::H | Key::U => Dim::MOLAR_ENERGY,
            Key::S | Key::Cp | Key::Cv => Dim::MOLAR_ENTROPY,
            Key::D => Dim([0, -3, 0, 0, 1, 0, 0]),
            Key::V | Key::Vcrit => Dim([0, 3, 0, 0, -1, 0, 0]),
            k => k.dim(),
        }
    }
    /// CoolProp parameter name.
    pub fn coolprop(self) -> &'static str {
        match self {
            Key::T => "T",
            Key::P => "P",
            Key::H => "Hmass",
            Key::S => "Smass",
            Key::U => "Umass",
            Key::D | Key::V => "Dmass",
            Key::Q => "Q",
            Key::Cp => "Cpmass",
            Key::Cv => "Cvmass",
            Key::Visc => "viscosity",
            Key::Cond => "conductivity",
            Key::Sound => "speed_of_sound",
            Key::Prandtl => "Prandtl",
            Key::SurfTension => "surface_tension",
            Key::MolarMass => "molar_mass",
            Key::Tcrit => "T_critical",
            Key::Pcrit => "p_critical",
            Key::Z => "Z",
            Key::Phase => "Phase",
            // derived in `backend::props` from these
            Key::KinVisc => "viscosity",
            Key::Diffusivity => "conductivity",
            Key::IsenExp => "isentropic_expansion_coefficient",
            Key::IsoComp => "isothermal_compressibility",
            Key::ExpCoef => "isobaric_expansion_coefficient",
            Key::Ttriple => "T_triple",
            Key::Ptriple => "p_triple",
            Key::Acentric => "acentric",
            Key::Vcrit => "rhomass_critical",
        }
    }
    /// Key for a named input argument (`P=`, `T=`, `h=` ...).
    pub fn from_input(name: &str) -> Option<Key> {
        Some(match name.to_lowercase().as_str() {
            "t" => Key::T,
            "p" => Key::P,
            "h" => Key::H,
            "s" => Key::S,
            "u" => Key::U,
            "d" | "rho" => Key::D,
            "v" => Key::V,
            "x" | "q" => Key::Q,
            _ => return None,
        })
    }
    /// Key for a CoolProp-style output string (`'H'`, `'Hmass'`, `'T'` ...).
    pub fn from_coolprop(name: &str) -> Option<Key> {
        Some(match name.to_lowercase().as_str() {
            "t" => Key::T,
            "p" => Key::P,
            "h" | "hmass" => Key::H,
            "s" | "smass" => Key::S,
            "u" | "umass" => Key::U,
            "d" | "dmass" => Key::D,
            "q" => Key::Q,
            "c" | "cpmass" => Key::Cp,
            "o" | "cvmass" => Key::Cv,
            "v" | "viscosity" => Key::Visc,
            "l" | "conductivity" => Key::Cond,
            "a" | "speed_of_sound" => Key::Sound,
            "prandtl" => Key::Prandtl,
            "i" | "surface_tension" => Key::SurfTension,
            "m" | "molar_mass" | "molarmass" => Key::MolarMass,
            "tcrit" | "t_critical" => Key::Tcrit,
            "pcrit" | "p_critical" => Key::Pcrit,
            "z" => Key::Z,
            "phase" => Key::Phase,
            "isentropic_expansion_coefficient" => Key::IsenExp,
            "isothermal_compressibility" => Key::IsoComp,
            "isobaric_expansion_coefficient" => Key::ExpCoef,
            "ttriple" | "t_triple" => Key::Ttriple,
            "ptriple" | "p_triple" => Key::Ptriple,
            "acentric" | "acentric_factor" => Key::Acentric,
            _ => return None,
        })
    }
}

/// Property function names and the property they return.
pub fn property_function(name: &str) -> Option<Key> {
    Some(match name {
        "enthalpy" => Key::H,
        "entropy" => Key::S,
        "temperature" => Key::T,
        "pressure" => Key::P,
        "density" => Key::D,
        "volume" => Key::V,
        "quality" => Key::Q,
        "intenergy" => Key::U,
        "cp" | "specheat" => Key::Cp,
        "cv" => Key::Cv,
        "viscosity" => Key::Visc,
        "conductivity" => Key::Cond,
        "soundspeed" => Key::Sound,
        "prandtl" => Key::Prandtl,
        "surfacetension" => Key::SurfTension,
        "compressibilityfactor" => Key::Z,
        "molarmass" => Key::MolarMass,
        "t_crit" => Key::Tcrit,
        "p_crit" => Key::Pcrit,
        "kinematicviscosity" => Key::KinVisc,
        "thermaldiffusivity" => Key::Diffusivity,
        "isentropicexponent" => Key::IsenExp,
        "isothermalcompress" => Key::IsoComp,
        "volexpcoef" => Key::ExpCoef,
        "t_triple" => Key::Ttriple,
        "p_triple" => Key::Ptriple,
        "acentricfactor" => Key::Acentric,
        "v_crit" => Key::Vcrit,
        _ => return None,
    })
}

pub const PROPERTY_FUNCTIONS: &[(&str, &str)] = &[
    ("enthalpy", "Specific enthalpy  enthalpy(Fluid, P=, T=)"),
    ("entropy", "Specific entropy  entropy(Fluid, P=, T=)"),
    ("temperature", "Temperature  temperature(Fluid, P=, h=)"),
    ("pressure", "Pressure  pressure(Fluid, T=, h=)"),
    ("density", "Density  density(Fluid, P=, T=)"),
    ("volume", "Specific volume  volume(Fluid, P=, T=)"),
    ("quality", "Vapour quality (-1 if single phase)  quality(Fluid, P=, h=)"),
    ("intenergy", "Specific internal energy  intenergy(Fluid, P=, T=)"),
    ("cp", "Isobaric specific heat  cp(Fluid, P=, T=)"),
    ("cv", "Isochoric specific heat  cv(Fluid, P=, T=)"),
    ("viscosity", "Dynamic viscosity  viscosity(Fluid, P=, T=)"),
    ("conductivity", "Thermal conductivity  conductivity(Fluid, P=, T=)"),
    ("soundspeed", "Speed of sound  soundspeed(Fluid, P=, T=)"),
    ("prandtl", "Prandtl number  prandtl(Fluid, P=, T=)"),
    ("surfacetension", "Surface tension  surfacetension(Fluid, T=)"),
    ("compressibilityfactor", "Compressibility factor Z  compressibilityfactor(Fluid, P=, T=)"),
    ("molarmass", "Molar mass  molarmass(Fluid)"),
    ("t_crit", "Critical temperature  t_crit(Fluid)"),
    ("p_crit", "Critical pressure  p_crit(Fluid)"),
    ("v_crit", "Critical specific volume  v_crit(Fluid)"),
    ("t_triple", "Triple-point temperature  t_triple(Fluid)"),
    ("p_triple", "Triple-point pressure  p_triple(Fluid)"),
    ("acentricfactor", "Acentric factor  acentricfactor(Fluid)"),
    ("normalboilingpt", "Normal boiling point (T_sat at 1 atm)  normalboilingpt(Fluid)"),
    ("enthalpy_formation", "Enthalpy of formation at 25 C per kg  enthalpy_formation(Fluid or 'A[x]&B[y]')"),
    ("lhv", "Lower heating value at 25 C per kg (water as vapour)  lhv(Fluid or 'A[x]&B[y]')"),
    ("hhv", "Higher heating value at 25 C per kg (water condensed)  hhv(Fluid or 'A[x]&B[y]')"),
    ("kinematicviscosity", "Kinematic viscosity mu/rho  kinematicviscosity(Fluid, P=, T=)"),
    ("thermaldiffusivity", "Thermal diffusivity k/(rho cp)  thermaldiffusivity(Fluid, P=, T=)"),
    ("isentropicexponent", "Isentropic exponent -(v/P)(dP/dv)_s (cp/cv for an ideal gas)  isentropicexponent(Fluid, P=, T=)"),
    ("isothermalcompress", "Isothermal compressibility -(1/v)(dv/dP)_T  isothermalcompress(Fluid, P=, T=)"),
    ("volexpcoef", "Volumetric expansion coefficient (1/v)(dv/dT)_P  volexpcoef(Fluid, P=, T=)"),
    ("t_sat", "Saturation temperature  t_sat(Fluid, P=)"),
    ("p_sat", "Saturation pressure  p_sat(Fluid, T=)"),
    ("propssi", "CoolProp PropsSI('Out','In1',v1,'In2',v2,'Fluid') in SI units"),
    ("props1si", "CoolProp Props1SI('Fluid','Out') in SI units"),
    ("hapropssi", "CoolProp humid air HAPropsSI('Out','In1',v1,'In2',v2,'In3',v3) in SI units"),
];

/// Fluid names and their aliases: (name passed to CoolProp, other names).
///
/// Every name is matched without regard to case, so `CO2`, `co2`,
/// `CarbonDioxide` and `R744` are the same fluid, and `Water`, `H2O` and
/// `Steam` another. The aliases cover chemical formulas, ASHRAE refrigerant
/// numbers (ANSI/ASHRAE Standard 34) and the names used by EES (Klein &
/// Nellis, 2012). Names not listed here are looked up among CoolProp's own
/// fluid names and aliases, also without regard to case, and otherwise passed
/// through unchanged (backends `IF97::Water`, `INCOMP::MEG-30%`).
pub const FLUID_ALIASES: &[(&str, &[&str])] = &[
    // EES Steam, Steam_IAPWS and Water are IAPWS-95 (EES help, "Steam_IAPWS");
    // Steam_NBS is the older Haar-Gallagher-Kell formulation, mapped to IAPWS-95
    ("Water", &["H2O", "Steam", "Steam_IAPWS", "Steam_NBS", "R718"]),
    ("CO2", &["CarbonDioxide", "Carbon_Dioxide", "R744"]),
    ("Air", &["Air_ha", "R729"]),
    ("Nitrogen", &["N2", "R728"]),
    ("Oxygen", &["O2", "R732"]),
    ("Argon", &["Ar", "R740"]),
    ("Hydrogen", &["H2", "R702"]),
    ("Helium", &["He", "R704"]),
    ("Neon", &["Ne", "R720"]),
    ("Krypton", &["Kr"]),
    ("Xenon", &["Xe"]),
    ("CarbonMonoxide", &["CO", "Carbon_Monoxide"]),
    ("SulfurDioxide", &["SO2", "SulphurDioxide", "R764"]),
    ("HydrogenSulfide", &["H2S", "HydrogenSulphide"]),
    ("NitrousOxide", &["N2O"]),
    ("Ammonia", &["NH3", "R717"]),
    ("HydrogenChloride", &["HCl"]),
    ("Chlorine", &["Cl2"]),
    ("SulfurHexafluoride", &["SF6", "SulphurHexafluoride"]),
    ("HeavyWater", &["D2O"]),
    ("Deuterium", &["D2"]),
    ("Methane", &["CH4", "R50"]),
    ("Ethane", &["C2H6", "R170"]),
    ("n-Propane", &["Propane", "C3H8", "R290"]),
    ("n-Butane", &["Butane", "nButane", "C4H10", "R600"]),
    ("IsoButane", &["i-Butane", "iButane", "R600a"]),
    ("n-Pentane", &["Pentane", "nPentane", "C5H12", "R601"]),
    ("Isopentane", &["i-Pentane", "iPentane", "R601a"]),
    ("n-Hexane", &["Hexane", "nHexane", "C6H14"]),
    ("n-Heptane", &["Heptane", "nHeptane", "C7H16"]),
    ("n-Octane", &["Octane", "nOctane", "C8H18"]),
    ("n-Decane", &["Decane", "nDecane", "C10H22"]),
    ("Ethylene", &["Ethene", "C2H4", "R1150"]),
    ("Propylene", &["Propene", "C3H6", "R1270"]),
    ("CycloHexane", &["C6H12"]),
    ("Methanol", &["CH3OH", "MethylAlcohol"]),
    ("Ethanol", &["C2H5OH", "EthylAlcohol"]),
    ("Benzene", &["C6H6"]),
    ("Toluene", &["C7H8", "MethylBenzene"]),
    ("Acetone", &["Propanone"]),
    ("DimethylEther", &["DME", "RE170"]),
    // liquid metals and heat-transfer fluids (EES names): CoolProp's
    // incompressible fluids, or DeltaF Studio's own liquids (`liquids.rs`)
    ("INCOMP::LiqNa", &["Sodium", "Na", "LiqNa"]),
    ("INCOMP::NaK", &["NaK", "Sodium-Potassium"]),
    ("LIQ::Lead", &["Lead", "Pb"]),
    ("LIQ::LBE", &["LBE", "LeadBismuth", "Lead-Bismuth", "PbBi"]),
    ("LIQ::SolarSalt", &["SolarSalt", "Solar_Salt", "NitrateSalt", "Salt(60NaNO3-40KNO3)", "Salt_60NaNO3_40KNO3"]),
    ("INCOMP::T66", &["Therminol66", "Therminol_66", "T66"]),
    ("INCOMP::T72", &["Therminol72", "Therminol_72", "T72"]),
    ("INCOMP::TVP1", &["TherminolVP1", "Therminol_VP1", "TherminolVP-1", "TVP1", "DowthermA", "Dowtherm_A"]),
    ("INCOMP::TD12", &["TherminolD12", "Therminol_D12", "TD12"]),
    ("INCOMP::DowJ", &["DowthermJ", "Dowtherm_J", "DowJ"]),
    ("INCOMP::DowQ", &["DowthermQ", "Dowtherm_Q", "DowQ"]),
    ("INCOMP::S800", &["Syltherm800", "Syltherm_800", "S800"]),
    ("INCOMP::XLT", &["SylthermXLT", "Syltherm_XLT", "XLT"]),
];

/// The entry of [`FLUID_ALIASES`] a name belongs to (its name or an alias).
fn alias_entry(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    FLUID_ALIASES.iter().find(|(c, a)| c.eq_ignore_ascii_case(name) || a.iter().any(|x| x.eq_ignore_ascii_case(name)))
}

/// Map a fluid name, in any of its aliases and in any case, onto the name
/// CoolProp (or [`crate::liquids`]) is called with.
///
/// * a name or alias of [`FLUID_ALIASES`] gives its first column;
/// * any other name is matched without regard to case against CoolProp's
///   fluids and their aliases (`toluene` → `Toluene`);
/// * a CoolProp backend prefix keeps the prefix and maps the fluid
///   (`IF97::Steam` → `IF97::Water`); `INCOMP::` and `LIQ::` names are kept;
/// * in a mixture every component is mapped
///   (`CarbonDioxide[0.9]&Ar[0.1]` → `CO2[0.9]&Argon[0.1]`);
/// * a `::Ideal` / `::Real` suffix ([`split_model`]) is dropped.
///
/// Unknown names are passed through, so CoolProp reports them.
pub fn coolprop_fluid(name: &str) -> String {
    // the model suffix (`CO2::Ideal`) is not part of the fluid
    let name = split_model(name).0;
    if let Some((c, _)) = alias_entry(name) {
        return c.to_string();
    }
    if let Some((b, f)) = name.split_once("::") {
        if b.eq_ignore_ascii_case("INCOMP") || b.eq_ignore_ascii_case("LIQ") {
            return name.to_string();
        }
        return format!("{b}::{}", coolprop_fluid(f));
    }
    if name.contains('&') || name.contains('[') {
        return name
            .split('&')
            .map(|part| match part.split_once('[') {
                Some((n, rest)) => format!("{}[{rest}", pure_name(n)),
                None => pure_name(part),
            })
            .collect::<Vec<_>>()
            .join("&");
    }
    pure_name(name)
}

/// A pure fluid name: alias table, then CoolProp's names without regard to case.
fn pure_name(name: &str) -> String {
    let name = name.trim();
    if let Some((c, _)) = alias_entry(name) {
        if !c.contains("::") {
            return c.to_string();
        }
    }
    backend::canonical_name(name).unwrap_or_else(|| name.to_string())
}

/// Every name a fluid is known by: the entry of [`FLUID_ALIASES`] and
/// CoolProp's aliases, without the duplicates that only differ in case.
/// The first item is the name CoolProp is called with.
pub fn fluid_aliases(name: &str) -> Vec<String> {
    let c = coolprop_fluid(name);
    let mut out: Vec<String> = vec![c.clone()];
    let mut push = |s: &str| {
        if !s.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(s)) {
            out.push(s.to_string());
        }
    };
    if let Some((_, a)) = alias_entry(&c) {
        a.iter().for_each(|x| push(x));
    }
    for a in backend::coolprop_aliases(&c) {
        // CoolProp lists upper-case and truncated legacy spellings too: keep
        // the ones in mixed or lower case, or written as a formula / number
        let shouty = a.len() > 4 && a.chars().all(|ch| !ch.is_ascii_lowercase());
        if !shouty {
            push(&a);
        }
    }
    out
}

/// Conversion of a property between the model unit system and SI.
#[derive(Debug, Clone)]
pub struct Conv {
    unit: Unit,
    /// specific volume: value is 1/density
    reciprocal: bool,
    /// per-mass SI value = `scale` × SI value of the model unit: 1/M for
    /// molar h, u, s, cp, cv, v (J/mol → J/kg), M for molar density, else 1
    scale: f64,
}

impl Conv {
    /// `m`: molar mass (kg/mol) of the fluid, used when the unit system is
    /// molar (EES `$UnitSystem MOLAR`).
    pub fn new(key: Key, us: Option<&UnitSystem>, m: Option<f64>) -> Conv {
        let molar = us.is_some_and(|u| u.molar) && m.is_some() && key.molar_dim() != key.dim();
        let dim = if molar { key.molar_dim() } else { key.dim() };
        let unit = match us {
            Some(us) => us.unit_for(dim),
            None => Unit::si(dim),
        };
        let scale = match (molar, m) {
            (true, Some(m)) if key == Key::D => m,
            (true, Some(m)) => 1.0 / m,
            _ => 1.0,
        };
        Conv { unit, reciprocal: matches!(key, Key::V | Key::Vcrit), scale }
    }
    /// Dimension of the value in the model.
    pub fn dim(&self) -> Dim {
        self.unit.dim
    }
    /// model value -> CoolProp SI value (for V returns density)
    #[inline]
    fn to_cp(&self, v: f64) -> f64 {
        let si = self.unit.to_si(v) * self.scale;
        if self.reciprocal {
            1.0 / si
        } else {
            si
        }
    }
    /// d(cp value)/d(model value)
    #[inline]
    fn to_cp_deriv(&self, v: f64) -> f64 {
        let k = self.unit.factor * self.scale;
        if self.reciprocal {
            let si = self.unit.to_si(v) * self.scale;
            -k / (si * si)
        } else {
            k
        }
    }
    #[inline]
    fn from_cp(&self, y: f64) -> f64 {
        let si = if self.reciprocal { 1.0 / y } else { y };
        self.unit.from_si(si / self.scale)
    }
    #[inline]
    fn from_cp_deriv(&self, y: f64) -> f64 {
        let d = if self.reciprocal { -1.0 / (y * y) } else { 1.0 };
        d / (self.unit.factor * self.scale)
    }
}

/// A property call `out = f(fluid, in1, in2)` with unit conversion.
/// Standard enthalpy of formation (kJ/mol) and absolute entropy (J/mol-K)
/// at 298.15 K of ideal-gas species, NIST-JANAF Thermochemical Tables
/// (Chase 1998). They set the reference state of the ideal-gas species, so
/// that enthalpies of reacting mixtures are consistent.
const JANAF: &[(&str, f64, f64)] = &[
    ("CO2", -393.522, 213.795),
    ("Water", -241.826, 188.834),
    ("Nitrogen", 0.0, 191.609),
    ("Oxygen", 0.0, 205.147),
    ("CarbonMonoxide", -110.527, 197.653),
    ("Hydrogen", 0.0, 130.680),
    ("Argon", 0.0, 154.845),
    ("Helium", 0.0, 126.152),
    ("Methane", -74.873, 186.251),
    ("Ammonia", -45.898, 192.774),
    ("SulfurDioxide", -296.842, 248.212),
];

/// Property model of a fluid name: the `::Ideal` / `::Real` suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// No suffix: one state input gives the ideal gas, two the real fluid.
    Auto,
    /// `Fluid::Ideal`: ideal gas with any number of state inputs.
    Ideal,
    /// `Fluid::Real`: the real fluid (equation of state), two state inputs.
    Real,
}

/// Split a fluid name into the name and its model suffix:
/// `CO2::Ideal` → (`CO2`, Ideal), `CarbonDioxide::real` → (`CarbonDioxide`,
/// Real), `IF97::Water` → (`IF97::Water`, Auto). The suffix is not
/// case-sensitive.
pub fn split_model(name: &str) -> (&str, Model) {
    let name = name.trim();
    if let Some((base, last)) = name.rsplit_once("::") {
        if last.eq_ignore_ascii_case("ideal") {
            return (base, Model::Ideal);
        }
        if last.eq_ignore_ascii_case("real") {
            return (base, Model::Real);
        }
    }
    (name, Model::Auto)
}

/// Ideal-gas property: `enthalpy(CO2, T=T)`, `entropy(CO2::Ideal, T=T, P=P)`
/// and the like, of a pure gas or of an ideal-gas mixture
/// (`'Nitrogen[0.75]&CO2[0.1]&Water[0.15]::Ideal'`, mole fractions).
///
/// The ideal-gas state is CoolProp's equation of state at a pressure low
/// enough (1 Pa) for the residual part to vanish, so cp0(T) is the fluid's
/// own ideal-gas heat capacity. Reference state: h(298.15 K) = the standard
/// enthalpy of formation for the species of [`crate::combustion`], so that
/// the enthalpies of reacting mixtures balance; s(298.15 K, 1 atm) = the
/// absolute entropy for the species of [`JANAF`]. Other fluids keep
/// CoolProp's reference.
///
/// Relations (e.g. Çengel & Boles, *Thermodynamics*, ch. 7): h = h(T),
/// u = h − R T, cv = cp − R, s(T, P) = s°(T) − R ln(P / 1 atm), P v = R T,
/// Z = 1, speed of sound √(cp/cv · R T). Entropy from T alone is s° (1 atm).
/// A mixture (Gibbs–Dalton, ch. 13): mass-fraction averages of h, cp and
/// s° of the components, R = R_u / M with M = Σ x_i M_i, and the entropy of
/// mixing −R Σ x_i ln x_i added to s°.
///
/// Inputs: one of T, h, u; or two of T, P, h, u, s, v, d that fix T and P
/// (h and u depend on T only, so T with h or u, or h with u, is not a state).
pub struct IdealGasCall {
    pub label: String,
    /// components: CoolProp name, mass fraction, R_i (J/kg-K), (h offset
    /// J/kg, s offset J/kg-K) added to CoolProp's low-pressure values
    comps: Vec<(String, f64, f64, (f64, f64))>,
    out: Key,
    inputs: Vec<Key>,
    conv_out: Conv,
    conv_in: Vec<Conv>,
    /// J/kg-K
    r: f64,
    /// entropy of mixing −R Σ x ln x, J/kg-K
    s_mix: f64,
}

const P_IG: f64 = 1.0;
const P_STD: f64 = 101_325.0;

/// Outputs an ideal gas gives (transport properties, quality and surface
/// tension need the real fluid).
fn ideal_output(k: Key) -> bool {
    matches!(k, Key::T | Key::P | Key::H | Key::U | Key::S | Key::D | Key::V | Key::Cp | Key::Cv | Key::Z | Key::Sound | Key::IsenExp | Key::IsoComp | Key::ExpCoef)
}

fn raw_h(fluid: &str, t: f64) -> Result<f64, String> {
    backend::props(fluid, Key::H, &[Key::T, Key::P], &[t, P_IG], None)
}

fn raw_s(fluid: &str, t: f64) -> Result<f64, String> {
    backend::props(fluid, Key::S, &[Key::T, Key::P], &[t, P_IG], None)
}

impl IdealGasCall {
    pub fn new(label: String, fluid: &str, out: Key, inputs: Vec<Key>, us: Option<&UnitSystem>) -> Result<IdealGasCall, String> {
        let cp = coolprop_fluid(fluid);
        let cp = cp.strip_prefix("HEOS::").unwrap_or(&cp).to_string();
        // components and mole fractions
        let mut parts = Vec::new();
        if cp.contains('&') || cp.contains('[') {
            for c in cp.split('&') {
                let Some((n, x)) = c.split_once('[') else {
                    return Err(format!("{label}: give the mole fraction of every component, e.g. {c}[0.5]"));
                };
                let x: f64 = x.trim_end_matches(']').trim().parse().map_err(|_| format!("{label}: bad mole fraction in '{c}'"))?;
                if !(x >= 0.0) {
                    return Err(format!("{label}: negative mole fraction in '{c}'"));
                }
                parts.push((n.trim().to_string(), x));
            }
        } else {
            parts.push((cp.clone(), 1.0));
        }
        if parts.iter().any(|(n, _)| n.contains("::")) {
            return Err(format!("{label}: {cp} has no ideal-gas model; use the real fluid with two state inputs"));
        }
        if !ideal_output(out) {
            return Err(format!(
                "{label}: the ideal gas gives T, P, h, u, s, v, density, cp, cv, Z, the speed of sound, the isentropic exponent, the compressibility and the expansion coefficient; use the real fluid for this property"
            ));
        }
        let caloric = |k: &Key| matches!(k, Key::T | Key::H | Key::U);
        let ok = match inputs.as_slice() {
            [k] => match k {
                Key::T => matches!(out, Key::H | Key::S | Key::U | Key::Cp | Key::Cv | Key::T | Key::IsenExp | Key::ExpCoef | Key::Sound),
                Key::H | Key::U => matches!(out, Key::T | Key::H | Key::U | Key::Cp | Key::Cv | Key::IsenExp | Key::ExpCoef | Key::Sound),
                _ => false,
            },
            [a, b] => {
                a != b
                    && !(caloric(a) && caloric(b))
                    && [a, b].iter().all(|k| matches!(k, Key::T | Key::P | Key::H | Key::U | Key::S | Key::V | Key::D))
                    // v (or d) with s fixes neither T nor P without a search in two variables
                    && !([a, b].iter().any(|k| matches!(k, Key::V | Key::D)) && [a, b].iter().any(|k| **k == Key::S))
            }
            _ => false,
        };
        if !ok {
            return Err(format!(
                "{label}: ideal-gas inputs are T, h or u alone (for h, u, cp, cv, s at 1 atm, T), or two of T, P, h, u, s, v, d that fix T and P (h and u depend on T only)"
            ));
        }
        let total: f64 = parts.iter().map(|p| p.1).sum();
        if !(total > 0.0) {
            return Err(format!("{label}: the mole fractions add up to zero"));
        }
        const RU: f64 = 8.314462618;
        let mut comps = Vec::new();
        let mut m = 0.0;
        let mut ms = Vec::new();
        for (n, x) in &parts {
            let mi = backend::props1_si(n, "molar_mass").or_else(|_| backend::props1_si(n, "M")).map_err(|e| format!("{label}: {n}: {e}"))?;
            ms.push(mi);
            m += x / total * mi;
        }
        let r = RU / m;
        let mut s_mix = 0.0;
        for ((n, x), mi) in parts.iter().zip(&ms) {
            let x = x / total;
            if x > 0.0 {
                s_mix -= r * x * x.ln();
            }
            let ri = RU / mi;
            let t0 = 298.15;
            let h_off = match crate::combustion::formation_enthalpy(n) {
                Some(hf) => hf * 1e3 / mi - raw_h(n, t0)?,
                None => 0.0,
            };
            // CoolProp s at P_IG -> s at 1 atm: - R ln(P_STD / P_IG)
            let s_off = match JANAF.iter().find(|j| j.0 == n) {
                Some((_, _, s0)) => s0 / mi - (raw_s(n, t0)? - ri * (P_STD / P_IG).ln()),
                None => 0.0,
            };
            comps.push((n.clone(), x * mi / m, ri, (h_off, s_off)));
        }
        let conv_in = inputs.iter().map(|k| Conv::new(*k, us, Some(m))).collect();
        Ok(IdealGasCall { label, comps, out, inputs, conv_out: Conv::new(out, us, Some(m)), conv_in, r, s_mix })
    }
    /// h (J/kg) at T
    fn h(&self, t: f64) -> Result<f64, String> {
        self.comps.iter().map(|(n, y, _, off)| Ok(y * (raw_h(n, t)? + off.0))).sum()
    }
    /// s° (J/kg-K) at T and 1 atm
    fn s0(&self, t: f64) -> Result<f64, String> {
        let s: f64 = self.comps.iter().map(|(n, y, ri, off)| Ok(y * (raw_s(n, t)? - ri * (P_STD / P_IG).ln() + off.1))).sum::<Result<f64, String>>()?;
        Ok(s + self.s_mix)
    }
    fn cp0(&self, t: f64) -> Result<f64, String> {
        self.comps.iter().map(|(n, y, _, _)| Ok(y * backend::props(n, Key::Cp, &[Key::T, Key::P], &[t, P_IG], None)?)).sum()
    }
    /// T from g(T) = x by Newton, with dg/dT given; from 300 K.
    fn invert(&self, x: f64, what: &str, f: impl Fn(f64) -> Result<(f64, f64), String>) -> Result<f64, String> {
        let mut t = 300.0;
        for _ in 0..80 {
            let (g, d) = f(t)?;
            let step = ((g - x) / d).clamp(-0.5 * t, 2.0 * t);
            t -= step;
            if step.abs() < 1e-10 * t {
                return Ok(t);
            }
        }
        Err(format!("{}: no temperature found for {what} = {x}", self.label))
    }
    fn t_from_h(&self, h: f64) -> Result<f64, String> {
        self.invert(h, "h", |t| Ok((self.h(t)?, self.cp0(t)?)))
    }
    fn t_from_u(&self, u: f64) -> Result<f64, String> {
        self.invert(u, "u", |t| Ok((self.h(t)? - self.r * t, self.cp0(t)? - self.r)))
    }
    /// T from s at pressure p: s°(T) = s + R ln(p / 1 atm), ds°/dT = cp0 / T
    fn t_from_s(&self, s: f64, p: f64) -> Result<f64, String> {
        let target = s + self.r * (p / P_STD).ln();
        self.invert(target, "s", |t| Ok((self.s0(t)?, self.cp0(t)? / t)))
    }
    /// The state (T K, P Pa if fixed) from the SI inputs.
    fn state(&self, x: &[f64]) -> Result<(f64, Option<f64>), String> {
        let get = |k: Key| self.inputs.iter().position(|i| *i == k).map(|i| x[i]);
        // Conv gives the density for a v input, as for CoolProp
        let rho = get(Key::D).or(get(Key::V));
        // temperature from a caloric input
        let t_cal = if let Some(t) = get(Key::T) {
            Some(t)
        } else if let Some(h) = get(Key::H) {
            Some(self.t_from_h(h)?)
        } else if let Some(u) = get(Key::U) {
            Some(self.t_from_u(u)?)
        } else {
            None
        };
        let p = get(Key::P);
        Ok(match (t_cal, p) {
            (Some(t), Some(p)) => (t, Some(p)),
            (Some(t), None) => {
                if let Some(s) = get(Key::S) {
                    (t, Some(P_STD * ((self.s0(t)? - s) / self.r).exp()))
                } else if let Some(d) = rho {
                    (t, Some(d * self.r * t))
                } else {
                    (t, None)
                }
            }
            (None, Some(p)) => {
                if let Some(s) = get(Key::S) {
                    (self.t_from_s(s, p)?, Some(p))
                } else if let Some(d) = rho {
                    (p / (d * self.r), Some(p))
                } else {
                    return Err(format!("{}: no state", self.label));
                }
            }
            (None, None) => return Err(format!("{}: no state", self.label)),
        })
    }
    /// SI output from the SI inputs (density for v, as in CoolProp calls)
    pub fn eval_si(&self, x: &[f64]) -> Result<f64, String> {
        let (t, p) = self.state(x)?;
        if t.is_nan() || t <= 0.0 {
            return Err(format!("{}: T = {t} K", self.label));
        }
        let need_p = || p.ok_or_else(|| format!("{}: give the pressure too (or another input that fixes it)", self.label));
        Ok(match self.out {
            Key::T => t,
            Key::P => need_p()?,
            Key::H => self.h(t)?,
            Key::U => self.h(t)? - self.r * t,
            Key::S => match p {
                Some(p) => self.s0(t)? - self.r * (p / P_STD).ln(),
                // one input: standard-state entropy (1 atm)
                None => self.s0(t)?,
            },
            Key::D | Key::V => need_p()? / (self.r * t),
            Key::Cp => self.cp0(t)?,
            Key::Cv => self.cp0(t)? - self.r,
            Key::Z => 1.0,
            Key::Sound => {
                let cp = self.cp0(t)?;
                (cp / (cp - self.r) * self.r * t).sqrt()
            }
            Key::IsenExp => {
                let cp = self.cp0(t)?;
                cp / (cp - self.r)
            }
            // Pv = RT: -(1/v)(dv/dP)_T = 1/P, (1/v)(dv/dT)_P = 1/T
            Key::IsoComp => 1.0 / need_p()?,
            Key::ExpCoef => 1.0 / t,
            _ => return Err(format!("{}: unsupported", self.label)),
        })
    }
}

impl Key {
    /// Range an input of this kind must lie in for a valid state.
    pub fn arg_kind(self) -> Option<crate::funcs::ArgKind> {
        use crate::funcs::ArgKind;
        match self {
            Key::T => Some(ArgKind::AbsTemp),
            Key::P => Some(ArgKind::AbsPressure),
            Key::Q => Some(ArgKind::Fraction),
            Key::D => Some(ArgKind::Positive),
            _ => None,
        }
    }
}

impl ExtFn for IdealGasCall {
    fn name(&self) -> &str {
        &self.label
    }
    fn arg_kinds(&self) -> Vec<Option<crate::funcs::ArgKind>> {
        self.inputs.iter().map(|k| k.arg_kind()).collect()
    }
    fn dims(&self) -> Option<(Dim, Vec<Dim>)> {
        Some((self.conv_out.dim(), self.conv_in.iter().map(|c| c.dim()).collect()))
    }
    fn eval(&self, args: &[f64]) -> Result<f64, String> {
        let si: Vec<f64> = args.iter().zip(&self.conv_in).map(|(a, c)| c.to_cp(*a)).collect();
        let y = self.eval_si(&si)?;
        Ok(self.conv_out.from_cp(y))
    }
}

pub struct PropCall {
    pub label: String,
    pub fluid: String,
    pub out: Key,
    pub inputs: Vec<Key>,
    pub conv_out: Conv,
    pub conv_in: Vec<Conv>,
    /// `PropsSI`: SI in and out, so no model dimensions
    pub si: bool,
}

impl PropCall {
    pub fn new(label: String, fluid: &str, out: Key, inputs: Vec<Key>, us: Option<&UnitSystem>) -> Result<PropCall, String> {
        if inputs.len() == 2 {
            if inputs[0] == inputs[1] {
                return Err(format!("{label}: the two inputs must be different properties"));
            }
            backend::pair_name(inputs[0], inputs[1]).ok_or_else(|| format!("{label}: unsupported input pair {:?}/{:?}", inputs[0], inputs[1]))?;
        }
        let fluid = coolprop_fluid(fluid);
        // molar mass for a molar unit system (kg/mol)
        let m = match us {
            Some(u) if u.molar => Some(backend::props(&fluid, Key::MolarMass, &[], &[], None).map_err(|e| format!("{label}: molar mass: {e}"))?),
            _ => None,
        };
        let conv_in = inputs.iter().map(|k| Conv::new(*k, us, m)).collect();
        Ok(PropCall { label, fluid, out, inputs, conv_out: Conv::new(out, us, m), conv_in, si: us.is_none() })
    }
}

impl ExtFn for PropCall {
    fn name(&self) -> &str {
        &self.label
    }
    fn arg_kinds(&self) -> Vec<Option<crate::funcs::ArgKind>> {
        self.inputs.iter().map(|k| k.arg_kind()).collect()
    }
    fn dims(&self) -> Option<(Dim, Vec<Dim>)> {
        if self.si {
            return None;
        }
        Some((self.conv_out.dim(), self.conv_in.iter().map(|c| c.dim()).collect()))
    }
    fn eval(&self, args: &[f64]) -> Result<f64, String> {
        let si: Vec<f64> = args.iter().zip(&self.conv_in).map(|(a, c)| c.to_cp(*a)).collect();
        let y = backend::props(&self.fluid, self.out, &self.inputs, &si, None)?;
        Ok(self.conv_out.from_cp(y))
    }
    fn eval_grad(&self, args: &[f64], grad: &mut [f64]) -> Result<f64, String> {
        let si: Vec<f64> = args.iter().zip(&self.conv_in).map(|(a, c)| c.to_cp(*a)).collect();
        let mut g = [0.0f64; 2];
        let y = backend::props(&self.fluid, self.out, &self.inputs, &si, Some(&mut g[..args.len()]));
        match y {
            Ok(y) if g[..args.len()].iter().all(|v| v.is_finite()) => {
                let dout = self.conv_out.from_cp_deriv(y);
                for k in 0..args.len() {
                    grad[k] = dout * g[k] * self.conv_in[k].to_cp_deriv(args[k]);
                }
                Ok(self.conv_out.from_cp(y))
            }
            Ok(_) | Err(_) => {
                let v = self.eval(args)?;
                fd_grad(self, args, grad)?;
                Ok(v)
            }
        }
    }
}

/// Names of humid air (EES `AirH2O` and plain-language aliases), compared
/// without case.
pub const HUMID_AIR_NAMES: &[&str] = &["AirH2O", "HumidAir", "Humid_Air", "MoistAir", "Moist_Air", "AirWater", "Air_H2O"];

/// Whether a fluid name (without `::Ideal` / `::Real`) is humid air.
pub fn is_humid_air(name: &str) -> bool {
    HUMID_AIR_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name.trim()))
}

/// EES-style humid-air call (`enthalpy(AirH2O, T=, P=, w=)`, `humrat`,
/// `relhum`, `wetbulb`, `dewpoint` ...) through CoolProp's `HAPropsSI`
/// (Herrmann, Kretzschmar & Gatley, ASHRAE RP-1485, 2009), in model units.
/// Specific quantities are per kg of dry air, as in EES.
pub struct HaCall {
    pub label: String,
    /// HAPropsSI output key and its dimension
    pub out: (&'static str, Dim),
    /// HAPropsSI input keys and dimensions
    pub ins: Vec<(&'static str, Dim)>,
    pub us: UnitSystem,
}

impl HaCall {
    /// HAPropsSI key and dimension of an EES humid-air input name.
    pub fn input(name: &str) -> Option<(&'static str, Dim)> {
        Some(match name.to_lowercase().as_str() {
            "t" => ("T", Dim::TEMP),
            "p" => ("P", Dim::PRESSURE),
            "w" => ("W", Dim::NONE),
            "r" => ("R", Dim::NONE),
            "b" => ("Twb", Dim::TEMP),
            "d" => ("Tdp", Dim::TEMP),
            "h" => ("H", Dim::SPEC_ENERGY),
            _ => return None,
        })
    }
    /// HAPropsSI key and dimension of an EES function on AirH2O.
    pub fn output(func: &str) -> Option<(&'static str, Dim)> {
        Some(match func {
            "enthalpy" => ("H", Dim::SPEC_ENERGY),
            "entropy" => ("S", Dim::SPEC_ENTROPY),
            "volume" => ("V", Dim::SPEC_VOLUME),
            "temperature" => ("T", Dim::TEMP),
            "cp" | "specheat" => ("C", Dim::SPEC_ENTROPY),
            "humrat" => ("W", Dim::NONE),
            "relhum" => ("R", Dim::NONE),
            "wetbulb" => ("Twb", Dim::TEMP),
            "dewpoint" => ("Tdp", Dim::TEMP),
            "viscosity" => ("mu", Dim::VISCOSITY),
            "conductivity" => ("k", Dim::CONDUCTIVITY),
            _ => return None,
        })
    }
}

impl ExtFn for HaCall {
    fn name(&self) -> &str {
        &self.label
    }
    fn arg_kinds(&self) -> Vec<Option<crate::funcs::ArgKind>> {
        use crate::funcs::ArgKind;
        self.ins
            .iter()
            .map(|(k, d)| match (*k, *d) {
                ("R", _) => Some(ArgKind::Fraction),
                (_, Dim::TEMP) => Some(ArgKind::AbsTemp),
                (_, Dim::PRESSURE) => Some(ArgKind::AbsPressure),
                _ => None,
            })
            .collect()
    }
    fn dims(&self) -> Option<(Dim, Vec<Dim>)> {
        Some((self.out.1, self.ins.iter().map(|i| i.1).collect()))
    }
    fn eval(&self, a: &[f64]) -> Result<f64, String> {
        let si: Vec<f64> = a.iter().zip(&self.ins).map(|(v, (_, d))| self.us.unit_for(*d).to_si(*v)).collect();
        let y = backend::ha_props_si(self.out.0, self.ins[0].0, si[0], self.ins[1].0, si[1], self.ins[2].0, si[2])?;
        if !y.is_finite() {
            return Err(format!("{}: no humid-air state for these inputs", self.label));
        }
        Ok(self.us.unit_for(self.out.1).from_si(y))
    }
}

/// CoolProp `PropsSI` / `Props1SI` / `HAPropsSI` with string keys (SI units).
pub struct RawCall {
    pub label: String,
    pub kind: RawKind,
}

pub enum RawKind {
    PropsSI { out: String, in1: String, in2: String, fluid: String },
    Props1SI { fluid: String, out: String },
    HAPropsSI { out: String, in1: String, in2: String, in3: String },
}

impl ExtFn for RawCall {
    fn name(&self) -> &str {
        &self.label
    }
    fn eval(&self, a: &[f64]) -> Result<f64, String> {
        match &self.kind {
            RawKind::PropsSI { out, in1, in2, fluid } => backend::props_si(out, in1, a[0], in2, a[1], fluid),
            RawKind::Props1SI { fluid, out } => backend::props1_si(fluid, out),
            RawKind::HAPropsSI { out, in1, in2, in3 } => backend::ha_props_si(out, in1, a[0], in2, a[1], in3, a[2]),
        }
    }
}

#[cfg(feature = "coolprop")]
pub mod backend {
    use self::cplib::COOLPROP;
    use super::Key;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::{c_char, c_long, CString};
    use std::sync::OnceLock;

    const BUF: usize = 512;

    /// The CoolProp shared library, loaded from a path that also works in an
    /// installed or copied program. (`coolprop_sys::COOLPROP` loads it from
    /// the build directory, whose path is compiled into the executable.)
    pub mod cplib {
        use coolprop_sys::bindings::CoolProp;
        use std::path::PathBuf;
        use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

        #[cfg(target_os = "windows")]
        pub const LIB_NAME: &str = "CoolProp.dll";
        #[cfg(target_os = "macos")]
        pub const LIB_NAME: &str = "libCoolProp.dylib";
        #[cfg(all(unix, not(target_os = "macos")))]
        pub const LIB_NAME: &str = "libCoolProp.so";

        pub type Shared<'a> = RwLockReadGuard<'a, CoolProp>;

        /// Readers may run concurrently on independent HEOS states; calls that
        /// touch global CoolProp state take the writer side.
        pub struct Lib(RwLock<CoolProp>);

        impl Lib {
            pub fn shared_access(&self) -> Shared<'_> {
                self.0.read().unwrap_or_else(|e| e.into_inner())
            }
            pub fn exclusive_access(&self) -> RwLockWriteGuard<'_, CoolProp> {
                self.0.write().unwrap_or_else(|e| e.into_inner())
            }
        }

        /// Where the library is looked for, in order: `$DFSTUDIO_COOLPROP`
        /// (a file), next to the executable, `../lib` (AppImage, Linux
        /// packages), `../Frameworks` and `../Resources` (macOS app bundle),
        /// and last the build directory.
        pub fn candidates() -> Vec<PathBuf> {
            let mut v = Vec::new();
            if let Some(p) = std::env::var_os("DFSTUDIO_COOLPROP") {
                v.push(PathBuf::from(p));
            }
            if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(|p| p.to_path_buf())) {
                v.push(dir.join(LIB_NAME));
                for sub in ["../lib", "../Frameworks", "../Resources"] {
                    v.push(dir.join(sub).join(LIB_NAME));
                }
            }
            v.push(PathBuf::from(coolprop_sys::COOLPROP_PATH));
            v
        }

        pub static COOLPROP: LazyLock<Lib> = LazyLock::new(|| {
            let mut tried = Vec::new();
            for p in candidates() {
                if !p.is_file() {
                    tried.push(p.display().to_string());
                    continue;
                }
                match unsafe { CoolProp::new(&p) } {
                    Ok(c) => return Lib(RwLock::new(c)),
                    Err(e) => tried.push(format!("{} ({e})", p.display())),
                }
            }
            panic!("the CoolProp library {LIB_NAME} was not found; looked in: {}", tried.join(", "))
        });
    }

    fn msg(buf: &[u8]) -> String {
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    }

    struct Indices {
        params: HashMap<&'static str, c_long>,
        pairs: HashMap<&'static str, c_long>,
    }

    const PAIRS: &[&str] = &[
        "PT_INPUTS",
        "HmassP_INPUTS",
        "PSmass_INPUTS",
        "PQ_INPUTS",
        "QT_INPUTS",
        "DmassT_INPUTS",
        "DmassP_INPUTS",
        "DmassHmass_INPUTS",
        "DmassSmass_INPUTS",
        "DmassUmass_INPUTS",
        "HmassSmass_INPUTS",
        "PUmass_INPUTS",
        "SmassT_INPUTS",
        "HmassT_INPUTS",
        "TUmass_INPUTS",
        "HmassQ_INPUTS",
        "QSmass_INPUTS",
        "DmassQ_INPUTS",
    ];
    const PARAMS: &[&str] = &[
        "T",
        "P",
        "Hmass",
        "Smass",
        "Umass",
        "Dmass",
        "Q",
        "Cpmass",
        "Cvmass",
        "viscosity",
        "conductivity",
        "speed_of_sound",
        "Prandtl",
        "surface_tension",
        "molar_mass",
        "T_critical",
        "p_critical",
        "Z",
        "Phase",
        "T_min",
        "T_max",
        "p_max",
        "T_triple",
        "isentropic_expansion_coefficient",
        "isothermal_compressibility",
        "isobaric_expansion_coefficient",
        "p_triple",
        "acentric",
        "rhomass_critical",
    ];

    fn idx() -> &'static Indices {
        static I: OnceLock<Indices> = OnceLock::new();
        I.get_or_init(|| {
            let cp = COOLPROP.exclusive_access();
            let mut params = HashMap::new();
            let mut pairs = HashMap::new();
            for p in PARAMS {
                let c = CString::new(*p).unwrap();
                params.insert(*p, unsafe { cp.get_param_index(c.as_ptr()) });
            }
            for p in PAIRS {
                let c = CString::new(*p).unwrap();
                pairs.insert(*p, unsafe { cp.get_input_pair_index(c.as_ptr()) });
            }
            Indices { params, pairs }
        })
    }

    /// CoolProp input pair name for two keys, plus whether the order must be swapped.
    pub fn pair_name(a: Key, b: Key) -> Option<(&'static str, bool)> {
        use Key::*;
        let a = if a == V { D } else { a };
        let b = if b == V { D } else { b };
        let table: &[(Key, Key, &str)] = &[
            (P, T, "PT_INPUTS"),
            (H, P, "HmassP_INPUTS"),
            (P, S, "PSmass_INPUTS"),
            (P, Q, "PQ_INPUTS"),
            (Q, T, "QT_INPUTS"),
            (D, T, "DmassT_INPUTS"),
            (D, P, "DmassP_INPUTS"),
            (D, H, "DmassHmass_INPUTS"),
            (D, S, "DmassSmass_INPUTS"),
            (D, U, "DmassUmass_INPUTS"),
            (H, S, "HmassSmass_INPUTS"),
            (P, U, "PUmass_INPUTS"),
            (S, T, "SmassT_INPUTS"),
            (H, T, "HmassT_INPUTS"),
            (T, U, "TUmass_INPUTS"),
            (H, Q, "HmassQ_INPUTS"),
            (Q, S, "QSmass_INPUTS"),
            (D, Q, "DmassQ_INPUTS"),
        ];
        for (x, y, n) in table {
            if *x == a && *y == b {
                return Some((n, false));
            }
            if *x == b && *y == a {
                return Some((n, true));
            }
        }
        None
    }

    /// AbstractState handles of a thread, one per fluid, released when the
    /// thread ends (study workers are short-lived threads).
    struct Handles(HashMap<String, c_long>);

    impl Drop for Handles {
        fn drop(&mut self) {
            if self.0.is_empty() {
                return;
            }
            let cp = COOLPROP.exclusive_access();
            let mut err: c_long = 0;
            let mut buf = [0u8; BUF];
            for h in self.0.values() {
                unsafe { cp.AbstractState_free(*h, &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
            }
        }
    }

    thread_local! {
        static STATES: RefCell<Handles> = RefCell::new(Handles(HashMap::new()));
    }

    fn state(fluid: &str) -> Result<c_long, String> {
        if let Some(h) = STATES.with(|s| s.borrow().0.get(fluid).copied()) {
            return Ok(h);
        }
        let (backend, names) = match fluid.split_once("::") {
            Some((b, f)) => (b.to_string(), f.to_string()),
            None => ("HEOS".to_string(), fluid.to_string()),
        };
        // mixture with fractions: A[0.9]&B[0.1]
        let mut comps = Vec::new();
        let mut fracs = Vec::new();
        for part in names.split('&') {
            if let Some((n, rest)) = part.split_once('[') {
                comps.push(n.to_string());
                fracs.push(rest.trim_end_matches(']').parse::<f64>().map_err(|_| format!("bad fraction in '{part}'"))?);
            } else {
                comps.push(part.to_string());
            }
        }
        let cb = CString::new(backend).unwrap();
        let cf = CString::new(comps.join("&")).map_err(|e| e.to_string())?;
        let mut err: c_long = 0;
        let mut buf = vec![0u8; BUF];
        // CoolProp keeps every AbstractState in one global handle table: adding
        // to it must not overlap any other call, which reads that table
        let h = {
            let cp = COOLPROP.exclusive_access();
            unsafe { cp.AbstractState_factory(cb.as_ptr(), cf.as_ptr(), &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) }
        };
        if err != 0 {
            return Err(format!("unknown fluid '{fluid}': {}", msg(&buf)));
        }
        if !fracs.is_empty() {
            if fracs.len() != comps.len() {
                return Err(format!("fluid '{fluid}': every component needs a fraction"));
            }
            let cp = COOLPROP.exclusive_access();
            unsafe { cp.AbstractState_set_fractions(h, fracs.as_ptr(), fracs.len() as c_long, &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
            if err != 0 {
                return Err(format!("fluid '{fluid}': {}", msg(&buf)));
            }
        }
        STATES.with(|s| s.borrow_mut().0.insert(fluid.to_string(), h));
        Ok(h)
    }

    /// Check a fluid name without evaluating anything.
    pub fn check_fluid(fluid: &str) -> Result<(), String> {
        if crate::liquids::get(fluid).is_some() {
            return Ok(());
        }
        state(fluid).map(|_| ())
    }

    /// Fluid limits in SI: (T_min, T_crit, p_crit, T_max, p_max).
    pub fn limits(fluid: &str) -> Result<(f64, f64, f64, f64, f64), String> {
        if let Some(l) = crate::liquids::get(fluid) {
            return Ok(l.limits());
        }
        let h = state(fluid)?;
        let ix = idx();
        let cp = COOLPROP.shared_access();
        let mut err: c_long = 0;
        let mut buf = [0u8; BUF];
        let mut get = |n: &str| -> f64 {
            err = 0;
            let v = unsafe { cp.AbstractState_keyed_output(h, ix.params[n], &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
            if err != 0 {
                f64::NAN
            } else {
                v
            }
        };
        let tmin = get("T_min").max(get("T_triple"));
        Ok((tmin, get("T_critical"), get("p_critical"), get("T_max"), get("p_max")))
    }

    /// Evaluate several outputs (SI) at one state given by two inputs (SI).
    pub fn state_point(fluid: &str, inputs: [Key; 2], vals: [f64; 2], outs: &[Key]) -> Result<Vec<f64>, String> {
        let mut r = Vec::with_capacity(outs.len());
        // the first call performs the flash; later outputs reuse the same inputs
        for o in outs {
            let y = props(fluid, *o, &inputs, &vals, None)?;
            r.push(if *o == Key::V { 1.0 / y } else { y });
        }
        Ok(r)
    }

    /// Evaluate `out(fluid, inputs)` in SI. With `grad`, also return the
    /// partial derivatives `d out / d input_k` (other input held constant).
    pub fn props(fluid: &str, out: Key, inputs: &[Key], vals: &[f64], grad: Option<&mut [f64]>) -> Result<f64, String> {
        if let Some(l) = crate::liquids::get(fluid) {
            if let Some(g) = grad {
                g.iter_mut().for_each(|x| *x = f64::NAN);
            }
            return l.props(out, inputs, vals);
        }
        if out == Key::ExpCoef && fluid.starts_with("INCOMP::") && !inputs.is_empty() {
            // incompressible fluids give no derivatives: -(1/rho) drho/dT at P
            if let Some(g) = grad {
                g.iter_mut().for_each(|x| *x = f64::NAN);
            }
            let t = props(fluid, Key::T, inputs, vals, None)?;
            let p = props(fluid, Key::P, inputs, vals, None)?;
            let rho = |t: f64| props(fluid, Key::D, &[Key::T, Key::P], &[t, p], None);
            let eps = 1e-2;
            return Ok(-(rho(t + eps)? - rho(t - eps)?) / (2.0 * eps * rho(t)?));
        }
        let h = state(fluid)?;
        let ix = idx();
        let mut err: c_long = 0;
        let mut buf = [0u8; BUF];
        let cp = COOLPROP.shared_access();
        let p = |k: Key| ix.params[k.coolprop()];
        // Key::V is evaluated as density; `Conv` applies the reciprocal.
        let out_param = p(out);
        let get = |param: c_long, err: &mut c_long, buf: &mut [u8; BUF]| unsafe { cp.AbstractState_keyed_output(h, param, err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
        if inputs.is_empty() {
            let y = get(out_param, &mut err, &mut buf);
            if err != 0 {
                return Err(msg(&buf));
            }
            return Ok(y);
        }
        if inputs.len() == 1 {
            return Err("property functions need two independent inputs".into());
        }
        let (pair, swap) = pair_name(inputs[0], inputs[1]).ok_or("unsupported input pair")?;
        let (v1, v2) = if swap { (vals[1], vals[0]) } else { (vals[0], vals[1]) };
        unsafe { cp.AbstractState_update(h, ix.pairs[pair], v1, v2, &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
        if err != 0 {
            let m = msg(&buf);
            // CoolProp has no T-h or T-u flash for HEOS: find the pressure.
            let t_pos = inputs.iter().position(|k| *k == Key::T);
            let p_pos = inputs.iter().position(|k| *k == Key::P);
            if m.contains("not yet supported") && t_pos.is_some() {
                let tk = t_pos.unwrap();
                let other = inputs[1 - tk];
                let (t, target) = (vals[tk], vals[1 - tk]);
                t_flash(&cp, fluid, h, other, t, target)?;
                err = 0;
            } else if let Some(pk) = p_pos.filter(|_| fluid.contains('&') && matches!(inputs[1 - p_pos.unwrap()], Key::H | Key::S | Key::U | Key::D | Key::V)) {
                // mixture: CoolProp's HSU_P / DP flash fails above about the
                // components' T_max; search T on P-T states instead
                let other = if inputs[1 - pk] == Key::V { Key::D } else { inputs[1 - pk] };
                p_flash(&cp, h, other, vals[pk], vals[1 - pk]).ok_or_else(|| format!("{fluid}: {m}"))?;
                err = 0;
            } else {
                return Err(format!("{fluid}: {m}"));
            }
        }
        let y = match out {
            // nu = mu/rho, alpha = k/(rho cp)
            Key::KinVisc => get(out_param, &mut err, &mut buf) / get(ix.params["Dmass"], &mut err, &mut buf),
            Key::Diffusivity => get(out_param, &mut err, &mut buf) / (get(ix.params["Dmass"], &mut err, &mut buf) * get(ix.params["Cpmass"], &mut err, &mut buf)),
            _ => get(out_param, &mut err, &mut buf),
        };
        if err != 0 {
            return Err(format!("{fluid}: {}", msg(&buf)));
        }
        if let Some(g) = grad {
            // Analytic partials only in single-phase states and for keys
            // CoolProp can differentiate.
            let analytic_ok = matches!(out, Key::T | Key::P | Key::H | Key::S | Key::U | Key::D | Key::V | Key::Cp | Key::Cv | Key::Sound | Key::Z) && !inputs.contains(&Key::Q);
            let two_phase = if analytic_ok {
                let ph = get(ix.params["Phase"], &mut err, &mut buf);
                err != 0 || ph == 6.0
            } else {
                true
            };
            if analytic_ok && !two_phase {
                for k in 0..2 {
                    let wrt = if inputs[k] == Key::V { Key::D } else { inputs[k] };
                    let cst = if inputs[1 - k] == Key::V { Key::D } else { inputs[1 - k] };
                    let of = if out == Key::V { Key::D } else { out };
                    let d = unsafe { cp.AbstractState_first_partial_deriv(h, p(of), p(wrt), p(cst), &mut err, buf.as_mut_ptr() as *mut c_char, BUF as c_long) };
                    g[k] = if err != 0 { f64::NAN } else { d };
                    err = 0;
                }
            } else {
                for gk in g.iter_mut() {
                    *gk = f64::NAN;
                }
            }
        }
        Ok(y)
    }

    /// Set state `h` from T and h (or u), which CoolProp's HEOS backend
    /// cannot flash. Along an isotherm the pressure is searched on three
    /// branches: vapour (1 Pa to p_sat), two-phase (quality from the
    /// saturated values) and liquid (p_sat to p_max); above T_crit, one
    /// branch from 1 Pa to p_max.
    ///
    /// T and h do not always fix one state: in a liquid, h barely depends
    /// on P ((dh/dP)_T = v(1 - beta T)), so h a little above h_f(T) is reached
    /// both by a wet state with a tiny quality and by a compressed liquid.
    /// Then the liquid is taken when its pressure is below p_crit (a pump
    /// outlet or condensate), otherwise the wet state.
    fn t_flash(cp: &cplib::Shared<'_>, fluid: &str, h: c_long, other: Key, t: f64, target: f64) -> Result<(), String> {
        let ix = idx();
        let po = ix.params[other.coolprop()];
        let pp = ix.params["P"];
        let read = |param: c_long| -> Option<f64> {
            let mut e: c_long = 0;
            let mut b = [0u8; BUF];
            let y = unsafe { cp.AbstractState_keyed_output(h, param, &mut e, b.as_mut_ptr() as *mut c_char, BUF as c_long) };
            (e == 0 && y.is_finite()).then_some(y)
        };
        // flash, then the residual of the other input
        let upd = |pair: &str, a: f64, b: f64| -> Option<f64> {
            let mut e: c_long = 0;
            let mut bf = [0u8; BUF];
            unsafe { cp.AbstractState_update(h, ix.pairs[pair], a, b, &mut e, bf.as_mut_ptr() as *mut c_char, BUF as c_long) };
            if e != 0 {
                return None;
            }
            read(po).map(|y| y - target)
        };
        let pt = |lnp: f64| upd("PT_INPUTS", lnp.exp(), t);
        // first root of pt on [a, b] (ln P), scanning from a towards b
        let branch = |a: f64, b: f64, n: usize| -> Option<f64> {
            let mut prev: Option<(f64, f64)> = None;
            for i in 0..=n {
                let x = a + (b - a) * i as f64 / n as f64;
                let Some(f) = pt(x) else { continue };
                if f == 0.0 {
                    return Some(x);
                }
                if let Some((x0, f0)) = prev {
                    if f.signum() != f0.signum() {
                        let (mut lo, mut hi, flo) = (x0, x, f0);
                        for _ in 0..200 {
                            let mid = 0.5 * (lo + hi);
                            match pt(mid) {
                                Some(fm) if fm.signum() == flo.signum() => lo = mid,
                                Some(_) => hi = mid,
                                None => break,
                            }
                            if (hi - lo).abs() < 1e-14 {
                                break;
                            }
                        }
                        return Some(0.5 * (lo + hi));
                    }
                }
                prev = Some((x, f));
            }
            None
        };
        let (tc, pc) = (read(ix.params["T_critical"]).unwrap_or(f64::NAN), read(ix.params["p_critical"]).unwrap_or(f64::NAN));
        let pmax = read(ix.params["p_max"]).filter(|p| *p > 0.0).unwrap_or(1e9);
        let p_low = 1.0f64.ln();
        let what = if other == Key::H { "h" } else { "u" };
        let none = |extra: String| format!("{fluid}: no state with T = {:.2} K and {what} = {:.3} kJ/kg{extra}", t, target / 1e3);
        if !(t < tc) {
            let r = branch(p_low, pmax.ln(), 120).ok_or_else(|| none(String::new()))?;
            pt(r);
            return Ok(());
        }
        let yf = upd("QT_INPUTS", 0.0, t).ok_or_else(|| none(" (no saturation state)".into()))? + target;
        let psat = read(pp).unwrap_or(f64::NAN);
        let yg = upd("QT_INPUTS", 1.0, t).ok_or_else(|| none(" (no saturation state)".into()))? + target;
        let liq = branch((psat * (1.0 + 1e-9)).ln(), pmax.ln(), 120);
        let wet = (target - yf) * (target - yg) <= 0.0;
        if wet {
            if let Some(r) = liq.filter(|r| r.exp() <= pc) {
                pt(r);
            } else {
                upd("QT_INPUTS", (target - yf) / (yg - yf), t);
            }
            return Ok(());
        }
        let vap = if target > yg { branch((psat * (1.0 - 1e-9)).ln(), p_low, 120) } else { None };
        match (vap, liq) {
            (Some(r), _) | (None, Some(r)) => {
                pt(r);
                Ok(())
            }
            _ => Err(none(if target > yg {
                format!(" (above the vapour range at this T: {what}_g = {:.3} kJ/kg, and the ideal gas is the upper limit)", yg / 1e3)
            } else {
                format!(" (below the saturated liquid, {what}_f = {:.3} kJ/kg; in a liquid {what} hardly depends on P, so T and {what} fix it poorly: give P instead)", yf / 1e3)
            })),
        }
    }

    /// Mixture state at pressure `p` where the property `other` (h, s, u or
    /// density) equals `target`, by a search in T on P-T flashes, for when
    /// CoolProp's own flash fails: it does above about the T_max of a
    /// component (natural gas fails from 300-500 C, methane T_max = 625 K).
    /// Newton from 300 K with the analytic (d other/dT)_P; failing that, a
    /// bracket that grows geometrically from 300 K (h, s and u rise with T at
    /// constant P, the density falls), narrowed by the Illinois variant of
    /// regula falsi. Leaves the state at the solution.
    fn p_flash(cp: &cplib::Shared<'_>, h: c_long, other: Key, p: f64, target: f64) -> Option<()> {
        let ix = idx();
        let po = ix.params[other.coolprop()];
        let f = |t: f64| -> Option<f64> {
            let mut e: c_long = 0;
            let mut b = [0u8; BUF];
            unsafe { cp.AbstractState_update(h, ix.pairs["PT_INPUTS"], p, t, &mut e, b.as_mut_ptr() as *mut c_char, BUF as c_long) };
            if e != 0 {
                return None;
            }
            let y = unsafe { cp.AbstractState_keyed_output(h, po, &mut e, b.as_mut_ptr() as *mut c_char, BUF as c_long) };
            (e == 0 && y.is_finite()).then_some(y - target)
        };
        // Newton on T with the analytic (d other/dT)_P of the current state
        let (pt_, pp_) = (ix.params["T"], ix.params["P"]);
        let slope = || {
            let mut e: c_long = 0;
            let mut b = [0u8; BUF];
            let d = unsafe { cp.AbstractState_first_partial_deriv(h, po, pt_, pp_, &mut e, b.as_mut_ptr() as *mut c_char, BUF as c_long) };
            (e == 0 && d.is_finite() && d != 0.0).then_some(d)
        };
        let mut t = 300.0;
        for _ in 0..40 {
            let (Some(r), Some(d)) = (f(t), slope()) else { break };
            let step = (r / d).clamp(-0.5 * t, 2.0 * t);
            t -= step;
            if step.abs() < 1e-12 * t {
                return f(t).map(|_| ());
            }
        }
        // the residual rises with T for h, s, u and falls for the density
        let rising = other != Key::D;
        let t0 = 300.0;
        let f0 = f(t0)?;
        let up = (f0 < 0.0) == rising;
        let (mut a, mut fa) = (t0, f0);
        let mut b = None;
        for k in 1..=40 {
            let t = if up { t0 * 1.25f64.powi(k) } else { t0 / 1.25f64.powi(k) };
            if !(20.0..=1e4).contains(&t) {
                break;
            }
            let Some(ft) = f(t) else { break };
            if ft == 0.0 || ft.signum() != fa.signum() {
                b = Some((t, ft));
                break;
            }
            (a, fa) = (t, ft);
        }
        let (mut b, mut fb) = b?;
        for _ in 0..200 {
            if fb == 0.0 || (b - a).abs() < 1e-13 * b {
                break;
            }
            let c = (a * fb - b * fa) / (fb - fa);
            let fc = f(c)?;
            if fc.signum() == fb.signum() {
                fa /= 2.0;
            } else {
                (a, fa) = (b, fb);
            }
            (b, fb) = (c, fc);
        }
        f(b).map(|_| ())
    }

    fn cs(s: &str) -> CString {
        CString::new(s).unwrap_or_default()
    }

    pub fn props_si(out: &str, n1: &str, v1: f64, n2: &str, v2: f64, fluid: &str) -> Result<f64, String> {
        let cp = COOLPROP.exclusive_access();
        let y = unsafe { cp.PropsSI(cs(out).as_ptr(), cs(n1).as_ptr(), v1, cs(n2).as_ptr(), v2, cs(fluid).as_ptr()) };
        if y.is_finite() {
            Ok(y)
        } else {
            Err(last_error(&cp))
        }
    }
    pub fn props1_si(fluid: &str, out: &str) -> Result<f64, String> {
        let cp = COOLPROP.exclusive_access();
        let y = unsafe { cp.Props1SI(cs(fluid).as_ptr(), cs(out).as_ptr()) };
        if y.is_finite() {
            Ok(y)
        } else {
            Err(last_error(&cp))
        }
    }
    pub fn ha_props_si(out: &str, n1: &str, v1: f64, n2: &str, v2: f64, n3: &str, v3: f64) -> Result<f64, String> {
        let cp = COOLPROP.exclusive_access();
        let y = unsafe { cp.HAPropsSI(cs(out).as_ptr(), cs(n1).as_ptr(), v1, cs(n2).as_ptr(), v2, cs(n3).as_ptr(), v3) };
        if y.is_finite() {
            Ok(y)
        } else {
            Err(last_error(&cp))
        }
    }
    fn last_error(cp: &coolprop_sys::bindings::CoolProp) -> String {
        let mut buf = vec![0u8; 1024];
        let key = cs("errstring");
        unsafe { cp.get_global_param_string(key.as_ptr(), buf.as_mut_ptr() as *mut c_char, 1024) };
        let m = msg(&buf);
        if m.is_empty() {
            "CoolProp returned an invalid value".into()
        } else {
            m
        }
    }

    /// CoolProp's aliases of a pure fluid (`CarbonDioxide` → `R744, co2, CO2, ...`).
    pub fn coolprop_aliases(fluid: &str) -> Vec<String> {
        let Ok(f) = CString::new(fluid) else { return Vec::new() };
        let k = cs("aliases");
        let mut buf = vec![0u8; 4096];
        let cp = COOLPROP.shared_access();
        let ok = unsafe { cp.get_fluid_param_string(f.as_ptr(), k.as_ptr(), buf.as_mut_ptr() as *mut c_char, 4096) };
        if ok == 0 {
            return Vec::new();
        }
        msg(&buf).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }

    /// CoolProp's spelling of a pure fluid name or alias written in any case
    /// (`toluene`, `TOLUENE`, `ToLuEnE` → `Toluene`). `None` if CoolProp has no
    /// fluid of that name.
    pub fn canonical_name(name: &str) -> Option<String> {
        static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
        let map = MAP.get_or_init(|| {
            let mut m = HashMap::new();
            for f in fluid_list() {
                for a in coolprop_aliases(&f) {
                    m.entry(a.to_lowercase()).or_insert_with(|| f.clone());
                }
                m.insert(f.to_lowercase(), f);
            }
            m
        });
        map.get(&name.trim().to_lowercase()).cloned()
    }

    /// Names of the pure fluids available in CoolProp (for completion).
    pub fn fluid_list() -> Vec<String> {
        let cp = COOLPROP.exclusive_access();
        let mut buf = vec![0u8; 1 << 16];
        let key = cs("FluidsList");
        unsafe { cp.get_global_param_string(key.as_ptr(), buf.as_mut_ptr() as *mut c_char, 1 << 16) };
        msg(&buf).split(',').map(|s| s.to_string()).filter(|s| !s.is_empty()).collect()
    }

    /// Names of CoolProp's incompressible fluids: pure (`T66`, `DowQ`, ...)
    /// and solutions (`MEG`, ... used as `INCOMP::MEG-30%`).
    pub fn incompressible_list() -> (Vec<String>, Vec<String>) {
        let cp = COOLPROP.exclusive_access();
        let get = |k: &str| {
            let mut buf = vec![0u8; 1 << 16];
            let key = cs(k);
            unsafe { cp.get_global_param_string(key.as_ptr(), buf.as_mut_ptr() as *mut c_char, 1 << 16) };
            msg(&buf).split(',').map(|s| s.to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>()
        };
        (get("incompressible_list_pure"), get("incompressible_list_solution"))
    }
}

#[cfg(not(feature = "coolprop"))]
pub mod backend {
    use super::Key;
    const NO: &str = "CoolProp support is disabled in this build";
    pub fn pair_name(_a: Key, _b: Key) -> Option<(&'static str, bool)> {
        Some(("", false))
    }
    pub fn check_fluid(f: &str) -> Result<(), String> {
        crate::liquids::get(f).map(|_| ()).ok_or_else(|| NO.into())
    }
    pub fn limits(f: &str) -> Result<(f64, f64, f64, f64, f64), String> {
        crate::liquids::get(f).map(|l| l.limits()).ok_or_else(|| NO.into())
    }
    pub fn state_point(f: &str, i: [Key; 2], v: [f64; 2], o: &[Key]) -> Result<Vec<f64>, String> {
        let l = crate::liquids::get(f).ok_or(NO)?;
        o.iter().map(|k| l.props(*k, &i, &v).map(|y| if *k == Key::V { 1.0 / y } else { y })).collect()
    }
    pub fn props(f: &str, o: Key, i: &[Key], v: &[f64], g: Option<&mut [f64]>) -> Result<f64, String> {
        let l = crate::liquids::get(f).ok_or(NO)?;
        if let Some(g) = g {
            g.iter_mut().for_each(|x| *x = f64::NAN);
        }
        l.props(o, i, v)
    }
    pub fn props_si(_: &str, _: &str, _: f64, _: &str, _: f64, _: &str) -> Result<f64, String> {
        Err(NO.into())
    }
    pub fn props1_si(_: &str, _: &str) -> Result<f64, String> {
        Err(NO.into())
    }
    pub fn ha_props_si(_: &str, _: &str, _: f64, _: &str, _: f64, _: &str, _: f64) -> Result<f64, String> {
        Err(NO.into())
    }
    pub fn fluid_list() -> Vec<String> {
        vec![]
    }
    pub fn coolprop_aliases(_: &str) -> Vec<String> {
        Vec::new()
    }
    pub fn canonical_name(_: &str) -> Option<String> {
        None
    }
    pub fn incompressible_list() -> (Vec<String>, Vec<String>) {
        (vec![], vec![])
    }
}
