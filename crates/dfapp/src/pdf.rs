//! Minimal vector PDF writer (ISO 32000-1:2008): lines, polygons, circles
//! and text in the standard Helvetica fonts. No external dependency and no
//! LaTeX installation is needed to export a report.
//!
//! The API uses top-left page coordinates in points (1/72 in); they are
//! flipped to the PDF bottom-left system when written.

use crate::varname::{self, Seg};
use std::fmt::Write as _;

pub type Rgb = [u8; 3];

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126, from the
/// Adobe Core 14 AFM metrics.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667,
    722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556,
    278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Text width in points (bold is approximated as 5 % wider).
pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    let w: u32 = s.chars().map(|c| if (' '..='~').contains(&c) { HELV[c as usize - 32] as u32 } else { 556 }).sum();
    w as f32 / 1000.0 * size * if bold { 1.05 } else { 1.0 }
}

/// Encode text for a WinAnsi-encoded standard font as a PDF literal string.
fn pdf_string(s: &str) -> String {
    let mut o = String::from("(");
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                o.push('\\');
                o.push(c);
            }
            ' '..='~' => o.push(c),
            _ => {
                let code: Option<u8> = match c {
                    '°' => Some(0xB0),
                    '·' => Some(0xB7),
                    '×' => Some(0xD7),
                    '²' => Some(0xB2),
                    '³' => Some(0xB3),
                    'µ' | 'μ' => Some(0xB5),
                    '±' => Some(0xB1),
                    '–' => Some(0x96),
                    '—' => Some(0x97),
                    '…' => Some(0x85),
                    'é' => Some(0xE9),
                    'á' => Some(0xE1),
                    'í' => Some(0xED),
                    'ó' => Some(0xF3),
                    'ú' => Some(0xFA),
                    'ñ' => Some(0xF1),
                    _ => None,
                };
                match code {
                    Some(b) => {
                        let _ = write!(o, "\\{b:03o}");
                    }
                    None => {
                        // Greek letters commonly used in thermodynamics
                        let rep = match c {
                            'η' => "eta",
                            'Δ' => "Delta ",
                            'ε' => "eps",
                            'ρ' => "rho",
                            '→' => "->",
                            _ => "?",
                        };
                        o.push_str(rep);
                    }
                }
            }
        }
    }
    o.push(')');
    o
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Page {
    h: f32,
    ops: String,
    /// The page uses the Symbol font (Greek letters of typeset names).
    symbol: bool,
}

impl Page {
    fn y(&self, y: f32) -> f32 {
        self.h - y
    }
    fn stroke_col(&mut self, c: Rgb) {
        let _ = writeln!(self.ops, "{:.3} {:.3} {:.3} RG", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    }
    fn fill_col(&mut self, c: Rgb) {
        let _ = writeln!(self.ops, "{:.3} {:.3} {:.3} rg", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    }
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, c: Rgb) {
        self.polyline(&[a, b], false, width, Some(c), None, None);
    }
    /// Polyline / polygon with optional stroke, fill and dash pattern (on, off).
    pub fn polyline(&mut self, pts: &[[f32; 2]], closed: bool, width: f32, stroke: Option<Rgb>, fill: Option<Rgb>, dash: Option<(f32, f32)>) {
        if pts.len() < 2 || (stroke.is_none() && fill.is_none()) {
            return;
        }
        let _ = writeln!(self.ops, "q {width:.2} w 1 j 1 J");
        if let Some((on, off)) = dash {
            let _ = writeln!(self.ops, "[{on:.1} {off:.1}] 0 d");
        }
        if let Some(c) = stroke {
            self.stroke_col(c);
        }
        if let Some(c) = fill {
            self.fill_col(c);
        }
        let _ = writeln!(self.ops, "{:.2} {:.2} m", pts[0][0], self.y(pts[0][1]));
        for p in &pts[1..] {
            let _ = writeln!(self.ops, "{:.2} {:.2} l", p[0], self.y(p[1]));
        }
        let op = match (closed || fill.is_some(), stroke.is_some(), fill.is_some()) {
            (_, true, true) => "b",
            (true, true, false) => "s",
            (false, true, false) => "S",
            _ => "f",
        };
        let _ = writeln!(self.ops, "{op} Q");
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, stroke: Option<Rgb>, fill: Option<Rgb>) {
        self.polyline(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], true, width, stroke, fill, None);
    }
    /// Circle from four cubic Bézier arcs (k = 0.5523 for a quarter circle).
    pub fn circle(&mut self, c: [f32; 2], r: f32, width: f32, stroke: Option<Rgb>, fill: Option<Rgb>) {
        if stroke.is_none() && fill.is_none() {
            return;
        }
        let k = 0.552_284_8 * r;
        let (x, y) = (c[0], self.y(c[1]));
        let _ = writeln!(self.ops, "q {width:.2} w");
        if let Some(s) = stroke {
            self.stroke_col(s);
        }
        if let Some(f) = fill {
            self.fill_col(f);
        }
        let _ = writeln!(
            self.ops,
            "{:.2} {y:.2} m {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c",
            x + r,
            x + r,
            y + k,
            x + k,
            y + r,
            y + r,
            x - k,
            y + r,
            x - r,
            y + k,
            x - r,
            x - r,
            y - k,
            x - k,
            y - r,
            y - r,
            x + k,
            y - r,
            x + r,
            y - k,
            x + r
        );
        let op = match (stroke.is_some(), fill.is_some()) {
            (true, true) => "b",
            (true, false) => "s",
            _ => "f",
        };
        let _ = writeln!(self.ops, "{op} Q");
    }
    /// Text with its baseline at `y`.
    pub fn text(&mut self, x: f32, y: f32, size: f32, s: &str, bold: bool, align: Align, c: Rgb) {
        let w = text_width(s, size, bold);
        let x = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        self.fill_col(c);
        let _ = writeln!(self.ops, "BT /{} {size:.2} Tf {x:.2} {:.2} Td {} Tj ET", if bold { "F2" } else { "F1" }, self.y(y), pdf_string(s));
    }
}

/// Code and advance width (1/1000 em) of a character in the standard
/// Symbol font (its built-in encoding, Adobe Core 14 AFM metrics).
fn symbol(c: char) -> Option<(u8, u16)> {
    Some(match c {
        'α' => (b'a', 631),
        'β' => (b'b', 549),
        'γ' => (b'g', 411),
        'δ' => (b'd', 494),
        'ε' => (b'e', 439),
        'ζ' => (b'z', 494),
        'η' => (b'h', 603),
        'θ' => (b'q', 521),
        'ι' => (b'i', 329),
        'κ' => (b'k', 549),
        'λ' => (b'l', 549),
        'μ' => (b'm', 576),
        'ν' => (b'n', 521),
        'ξ' => (b'x', 493),
        'ο' => (b'o', 549),
        'π' => (b'p', 549),
        'ρ' => (b'r', 549),
        'σ' => (b's', 603),
        'τ' => (b't', 439),
        'υ' => (b'u', 576),
        'φ' => (b'f', 521),
        'χ' => (b'c', 549),
        'ψ' => (b'y', 686),
        'ω' => (b'w', 686),
        'Γ' => (b'G', 603),
        'Δ' => (b'D', 612),
        'Θ' => (b'Q', 741),
        'Λ' => (b'L', 686),
        'Ξ' => (b'X', 645),
        'Π' => (b'P', 768),
        'Σ' => (b'S', 592),
        'Υ' => (b'U', 690),
        'Φ' => (b'F', 763),
        'Ψ' => (b'Y', 795),
        'Ω' => (b'W', 768),
        '∞' => (0xA5, 713),
        '′' => (0xA2, 247),
        _ => return None,
    })
}

/// Width of text that may hold Greek letters (set in the Symbol font).
fn mixed_width(s: &str, size: f32, bold: bool) -> f32 {
    s.chars().map(|c| symbol(c).map(|(_, w)| w as f32 / 1000.0 * size).unwrap_or_else(|| text_width(&c.to_string(), size, bold))).sum()
}

/// Width of a line with variable names (typeset when names are formatted).
pub fn rich_width(segs: &[Seg], size: f32, bold: bool) -> f32 {
    if !varname::formatted() {
        return text_width(&varname::concat(segs), size, bold);
    }
    varname::typeset(segs, size, true, &|t, s| mixed_width(t, s, bold)).width
}

impl Page {
    /// Text with variable names typeset (Greek letters in the Symbol font,
    /// sub- and superscripts, accents); plain [`Page::text`] when names are
    /// shown as written.
    pub fn rich(&mut self, x: f32, y: f32, size: f32, segs: &[Seg], bold: bool, align: Align, c: Rgb) {
        if !varname::formatted() {
            return self.text(x, y, size, &varname::concat(segs), bold, align, c);
        }
        let ts = varname::typeset(segs, size, true, &|t, s| mixed_width(t, s, bold));
        let x0 = match align {
            Align::Left => x,
            Align::Center => x - ts.width / 2.0,
            Align::Right => x - ts.width,
        };
        self.fill_col(c);
        for r in &ts.runs {
            let mut cx = x0 + r.x;
            let by = y - r.rise;
            // chunks of one font
            let mut chunk = String::new();
            let mut chunk_sym = false;
            let flush = |page: &mut Page, chunk: &mut String, sym: bool, cx: &mut f32| {
                if chunk.is_empty() {
                    return;
                }
                if sym {
                    let mut lit = String::from("(");
                    for ch in chunk.chars() {
                        let b = symbol(ch).map(|s| s.0).unwrap_or(b'?');
                        if b.is_ascii_graphic() && !matches!(b, b'(' | b')' | b'\\') {
                            lit.push(b as char);
                        } else {
                            let _ = write!(lit, "\\{b:03o}");
                        }
                    }
                    lit.push(')');
                    page.symbol = true;
                    let _ = writeln!(page.ops, "BT /F3 {:.2} Tf {:.2} {:.2} Td {lit} Tj ET", r.size, *cx, page.y(by));
                } else {
                    let _ = writeln!(page.ops, "BT /{} {:.2} Tf {:.2} {:.2} Td {} Tj ET", if bold { "F2" } else { "F1" }, r.size, *cx, page.y(by), pdf_string(chunk));
                }
                *cx += mixed_width(chunk, r.size, bold);
                chunk.clear();
            };
            for ch in r.text.chars() {
                let sym = symbol(ch).is_some();
                if sym != chunk_sym {
                    flush(self, &mut chunk, chunk_sym, &mut cx);
                    chunk_sym = sym;
                }
                chunk.push(ch);
            }
            flush(self, &mut chunk, chunk_sym, &mut cx);
        }
        for m in &ts.marks {
            let s = m.size;
            let (a, b) = (x0 + m.x0, x0 + m.x1);
            let mid = (a + b) / 2.0;
            let my = y - m.y;
            let w = 0.06 * s;
            match m.accent {
                varname::Accent::Dot => self.circle([mid, my - 0.07 * s], 0.06 * s, 0.0, None, Some(c)),
                varname::Accent::DDot => {
                    for dx in [-0.13, 0.13] {
                        self.circle([mid + dx * s, my - 0.07 * s], 0.055 * s, 0.0, None, Some(c));
                    }
                }
                varname::Accent::Bar => self.line([a + 0.04 * s, my - 0.04 * s], [b - 0.04 * s, my - 0.04 * s], w, c),
                varname::Accent::Hat => self.polyline(&[[mid - 0.17 * s, my], [mid, my - 0.15 * s], [mid + 0.17 * s, my]], false, w, Some(c), None, None),
                varname::Accent::Tilde => {
                    let pts: Vec<[f32; 2]> = (0..=12)
                        .map(|i| {
                            let t = i as f32 / 12.0;
                            [mid - 0.2 * s + 0.4 * s * t, my - 0.07 * s - 0.05 * s * (t * std::f32::consts::TAU).sin()]
                        })
                        .collect();
                    self.polyline(&pts, false, w, Some(c), None, None);
                }
                varname::Accent::Vec => {
                    let (p, q) = ([a, my - 0.07 * s], [b.max(a + 0.4 * s), my - 0.07 * s]);
                    self.line(p, q, w, c);
                    self.polyline(&[[q[0] - 0.13 * s, q[1] - 0.08 * s], q, [q[0] - 0.13 * s, q[1] + 0.08 * s]], false, w, Some(c), None, None);
                }
            }
        }
    }
}

pub struct Pdf {
    pub width: f32,
    pub height: f32,
    pages: Vec<Page>,
    title: String,
}

impl Pdf {
    /// A4 landscape is 842 × 595 pt.
    pub fn new(width: f32, height: f32, title: &str) -> Self {
        Pdf { width, height, pages: Vec::new(), title: title.into() }
    }
    pub fn page(&mut self) -> &mut Page {
        self.pages.push(Page { h: self.height, ops: String::new(), symbol: false });
        self.pages.last_mut().unwrap()
    }
    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        let n_pages = self.pages.len();
        // the Symbol font, when a page uses it, is the last object
        let symbol = self.pages.iter().any(|p| p.symbol);
        let f3 = if symbol { format!(" /F3 {} 0 R", 6 + 2 * n_pages) } else { String::new() };
        // object numbers: 1 catalog, 2 pages, 3 F1, 4 F2, 5 info, then (page, content) pairs
        let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
            offsets.push(out.len());
            let n = offsets.len();
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
        let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", 6 + 2 * i)).collect();
        obj(&mut out, format!("<< /Type /Pages /Kids [{}] /Count {n_pages} >>", kids.join(" ")).as_bytes());
        obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
        obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
        obj(&mut out, format!("<< /Title {} /Producer (DeltaF Studio) >>", pdf_string(&self.title)).as_bytes());
        for (i, p) in self.pages.iter().enumerate() {
            obj(
                &mut out,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.0} {:.0}] /Resources << /Font << /F1 3 0 R /F2 4 0 R{f3} >> >> /Contents {} 0 R >>",
                    self.width,
                    self.height,
                    7 + 2 * i
                )
                .as_bytes(),
            );
            let mut body = format!("<< /Length {} >>\nstream\n", p.ops.len()).into_bytes();
            body.extend_from_slice(p.ops.as_bytes());
            body.extend_from_slice(b"\nendstream");
            obj(&mut out, &body);
        }
        if symbol {
            obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>");
        }
        let xref = out.len();
        let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
        for o in &offsets {
            let _ = writeln!(x, "{o:010} 00000 n ");
        }
        let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1);
        out.extend_from_slice(x.as_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_structure() {
        let mut pdf = Pdf::new(842.0, 595.0, "Test (1)");
        let p = pdf.page();
        p.text(40.0, 40.0, 14.0, "Rankine cycle: T = 550 °C, η = 0.37", true, Align::Left, [0, 0, 0]);
        p.polyline(&[[10.0, 10.0], [100.0, 50.0], [50.0, 90.0]], true, 1.0, Some([0, 0, 0]), Some([200, 100, 100]), None);
        p.circle([300.0, 300.0], 20.0, 1.0, Some([0, 0, 0]), None);
        pdf.page().line([0.0, 0.0], [10.0, 10.0], 1.0, [0, 0, 0]);
        let b = pdf.finish();
        let s = String::from_utf8_lossy(&b);
        assert!(s.starts_with("%PDF-1.4") && s.trim_end().ends_with("%%EOF"));
        assert!(s.contains("/Count 2") && s.contains("\\260") && s.contains("eta") && s.contains("(Test \\(1\\))"));
        // every xref offset points at the start of "n 0 obj"
        let xref_at: usize = s.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let table = String::from_utf8_lossy(&b[xref_at..]).to_string();
        for (k, line) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(b[off..].starts_with(format!("{} 0 obj", k + 1).as_bytes()), "object {}", k + 1);
        }
        assert!((text_width("Hello", 10.0, false) - 22.78).abs() < 0.01);
    }

    #[test]
    fn typeset_names_use_the_symbol_font() {
        // names are formatted by default
        let mut pdf = Pdf::new(200.0, 100.0, "t");
        pdf.page().text(10.0, 10.0, 10.0, "plain", false, Align::Left, [0, 0, 0]);
        let plain = String::from_utf8_lossy(&pdf.finish()).to_string();
        assert!(!plain.contains("/Symbol"), "no Symbol font unless a page uses it");
        let mut pdf = Pdf::new(200.0, 100.0, "t");
        pdf.page().rich(10.0, 50.0, 10.0, &[Seg::Name("eta_th"), Seg::Text(" = 0.37")], false, Align::Left, [0, 0, 0]);
        let b = pdf.finish();
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("/F3 8 0 R") && s.contains("8 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>"));
        assert!(s.contains("/F3 10.00 Tf") && s.contains("(h) Tj") && s.contains("(th) Tj") && s.contains("( = 0.37) Tj"));
        // the subscript is smaller and lower
        assert!(s.contains("/F1 7.60 Tf"));
    }
}
