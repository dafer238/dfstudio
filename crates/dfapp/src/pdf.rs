//! Minimal vector PDF writer (ISO 32000-1:2008): lines, polygons, circles
//! and text in the standard Helvetica fonts. No external dependency and no
//! LaTeX installation is needed to export a report.
//!
//! The API uses top-left page coordinates in points (1/72 in); they are
//! flipped to the PDF bottom-left system when written.

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
        self.pages.push(Page { h: self.height, ops: String::new() });
        self.pages.last_mut().unwrap()
    }
    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        let n_pages = self.pages.len();
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
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.0} {:.0}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
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
}
