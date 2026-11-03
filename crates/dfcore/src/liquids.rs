//! Liquid metals and molten salts that CoolProp does not provide, as
//! incompressible liquids with temperature-dependent properties (fluid
//! names `LIQ::Lead`, `LIQ::LBE`, `LIQ::SolarSalt`).
//!
//! Correlations (T in K unless stated):
//! * lead and lead-bismuth eutectic: OECD/NEA (2015), *Handbook on
//!   Lead-bismuth Eutectic Alloy and Lead Properties, Materials
//!   Compatibility, Thermal-hydraulics and Technologies*, NEA No. 7268,
//!   ch. 2 (recommended correlations);
//! * solar salt (60 wt% NaNO3, 40 wt% KNO3): Zavoico, A. B. (2001), *Solar
//!   Power Tower Design Basis Document*, SAND2001-2100, Sandia National
//!   Laboratories (T in degC).
//!
//! Model: the specific heat is a function of T only, so
//! `h(T, P) = int_{T_m}^{T} cp dT + (P - P_ref)/rho(T)` and
//! `s(T) = int_{T_m}^{T} cp/T dT` (incompressible liquid, the convention of
//! CoolProp's INCOMP backend); h = s = 0 at the melting point and
//! P_ref = 101325 Pa. Gradients are taken by finite differences by the caller.

use crate::props::Key;

const P_REF: f64 = 101_325.0;

/// cp = a + b T + c T^2 + d T^3 + e / T^2 (J/kg-K).
#[derive(Clone, Copy)]
struct Cp {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
}

impl Cp {
    fn cp(&self, t: f64) -> f64 {
        self.a + self.b * t + self.c * t * t + self.d * t * t * t + self.e / (t * t)
    }
    /// Antiderivative of cp.
    fn h(&self, t: f64) -> f64 {
        self.a * t + self.b * t * t / 2.0 + self.c * t.powi(3) / 3.0 + self.d * t.powi(4) / 4.0 - self.e / t
    }
    /// Antiderivative of cp/T.
    fn s(&self, t: f64) -> f64 {
        self.a * t.ln() + self.b * t + self.c * t * t / 2.0 + self.d * t.powi(3) / 3.0 - self.e / (2.0 * t * t)
    }
}

/// A liquid: properties as functions of T (K), SI units.
pub struct Liquid {
    pub name: &'static str,
    /// melting point (h = s = 0 there) and upper validity limit, K
    pub t_min: f64,
    pub t_max: f64,
    /// kg/mol
    pub molar_mass: f64,
    cp: Cp,
    rho: fn(f64) -> f64,
    mu: fn(f64) -> f64,
    k: fn(f64) -> f64,
    p_sat: Option<fn(f64) -> f64>,
    pub source: &'static str,
}

const C0: f64 = 273.15;

pub static LIQUIDS: &[Liquid] = &[
    Liquid {
        name: "Lead",
        t_min: 600.6,
        t_max: 2000.0,
        molar_mass: 0.2072,
        cp: Cp { a: 175.1, b: -4.961e-2, c: 1.985e-5, d: -2.099e-9, e: -1.524e6 },
        rho: |t| 11441.0 - 1.2795 * t,
        mu: |t| 4.55e-4 * (1069.0 / t).exp(),
        k: |t| 9.2 + 0.011 * t,
        p_sat: Some(|t| 5.76e9 * (-22131.0 / t).exp()),
        source: "OECD/NEA (2015), Handbook on lead-bismuth eutectic alloy and lead properties",
    },
    Liquid {
        name: "LBE",
        t_min: 398.0,
        t_max: 1500.0,
        molar_mass: 0.208,
        cp: Cp { a: 164.8, b: -3.94e-2, c: 1.25e-5, d: 0.0, e: -4.56e5 },
        rho: |t| 11065.0 - 1.293 * t,
        mu: |t| 4.94e-4 * (754.1 / t).exp(),
        k: |t| 3.284 + 1.617e-2 * t - 2.305e-6 * t * t,
        p_sat: Some(|t| 1.22e10 * (-22552.0 / t).exp()),
        source: "OECD/NEA (2015), Handbook on lead-bismuth eutectic alloy and lead properties",
    },
    Liquid {
        name: "SolarSalt",
        // freezing at about 221 degC; correlations fitted over 300..600 degC
        t_min: 220.0 + C0,
        t_max: 600.0 + C0,
        molar_mass: 0.0914,
        cp: Cp { a: 1443.0 - 0.172 * C0, b: 0.172, c: 0.0, d: 0.0, e: 0.0 },
        rho: |t| 2090.0 - 0.636 * (t - C0),
        mu: |t| {
            let c = t - C0;
            1e-3 * (22.714 - 0.120 * c + 2.281e-4 * c * c - 1.474e-7 * c * c * c)
        },
        k: |t| 0.443 + 1.9e-4 * (t - C0),
        p_sat: None,
        source: "Zavoico (2001), SAND2001-2100",
    },
];

/// The liquid named `LIQ::<name>` (case-insensitive), if any.
pub fn get(fluid: &str) -> Option<&'static Liquid> {
    let (b, n) = fluid.split_once("::")?;
    if !b.eq_ignore_ascii_case("liq") {
        return None;
    }
    LIQUIDS.iter().find(|l| l.name.eq_ignore_ascii_case(n))
}

impl Liquid {
    fn h_t(&self, t: f64) -> f64 {
        self.cp.h(t) - self.cp.h(self.t_min)
    }
    fn s_t(&self, t: f64) -> f64 {
        self.cp.s(t) - self.cp.s(self.t_min)
    }
    pub fn h(&self, t: f64, p: f64) -> f64 {
        self.h_t(t) + (p - P_REF) / (self.rho)(t)
    }
    fn check_t(&self, t: f64) -> Result<f64, String> {
        // a margin below the melting point keeps Newton iterations alive
        if !(t.is_finite() && t > 0.8 * self.t_min && t < 1.5 * self.t_max) {
            return Err(format!("LIQ::{}: T = {t:.2} K outside the range of the correlations ({:.1}..{:.1} K)", self.name, self.t_min, self.t_max));
        }
        Ok(t)
    }
    /// Newton on T for `f(T) = target` with derivative `df`.
    fn solve_t(&self, f: impl Fn(f64) -> f64, df: impl Fn(f64) -> f64, target: f64) -> Result<f64, String> {
        let mut t = 0.5 * (self.t_min + self.t_max);
        for _ in 0..60 {
            let r = f(t) - target;
            let d = df(t);
            let step = r / d;
            t = (t - step).clamp(0.5 * self.t_min, 2.0 * self.t_max);
            if step.abs() < 1e-10 * t {
                return self.check_t(t);
            }
        }
        Err(format!("LIQ::{}: no temperature for these inputs", self.name))
    }
    /// Saturation temperature at `p` (Pa).
    fn t_sat(&self, p: f64) -> Result<f64, String> {
        let ps = self.p_sat.ok_or_else(|| format!("LIQ::{}: no vapour-pressure correlation", self.name))?;
        // ln p_sat is close to linear in 1/T: Newton in 1/T
        let mut x = 1.0 / self.t_max;
        for _ in 0..80 {
            let t = 1.0 / x;
            let f = ps(t).ln() - p.ln();
            let dx = 1e-7 * x;
            let d = (ps(1.0 / (x + dx)).ln() - ps(1.0 / (x - dx)).ln()) / (2.0 * dx);
            let step = f / d;
            x -= step;
            if step.abs() < 1e-13 * x {
                return Ok(1.0 / x);
            }
        }
        Err(format!("LIQ::{}: no saturation temperature at P = {p} Pa", self.name))
    }

    /// Property `out` from two inputs (SI).
    pub fn props(&self, out: Key, inputs: &[Key], vals: &[f64]) -> Result<f64, String> {
        let constant = match out {
            Key::MolarMass => Some(self.molar_mass),
            Key::Tcrit | Key::Pcrit => return Err(format!("LIQ::{}: no critical point in the liquid model", self.name)),
            _ => None,
        };
        if let Some(c) = constant {
            return Ok(c);
        }
        if inputs.len() != 2 {
            return Err("property functions need two independent inputs".into());
        }
        let get = |k: Key| inputs.iter().position(|x| *x == k).map(|i| vals[i]);
        let rho_of = |t: f64| (self.rho)(t);
        let (t, p) = match (get(Key::T), get(Key::P), get(Key::H), get(Key::S), get(Key::Q), get(Key::D)) {
            (Some(t), Some(p), ..) => (self.check_t(t)?, p),
            (_, Some(p), Some(h), ..) => {
                let t = self.solve_t(|t| self.h(t, p), |t| self.cp.cp(t), h)?;
                (t, p)
            }
            (_, Some(p), _, Some(s), ..) => (self.solve_t(|t| self.s_t(t), |t| self.cp.cp(t) / t, s)?, p),
            (Some(t), _, Some(h), ..) => {
                let t = self.check_t(t)?;
                (t, P_REF + (h - self.h_t(t)) * rho_of(t))
            }
            (Some(t), _, _, _, Some(_), _) => {
                let ps = self.p_sat.ok_or_else(|| format!("LIQ::{}: no vapour-pressure correlation", self.name))?;
                (self.check_t(t)?, ps(t))
            }
            (_, Some(p), _, _, Some(_), _) => (self.t_sat(p)?, p),
            (_, Some(p), _, _, _, Some(d)) => {
                let eps = 1e-3;
                let t = self.solve_t(rho_of, |t| ((self.rho)(t + eps) - (self.rho)(t - eps)) / (2.0 * eps), d)?;
                (t, p)
            }
            _ => return Err(format!("LIQ::{}: unsupported inputs {inputs:?} (use T and P, P and h, P and s, T and h, or T/P with x for saturation)", self.name)),
        };
        let rho = rho_of(t);
        let cp = self.cp.cp(t);
        Ok(match out {
            Key::T => t,
            Key::P => p,
            Key::H => self.h(t, p),
            Key::S => self.s_t(t),
            Key::U => self.h(t, p) - p / rho,
            Key::D | Key::V => rho,
            Key::Cp | Key::Cv => cp,
            Key::Visc => (self.mu)(t),
            Key::Cond => (self.k)(t),
            Key::Prandtl => cp * (self.mu)(t) / (self.k)(t),
            // subcooled liquid (CoolProp convention)
            Key::Q => -1.0,
            Key::Phase => 0.0,
            _ => return Err(format!("LIQ::{}: {out:?} is not available for this liquid", self.name)),
        })
    }

    /// (T_min, T_crit, p_crit, T_max, p_max) as `backend::limits`.
    pub fn limits(&self) -> (f64, f64, f64, f64, f64) {
        (self.t_min, f64::NAN, f64::NAN, self.t_max, 1e8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liq(n: &str) -> &'static Liquid {
        get(&format!("LIQ::{n}")).unwrap()
    }

    #[test]
    fn handbook_values() {
        // OECD/NEA (2015): lead at 700 K, LBE at 700 K
        let pb = liq("lead");
        assert!((pb.props(Key::D, &[Key::T, Key::P], &[700.0, 1e5]).unwrap() - 10545.35).abs() < 0.01);
        let cp = pb.props(Key::Cp, &[Key::T, Key::P], &[700.0, 1e5]).unwrap();
        assert!((cp - 146.3).abs() < 0.2, "{cp}");
        let lbe = liq("LBE");
        let cp = lbe.props(Key::Cp, &[Key::T, Key::P], &[700.0, 1e5]).unwrap();
        assert!((cp - 142.4).abs() < 0.2, "{cp}");
        // solar salt at 400 degC: rho = 1835.6, cp = 1511.8 (Zavoico 2001)
        let ss = liq("solarsalt");
        assert!((ss.props(Key::D, &[Key::T, Key::P], &[673.15, 1e5]).unwrap() - 1835.6).abs() < 1e-9);
        assert!((ss.props(Key::Cp, &[Key::T, Key::P], &[673.15, 1e5]).unwrap() - 1511.8).abs() < 1e-9);
    }

    #[test]
    fn inverse_flashes_are_consistent() {
        for l in LIQUIDS {
            let t = 0.5 * (l.t_min + l.t_max);
            let p = 5e5;
            let h = l.props(Key::H, &[Key::T, Key::P], &[t, p]).unwrap();
            let s = l.props(Key::S, &[Key::T, Key::P], &[t, p]).unwrap();
            assert!((l.props(Key::T, &[Key::P, Key::H], &[p, h]).unwrap() - t).abs() < 1e-8, "{}", l.name);
            assert!((l.props(Key::T, &[Key::P, Key::S], &[p, s]).unwrap() - t).abs() < 1e-8, "{}", l.name);
            assert!((l.props(Key::P, &[Key::T, Key::H], &[t, h]).unwrap() - p).abs() < 1e-6, "{}", l.name);
            // cp is dh/dT
            let e = 1e-3;
            let fd = (l.h(t + e, p) - l.h(t - e, p)) / (2.0 * e);
            let rho_term = (p - P_REF) * ((1.0 / (l.rho)(t + e) - 1.0 / (l.rho)(t - e)) / (2.0 * e));
            assert!((fd - rho_term - l.cp.cp(t)).abs() < 1e-5 * l.cp.cp(t), "{}", l.name);
            if l.p_sat.is_some() {
                let ps = l.props(Key::P, &[Key::T, Key::Q], &[t, 0.0]).unwrap();
                assert!((l.props(Key::T, &[Key::P, Key::Q], &[ps, 0.0]).unwrap() - t).abs() < 1e-6, "{}", l.name);
            }
        }
    }
}
