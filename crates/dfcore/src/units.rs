//! Physical units, dimensions and unit systems.
//!
//! Design: the equations of a model are
//! evaluated in the model's *unit system* (e.g. °C, bar, kJ/kg). Numeric
//! literals that carry a unit (`25 [C]`) are converted to the unit system,
//! property functions convert their inputs/outputs at the call boundary, and
//! every variable can additionally have its own *display unit*. Conversions
//! are affine (`SI = factor * value + offset`), so they cost one FMA.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

/// Exponents of the SI base dimensions: M, L, T, Θ, N, I, J.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Dim(pub [i8; 7]);

impl Dim {
    pub const NONE: Dim = Dim([0; 7]);
    pub const MASS: Dim = Dim([1, 0, 0, 0, 0, 0, 0]);
    pub const LENGTH: Dim = Dim([0, 1, 0, 0, 0, 0, 0]);
    pub const TIME: Dim = Dim([0, 0, 1, 0, 0, 0, 0]);
    pub const TEMP: Dim = Dim([0, 0, 0, 1, 0, 0, 0]);
    pub const AMOUNT: Dim = Dim([0, 0, 0, 0, 1, 0, 0]);
    pub const PRESSURE: Dim = Dim([1, -1, -2, 0, 0, 0, 0]);
    pub const ENERGY: Dim = Dim([1, 2, -2, 0, 0, 0, 0]);
    pub const POWER: Dim = Dim([1, 2, -3, 0, 0, 0, 0]);
    pub const SPEC_ENERGY: Dim = Dim([0, 2, -2, 0, 0, 0, 0]);
    pub const SPEC_ENTROPY: Dim = Dim([0, 2, -2, -1, 0, 0, 0]);
    pub const DENSITY: Dim = Dim([1, -3, 0, 0, 0, 0, 0]);
    pub const SPEC_VOLUME: Dim = Dim([-1, 3, 0, 0, 0, 0, 0]);
    pub const VELOCITY: Dim = Dim([0, 1, -1, 0, 0, 0, 0]);
    pub const VISCOSITY: Dim = Dim([1, -1, -1, 0, 0, 0, 0]);
    pub const CONDUCTIVITY: Dim = Dim([1, 1, -3, -1, 0, 0, 0]);
    pub const MOLAR_MASS: Dim = Dim([1, 0, 0, 0, -1, 0, 0]);
    pub const SURFACE_TENSION: Dim = Dim([1, 0, -2, 0, 0, 0, 0]);
    pub const MASS_FLOW: Dim = Dim([1, 0, -1, 0, 0, 0, 0]);

    pub fn mul(self, o: Dim) -> Dim {
        let mut r = [0i8; 7];
        for i in 0..7 {
            r[i] = self.0[i] + o.0[i];
        }
        Dim(r)
    }
    pub fn div(self, o: Dim) -> Dim {
        let mut r = [0i8; 7];
        for i in 0..7 {
            r[i] = self.0[i] - o.0[i];
        }
        Dim(r)
    }
    pub fn powi(self, n: i32) -> Dim {
        let mut r = [0i8; 7];
        for i in 0..7 {
            r[i] = (self.0[i] as i32 * n) as i8;
        }
        Dim(r)
    }
    pub fn is_none(self) -> bool {
        self == Dim::NONE
    }
    /// Human readable name of well-known dimensions.
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            Dim::NONE => "dimensionless",
            Dim::MASS => "mass",
            Dim::LENGTH => "length",
            Dim::TIME => "time",
            Dim::TEMP => "temperature",
            Dim::PRESSURE => "pressure",
            Dim::ENERGY => "energy",
            Dim::POWER => "power",
            Dim::SPEC_ENERGY => "specific energy",
            Dim::SPEC_ENTROPY => "specific entropy / heat capacity",
            Dim::DENSITY => "density",
            Dim::SPEC_VOLUME => "specific volume",
            Dim::VELOCITY => "velocity",
            Dim::VISCOSITY => "dynamic viscosity",
            Dim::CONDUCTIVITY => "thermal conductivity",
            Dim::MOLAR_MASS => "molar mass",
            Dim::MASS_FLOW => "mass flow rate",
            _ => return None,
        })
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SYM: [&str; 7] = ["kg", "m", "s", "K", "mol", "A", "cd"];
        let mut num = Vec::new();
        let mut den = Vec::new();
        for i in 0..7 {
            let e = self.0[i];
            if e > 0 {
                num.push(if e == 1 { SYM[i].to_string() } else { format!("{}^{}", SYM[i], e) });
            } else if e < 0 {
                den.push(if e == -1 { SYM[i].to_string() } else { format!("{}^{}", SYM[i], -e) });
            }
        }
        let n = if num.is_empty() { "1".to_string() } else { num.join("*") };
        if den.is_empty() {
            write!(f, "{n}")
        } else {
            write!(f, "{n}/{}", den.join("*"))
        }
    }
}

/// An affine unit: `si = value * factor + offset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub symbol: String,
    pub factor: f64,
    pub offset: f64,
    pub dim: Dim,
}

impl Unit {
    pub fn si(dim: Dim) -> Unit {
        Unit { symbol: si_symbol(dim), factor: 1.0, offset: 0.0, dim }
    }
    pub fn none() -> Unit {
        Unit { symbol: "-".into(), factor: 1.0, offset: 0.0, dim: Dim::NONE }
    }
    #[inline]
    pub fn to_si(&self, v: f64) -> f64 {
        v * self.factor + self.offset
    }
    #[inline]
    pub fn from_si(&self, v: f64) -> f64 {
        (v - self.offset) / self.factor
    }
    /// Temperature-difference unit (`dK`, `deltaC`, ...): converts without
    /// the affine offset of absolute temperature scales.
    pub fn is_delta(&self) -> bool {
        self.dim == Dim::TEMP && DELTA_TEMP.contains(&self.symbol.trim().to_lowercase().as_str())
    }
    /// Convert a value expressed in `self` into `other` (same dimension).
    /// When either side is a temperature difference only the scale factors
    /// apply (5 dK is 5 K of difference in any absolute scale).
    pub fn convert(&self, v: f64, other: &Unit) -> f64 {
        if self.is_delta() || other.is_delta() {
            return v * self.factor / other.factor;
        }
        other.from_si(self.to_si(v))
    }
}

pub fn si_symbol(dim: Dim) -> String {
    match dim {
        Dim::NONE => "-".into(),
        Dim::TEMP => "K".into(),
        Dim::PRESSURE => "Pa".into(),
        Dim::ENERGY => "J".into(),
        Dim::POWER => "W".into(),
        Dim::SPEC_ENERGY => "J/kg".into(),
        Dim::SPEC_ENTROPY => "J/kg-K".into(),
        Dim::DENSITY => "kg/m^3".into(),
        Dim::SPEC_VOLUME => "m^3/kg".into(),
        Dim::VELOCITY => "m/s".into(),
        Dim::VISCOSITY => "Pa-s".into(),
        Dim::CONDUCTIVITY => "W/m-K".into(),
        Dim::MOLAR_MASS => "kg/mol".into(),
        Dim::MASS_FLOW => "kg/s".into(),
        d => d.to_string(),
    }
}

/// Names of the temperature-difference unit (factor 1, no offset).
const DELTA_TEMP: &[&str] = &["dk", "deltak", "deltac", "dc"];

struct Atom {
    factor: f64,
    offset: f64,
    dim: Dim,
}

fn atoms() -> &'static HashMap<&'static str, (f64, f64, Dim)> {
    static T: OnceLock<HashMap<&'static str, (f64, f64, Dim)>> = OnceLock::new();
    T.get_or_init(|| {
        use Dim as D;
        let force = Dim([1, 1, -2, 0, 0, 0, 0]);
        let vol = Dim([0, 3, 0, 0, 0, 0, 0]);
        let area = Dim([0, 2, 0, 0, 0, 0, 0]);
        let freq = Dim([0, 0, -1, 0, 0, 0, 0]);
        let current = Dim([0, 0, 0, 0, 0, 1, 0]);
        let mut m: HashMap<&'static str, (f64, f64, Dim)> = HashMap::new();
        let mut add = |names: &[&'static str], f: f64, o: f64, d: Dim| {
            for n in names {
                m.insert(*n, (f, o, d));
            }
        };
        add(&["-", "", "1"], 1.0, 0.0, D::NONE);
        add(&["%"], 0.01, 0.0, D::NONE);
        add(&["rad"], 1.0, 0.0, D::NONE);
        add(&["deg", "degree", "degrees"], std::f64::consts::PI / 180.0, 0.0, D::NONE);
        // mass
        add(&["kg"], 1.0, 0.0, D::MASS);
        add(&["g"], 1e-3, 0.0, D::MASS);
        add(&["mg"], 1e-6, 0.0, D::MASS);
        add(&["t", "tonne", "ton"], 1e3, 0.0, D::MASS);
        add(&["lbm", "lb"], 0.45359237, 0.0, D::MASS);
        // length
        add(&["m"], 1.0, 0.0, D::LENGTH);
        add(&["km"], 1e3, 0.0, D::LENGTH);
        add(&["cm"], 1e-2, 0.0, D::LENGTH);
        add(&["mm"], 1e-3, 0.0, D::LENGTH);
        add(&["um", "µm", "micron"], 1e-6, 0.0, D::LENGTH);
        add(&["in", "inch"], 0.0254, 0.0, D::LENGTH);
        add(&["ft"], 0.3048, 0.0, D::LENGTH);
        add(&["mi", "mile"], 1609.344, 0.0, D::LENGTH);
        // time
        add(&["s", "sec"], 1.0, 0.0, D::TIME);
        add(&["ms"], 1e-3, 0.0, D::TIME);
        add(&["min"], 60.0, 0.0, D::TIME);
        add(&["h", "hr", "hour"], 3600.0, 0.0, D::TIME);
        add(&["day", "d"], 86400.0, 0.0, D::TIME);
        add(&["yr", "year"], 365.0 * 86400.0, 0.0, D::TIME);
        // temperature (absolute)
        add(&["k", "kelvin"], 1.0, 0.0, D::TEMP);
        add(&["c", "degc", "°c", "ºc", "celsius"], 1.0, 273.15, D::TEMP);
        add(&["f", "degf", "°f", "ºf", "fahrenheit"], 5.0 / 9.0, 459.67 * 5.0 / 9.0, D::TEMP);
        add(&["r", "degr", "rankine"], 5.0 / 9.0, 0.0, D::TEMP);
        add(DELTA_TEMP, 1.0, 0.0, D::TEMP);
        // amount
        add(&["mol"], 1.0, 0.0, D::AMOUNT);
        add(&["kmol"], 1e3, 0.0, D::AMOUNT);
        add(&["lbmol"], 453.59237, 0.0, D::AMOUNT);
        add(&["a", "amp"], 1.0, 0.0, current);
        // pressure
        add(&["pa"], 1.0, 0.0, D::PRESSURE);
        add(&["kpa"], 1e3, 0.0, D::PRESSURE);
        add(&["mpa"], 1e6, 0.0, D::PRESSURE);
        add(&["gpa"], 1e9, 0.0, D::PRESSURE);
        add(&["bar", "bara"], 1e5, 0.0, D::PRESSURE);
        add(&["mbar"], 1e2, 0.0, D::PRESSURE);
        add(&["atm"], 101325.0, 0.0, D::PRESSURE);
        add(&["psi", "psia"], 6894.757293168361, 0.0, D::PRESSURE);
        add(&["mmhg", "torr"], 133.322387415, 0.0, D::PRESSURE);
        // energy
        add(&["j"], 1.0, 0.0, D::ENERGY);
        add(&["kj"], 1e3, 0.0, D::ENERGY);
        add(&["mj"], 1e6, 0.0, D::ENERGY);
        add(&["gj"], 1e9, 0.0, D::ENERGY);
        add(&["wh"], 3600.0, 0.0, D::ENERGY);
        add(&["kwh"], 3.6e6, 0.0, D::ENERGY);
        add(&["mwh"], 3.6e9, 0.0, D::ENERGY);
        add(&["gwh"], 3.6e12, 0.0, D::ENERGY);
        add(&["cal"], 4.184, 0.0, D::ENERGY);
        add(&["kcal"], 4184.0, 0.0, D::ENERGY);
        add(&["btu"], 1055.05585262, 0.0, D::ENERGY);
        add(&["therm"], 1.05505585262e8, 0.0, D::ENERGY);
        // power
        add(&["w"], 1.0, 0.0, D::POWER);
        add(&["kw"], 1e3, 0.0, D::POWER);
        add(&["mw"], 1e6, 0.0, D::POWER);
        add(&["gw"], 1e9, 0.0, D::POWER);
        add(&["hp"], 745.69987158227022, 0.0, D::POWER);
        // force
        add(&["n"], 1.0, 0.0, force);
        add(&["kn"], 1e3, 0.0, force);
        add(&["lbf"], 4.4482216152605, 0.0, force);
        // volume / area
        add(&["l", "liter", "litre"], 1e-3, 0.0, vol);
        add(&["ml"], 1e-6, 0.0, vol);
        add(&["gal"], 3.785411784e-3, 0.0, vol);
        add(&["ha"], 1e4, 0.0, area);
        add(&["hz"], 1.0, 0.0, freq);
        add(&["rpm"], 1.0 / 60.0, 0.0, freq);
        m
    })
}

fn lookup_atom(name: &str) -> Option<Atom> {
    let key = name.to_lowercase();
    // A few case-sensitive collisions: "mW" vs "MW", "mPa" etc. We follow
    // engineering habit: MW/MPa/MJ are mega (lower-case milli is rare).
    if let Some(&(f, o, d)) = atoms().get(key.as_str()) {
        return Some(Atom { factor: f, offset: o, dim: d });
    }
    // trailing digit exponent: m3, m2, s2
    let digits: String = key.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() < key.len() {
        let base = &key[..key.len() - digits.len()];
        let e: i32 = digits.chars().rev().collect::<String>().parse().ok()?;
        let a = lookup_atom(base)?;
        return Some(Atom { factor: a.factor.powi(e), offset: 0.0, dim: a.dim.powi(e) });
    }
    None
}

/// Parse a unit expression such as `kJ/kg-K`, `kJ/(kg*K)`, `m^3/kg`, `W/m^2-K`.
/// EES convention: everything after the first `/` is in the denominator and
/// `-`, `*`, `·` or blanks separate factors.
pub fn parse_unit(text: &str) -> Result<Unit, String> {
    let t = text.trim();
    if t.is_empty() || t == "-" {
        return Ok(Unit::none());
    }
    // protect negative exponents (m^-1) from the '-' factor separator
    let cleaned: String = t.replace("^-", "^~").chars().filter(|c| *c != '(' && *c != ')').collect();
    let mut parts = cleaned.splitn(2, '/');
    let num = parts.next().unwrap_or("");
    let den = parts.next().unwrap_or("");
    let mut factor = 1.0;
    let mut dim = Dim::NONE;
    let mut nfactors = 0;
    let mut single: Option<Atom> = None;
    for (s, sign) in [(num, 1), (den, -1)] {
        for f in s.split(|c: char| c == '-' || c == '*' || c == '·' || c == ' ' || c == '/' || c == '.') {
            let f = f.trim();
            if f.is_empty() || f == "1" {
                continue;
            }
            let (base, exp) = match f.split_once('^') {
                Some((b, e)) => (b, e.replace('~', "-").parse::<i32>().map_err(|_| format!("bad exponent in '{f}'"))?),
                None => (f, 1),
            };
            let a = lookup_atom(base).ok_or_else(|| format!("unknown unit '{base}'"))?;
            nfactors += 1;
            factor *= a.factor.powi(exp * sign);
            dim = dim.mul(a.dim.powi(exp * sign));
            if nfactors == 1 && exp == 1 && sign == 1 {
                single = Some(a);
            } else {
                single = None;
            }
        }
    }
    // Affine offset only applies to a lone absolute temperature unit.
    let offset = match (&single, nfactors) {
        (Some(a), 1) => a.offset,
        _ => 0.0,
    };
    Ok(Unit { symbol: t.to_string(), factor, offset, dim })
}

// ------------------------------------------------------------------ unit systems

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    K,
    C,
    F,
    R,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressUnit {
    Pa,
    KPa,
    Bar,
    MPa,
    Atm,
    Psia,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnergyUnit {
    J,
    KJ,
    Btu,
}

/// The unit system in which a model's equations are written and evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitSystem {
    pub name: String,
    pub temperature: TempUnit,
    pub pressure: PressUnit,
    pub energy: EnergyUnit,
    /// Trigonometric functions take degrees instead of radians.
    pub degrees: bool,
}

impl Default for UnitSystem {
    fn default() -> Self {
        UnitSystem::engineering()
    }
}

impl UnitSystem {
    /// Pure SI: K, Pa, J.
    pub fn si() -> Self {
        UnitSystem { name: "SI".into(), temperature: TempUnit::K, pressure: PressUnit::Pa, energy: EnergyUnit::J, degrees: false }
    }
    /// Engineering: °C, bar, kJ (kJ/kg, kW, kJ/kg-K).
    pub fn engineering() -> Self {
        UnitSystem { name: "Engineering (C, bar, kJ)".into(), temperature: TempUnit::C, pressure: PressUnit::Bar, energy: EnergyUnit::KJ, degrees: false }
    }
    /// SI with kPa and kJ: K, kPa, kJ.
    pub fn si_kpa() -> Self {
        UnitSystem { name: "SI kPa (K, kPa, kJ)".into(), temperature: TempUnit::K, pressure: PressUnit::KPa, energy: EnergyUnit::KJ, degrees: false }
    }
    /// Imperial / English engineering: °F, psia, Btu (Btu/lbm, Btu/lbm-R).
    pub fn imperial() -> Self {
        UnitSystem { name: "Imperial (F, psia, Btu)".into(), temperature: TempUnit::F, pressure: PressUnit::Psia, energy: EnergyUnit::Btu, degrees: false }
    }
    pub fn presets() -> Vec<UnitSystem> {
        vec![Self::engineering(), Self::si(), Self::si_kpa(), Self::imperial()]
    }

    /// Parse a `$UnitSystem` directive argument, e.g. `SI C bar kJ`.
    pub fn from_directive(args: &str) -> Result<UnitSystem, String> {
        let mut us = UnitSystem::si();
        us.name = format!("Custom ({})", args.trim());
        for w in args.split(|c: char| c.is_whitespace() || c == ',') {
            match w.to_lowercase().as_str() {
                "" | "mass" | "si" => {}
                "eng" | "engineering" => us = UnitSystem::engineering(),
                "imperial" | "english" | "eng_us" => us = UnitSystem::imperial(),
                "c" => us.temperature = TempUnit::C,
                "k" => us.temperature = TempUnit::K,
                "f" => us.temperature = TempUnit::F,
                "r" => us.temperature = TempUnit::R,
                "pa" => us.pressure = PressUnit::Pa,
                "kpa" => us.pressure = PressUnit::KPa,
                "bar" => us.pressure = PressUnit::Bar,
                "mpa" => us.pressure = PressUnit::MPa,
                "atm" => us.pressure = PressUnit::Atm,
                "psia" | "psi" => us.pressure = PressUnit::Psia,
                "j" => us.energy = EnergyUnit::J,
                "kj" => us.energy = EnergyUnit::KJ,
                "btu" => us.energy = EnergyUnit::Btu,
                "deg" => us.degrees = true,
                "rad" => us.degrees = false,
                other => return Err(format!("unknown $UnitSystem option '{other}'")),
            }
        }
        Ok(us)
    }

    fn temp_unit(&self) -> Unit {
        let s = match self.temperature {
            TempUnit::K => "K",
            TempUnit::C => "C",
            TempUnit::F => "F",
            TempUnit::R => "R",
        };
        parse_unit(s).unwrap()
    }
    fn press_unit(&self) -> Unit {
        let s = match self.pressure {
            PressUnit::Pa => "Pa",
            PressUnit::KPa => "kPa",
            PressUnit::Bar => "bar",
            PressUnit::MPa => "MPa",
            PressUnit::Atm => "atm",
            PressUnit::Psia => "psia",
        };
        parse_unit(s).unwrap()
    }

    /// The unit in which quantities of dimension `dim` are expressed in this
    /// system (used for literals, property calls and default display).
    pub fn unit_for(&self, dim: Dim) -> Unit {
        if dim == Dim::TEMP {
            return self.temp_unit();
        }
        if dim == Dim::PRESSURE {
            return self.press_unit();
        }
        let imperial = self.energy == EnergyUnit::Btu;
        if imperial {
            // English engineering: lbm, ft, s, R, Btu
            return match dim {
                Dim::SPEC_ENERGY => parse_unit("Btu/lbm").unwrap(),
                Dim::SPEC_ENTROPY => parse_unit("Btu/lbm-R").unwrap(),
                Dim::ENERGY => parse_unit("Btu").unwrap(),
                Dim::POWER => parse_unit("Btu/s").unwrap(),
                Dim::DENSITY => parse_unit("lbm/ft^3").unwrap(),
                Dim::SPEC_VOLUME => parse_unit("ft^3/lbm").unwrap(),
                Dim::MASS => parse_unit("lbm").unwrap(),
                Dim::MASS_FLOW => parse_unit("lbm/s").unwrap(),
                Dim::VELOCITY => parse_unit("ft/s").unwrap(),
                Dim::LENGTH => parse_unit("ft").unwrap(),
                Dim::CONDUCTIVITY => parse_unit("Btu/hr-ft-R").unwrap(),
                Dim::MOLAR_MASS => parse_unit("lbm/lbmol").unwrap(),
                Dim::VISCOSITY => parse_unit("lbm/ft-s").unwrap(),
                d => Unit::si(d),
            };
        }
        let mut u = Unit::si(dim);
        let kj = self.energy == EnergyUnit::KJ;
        // energy-like dimensions (contain L^2 T^-2) scale with kJ
        if kj && dim.0[1] == 2 && dim.0[2] <= -2 {
            u.factor = 1e3;
            u.symbol = match dim {
                Dim::SPEC_ENERGY => "kJ/kg".into(),
                Dim::SPEC_ENTROPY => "kJ/kg-K".into(),
                Dim::ENERGY => "kJ".into(),
                Dim::POWER => "kW".into(),
                d => format!("1000*{}", si_symbol(d)),
            };
        }
        if dim == Dim::MOLAR_MASS {
            u = parse_unit("kg/kmol").unwrap();
        }
        // temperature differences inside compound units: C and K are equal
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_compound() {
        let u = parse_unit("kJ/kg-K").unwrap();
        assert_eq!(u.dim, Dim::SPEC_ENTROPY);
        assert!((u.factor - 1000.0).abs() < 1e-12);
        let c = parse_unit("C").unwrap();
        assert!((c.to_si(25.0) - 298.15).abs() < 1e-12);
        let w = parse_unit("W/m^2-K").unwrap();
        assert_eq!(w.dim, Dim([1, 0, -3, -1, 0, 0, 0]));
        let v = parse_unit("m3/kg").unwrap();
        assert_eq!(v.dim, Dim::SPEC_VOLUME);
        let f = parse_unit("F").unwrap();
        assert!((f.to_si(32.0) - 273.15).abs() < 1e-9);
        // delta-C inside compound unit has no offset
        assert_eq!(parse_unit("kJ/kg-C").unwrap().offset, 0.0);
    }
    #[test]
    fn delta_temperature() {
        let c = parse_unit("C").unwrap();
        let dk = parse_unit("dK").unwrap();
        assert!(dk.is_delta() && !c.is_delta() && !parse_unit("K").unwrap().is_delta());
        assert_eq!(dk.convert(5.0, &c), 5.0);
        assert_eq!(c.convert(5.0, &dk), 5.0);
        let f = parse_unit("F").unwrap();
        assert!((dk.convert(5.0, &f) - 9.0).abs() < 1e-12);
    }
    #[test]
    fn systems() {
        let e = UnitSystem::engineering();
        assert_eq!(e.unit_for(Dim::SPEC_ENERGY).factor, 1000.0);
        assert_eq!(e.unit_for(Dim::PRESSURE).factor, 1e5);
        assert_eq!(e.unit_for(Dim::VELOCITY).factor, 1.0);
        assert_eq!(e.unit_for(Dim::CONDUCTIVITY).factor, 1.0);
    }
}
