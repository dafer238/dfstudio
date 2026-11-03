//! Decimal and list separators of a script.
//!
//! Scripts written on a Spanish (or other European) locale use
//! a decimal comma and separate arguments and list items with a semicolon:
//! `h = enthalpy(Water; T=T[1]; P=1,5)`. With the US convention the same
//! line is `h = enthalpy(Water, T=T[1], P=1.5)`.
//!
//! The lexer reads the US form. A script written in the comma form is
//! recognised by [`detect`] and translated by [`to_us`] before lexing. Both
//! conversions swap single ASCII bytes only, so every byte offset (and so
//! every span, diagnostic and editor position) is the same in both forms.
//! Comments (`"..."`, `{...}`, `// ...`) are never changed.
//!
//! Rules, outside comments:
//! * comma form → US: every `,` is a decimal separator → `.`; a `;` inside
//!   `(...)` or `[...]`, or on a `$directive`, `Duplicate` or `For` line, is a
//!   list separator → `,`; any other `;` separates equations and is kept;
//! * US → comma form: every `,` → `;`; a `.` that belongs to a number → `,`.
//!
//! The two are inverse of each other on any script that is valid in its form.

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Separators {
    /// `1.5` and `f(a, b)`
    #[default]
    Point,
    /// `1,5` and `f(a; b)` (EES on a Spanish / European locale)
    Comma,
}

/// Walk the code bytes of `src` (outside comments). `f(i, byte, depth,
/// list_line)` is called for each code byte; depth counts `(` and `[`.
fn walk(src: &str, mut f: impl FnMut(usize, u8, i32, bool)) {
    let b = src.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    let mut line_start = true;
    let mut list_line = false;
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'{' => {
                let mut lvl = 0;
                while i < b.len() {
                    match b[i] {
                        b'{' => lvl += 1,
                        b'}' => {
                            lvl -= 1;
                            if lvl == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'\n' => {
                if depth <= 0 {
                    line_start = true;
                    list_line = false;
                    depth = 0;
                }
            }
            b' ' | b'\t' | b'\r' => {}
            _ => {
                if line_start {
                    line_start = false;
                    let rest = &src[i..];
                    let word: String = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect::<String>().to_ascii_lowercase();
                    list_line = c == b'$' || word == "duplicate" || word == "for";
                }
                match c {
                    b'(' | b'[' => depth += 1,
                    b')' | b']' => depth -= 1,
                    _ => {}
                }
            }
        }
        f(i, c, depth, list_line);
        i += 1;
    }
}

/// Which separators the script uses. Only text that is invalid with points
/// is classed as comma form: a `;` inside parentheses or brackets, a `;` in
/// a directive, `Duplicate` or `For` line, or a comma between two digits
/// outside parentheses in an equation (`x = 1,5`).
pub fn detect(src: &str) -> Separators {
    let b = src.as_bytes();
    let mut comma = false;
    walk(src, |i, c, depth, list| {
        if comma {
            return;
        }
        if c == b';' && (depth > 0 || list) {
            comma = true;
        } else if c == b',' && depth == 0 && !list && i > 0 && b[i - 1].is_ascii_digit() && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            comma = true;
        }
    });
    if comma {
        Separators::Comma
    } else {
        Separators::Point
    }
}

/// Comma form → US form (see the module rules).
pub fn to_us(src: &str) -> String {
    let mut out = src.as_bytes().to_vec();
    walk(src, |i, c, depth, list| match c {
        b',' => out[i] = b'.',
        b';' if depth > 0 || list => out[i] = b',',
        _ => {}
    });
    // only ASCII bytes were swapped
    String::from_utf8(out).expect("ASCII swap keeps UTF-8")
}

/// US form → comma form (see the module rules).
pub fn to_comma(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let ident = |x: u8| x.is_ascii_alphanumeric() || x == b'_' || x == b']' || x == b')';
    walk(src, |i, c, _, _| match c {
        b',' => out[i] = b';',
        b'.' => {
            let prev = if i > 0 { b[i - 1] } else { b' ' };
            let next = b.get(i + 1).copied().unwrap_or(b' ');
            if prev == b'.' || next == b'.' {
                return;
            }
            // a digit before (1.5, 1.) or a digit after a non-name (.5)
            let num = if prev.is_ascii_digit() {
                // the digits before must start a number, not end a name (x1.5 cannot occur)
                let mut k = i;
                while k > 0 && b[k - 1].is_ascii_digit() {
                    k -= 1;
                }
                k == 0 || !ident(b[k - 1]) || b[k - 1].is_ascii_digit()
            } else {
                next.is_ascii_digit() && !ident(prev)
            };
            if num {
                out[i] = b',';
            }
        }
        _ => {}
    });
    String::from_utf8(out).expect("ASCII swap keeps UTF-8")
}

/// The script in US form: unchanged when it already is.
pub fn normalize(src: &str) -> Cow<'_, str> {
    match detect(src) {
        Separators::Point => Cow::Borrowed(src),
        Separators::Comma => Cow::Owned(to_us(src)),
    }
}

/// Convert `src` to `to`, whatever form it is in now.
pub fn convert(src: &str, to: Separators) -> String {
    match (detect(src), to) {
        (Separators::Comma, Separators::Point) => to_us(src),
        (Separators::Point, Separators::Comma) => to_comma(src),
        _ => src.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const US: &str = "\"comment, with 1.5; kept\"\nh[1] = enthalpy(Water, T=T[1], P=1.5e5) {kJ/kg, 2.5}\nx = .5 + 2.25*y[2,3]; z = 1\nDuplicate i = 1, N\n  a[i] = max(i, 0.1)\nEnd\n$Guess h[1] = 300.5, T[5] = 400\n$Bounds x = 0 .. 1\nw = 1. + 3 // note, 1.5\n";
    const ES: &str = "\"comment, with 1.5; kept\"\nh[1] = enthalpy(Water; T=T[1]; P=1,5e5) {kJ/kg, 2.5}\nx = ,5 + 2,25*y[2;3]; z = 1\nDuplicate i = 1; N\n  a[i] = max(i; 0,1)\nEnd\n$Guess h[1] = 300,5; T[5] = 400\n$Bounds x = 0 .. 1\nw = 1, + 3 // note, 1.5\n";

    #[test]
    fn detect_and_round_trip() {
        assert_eq!(detect(US), Separators::Point);
        assert_eq!(detect(ES), Separators::Comma);
        assert_eq!(to_comma(US), ES);
        assert_eq!(to_us(ES), US);
        assert_eq!(detect("x = 1,5"), Separators::Comma);
        assert_eq!(detect("x = f(1, 5)"), Separators::Point);
        assert_eq!(normalize(US), US);
    }

    #[test]
    fn comma_script_solves_like_the_point_one() {
        let es = "a = 1,5\nb = max(a; 2,25) + a*2\n";
        let m = crate::compile(es, &Default::default());
        let (_, sol) = crate::solve(&m, &Default::default());
        assert!(sol.converged, "{:?}", m.diagnostics);
        let b = m.vars.iter().position(|v| v.name == "b").unwrap();
        assert!((sol.x[b] - 5.25).abs() < 1e-12);
    }
}
