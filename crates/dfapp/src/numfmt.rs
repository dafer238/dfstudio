//! Numbers in result tables: a fixed number of decimal places (View >
//! Numbers), scientific notation where fixed notation would hide the value,
//! and columns aligned on the decimal point.
//!
//! With N decimal places a value is written `{:.N}` when it keeps at least
//! two significant digits that way, |x| >= 10^(1-N), and below 1e7;
//! otherwise it is written `{:.Ne}` (`1.234e-8`, `1.235e9`), which a script
//! reads back as it is. Exact zero is `0.000`.

use std::sync::atomic::{AtomicU8, Ordering};

/// Default number of decimal places.
pub const DEFAULT_DECIMALS: u8 = 3;
/// Most decimal places offered.
pub const MAX_DECIMALS: u8 = 10;
/// From this magnitude on, values are written in scientific notation.
const BIG: f64 = 1e7;

static DECIMALS: AtomicU8 = AtomicU8::new(DEFAULT_DECIMALS);

/// Set the number of decimal places (each frame, from the settings).
pub fn set_decimals(n: u8) {
    DECIMALS.store(n.min(MAX_DECIMALS), Ordering::Relaxed);
}

pub fn decimals() -> u8 {
    DECIMALS.load(Ordering::Relaxed)
}

/// `x` with the current number of decimal places.
pub fn fmt(x: f64) -> String {
    fmt_n(x, decimals() as usize)
}

/// `x` with `n` decimal places, or in scientific notation with `n`
/// decimals in the mantissa when fixed notation would keep fewer than two
/// significant digits or the value is 1e7 or more. `—` if not finite.
pub fn fmt_n(x: f64, n: usize) -> String {
    if !x.is_finite() {
        return "—".into();
    }
    if x == 0.0 {
        return format!("{:.n$}", 0.0);
    }
    let a = x.abs();
    if a >= BIG || a < 10f64.powi(1 - n as i32) {
        return format!("{x:.n$e}");
    }
    let s = format!("{x:.n$}");
    // a value that rounds to zero at the last place keeps no minus sign
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

/// Where a cell's integer part ends: the length in characters of the
/// leading sign and digits. `None` for text that is not a number (`liquid`,
/// `sat`), which is left as it is. `—` (no value) counts as an integer part.
fn head_len(s: &str) -> Option<usize> {
    if s == "—" {
        return Some(1);
    }
    let t = s.strip_prefix('+').unwrap_or(s);
    t.parse::<f64>().ok()?;
    Some(s.chars().take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '+').count())
}

/// The cells of a column padded with spaces so that, in a monospace font,
/// their decimal points (or the end of the integer part) line up and the
/// numbers end at the same place: `300.150`, `  1.234e-8`. Text cells are
/// kept as they are.
pub fn align<S: AsRef<str>>(cells: &[S]) -> Vec<String> {
    let parts: Vec<Option<(usize, usize)>> = cells
        .iter()
        .map(|c| {
            let s = c.as_ref();
            head_len(s).map(|h| (h, s.chars().count() - h))
        })
        .collect();
    let head = parts.iter().flatten().map(|p| p.0).max().unwrap_or(0);
    let tail = parts.iter().flatten().map(|p| p.1).max().unwrap_or(0);
    cells
        .iter()
        .zip(&parts)
        .map(|(c, p)| match p {
            Some((h, t)) => format!("{}{}{}", " ".repeat(head - h), c.as_ref(), " ".repeat(tail - t)),
            None => c.as_ref().to_string(),
        })
        .collect()
}

/// Split a cell into its integer part and the rest (decimal point,
/// decimals, exponent), for aligning in a proportional font (the PDF).
/// `None` for text that is not a number.
pub fn split(s: &str) -> Option<(&str, &str)> {
    let h = head_len(s)?;
    let i = s.char_indices().nth(h).map(|(i, _)| i).unwrap_or(s.len());
    Some(s.split_at(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_scientific() {
        assert_eq!(fmt_n(300.15, 3), "300.150");
        assert_eq!(fmt_n(0.5, 3), "0.500");
        assert_eq!(fmt_n(-12.0, 3), "-12.000");
        assert_eq!(fmt_n(0.0, 3), "0.000");
        assert_eq!(fmt_n(-0.0, 3), "0.000");
        assert_eq!(fmt_n(101325.0, 3), "101325.000");
        // two significant digits kept in fixed notation: 0.01 and above
        assert_eq!(fmt_n(0.012, 3), "0.012");
        assert_eq!(fmt_n(0.00999, 3), "9.990e-3");
        assert_eq!(fmt_n(0.00000001234, 3), "1.234e-8");
        assert_eq!(fmt_n(0.00000001234, 4), "1.2340e-8");
        assert_eq!(fmt_n(-0.00000001234, 3), "-1.234e-8");
        assert_eq!(fmt_n(1.23456e9, 3), "1.235e9");
        assert_eq!(fmt_n(9_999_999.0, 3), "9999999.000");
        assert_eq!(fmt_n(1e7, 3), "1.000e7");
        // no decimals
        assert_eq!(fmt_n(300.15, 0), "300");
        assert_eq!(fmt_n(25.0, 0), "25");
        assert_eq!(fmt_n(0.5, 0), "5e-1");
        assert_eq!(fmt_n(f64::NAN, 3), "—");
        // a script reads the scientific form back
        for s in ["1.234e-8", "1.235e9", "-1.234e-8"] {
            assert!(s.parse::<f64>().is_ok());
        }
    }

    #[test]
    fn columns_align_on_the_point() {
        let c = align(&["300.150", "1.234e-8", "-12.000", "—", "liquid"]);
        assert_eq!(c, vec!["300.150   ", "  1.234e-8", "-12.000   ", "  —       ", "liquid"]);
        let pos: Vec<_> = c[..3].iter().map(|s| s.find('.').unwrap()).collect();
        assert!(pos.iter().all(|p| *p == pos[0]));
        assert_eq!(align(&["+12.5", "-3.25"]), vec!["+12.5 ", " -3.25"]);
        assert_eq!(align::<&str>(&[]), Vec::<String>::new());
        assert_eq!(split("300.150"), Some(("300", ".150")));
        assert_eq!(split("—"), Some(("—", "")));
        assert_eq!(split("sat"), None);
    }
}
