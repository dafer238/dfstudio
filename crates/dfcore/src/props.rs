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

/// Thermodynamic property keys understood by DF Studio.
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
}

impl Key {
    pub fn dim(self) -> Dim {
        match self {
            Key::T | Key::Tcrit => Dim::TEMP,
            Key::P | Key::Pcrit => Dim::PRESSURE,
            Key::H | Key::U => Dim::SPEC_ENERGY,
            Key::S | Key::Cp | Key::Cv => Dim::SPEC_ENTROPY,
            Key::D => Dim::DENSITY,
            Key::V => Dim::SPEC_VOLUME,
            Key::Visc => Dim::VISCOSITY,
            Key::Cond => Dim::CONDUCTIVITY,
            Key::Sound => Dim::VELOCITY,
            Key::SurfTension => Dim::SURFACE_TENSION,
            Key::MolarMass => Dim::MOLAR_MASS,
            Key::Q | Key::Prandtl | Key::Z | Key::Phase => Dim::NONE,
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
    ("t_sat", "Saturation temperature  t_sat(Fluid, P=)"),
    ("p_sat", "Saturation pressure  p_sat(Fluid, T=)"),
    ("propssi", "CoolProp PropsSI('Out','In1',v1,'In2',v2,'Fluid') in SI units"),
    ("props1si", "CoolProp Props1SI('Fluid','Out') in SI units"),
    ("hapropssi", "CoolProp humid air HAPropsSI('Out','In1',v1,'In2',v2,'In3',v3) in SI units"),
];

/// Map EES fluid names onto CoolProp names. Unknown names are passed through
/// (CoolProp accepts many aliases, backends `IF97::Water`, `INCOMP::MEG-30%`
/// and mixtures `CO2[0.9]&Argon[0.1]`).
pub fn coolprop_fluid(name: &str) -> String {
    let l = name.to_lowercase();
    match l.as_str() {
        "carbondioxide" | "co2" | "r744" => "CO2".into(),
        "steam" | "steam_nbs" | "water" | "h2o" | "r718" => "Water".into(),
        // EES Steam_IAPWS is IAPWS-95, like Water and Steam (EES help, "Steam_IAPWS")
        "steam_iapws" => "Water".into(),
        "air" | "air_ha" => "Air".into(),
        "nitrogen" | "n2" => "Nitrogen".into(),
        "oxygen" | "o2" => "Oxygen".into(),
        "argon" | "ar" => "Argon".into(),
        "hydrogen" | "h2" => "Hydrogen".into(),
        "helium" | "he" => "Helium".into(),
        "methane" | "ch4" => "Methane".into(),
        "carbonmonoxide" | "co" => "CarbonMonoxide".into(),
        "sulfurdioxide" | "so2" => "SulfurDioxide".into(),
        "ammonia" | "nh3" | "r717" => "Ammonia".into(),
        // liquid metals and heat-transfer fluids (EES names): CoolProp's
        // incompressible fluids, or DF Studio's own liquids (`liquids.rs`)
        "sodium" | "na" | "liqna" => "INCOMP::LiqNa".into(),
        "nak" | "sodium-potassium" => "INCOMP::NaK".into(),
        "lead" | "pb" => "LIQ::Lead".into(),
        "lbe" | "leadbismuth" | "lead-bismuth" | "pbbi" => "LIQ::LBE".into(),
        "solarsalt" | "solar_salt" | "nitratesalt" | "salt(60nano3-40kno3)" | "salt_60nano3_40kno3" => "LIQ::SolarSalt".into(),
        "therminol66" | "therminol_66" | "t66" => "INCOMP::T66".into(),
        "therminol72" | "therminol_72" | "t72" => "INCOMP::T72".into(),
        "therminolvp1" | "therminol_vp1" | "therminolvp-1" | "tvp1" | "dowtherma" | "dowtherm_a" => "INCOMP::TVP1".into(),
        "therminold12" | "therminol_d12" | "td12" => "INCOMP::TD12".into(),
        "dowthermj" | "dowtherm_j" | "dowj" => "INCOMP::DowJ".into(),
        "dowthermq" | "dowtherm_q" | "dowq" => "INCOMP::DowQ".into(),
        "syltherm800" | "syltherm_800" | "s800" => "INCOMP::S800".into(),
        "sylthermxlt" | "syltherm_xlt" | "xlt" => "INCOMP::XLT".into(),
        _ => name.to_string(),
    }
}

/// Conversion of a property between the model unit system and SI.
#[derive(Debug, Clone)]
pub struct Conv {
    unit: Unit,
    /// specific volume: value is 1/density
    reciprocal: bool,
}

impl Conv {
    pub fn new(key: Key, us: Option<&UnitSystem>) -> Conv {
        let unit = match us {
            Some(us) => us.unit_for(key.dim()),
            None => Unit::si(key.dim()),
        };
        Conv { unit, reciprocal: key == Key::V }
    }
    /// model value -> CoolProp SI value (for V returns density)
    #[inline]
    fn to_cp(&self, v: f64) -> f64 {
        let si = self.unit.to_si(v);
        if self.reciprocal {
            1.0 / si
        } else {
            si
        }
    }
    /// d(cp value)/d(model value)
    #[inline]
    fn to_cp_deriv(&self, v: f64) -> f64 {
        if self.reciprocal {
            let si = self.unit.to_si(v);
            -self.unit.factor / (si * si)
        } else {
            self.unit.factor
        }
    }
    #[inline]
    fn from_cp(&self, y: f64) -> f64 {
        let si = if self.reciprocal { 1.0 / y } else { y };
        self.unit.from_si(si)
    }
    #[inline]
    fn from_cp_deriv(&self, y: f64) -> f64 {
        let d = if self.reciprocal { -1.0 / (y * y) } else { 1.0 };
        d / self.unit.factor
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

/// Ideal-gas property from one input: `enthalpy(CO2, T=T)` and the like.
///
/// The ideal-gas state is CoolProp's equation of state at a pressure low
/// enough (1 Pa) for the residual part to vanish, so cp0(T) is the fluid's
/// own ideal-gas heat capacity. For the species of [`JANAF`] the reference
/// the thermochemical one: h(298.15 K) = enthalpy of formation, s(298.15 K, 1 atm) =
/// absolute entropy, s at 1 atm (standard state) when no pressure is given.
/// Other fluids keep CoolProp's reference. u = h - R T, cv = cp - R.
pub struct IdealGasCall {
    pub label: String,
    fluid: String,
    out: Key,
    inp: Key,
    conv_out: Conv,
    conv_in: Conv,
    /// J/kg-K
    r: f64,
    /// (h offset J/kg, s offset J/kg-K) added to CoolProp's low-pressure values
    off: (f64, f64),
}

const P_IG: f64 = 1.0;
const P_STD: f64 = 101_325.0;

impl IdealGasCall {
    pub fn new(label: String, fluid: &str, out: Key, inp: Key, us: Option<&UnitSystem>) -> Result<IdealGasCall, String> {
        let cp = coolprop_fluid(fluid);
        if cp.contains("::") || cp.contains('[') || cp.contains('&') {
            return Err(format!("{label}: {cp} has no ideal-gas state; give two state inputs"));
        }
        let ok = match inp {
            Key::T => matches!(out, Key::H | Key::S | Key::U | Key::Cp | Key::Cv),
            Key::H | Key::U => out == Key::T,
            _ => false,
        };
        if !ok {
            return Err(format!(
                "{label}: with one input, the ideal-gas functions are enthalpy, entropy, intEnergy, cp, cv (T=...) and temperature (h=... or u=...); otherwise give two state inputs"
            ));
        }
        let m = backend::props1_si(&cp, "molar_mass").or_else(|_| backend::props1_si(&cp, "M")).map_err(|e| format!("{label}: {e}"))?;
        let r = 8.314462618 / m;
        let mut g = IdealGasCall { label, fluid: cp.clone(), out, inp, conv_out: Conv::new(out, us), conv_in: Conv::new(inp, us), r, off: (0.0, 0.0) };
        if let Some((_, hf, s0)) = JANAF.iter().find(|j| j.0 == cp) {
            let t0 = 298.15;
            let h0 = g.raw_h(t0)?;
            let s_raw = g.raw_s(t0)?;
            // CoolProp s at P_IG -> s at 1 atm: - R ln(P_STD / P_IG)
            g.off = (hf * 1e3 / m - h0, s0 / m - (s_raw - r * (P_STD / P_IG).ln()));
        }
        Ok(g)
    }
    fn raw_h(&self, t: f64) -> Result<f64, String> {
        backend::props(&self.fluid, Key::H, &[Key::T, Key::P], &[t, P_IG], None)
    }
    fn raw_s(&self, t: f64) -> Result<f64, String> {
        backend::props(&self.fluid, Key::S, &[Key::T, Key::P], &[t, P_IG], None)
    }
    fn cp0(&self, t: f64) -> Result<f64, String> {
        backend::props(&self.fluid, Key::Cp, &[Key::T, Key::P], &[t, P_IG], None)
    }
    /// h (J/kg) at T
    fn h(&self, t: f64) -> Result<f64, String> {
        Ok(self.raw_h(t)? + self.off.0)
    }
    /// SI output from the SI input
    pub fn eval_si(&self, x: f64) -> Result<f64, String> {
        match (self.inp, self.out) {
            (Key::T, Key::H) => self.h(x),
            (Key::T, Key::U) => Ok(self.h(x)? - self.r * x),
            // standard-state entropy (1 atm)
            (Key::T, Key::S) => Ok(self.raw_s(x)? - self.r * (P_STD / P_IG).ln() + self.off.1),
            (Key::T, Key::Cp) => self.cp0(x),
            (Key::T, Key::Cv) => Ok(self.cp0(x)? - self.r),
            (k, Key::T) => {
                // invert h(T) (or u(T)) by Newton on cp0, from 300 K
                let f = |t: f64| -> Result<(f64, f64), String> {
                    let cp = self.cp0(t)?;
                    Ok(if k == Key::H { (self.h(t)? - x, cp) } else { (self.h(t)? - self.r * t - x, cp - self.r) })
                };
                let mut t = 300.0;
                for _ in 0..60 {
                    let (r, d) = f(t)?;
                    let step = (r / d).clamp(-0.5 * t, 2.0 * t);
                    t -= step;
                    if step.abs() < 1e-10 * t {
                        return Ok(t);
                    }
                }
                Err(format!("{}: no temperature found for {x} J/kg", self.label))
            }
            _ => Err(format!("{}: unsupported", self.label)),
        }
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
        vec![self.inp.arg_kind()]
    }
    fn dims(&self) -> Option<(Dim, Vec<Dim>)> {
        Some((self.out.dim(), vec![self.inp.dim()]))
    }
    fn eval(&self, args: &[f64]) -> Result<f64, String> {
        let y = self.eval_si(self.conv_in.to_cp(args[0]))?;
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
}

impl PropCall {
    pub fn new(label: String, fluid: &str, out: Key, inputs: Vec<Key>, us: Option<&UnitSystem>) -> Result<PropCall, String> {
        if inputs.len() == 2 {
            if inputs[0] == inputs[1] {
                return Err(format!("{label}: the two inputs must be different properties"));
            }
            backend::pair_name(inputs[0], inputs[1]).ok_or_else(|| format!("{label}: unsupported input pair {:?}/{:?}", inputs[0], inputs[1]))?;
        }
        let conv_in = inputs.iter().map(|k| Conv::new(*k, us)).collect();
        Ok(PropCall { label, fluid: coolprop_fluid(fluid), out, inputs, conv_out: Conv::new(out, us), conv_in })
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
        Some((self.out.dim(), self.inputs.iter().map(|k| k.dim()).collect()))
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
            if m.contains("not yet supported") && t_pos.is_some() {
                let tk = t_pos.unwrap();
                let other = inputs[1 - tk];
                let (t, target) = (vals[tk], vals[1 - tk]);
                t_flash(&cp, fluid, h, other, t, target)?;
                err = 0;
            } else {
                return Err(format!("{fluid}: {m}"));
            }
        }
        let y = get(out_param, &mut err, &mut buf);
        if err != 0 {
            return Err(format!("{fluid}: {}", msg(&buf)));
        }
        if let Some(g) = grad {
            // Analytic partials only in single-phase states and for keys
            // CoolProp can differentiate.
            let analytic_ok = !matches!(out, Key::Q | Key::Phase | Key::Visc | Key::Cond | Key::Prandtl | Key::SurfTension) && !inputs.contains(&Key::Q);
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

    /// Names of the pure fluids available in CoolProp (for completion).
    pub fn fluid_list() -> Vec<String> {
        let cp = COOLPROP.exclusive_access();
        let mut buf = vec![0u8; 1 << 16];
        let key = cs("FluidsList");
        unsafe { cp.get_global_param_string(key.as_ptr(), buf.as_mut_ptr() as *mut c_char, 1 << 16) };
        msg(&buf).split(',').map(|s| s.to_string()).filter(|s| !s.is_empty()).collect()
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
}
