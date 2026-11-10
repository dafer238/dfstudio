//! Lookup tables: `lookup`, `interpolate` (`1`, `2`, `3`) and
//! `interpolate2dm` on numeric tables read from CSV or text files, as the
//! EES functions of the same names (Klein & Nellis, 2012).
//!
//! A table file has one value per cell, separated by commas, semicolons or
//! tabs (with `;` or tabs a decimal comma is accepted). The first line may
//! name the columns, optionally with a unit in brackets (`T [C]`): values of
//! a column with a unit are converted to the model units, as literals are.

use crate::funcs::ExtFn;
use crate::units::{parse_unit, Dim, UnitSystem};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    /// Column names (`1`, `2`, ... without a header).
    pub cols: Vec<String>,
    /// Dimension of each column (None: no unit).
    pub dims: Vec<Option<Dim>>,
    /// Values by column, in model units.
    pub data: Vec<Vec<f64>>,
    /// The cells as written (for `interpolate2dm`, whose first row holds numbers).
    pub grid: Vec<Vec<f64>>,
}

fn split(line: &str, sep: char) -> Vec<String> {
    line.split(sep).map(|c| c.trim().trim_matches('"').to_string()).collect()
}

fn number(cell: &str, comma: bool) -> Option<f64> {
    let c = if comma { cell.replace(',', ".") } else { cell.to_string() };
    c.trim().parse().ok()
}

impl Table {
    /// Read a table from the text of a CSV / text file.
    pub fn parse(name: &str, text: &str, us: &UnitSystem) -> Result<Table, String> {
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let first = *lines.first().ok_or_else(|| format!("table '{name}' is empty"))?;
        let sep = if first.contains('\t') {
            '\t'
        } else if first.contains(';') {
            ';'
        } else {
            ','
        };
        let comma = sep != ',';
        let head = split(first, sep);
        let has_header = head.iter().any(|c| !c.is_empty() && number(c, comma).is_none());
        let ncol = head.len();
        let mut grid = Vec::new();
        for (k, l) in lines.iter().enumerate().skip(has_header as usize) {
            let cells = split(l, sep);
            let row: Vec<f64> = (0..ncol).map(|j| cells.get(j).and_then(|c| number(c, comma)).unwrap_or(f64::NAN)).collect();
            if cells.iter().any(|c| !c.is_empty() && number(c, comma).is_none()) {
                return Err(format!("table '{name}', line {}: a cell is not a number", k + 1));
            }
            grid.push(row);
        }
        let mut cols = Vec::new();
        let mut dims = Vec::new();
        let mut factor = Vec::new();
        for (j, h) in head.iter().enumerate() {
            if !has_header {
                cols.push((j + 1).to_string());
                dims.push(None);
                factor.push(None);
                continue;
            }
            let (n, u) = match h.split_once('[') {
                Some((n, u)) => (n.trim().to_string(), Some(u.trim_end_matches(']').trim().to_string())),
                None => (h.trim().to_string(), None),
            };
            let unit = match u.filter(|u| !u.is_empty() && u != "-") {
                Some(u) => Some(parse_unit(&u).map_err(|e| format!("table '{name}', column {n}: {e}"))?),
                None => None,
            };
            dims.push(unit.as_ref().map(|u| u.dim).filter(|d| !d.is_none()));
            factor.push(unit);
            cols.push(n);
        }
        let data = (0..ncol)
            .map(|j| {
                grid.iter()
                    .map(|r| match &factor[j] {
                        Some(u) if !u.dim.is_none() => u.convert(r[j], &us.unit_for(u.dim)),
                        Some(u) => u.to_si(r[j]),
                        None => r[j],
                    })
                    .collect()
            })
            .collect();
        Ok(Table { name: name.to_string(), cols, dims, data, grid })
    }

    pub fn rows(&self) -> usize {
        self.data.first().map_or(0, |c| c.len())
    }

    /// Column index of a name (any case) or a 1-based number.
    pub fn col(&self, c: &str) -> Result<usize, String> {
        let c = c.trim().trim_matches(['\'', '"']);
        if let Some(j) = self.cols.iter().position(|n| n.eq_ignore_ascii_case(c)) {
            return Ok(j);
        }
        match c.parse::<usize>() {
            Ok(j) if (1..=self.cols.len()).contains(&j) => Ok(j - 1),
            _ => Err(format!("table '{}' has no column '{c}' (columns: {})", self.name, self.cols.join(", "))),
        }
    }
}

/// Linear interpolation of `v` at a fractional 1-based position.
fn at_pos(v: &[f64], pos: f64, what: &str) -> Result<f64, String> {
    let n = v.len();
    if n == 0 || !pos.is_finite() || pos < 1.0 || pos > n as f64 {
        return Err(format!("{what} {pos} is outside 1..{n}"));
    }
    let i = (pos.floor() as usize).min(n - 1).max(1);
    let f = pos - i as f64;
    Ok(if f == 0.0 { v[i - 1] } else { v[i - 1] + f * (v[i] - v[i - 1]) })
}

/// Polynomial interpolation of order `k` (1 linear, 2 quadratic, 3 cubic) of
/// y(x) through the k + 1 points nearest to `x` (Lagrange form), x monotonic.
/// Outside the data the end polynomial extrapolates.
pub fn interp(xs: &[f64], ys: &[f64], x: f64, k: usize) -> Result<f64, String> {
    let pts: Vec<(f64, f64)> = xs.iter().zip(ys).filter(|(a, b)| a.is_finite() && b.is_finite()).map(|(a, b)| (*a, *b)).collect();
    let n = pts.len();
    if n < 2 {
        return Err("interpolation needs at least two rows".into());
    }
    let inc = pts[n - 1].0 > pts[0].0;
    if pts.windows(2).any(|w| if inc { w[1].0 <= w[0].0 } else { w[1].0 >= w[0].0 }) {
        return Err("the column of the known value must be strictly increasing or decreasing".into());
    }
    let m = (k + 1).min(n);
    // interval [i, i+1] holding x (the end one outside the data)
    let i = (0..n - 1).find(|&i| if inc { x <= pts[i + 1].0 } else { x >= pts[i + 1].0 }).unwrap_or(n - 2);
    // window of m points around the interval, toward the nearer side
    let nearer_left = (x - pts[i].0).abs() <= (pts[i + 1].0 - x).abs();
    let before = (m - 2) / 2 + usize::from(m % 2 == 1 && nearer_left);
    let start = i.saturating_sub(before).min(n - m);
    let w = &pts[start..start + m];
    let mut y = 0.0;
    for (a, (xa, ya)) in w.iter().enumerate() {
        let mut l = 1.0;
        for (b, (xb, _)) in w.iter().enumerate() {
            if a != b {
                l *= (x - xb) / (xa - xb);
            }
        }
        y += l * ya;
    }
    Ok(y)
}

pub enum Kind {
    /// lookup(table, row, col): fixed column (`Some`) or numeric column argument
    Lookup(Option<usize>),
    /// interpolate: (known column, wanted column, order, first row, last row)
    Interp(usize, usize, usize, usize, usize),
    /// interpolate2dm(table, x, y)
    Grid,
}

pub struct TableCall {
    pub label: String,
    pub table: Arc<Table>,
    pub kind: Kind,
}

impl ExtFn for TableCall {
    fn name(&self) -> &str {
        &self.label
    }
    fn dims(&self) -> Option<(Dim, Vec<Dim>)> {
        match self.kind {
            Kind::Interp(x, y, ..) => Some((self.table.dims[y]?, vec![self.table.dims[x]?])),
            Kind::Lookup(Some(c)) => Some((self.table.dims[c]?, vec![Dim::NONE])),
            _ => None,
        }
    }
    fn eval(&self, a: &[f64]) -> Result<f64, String> {
        let t = &self.table;
        let fail = |e: String| format!("{}: {e}", self.label);
        match self.kind {
            Kind::Lookup(Some(c)) => at_pos(&t.data[c], a[0], "row").map_err(fail),
            Kind::Lookup(None) => {
                // fractional column: between the values of the two columns at that row
                let row: Vec<f64> = t.data.iter().map(|c| at_pos(c, a[0], "row")).collect::<Result<_, _>>().map_err(fail)?;
                at_pos(&row, a[1], "column").map_err(fail)
            }
            Kind::Interp(x, y, k, first, last) => interp(&t.data[x][first..last], &t.data[y][first..last], a[0], k).map_err(fail),
            Kind::Grid => {
                // first row: x values; first column: y values
                let g = &t.grid;
                if g.len() < 3 || g[0].len() < 3 {
                    return Err(fail("interpolate2dm needs x values in the first row and y values in the first column".into()));
                }
                let xs = &g[0][1..];
                let ys: Vec<f64> = g[1..].iter().map(|r| r[0]).collect();
                // along x on each row, then along y
                let along_x: Vec<f64> = g[1..].iter().map(|r| interp(xs, &r[1..], a[0], 1)).collect::<Result<_, _>>().map_err(fail)?;
                interp(&ys, &along_x, a[1], 1).map_err(fail)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_orders() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x| x * x * x - 2.0 * x).collect();
        // exact for polynomials up to the order
        assert!((interp(&xs, &ys, 2.5, 3).unwrap() - (2.5f64.powi(3) - 5.0)).abs() < 1e-12);
        let q: Vec<f64> = xs.iter().map(|x| x * x).collect();
        assert!((interp(&xs, &q, 1.3, 2).unwrap() - 1.69).abs() < 1e-12);
        assert!((interp(&xs, &q, 1.5, 1).unwrap() - 2.5).abs() < 1e-12);
        // decreasing x, extrapolation
        let xd = [4.0, 3.0, 2.0, 1.0];
        let yd = [8.0, 6.0, 4.0, 2.0];
        assert!((interp(&xd, &yd, 5.0, 3).unwrap() - 10.0).abs() < 1e-12);
        assert!(interp(&[0.0, 1.0, 1.0], &[0.0, 1.0, 2.0], 0.5, 1).is_err());
    }

    #[test]
    fn tables_with_units_and_decimal_commas() {
        let us = UnitSystem::engineering();
        let t = Table::parse("t", "T [K]; P [kPa]; x\n300; 100; 0,5\n400; 200; 1,5\n", &us).unwrap();
        assert_eq!(t.cols, ["T", "P", "x"]);
        assert!((t.data[0][0] - 26.85).abs() < 1e-9 && (t.data[1][1] - 2.0).abs() < 1e-12 && t.data[2][1] == 1.5);
        let g = Table::parse("g", "0,1,2\n10,1,2\n20,3,4\n", &us).unwrap();
        assert_eq!(g.cols, ["1", "2", "3"]);
        let call = TableCall { label: "g".into(), table: Arc::new(g), kind: Kind::Grid };
        assert!((call.eval(&[1.5, 15.0]).unwrap() - 2.5).abs() < 1e-12);
    }
}
