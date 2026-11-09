//! Enthalpy of formation and heating values (LHV, HHV) of fuels and fuel
//! mixtures.
//!
//! Standard enthalpies of formation of the ideal gas at 298.15 K and 1 bar
//! from NIST-JANAF (Chase, 1998) and the NIST Chemistry WebBook (Linstrom &
//! Mallard, eds.). Heating values: complete combustion at 25 °C to CO2,
//! H2O, SO2 and N2; the LHV leaves the water as vapour, the HHV condenses
//! it (Δh_vap(25 °C) = 44.004 kJ/mol, from h_f of liquid water,
//! −285.830 kJ/mol). Mixtures are given as CoolProp strings with mole
//! fractions (`'Methane[0.9]&Ethane[0.1]'`).

use crate::props::coolprop_fluid;

/// Species: CoolProp name, Δh_f° (kJ/mol, gas, 298.15 K), atoms C, H, O, N, S, other molar mass (g/mol).
const SPECIES: &[(&str, f64, [u32; 5], f64)] = &[
    ("Methane", -74.873, [1, 4, 0, 0, 0], 0.0),
    ("Ethane", -83.85, [2, 6, 0, 0, 0], 0.0),
    ("n-Propane", -104.7, [3, 8, 0, 0, 0], 0.0),
    ("n-Butane", -125.6, [4, 10, 0, 0, 0], 0.0),
    ("IsoButane", -134.2, [4, 10, 0, 0, 0], 0.0),
    ("n-Pentane", -146.8, [5, 12, 0, 0, 0], 0.0),
    ("Isopentane", -153.7, [5, 12, 0, 0, 0], 0.0),
    ("Neopentane", -168.0, [5, 12, 0, 0, 0], 0.0),
    ("n-Hexane", -166.9, [6, 14, 0, 0, 0], 0.0),
    ("n-Heptane", -187.8, [7, 16, 0, 0, 0], 0.0),
    ("n-Octane", -208.7, [8, 18, 0, 0, 0], 0.0),
    ("n-Nonane", -228.2, [9, 20, 0, 0, 0], 0.0),
    ("n-Decane", -249.5, [10, 22, 0, 0, 0], 0.0),
    ("Ethylene", 52.4, [2, 4, 0, 0, 0], 0.0),
    ("Propylene", 20.0, [3, 6, 0, 0, 0], 0.0),
    ("Benzene", 82.9, [6, 6, 0, 0, 0], 0.0),
    ("Toluene", 50.1, [7, 8, 0, 0, 0], 0.0),
    ("Methanol", -201.0, [1, 4, 1, 0, 0], 0.0),
    ("Ethanol", -234.8, [2, 6, 1, 0, 0], 0.0),
    ("DimethylEther", -184.1, [2, 6, 1, 0, 0], 0.0),
    ("Hydrogen", 0.0, [0, 2, 0, 0, 0], 0.0),
    ("CarbonMonoxide", -110.527, [1, 0, 1, 0, 0], 0.0),
    ("HydrogenSulfide", -20.6, [0, 2, 0, 0, 1], 0.0),
    ("Ammonia", -45.898, [0, 3, 0, 1, 0], 0.0),
    ("CarbonylSulfide", -138.4, [1, 0, 1, 0, 1], 0.0),
    // products and inerts
    ("CO2", -393.522, [1, 0, 2, 0, 0], 0.0),
    ("Water", -241.826, [0, 2, 1, 0, 0], 0.0),
    ("SulfurDioxide", -296.842, [0, 0, 2, 0, 1], 0.0),
    ("Nitrogen", 0.0, [0, 0, 0, 2, 0], 0.0),
    ("Oxygen", 0.0, [0, 0, 2, 0, 0], 0.0),
    ("NitrousOxide", 82.05, [0, 0, 1, 2, 0], 0.0),
    ("Argon", 0.0, [0, 0, 0, 0, 0], 39.948),
    ("Helium", 0.0, [0, 0, 0, 0, 0], 4.002602),
    ("Neon", 0.0, [0, 0, 0, 0, 0], 20.1797),
];

/// Atomic masses (g/mol), IUPAC 2021 conventional values: C, H, O, N, S.
const ATOM: [f64; 5] = [12.011, 1.008, 15.999, 14.007, 32.06];
const H_CO2: f64 = -393.522;
const H_H2O_GAS: f64 = -241.826;
const H_H2O_LIQ: f64 = -285.830;
const H_SO2: f64 = -296.842;

/// Formation and combustion data of one mole of a fluid (or mixture).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Molar {
    /// g/mol
    pub m: f64,
    /// Δh_f°, kJ/mol
    pub hf: f64,
    /// lower heating value, kJ/mol (positive)
    pub lhv: f64,
    /// higher heating value, kJ/mol
    pub hhv: f64,
}

fn species(name: &str) -> Result<Molar, String> {
    let cp = coolprop_fluid(name.trim());
    let (_, hf, a, extra) = SPECIES
        .iter()
        .find(|s| s.0.eq_ignore_ascii_case(&cp))
        .ok_or_else(|| format!("no enthalpy of formation for '{name}' (known: {})", SPECIES.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")))?;
    let m = extra + a.iter().zip(ATOM).map(|(n, w)| *n as f64 * w).sum::<f64>();
    let [c, h, _, _, s] = a.map(|x| x as f64);
    // complete combustion: C → CO2, H → H2O, S → SO2, N → N2
    let prod_gas = c * H_CO2 + h / 2.0 * H_H2O_GAS + s * H_SO2;
    let prod_liq = c * H_CO2 + h / 2.0 * H_H2O_LIQ + s * H_SO2;
    Ok(Molar { m, hf: *hf, lhv: hf - prod_gas, hhv: hf - prod_liq })
}

/// Standard enthalpy of formation (kJ/mol, ideal gas, 298.15 K) of a
/// species by its CoolProp name.
pub fn formation_enthalpy(coolprop_name: &str) -> Option<f64> {
    SPECIES.iter().find(|s| s.0.eq_ignore_ascii_case(coolprop_name)).map(|s| s.1)
}

/// Data of a fluid or a mixture `'A[x1]&B[x2]'` (mole fractions, normalised).
pub fn molar(fluid: &str) -> Result<Molar, String> {
    let f = fluid.trim().trim_matches(['\'', '"']);
    let f = f.split_once("::").filter(|(b, _)| b.eq_ignore_ascii_case("HEOS")).map_or(f, |(_, r)| r);
    if !f.contains('&') && !f.contains('[') {
        return species(f);
    }
    let mut parts = Vec::new();
    for c in f.split('&') {
        let (n, x) = match c.split_once('[') {
            Some((n, x)) => (n, x.trim_end_matches(']').trim().parse::<f64>().map_err(|_| format!("bad mole fraction in '{c}'"))?),
            None => return Err(format!("'{c}': give the mole fraction, e.g. {c}[0.5]")),
        };
        parts.push((species(n)?, x));
    }
    let total: f64 = parts.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        return Err("the mole fractions add up to zero".into());
    }
    let mut r = Molar { m: 0.0, hf: 0.0, lhv: 0.0, hhv: 0.0 };
    for (s, x) in parts {
        let x = x / total;
        r.m += x * s.m;
        r.hf += x * s.hf;
        r.lhv += x * s.lhv;
        r.hhv += x * s.hhv;
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heating_values() {
        // methane: LHV 50.0 MJ/kg, HHV 55.5 MJ/kg; hydrogen: 120.0 / 141.8 MJ/kg
        let m = molar("CH4").unwrap();
        assert!((m.lhv / m.m - 50.01).abs() < 0.02, "{}", m.lhv / m.m);
        assert!((m.hhv / m.m - 55.50).abs() < 0.02, "{}", m.hhv / m.m);
        let h = molar("Hydrogen").unwrap();
        assert!((h.lhv / h.m - 119.96).abs() < 0.05 && (h.hhv / h.m - 141.79).abs() < 0.05);
        // inerts dilute the mixture: 90 % CH4 + 10 % N2
        let g = molar("Methane[0.9]&Nitrogen[0.1]").unwrap();
        assert!((g.lhv - 0.9 * m.lhv).abs() < 1e-9);
        assert!(molar("Methane&Ethane").is_err() && molar("Unobtainium").is_err());
    }
}
