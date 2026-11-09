//! Composition of mixtures: conversion between mole, mass and volume
//! fractions, the CoolProp mixture string (`'Methane[0.9]&Ethane[0.1]'`,
//! mole fractions), preset compositions, and the products of complete
//! combustion of a fuel mixture with air.
//!
//! * Mole fraction x_i = n_i / n; mass fraction w_i = x_i M_i / Σ x_j M_j,
//!   so x_i = (w_i / M_i) / Σ (w_j / M_j).
//! * Volume fraction of a gas mixture: by Amagat's law the partial volume
//!   of an ideal gas at the mixture's T and P is proportional to its amount,
//!   so the volume fraction equals the mole fraction (Çengel & Boles,
//!   *Thermodynamics*, §13-1). For real gases at moderate pressure the two
//!   differ by the differences of the compressibility factors (under about
//!   0.5 % for natural gas at standard conditions, ISO 6976); for liquids
//!   they are unrelated, and a volume basis is not offered for them.
//! * Complete combustion with dry air, excess-air ratio λ ≥ 1: C → CO2,
//!   H → H2O, S → SO2, N → N2; O2 needed = C + H/4 + S − O/2 per mole of
//!   fuel. Dry air: U.S. Standard Atmosphere (1976), N2 78.084 %, O2
//!   20.9476 %, Ar 0.934 %, CO2 0.0314 % (mole), normalised.

use crate::combustion;

/// Basis of the fractions given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Basis {
    #[default]
    Mole,
    Mass,
    /// Volume fraction of a gas mixture (= mole fraction, Amagat).
    Volume,
}

/// Molar mass (g/mol) of a pure component by name (alias or CoolProp name).
pub fn molar_mass(name: &str) -> Result<f64, String> {
    let cp = crate::props::coolprop_fluid(name.trim());
    if let Ok(m) = combustion::molar(&cp) {
        return Ok(m.m);
    }
    crate::props::backend::props1_si(&cp, "molar_mass").map(|m| m * 1e3).map_err(|e| format!("{name}: {e}"))
}

/// Mole fractions (normalised) from values on a basis, with the molar
/// masses `m` (g/mol) of the components.
pub fn to_mole(values: &[f64], m: &[f64], basis: Basis) -> Result<Vec<f64>, String> {
    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("fractions must be zero or positive".into());
    }
    let n: Vec<f64> = match basis {
        Basis::Mole | Basis::Volume => values.to_vec(),
        Basis::Mass => values.iter().zip(m).map(|(w, mi)| w / mi).collect(),
    };
    let total: f64 = n.iter().sum();
    if !(total > 0.0) {
        return Err("the fractions add up to zero".into());
    }
    Ok(n.iter().map(|v| v / total).collect())
}

/// Values on a basis (normalised to 1) from mole fractions.
pub fn from_mole(x: &[f64], m: &[f64], basis: Basis) -> Vec<f64> {
    let v: Vec<f64> = match basis {
        Basis::Mole | Basis::Volume => x.to_vec(),
        Basis::Mass => x.iter().zip(m).map(|(x, mi)| x * mi).collect(),
    };
    let total: f64 = v.iter().sum();
    if total > 0.0 {
        v.iter().map(|a| a / total).collect()
    } else {
        v
    }
}

/// Number with up to 6 significant digits, without trailing zeros.
fn short(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let digits = (5 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let s = format!("{v:.digits$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// CoolProp mixture string from names and mole fractions; components with
/// a zero fraction are left out.
pub fn coolprop_string(names: &[String], x: &[f64]) -> String {
    names.iter().zip(x).filter(|(_, x)| **x > 0.0).map(|(n, x)| format!("{}[{}]", n.trim(), short(*x))).collect::<Vec<_>>().join("&")
}

/// Components and mole fractions of a mixture string (`A[x]&B[y]`,
/// optionally quoted, with a `HEOS::` prefix or a `::Ideal` suffix), and
/// whether it is the ideal-gas mixture.
pub fn parse(text: &str) -> Result<(Vec<(String, f64)>, bool), String> {
    let t = text.trim().trim_matches(['\'', '"']).trim();
    let (base, model) = crate::props::split_model(t);
    let base = base.strip_prefix("HEOS::").unwrap_or(base);
    let mut out = Vec::new();
    for c in base.split('&').filter(|c| !c.trim().is_empty()) {
        match c.split_once('[') {
            Some((n, x)) => {
                let x = x.trim_end_matches(']').trim().parse::<f64>().map_err(|_| format!("bad fraction in '{c}'"))?;
                out.push((n.trim().to_string(), x));
            }
            None => out.push((c.trim().to_string(), 0.0)),
        }
    }
    Ok((out, model == crate::props::Model::Ideal))
}

/// A preset composition.
pub struct Preset {
    pub name: &'static str,
    pub note: &'static str,
    pub basis: Basis,
    pub comps: &'static [(&'static str, f64)],
    /// Write the ideal-gas mixture (`::Ideal`).
    pub ideal: bool,
}

/// Preset compositions. The natural gases are illustrative compositions
/// within the usual ranges of pipeline gas (EN 16726 H-gas; Kidnay et al.,
/// *Fundamentals of Natural Gas Processing*, ch. 1): replace them with the
/// analysis of the gas at hand. Refrigerant blends: ASHRAE 34 nominal
/// compositions (mass).
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Natural gas, typical (without sulfur)",
        note: "pipeline H-gas, sweetened: no H2S (illustrative)",
        basis: Basis::Mole,
        comps: &[
            ("Methane", 92.0),
            ("Ethane", 3.5),
            ("n-Propane", 0.8),
            ("IsoButane", 0.15),
            ("n-Butane", 0.15),
            ("Isopentane", 0.05),
            ("n-Pentane", 0.05),
            ("Nitrogen", 2.3),
            ("CO2", 1.0),
        ],
        ideal: false,
    },
    Preset {
        name: "Natural gas, with sulfur (H2S)",
        note: "sour gas, sulfur as H2S: burns to SO2 (illustrative)",
        basis: Basis::Mole,
        comps: &[("Methane", 85.0), ("Ethane", 4.5), ("n-Propane", 1.5), ("n-Butane", 0.5), ("Nitrogen", 2.0), ("CO2", 4.5), ("HydrogenSulfide", 2.0)],
        ideal: false,
    },
    Preset {
        name: "Natural gas, lean (without sulfur)",
        note: "high-methane gas (illustrative)",
        basis: Basis::Mole,
        comps: &[("Methane", 96.5), ("Ethane", 1.8), ("n-Propane", 0.4), ("Nitrogen", 0.8), ("CO2", 0.5)],
        ideal: false,
    },
    Preset {
        name: "Dry air",
        note: "U.S. Standard Atmosphere 1976",
        basis: Basis::Mole,
        comps: &[("Nitrogen", 78.084), ("Oxygen", 20.9476), ("Argon", 0.934), ("CO2", 0.0314)],
        ideal: false,
    },
    Preset {
        name: "Biogas",
        note: "anaerobic digestion, 60 % CH4 (illustrative)",
        basis: Basis::Mole,
        comps: &[("Methane", 60.0), ("CO2", 38.0), ("Nitrogen", 1.5), ("HydrogenSulfide", 0.5)],
        ideal: false,
    },
    Preset {
        name: "Syngas",
        note: "gasifier gas, H2/CO (illustrative)",
        basis: Basis::Mole,
        comps: &[("Hydrogen", 40.0), ("CarbonMonoxide", 40.0), ("CO2", 10.0), ("Methane", 5.0), ("Nitrogen", 5.0)],
        ideal: false,
    },
    Preset { name: "LPG (propane/butane 60/40)", note: "two-phase in storage (mass basis)", basis: Basis::Mass, comps: &[("n-Propane", 60.0), ("n-Butane", 40.0)], ideal: false },
    Preset { name: "R407C", note: "R32/R125/R134a 23/25/52 % mass", basis: Basis::Mass, comps: &[("R32", 23.0), ("R125", 25.0), ("R134a", 52.0)], ideal: false },
    Preset { name: "R410A", note: "R32/R125 50/50 % mass", basis: Basis::Mass, comps: &[("R32", 50.0), ("R125", 50.0)], ideal: false },
    Preset { name: "R404A", note: "R125/R143a/R134a 44/52/4 % mass", basis: Basis::Mass, comps: &[("R125", 44.0), ("R143a", 52.0), ("R134a", 4.0)], ideal: false },
];

/// Dry air (mole fractions, normalised): N2, O2, Ar, CO2.
fn air() -> [(&'static str, f64); 4] {
    let a = [("Nitrogen", 78.084), ("Oxygen", 20.9476), ("Argon", 0.934), ("CO2", 0.0314)];
    let t: f64 = a.iter().map(|c| c.1).sum();
    a.map(|(n, v)| (n, v / t))
}

/// Result of the combustion of a fuel mixture.
#[derive(Debug, Clone)]
pub struct Products {
    /// CoolProp names and mole fractions of the products (normalised).
    pub comps: Vec<(String, f64)>,
    /// kmol of air per kmol of fuel.
    pub air_per_fuel_mol: f64,
    /// kg of air per kg of fuel.
    pub air_per_fuel_mass: f64,
    /// stoichiometric kg of air per kg of fuel.
    pub afr_stoich: f64,
    /// kmol of products per kmol of fuel.
    pub products_per_fuel_mol: f64,
}

/// Complete combustion of a fuel (CoolProp names, mole fractions) with dry
/// air at the excess-air ratio `lambda` (1 = stoichiometric).
pub fn products(fuel: &[(String, f64)], lambda: f64) -> Result<Products, String> {
    if !(lambda >= 1.0) {
        return Err("complete combustion needs an excess-air ratio of 1 or more".into());
    }
    let total: f64 = fuel.iter().map(|c| c.1).sum();
    if !(total > 0.0) {
        return Err("the fuel fractions add up to zero".into());
    }
    let mut acc: Vec<(String, f64)> = Vec::new();
    let mut add = |n: &str, v: f64| {
        if v == 0.0 {
            return;
        }
        match acc.iter_mut().find(|a| a.0 == n) {
            Some(a) => a.1 += v,
            None => acc.push((n.to_string(), v)),
        }
    };
    let (mut o2_need, mut m_fuel) = (0.0, 0.0);
    for (n, x) in fuel {
        let x = x / total;
        let cp = crate::props::coolprop_fluid(n);
        let a = combustion::atoms(&cp).ok_or_else(|| format!("{n}: no atomic composition (use a species of the heating-value table)"))?;
        m_fuel += x * molar_mass(&cp)?;
        let [c, h, o, nn, s] = a.map(|v| v as f64);
        if a == [0; 5] {
            // noble gas: passes through
            add(&cp, x);
            continue;
        }
        o2_need += x * (c + h / 4.0 + s - o / 2.0);
        add("CO2", x * c);
        add("Water", x * h / 2.0);
        add("SulfurDioxide", x * s);
        add("Nitrogen", x * nn / 2.0);
    }
    if o2_need <= 0.0 {
        return Err("this mixture needs no oxygen: it is not a fuel".into());
    }
    let air = air();
    let o2_air = air.iter().find(|a| a.0 == "Oxygen").map(|a| a.1).unwrap_or(0.21);
    let n_air = lambda * o2_need / o2_air;
    for (n, v) in air {
        add(n, n_air * v);
    }
    // the oxygen burnt
    add("Oxygen", -o2_need);
    let m_air: f64 = air.iter().map(|(n, v)| v * molar_mass(n).unwrap_or(0.0)).sum();
    acc.retain(|a| a.1 > 1e-15);
    let n_prod: f64 = acc.iter().map(|a| a.1).sum();
    let comps = acc.into_iter().map(|(n, v)| (n, v / n_prod)).collect();
    Ok(Products { comps, air_per_fuel_mol: n_air, air_per_fuel_mass: n_air * m_air / m_fuel, afr_stoich: n_air / lambda * m_air / m_fuel, products_per_fuel_mol: n_prod })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methane_with_air() {
        // CH4 + 2 (O2 + 3.76 N2): AFR = 17.2 kg/kg (Turns, ch. 2)
        let p = products(&[("Methane".into(), 1.0)], 1.0).unwrap();
        assert!((p.afr_stoich - 17.2).abs() < 0.05, "{}", p.afr_stoich);
        let x = |n: &str| p.comps.iter().find(|c| c.0 == n).map_or(0.0, |c| c.1);
        // 1 CO2 + 2 H2O + ~7.52 N2 (+ Ar, CO2 of the air), no O2 left
        assert!((x("Water") * p.products_per_fuel_mol - 2.0).abs() < 1e-9);
        assert!((x("CO2") * p.products_per_fuel_mol - 1.0).abs() < 1e-2);
        assert!(x("Oxygen") < 1e-12);
        // excess air leaves O2: (lambda - 1) * 2 per mole of fuel
        let q = products(&[("Methane".into(), 1.0)], 1.5).unwrap();
        let o2 = q.comps.iter().find(|c| c.0 == "Oxygen").unwrap().1 * q.products_per_fuel_mol;
        assert!((o2 - 1.0).abs() < 1e-9);
        // H2S burns to SO2
        let s = products(&[("Methane".into(), 0.98), ("H2S".into(), 0.02)], 1.1).unwrap();
        assert!(s.comps.iter().any(|c| c.0 == "SulfurDioxide" && c.1 > 0.0));
        assert!(products(&[("Nitrogen".into(), 1.0)], 1.2).is_err());
    }

    #[test]
    fn bases_and_strings() {
        // mass 50/50 of R32 (52.024) and R125 (120.02): x_R32 = 0.6976
        let m = [52.024, 120.02];
        let x = to_mole(&[50.0, 50.0], &m, Basis::Mass).unwrap();
        assert!((x[0] - 0.6976).abs() < 1e-4);
        let w = from_mole(&x, &m, Basis::Mass);
        assert!((w[0] - 0.5).abs() < 1e-12);
        assert_eq!(to_mole(&[1.0, 3.0], &m, Basis::Volume).unwrap(), vec![0.25, 0.75]);
        let s = coolprop_string(&["Methane".into(), "Ethane".into(), "CO2".into()], &[0.9, 0.1, 0.0]);
        assert_eq!(s, "Methane[0.9]&Ethane[0.1]");
        let (c, ideal) = parse("'Methane[0.9]&Ethane[0.1]::Ideal'").unwrap();
        assert!(ideal && c.len() == 2 && c[1].1 == 0.1);
        assert_eq!(short(1.0 / 3.0), "0.333333");
        assert_eq!(short(0.000314), "0.000314");
    }
}
