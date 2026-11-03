//! Data for thermodynamic property diagrams (T-s, P-h, P-v, h-s, T-h ...):
//! saturation dome, isolines and process paths between states. All values
//! are SI; callers convert to display units.

use crate::props::{backend, coolprop_fluid, Key};

pub type Curve = Vec<[f64; 2]>;

#[derive(Debug, Clone, Default)]
pub struct Dome {
    pub liquid: Curve,
    pub vapor: Curve,
    /// critical point (x, y), if it can be evaluated
    pub critical: Option<[f64; 2]>,
}

fn xy(fluid: &str, inputs: [Key; 2], vals: [f64; 2], x: Key, y: Key) -> Option<[f64; 2]> {
    let r = backend::state_point(fluid, inputs, vals, &[x, y]).ok()?;
    (r[0].is_finite() && r[1].is_finite()).then_some([r[0], r[1]])
}

/// Saturation dome with `n` points per branch (denser near the critical point).
pub fn dome(fluid: &str, x: Key, y: Key, n: usize) -> Result<Dome, String> {
    let f = coolprop_fluid(fluid);
    let (tmin, tc, pc, _, _) = backend::limits(&f)?;
    let mut d = Dome::default();
    for i in 0..n {
        // cosine spacing clusters points near Tc
        let u = i as f64 / (n - 1) as f64;
        let t = tmin + (tc - tmin) * (1.0 - (1.0 - u).powi(2)) * 0.9999;
        if let Some(p) = xy(&f, [Key::Q, Key::T], [0.0, t], x, y) {
            d.liquid.push(p);
        }
        if let Some(p) = xy(&f, [Key::Q, Key::T], [1.0, t], x, y) {
            d.vapor.push(p);
        }
    }
    d.critical = xy(&f, [Key::P, Key::T], [pc, tc], x, y).or_else(|| {
        let a = d.liquid.last()?;
        let b = d.vapor.last()?;
        Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
    });
    Ok(d)
}

/// Line of constant `key = value` swept in temperature between `t0` and `t1` (K).
/// Two-phase portions are filled by quality sweeps so isobars are flat in T-s.
pub fn isoline(fluid: &str, key: Key, value: f64, x: Key, y: Key, t0: f64, t1: f64, n: usize) -> Curve {
    let f = coolprop_fluid(fluid);
    let mut c = Curve::new();
    if key == Key::T {
        // isotherm: sweep pressure logarithmically
        let Ok((_, _, _, _, pmax)) = backend::limits(&f) else { return c };
        let (p0, p1) = (1e3f64.ln(), pmax.min(1e9).ln());
        for i in 0..n {
            let p = (p0 + (p1 - p0) * i as f64 / (n - 1) as f64).exp();
            if let Some(pt) = xy(&f, [Key::P, Key::T], [p, value], x, y) {
                c.push(pt);
            }
        }
        return c;
    }
    let mut sat_inserted = false;
    let tsat = if key == Key::P { backend::state_point(&f, [Key::P, Key::Q], [value, 0.0], &[Key::T]).ok().map(|v| v[0]) } else { None };
    for i in 0..n {
        let t = t0 + (t1 - t0) * i as f64 / (n - 1) as f64;
        if let (Some(ts), false) = (tsat, sat_inserted) {
            if t > ts {
                for q in 0..=20 {
                    if let Some(pt) = xy(&f, [Key::P, Key::Q], [value, q as f64 / 20.0], x, y) {
                        c.push(pt);
                    }
                }
                sat_inserted = true;
            }
        }
        let inputs = [key, Key::T];
        if let Some(pt) = xy(&f, inputs, [value, t], x, y) {
            c.push(pt);
        }
    }
    c
}

/// Process path between two states at (nearly) constant pressure, swept in
/// entropy: gives the curved heat-exchanger paths of a cycle diagram.
pub fn isobaric_path(fluid: &str, p: f64, s0: f64, s1: f64, x: Key, y: Key, n: usize) -> Curve {
    let f = coolprop_fluid(fluid);
    (0..n).filter_map(|i| xy(&f, [Key::P, Key::S], [p, s0 + (s1 - s0) * i as f64 / (n - 1) as f64], x, y)).collect()
}

#[cfg(all(test, feature = "coolprop"))]
mod tests {
    use super::*;
    #[test]
    fn water_dome_ts() {
        let d = dome("Water", Key::S, Key::T, 60).unwrap();
        assert!(d.liquid.len() > 50 && d.vapor.len() > 50);
        let c = d.critical.unwrap();
        assert!((c[1] - 647.096).abs() < 0.1);
        let iso = isoline("Water", Key::P, 1e5, Key::S, Key::T, 300.0, 600.0, 50);
        assert!(iso.len() > 40);
    }
}
