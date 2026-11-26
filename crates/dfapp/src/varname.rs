//! Formatted variable names, following the conventions of EES's Formatted
//! Equations window:
//!
//! - `_` starts a subscript and `|` a superscript; both attach to the base
//!   symbol (`h|o_f` is h with superscript ° and subscript f, stacked).
//!   Several parts of one kind at the same level are separated by commas
//!   (`eta_is_t` is η with subscript "is,t").
//! - A doubled mark goes one level deeper and attaches to the previous
//!   script (`T_c__1`: subscript c, which has its own subscript 1); three
//!   marks go two levels deeper, and so on.
//! - A part that is exactly the name of a Greek letter becomes the letter
//!   (`eta` → η, `Delta` → Δ). Capitals that look like Latin letters
//!   (`Eta`, `Nu`, `Rho`, ...) are left as written, so `Nu` stays Nusselt's.
//! - `dot`, `ddot`, `bar`, `hat`, `tilde` and `vec` decorate the symbol they
//!   follow (`m_dot` → ṁ); `star` adds a superscript `*`, `prime` a ′ and
//!   `infinity` is ∞.
//! - Array indices are subscripts: `T[1]` → T₁, `T_in[1,2]` → T with
//!   subscript "in,1,2".
//! - A lone `o` as superscript is the degree sign (`h|o_f` → h°_f).
//!
//! The parser and the layout are independent of the drawing back end: the
//! egui widgets below and the PDF report both use [`typeset`].

use eframe::egui::{self, Color32, FontFamily, FontId, Galley, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ------------------------------------------------------------------ mode

/// How variable names are shown (View > Variable names).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum NameStyle {
    /// Typeset with Greek letters, subscripts, superscripts and accents.
    #[default]
    Formatted,
    /// As written in the script.
    Raw,
}

/// How units are shown (View > Units).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum UnitStyle {
    /// With exponents instead of `/`: `kJ/kg-K` as kJ kg⁻¹ K⁻¹.
    #[default]
    Exponents,
    /// As written (`kJ/kg-K`).
    Raw,
}

static RAW: AtomicBool = AtomicBool::new(false);
static BOTH: AtomicBool = AtomicBool::new(false);
static UNITS_RAW: AtomicBool = AtomicBool::new(false);

/// Apply the View menu choices: the name style, whether the Variables
/// sidebar and the Solution variables add a column with the raw name, and
/// the unit style.
pub fn set_style(style: NameStyle, both: bool, units: UnitStyle) {
    RAW.store(style == NameStyle::Raw, Ordering::Relaxed);
    BOTH.store(both, Ordering::Relaxed);
    UNITS_RAW.store(units == UnitStyle::Raw, Ordering::Relaxed);
}

/// Units are shown with exponents.
pub fn unit_exp() -> bool {
    !UNITS_RAW.load(Ordering::Relaxed)
}

/// What [`typeset`] formats: names, units, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fmt {
    pub names: bool,
    pub units: bool,
}

impl Fmt {
    /// The current View menu choices.
    pub fn now() -> Fmt {
        Fmt { names: formatted(), units: unit_exp() }
    }
}

/// Names are typeset.
pub fn formatted() -> bool {
    !RAW.load(Ordering::Relaxed)
}

/// The Variables sidebar and the Solution variables show the raw name too
/// (only meaningful while names are typeset).
pub fn both() -> bool {
    formatted() && BOTH.load(Ordering::Relaxed)
}

// ------------------------------------------------------------------ parsing

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Accent {
    Dot,
    DDot,
    Bar,
    Hat,
    Tilde,
    Vec,
}

/// A symbol with its scripts.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Sym {
    pub text: String,
    pub accent: Option<Accent>,
    pub sub: Vec<Sym>,
    pub sup: Vec<Sym>,
}

impl Sym {
    fn leaf(text: String) -> Sym {
        Sym { text, ..Default::default() }
    }
    fn node(&mut self, path: &[(bool, usize)]) -> &mut Sym {
        match path.split_first() {
            None => self,
            Some((&(sub, i), rest)) => (if sub { &mut self.sub[i] } else { &mut self.sup[i] }).node(rest),
        }
    }
    /// Nothing to typeset: plain text.
    pub fn is_plain(&self) -> bool {
        self.accent.is_none() && self.sub.is_empty() && self.sup.is_empty()
    }
}

const GREEK: [(&str, char, Option<char>); 24] = [
    ("alpha", 'α', None),
    ("beta", 'β', None),
    ("gamma", 'γ', Some('Γ')),
    ("delta", 'δ', Some('Δ')),
    ("epsilon", 'ε', None),
    ("zeta", 'ζ', None),
    ("eta", 'η', None),
    ("theta", 'θ', Some('Θ')),
    ("iota", 'ι', None),
    ("kappa", 'κ', None),
    ("lambda", 'λ', Some('Λ')),
    ("mu", 'μ', None),
    ("nu", 'ν', None),
    ("xi", 'ξ', Some('Ξ')),
    ("omicron", 'ο', None),
    ("pi", 'π', Some('Π')),
    ("rho", 'ρ', None),
    ("sigma", 'σ', Some('Σ')),
    ("tau", 'τ', None),
    ("upsilon", 'υ', Some('Υ')),
    ("phi", 'φ', Some('Φ')),
    ("chi", 'χ', None),
    ("psi", 'ψ', Some('Ψ')),
    ("omega", 'ω', Some('Ω')),
];

/// The Greek letter a word names: `eta` → η, `Delta` → Δ. Capitals that
/// look like Latin letters are not converted.
pub fn greek(word: &str) -> Option<char> {
    let first_upper = word.chars().next()?.is_ascii_uppercase();
    let lower = word.to_ascii_lowercase();
    let (_, lo, up) = GREEK.iter().find(|g| g.0 == lower)?;
    if word == lower {
        Some(*lo)
    } else if first_upper && word[1..] == lower[1..] {
        *up
    } else {
        None
    }
}

fn word(w: &str) -> String {
    if w == "infinity" {
        return "∞".into();
    }
    greek(w).map(String::from).unwrap_or_else(|| w.to_string())
}

fn accent(w: &str) -> Option<Accent> {
    Some(match w.to_ascii_lowercase().as_str() {
        "dot" => Accent::Dot,
        "ddot" => Accent::DDot,
        "bar" => Accent::Bar,
        "hat" => Accent::Hat,
        "tilde" => Accent::Tilde,
        "vec" => Accent::Vec,
        _ => return None,
    })
}

/// Parse a variable name (as written, e.g. `eta_is_t`, `h|o_f`, `T[3]`).
/// `None` when the name has no structure to show (empty parts, a leading
/// mark): it is then shown as written.
pub fn parse(raw: &str) -> Option<Sym> {
    let (head, index) = match raw.find('[') {
        Some(i) if raw.ends_with(']') => (&raw[..i], Some(raw[i + 1..raw.len() - 1].trim())),
        Some(_) => return None,
        None => (raw, None),
    };
    // tokens: base, then (mark, depth, text)
    let mut toks: Vec<(bool, usize, &str)> = Vec::new();
    let b = head.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] != b'_' && b[i] != b'|' {
        i += 1;
    }
    let base = &head[..i];
    if base.is_empty() {
        return None;
    }
    while i < b.len() {
        let sub = b[i] == b'_';
        let mut depth = 0;
        while i < b.len() && (b[i] == b'_' || b[i] == b'|') {
            depth += 1;
            i += 1;
        }
        let s = i;
        while i < b.len() && b[i] != b'_' && b[i] != b'|' {
            i += 1;
        }
        if s == i {
            return None;
        }
        toks.push((sub, depth, &head[s..i]));
    }
    let mut root = Sym::leaf(word(base));
    // path to the last part added at each level (level 0: the root)
    let mut levels: Vec<Vec<(bool, usize)>> = vec![Vec::new()];
    for (sub, depth, t) in toks {
        let target = levels[(depth - 1).min(levels.len() - 1)].clone();
        let node = root.node(&target);
        if let Some(a) = accent(t) {
            node.accent = Some(a);
            continue;
        }
        if t.eq_ignore_ascii_case("prime") {
            node.text.push('′');
            continue;
        }
        let (sub, part) = if t.eq_ignore_ascii_case("star") {
            (false, Sym::leaf("*".into()))
        } else if !sub && t == "o" {
            (false, Sym::leaf("°".into()))
        } else {
            (sub, Sym::leaf(word(t)))
        };
        let list = if sub { &mut node.sub } else { &mut node.sup };
        list.push(part);
        let mut path = target;
        path.push((sub, list.len() - 1));
        let lvl = path.len();
        levels.truncate(lvl);
        levels.push(path);
    }
    if let Some(ix) = index {
        if ix.is_empty() {
            return None;
        }
        root.sub.push(Sym::leaf(ix.replace(' ', "")));
    }
    Some(root)
}

// ------------------------------------------------------------------ layout

/// A piece of text placed by [`typeset`]; `x` from the left edge, `rise`
/// of its baseline above the line's baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub x: f32,
    pub rise: f32,
    pub size: f32,
}

/// An accent over the symbol spanning `x0..x1`: `y` is the height of its
/// lowest point above the line's baseline, `size` the symbol's font size.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub accent: Accent,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub size: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typeset {
    pub runs: Vec<Run>,
    pub marks: Vec<Mark>,
    pub width: f32,
}

/// Text in a line: a variable name (typeset when names are formatted) or
/// plain text.
#[derive(Clone, Copy, Debug)]
pub enum Seg<'a> {
    Name(&'a str),
    Text(&'a str),
    /// A unit symbol (`kJ/kg-K`): with exponents when units are formatted.
    Unit(&'a str),
}

/// The factors of a unit with their exponents, `kJ/kg-K` → kJ¹ kg⁻¹ K⁻¹,
/// following the unit parser's convention (everything after the first `/`
/// is in the denominator; `-`, `*`, `·`, `.` and blanks separate factors;
/// `m^-1` and `m3` are powers). `None` when there is nothing to rewrite
/// (`kW`, `C`, `-`): the unit is then shown as written.
pub fn unit_factors(unit: &str) -> Option<Vec<(String, i32)>> {
    let t = unit.trim();
    if t.is_empty() || t == "-" {
        return None;
    }
    let cleaned: String = t.replace("**", "^").replace("^-", "^~").chars().filter(|c| *c != '(' && *c != ')').collect();
    let (num, den) = cleaned.split_once('/').unwrap_or((&cleaned, ""));
    let mut out = Vec::new();
    let mut structured = t.contains(['/', '^', '*', '·']) || t[1..].contains('-');
    // trailing digits of a unit name are a power: m3, s2
    let digit_power = |f: &str, structured: &mut bool| -> Option<(String, i32)> {
        let digits = f.len() - f.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && digits < f.len() && f.chars().any(|c| c.is_alphabetic()) {
            *structured = true;
            Some((f[..f.len() - digits].to_string(), f[f.len() - digits..].parse::<i32>().ok()?))
        } else {
            Some((f.to_string(), 1))
        }
    };
    for (part, sign) in [(num, 1), (den, -1)] {
        for f in part.split(['-', '*', '·', ' ', '/', '.']) {
            let f = f.trim();
            if f.is_empty() || f == "1" {
                continue;
            }
            let (head, exp) = match f.split_once('^') {
                Some((b, e)) => (b, Some(e.replace('~', "-").parse::<i32>().ok()?)),
                None => (f, None),
            };
            if head.is_empty() {
                return None;
            }
            // unit names written together (kgK): one factor each, the
            // exponent with the last one (as the unit parser reads them)
            let pieces = dfcore::units::split_joined(head).unwrap_or_else(|| vec![head]);
            if pieces.len() > 1 {
                structured = true;
            }
            let last = pieces.len() - 1;
            for (k, p) in pieces.into_iter().enumerate() {
                let (base, e) = match exp {
                    Some(e) if k == last => (p.to_string(), e),
                    _ => digit_power(p, &mut structured)?,
                };
                out.push((base, e * sign));
            }
        }
    }
    (structured && !out.is_empty()).then_some(out)
}

/// The segments change when typeset with `fmt` (else the plain text is
/// shown, exactly as before formatting existed).
pub fn needs(segs: &[Seg], fmt: Fmt) -> bool {
    segs.iter().any(|s| match s {
        Seg::Name(n) => fmt.names && parse(n).is_some_and(|p| !p.is_plain() || p.text != *n),
        Seg::Unit(u) => fmt.units && unit_factors(u).is_some(),
        Seg::Text(_) => false,
    })
}

impl<'a> Seg<'a> {
    /// A name when `name`, else text.
    pub fn of(t: &'a str, name: bool) -> Seg<'a> {
        if name {
            Seg::Name(t)
        } else {
            Seg::Text(t)
        }
    }
}

/// Scale of a script relative to its symbol.
const SCRIPT: f32 = 0.76;
/// Scripts never get smaller than this fraction of the line's size.
const MIN_SCALE: f32 = 0.62;

/// Height of the top of `text` above its baseline, as a fraction of the
/// font size (for placing accents).
fn top(text: &str) -> f32 {
    let tall = text.chars().any(|c| c.is_uppercase() || c.is_ascii_digit() || "bdfhklitβδζθλξ∞".contains(c));
    if tall {
        0.74
    } else {
        0.54
    }
}

fn set_sym(s: &Sym, x: f32, rise: f32, size: f32, root: f32, w: &dyn Fn(&str, f32) -> f32, out: &mut Typeset) -> f32 {
    let bw = w(&s.text, size);
    out.runs.push(Run { text: s.text.clone(), x, rise, size });
    if let Some(a) = s.accent {
        out.marks.push(Mark { accent: a, x0: x, x1: x + bw, y: rise + top(&s.text) * size + 0.08 * size, size });
    }
    let x1 = x + bw + 0.04 * size;
    let ss = (size * SCRIPT).max(root * MIN_SCALE);
    let list = |parts: &[Sym], rise: f32, out: &mut Typeset| -> f32 {
        let mut x = x1;
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                out.runs.push(Run { text: ",".into(), x, rise, size: ss });
                x += w(",", ss) + 0.04 * ss;
            }
            x = set_sym(p, x, rise, ss, root, w, out);
        }
        x
    };
    let e1 = list(&s.sub, rise - 0.24 * size, out);
    let e2 = list(&s.sup, rise + 0.42 * size, out);
    if s.sub.is_empty() && s.sup.is_empty() {
        x + bw
    } else {
        e1.max(e2)
    }
}

/// Lay out a line of segments at font `size`; `w(text, size)` is the
/// advance width of a text. Names and units are typeset as `fmt` says.
pub fn typeset(segs: &[Seg], size: f32, fmt: Fmt, w: &dyn Fn(&str, f32) -> f32) -> Typeset {
    let mut out = Typeset::default();
    let mut x = 0.0;
    let mut plain = String::new();
    let flush = |plain: &mut String, x: &mut f32, out: &mut Typeset| {
        if !plain.is_empty() {
            out.runs.push(Run { text: std::mem::take(plain), x: *x, rise: 0.0, size });
            *x += w(&out.runs.last().unwrap().text, size);
        }
    };
    for s in segs {
        match *s {
            Seg::Unit(u) if fmt.units => match unit_factors(u) {
                Some(fs) => {
                    flush(&mut plain, &mut x, &mut out);
                    for (i, (base, e)) in fs.iter().enumerate() {
                        if i > 0 {
                            // thin space (SI brochure); an advance, as the
                            // default font has no U+2009 glyph
                            x += 0.18 * size;
                        }
                        let sup = if *e == 1 { Vec::new() } else { vec![Sym::leaf(e.to_string().replace('-', "\u{2212}"))] };
                        x = set_sym(&Sym { text: base.clone(), sup, ..Default::default() }, x, 0.0, size, size, w, &mut out);
                    }
                }
                None => plain.push_str(u),
            },
            Seg::Name(n) if fmt.names => match parse(n) {
                Some(sym) if !sym.is_plain() => {
                    flush(&mut plain, &mut x, &mut out);
                    x = set_sym(&sym, x, 0.0, size, size, w, &mut out);
                }
                Some(sym) => plain.push_str(&sym.text),
                None => plain.push_str(n),
            },
            Seg::Name(t) | Seg::Text(t) | Seg::Unit(t) => plain.push_str(t),
        }
    }
    flush(&mut plain, &mut x, &mut out);
    out.width = x;
    out
}

// ------------------------------------------------------------------ egui

/// A typeset line ready to paint: galleys at offsets from the top-left
/// corner of `size`.
pub struct Shaped {
    galleys: Vec<(Vec2, Arc<Galley>)>,
    marks: Vec<Mark>,
    /// Baseline below the top edge.
    baseline: f32,
    pub size: Vec2,
}

fn galley_baseline(g: &Galley) -> f32 {
    g.rows.first().and_then(|r| r.row.glyphs.first().map(|gl| r.pos.y + gl.pos.y)).unwrap_or(g.size().y * 0.8)
}

/// Lay out `segs` with `font` (names typeset when formatting is on).
pub fn shape(painter: &egui::Painter, segs: &[Seg], font: &FontId, color: Color32) -> Shaped {
    shape_with(painter, segs, font, color, Fmt::now())
}

fn shape_with(painter: &egui::Painter, segs: &[Seg], font: &FontId, color: Color32, fmt: Fmt) -> Shaped {
    let fam = if fmt.names { FontFamily::Proportional } else { font.family.clone() };
    let prime_ok = painter.fonts_mut(|f| f.has_glyph(&FontId::new(font.size, fam.clone()), '′'));
    let fix = |t: &str| if prime_ok { t.to_string() } else { t.replace('′', "'") };
    let lay = |t: &str, size: f32| painter.layout_no_wrap(fix(t), FontId::new(size, fam.clone()), color);
    let ts = typeset(segs, font.size, fmt, &|t, s| lay(t, s).size().x);
    // vertical extent relative to the baseline (y down)
    let mut placed = Vec::new();
    let (mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY);
    for r in &ts.runs {
        let g = lay(&r.text, r.size);
        let top = -r.rise - galley_baseline(&g);
        y0 = y0.min(top);
        y1 = y1.max(top + g.size().y);
        placed.push((Vec2::new(r.x, top), g));
    }
    for m in &ts.marks {
        y0 = y0.min(-(m.y + 0.2 * m.size));
    }
    if placed.is_empty() {
        let g = lay("", font.size);
        let b = galley_baseline(&g);
        y0 = -b;
        y1 = g.size().y - b;
        placed.push((Vec2::new(0.0, -b), g));
    }
    let baseline = -y0;
    Shaped { galleys: placed.into_iter().map(|(o, g)| (o + Vec2::new(0.0, baseline), g)).collect(), marks: ts.marks, baseline, size: Vec2::new(ts.width, y1 - y0) }
}

impl Shaped {
    /// Size of a plain text line in `font` (what a raw label takes), and
    /// where this line's top goes within it so that the baselines match;
    /// scripts may stick out a little, as in typeset text.
    pub fn in_line(&self, painter: &egui::Painter, font: &FontId) -> (Vec2, f32) {
        let g = painter.layout_no_wrap("Hg".into(), font.clone(), Color32::PLACEHOLDER);
        (Vec2::new(self.size.x, g.size().y), galley_baseline(&g) - self.baseline)
    }
    /// Paint with the top-left corner at `pos`.
    pub fn paint(&self, painter: &egui::Painter, pos: Pos2, color: Color32) {
        for (o, g) in &self.galleys {
            painter.galley_with_override_text_color(pos + *o, g.clone(), color);
        }
        let base = pos.y + self.baseline;
        for m in &self.marks {
            paint_mark(painter, m, pos.x, base, color);
        }
    }
    /// Paint aligned at `pos` like [`egui::Painter::text`]; returns the rect.
    pub fn paint_at(&self, painter: &egui::Painter, pos: Pos2, align: egui::Align2, color: Color32) -> Rect {
        let r = align.anchor_size(pos, self.size);
        self.paint(painter, r.min, color);
        r
    }
}

fn paint_mark(painter: &egui::Painter, m: &Mark, x: f32, base: f32, color: Color32) {
    let mut mesh = egui::Mesh::default();
    mark_mesh(&mut mesh, m, x, base);
    for v in &mut mesh.vertices {
        v.color = color;
    }
    painter.add(Shape::mesh(mesh));
}

/// A variable name as a label: `rich` (the raw name with its styling)
/// unchanged when names are raw, typeset otherwise.
pub fn label(ui: &mut egui::Ui, raw: &str, rich: RichText, color: Option<Color32>) -> Response {
    line_label(ui, &[Seg::Name(raw)], rich, color)
}

/// A line mixing names and text as a label (see [`label`]); `rich` carries
/// the styling and is shown as is in raw mode; `color` is the typeset
/// text's colour (default: the text colour).
pub fn line_label(ui: &mut egui::Ui, segs: &[Seg], rich: RichText, color: Option<Color32>) -> Response {
    sized_label(ui, segs, rich, color, egui::TextStyle::Body)
}

/// A unit as a small, weak label (column headers): `kJ kg⁻¹` with exponents.
pub fn unit_label(ui: &mut egui::Ui, unit: &str) -> Response {
    let c = ui.visuals().weak_text_color();
    sized_label(ui, &[Seg::Unit(unit)], RichText::new(unit).small().weak(), Some(c), egui::TextStyle::Small)
}

/// The closed label of a unit drop-down (the open list stays as written).
pub fn unit_text(ui: &egui::Ui, unit: &str) -> egui::WidgetText {
    if !needs(&[Seg::Unit(unit)], Fmt::now()) {
        return unit.into();
    }
    widget_text(ui.ctx(), &[Seg::Unit(unit)], egui::TextStyle::Button.resolve(ui.style()))
}

/// A variable name as a heading (see [`label`]).
pub fn heading(ui: &mut egui::Ui, raw: &str, rich: RichText) -> Response {
    let c = ui.visuals().strong_text_color();
    sized_label(ui, &[Seg::Name(raw)], rich, Some(c), egui::TextStyle::Heading)
}

fn sized_label(ui: &mut egui::Ui, segs: &[Seg], rich: RichText, color: Option<Color32>, style: egui::TextStyle) -> Response {
    if !needs(segs, Fmt::now()) {
        return ui.label(rich);
    }
    let color = color.unwrap_or_else(|| ui.visuals().text_color());
    let font = rich_font(ui, &rich, style);
    let shaped = shape(ui.painter(), segs, &font, color);
    let (size, dy) = shaped.in_line(ui.painter(), &font);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        shaped.paint(ui.painter(), rect.min + Vec2::new(0.0, dy), color);
    }
    resp
}

/// Font of a typeset line in a widget: proportional at the style's size
/// when names are typeset, else the raw text's own font (monospace stays
/// monospace when only units change).
fn rich_font(ui: &egui::Ui, rich: &RichText, style: egui::TextStyle) -> FontId {
    if formatted() {
        return FontId::proportional(style.resolve(ui.style()).size);
    }
    let w: egui::WidgetText = rich.clone().into();
    let job = w.into_layout_job(ui.style(), egui::FontSelection::Style(style.clone()), egui::Align::Min);
    job.sections.first().map(|s| s.format.font_id.clone()).unwrap_or_else(|| style.resolve(ui.style()))
}

/// Font of a line's rows (as [`shape`] lays it out).
fn line_font(font: &FontId) -> FontId {
    if formatted() {
        FontId::proportional(font.size)
    } else {
        font.clone()
    }
}

/// A line with names as a hyperlink-styled button, like
/// [`egui::Ui::link`] (used as is in raw mode).
pub fn link(ui: &mut egui::Ui, segs: &[Seg], rich: RichText) -> Response {
    if !needs(segs, Fmt::now()) {
        return ui.link(rich);
    }
    let color = ui.visuals().hyperlink_color;
    let font = rich_font(ui, &rich, egui::TextStyle::Body);
    let shaped = shape(ui.painter(), segs, &font, color);
    let (size, dy) = shaped.in_line(ui.painter(), &font);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        shaped.paint(ui.painter(), rect.min + Vec2::new(0.0, dy), color);
        if resp.hovered() {
            ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, color));
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Paint a line with names like [`egui::Painter::text`] (which is used as
/// is in raw mode); returns the text's rect.
pub fn paint(painter: &egui::Painter, pos: Pos2, align: egui::Align2, segs: &[Seg], font: FontId, color: Color32) -> Rect {
    if !needs(segs, Fmt::now()) {
        return painter.text(pos, align, concat(segs), font, color);
    }
    shape(painter, segs, &font, color).paint_at(painter, pos, align, color)
}

/// Size of a line painted by [`paint`].
pub fn measure(painter: &egui::Painter, segs: &[Seg], font: FontId) -> Vec2 {
    if !needs(segs, Fmt::now()) {
        return painter.layout_no_wrap(concat(segs), font, Color32::PLACEHOLDER).size();
    }
    shape(painter, segs, &font, Color32::PLACEHOLDER).size
}

/// Segments of a label line `name = value unit`: the name is typeset,
/// the rest is text. Lines of another form are plain text.
pub fn line_segs(line: &str) -> Vec<Seg<'_>> {
    match line.split_once(" = ") {
        Some((n, _))
            if !n.is_empty()
                && n.chars().all(|c| c.is_alphanumeric() || "_|[], ".contains(c))
                && !n.contains("  ")
                && !n.starts_with(' ')
                && (!n.contains(' ') || n.contains('[')) =>
        {
            let mut v = vec![Seg::Name(n)];
            v.extend(value_segs(&line[n.len()..]));
            v
        }
        _ => vec![Seg::Text(line)],
    }
}

/// A name followed by ` = value unit`.
pub fn with_value<'a>(name: &'a str, rest: &'a str) -> Vec<Seg<'a>> {
    let mut v = vec![Seg::Name(name)];
    v.extend(value_segs(rest));
    v
}

/// ` = 550 kJ/kg` (or `550 kJ/kg`): the number as text, then the unit.
pub fn value_segs(s: &str) -> Vec<Seg<'_>> {
    let lead = s.len() - s.trim_start_matches([' ', '=']).len();
    let rest = &s[lead..];
    match rest.split_once(' ') {
        Some((num, unit)) if !unit.is_empty() && num.parse::<f64>().is_ok() && !unit.contains(' ') => {
            let cut = lead + num.len() + 1;
            vec![Seg::Text(&s[..cut]), Seg::Unit(&s[cut..])]
        }
        _ => vec![Seg::Text(s)],
    }
}

/// Paint lines (`name = value` lines get the name typeset) like
/// [`egui::Painter::text`] with the lines joined by newlines, which is what
/// raw mode does; each line is aligned horizontally as `align` says.
pub fn paint_lines(painter: &egui::Painter, pos: Pos2, align: egui::Align2, lines: &[String], font: FontId, color: Color32) -> Rect {
    if !lines.iter().any(|l| needs(&line_segs(l), Fmt::now())) {
        return painter.text(pos, align, lines.join("\n"), font, color);
    }
    let shaped: Vec<Shaped> = lines.iter().map(|l| shape(painter, &line_segs(l), &font, color)).collect();
    let size = lines_size(painter, &shaped, &font);
    let r = align.anchor_size(pos, size);
    let rh = painter.fonts_mut(|f| f.row_height(&line_font(&font)));
    let mut y = r.min.y;
    for sh in &shaped {
        let x = match align.x() {
            egui::Align::Min => r.min.x,
            egui::Align::Center => r.center().x - sh.size.x / 2.0,
            egui::Align::Max => r.max.x - sh.size.x,
        };
        let h = sh.size.y.max(rh);
        sh.paint(painter, Pos2::new(x, y + (h - sh.size.y) / 2.0), color);
        y += h;
    }
    r
}

fn lines_size(painter: &egui::Painter, shaped: &[Shaped], font: &FontId) -> Vec2 {
    let rh = painter.fonts_mut(|f| f.row_height(&line_font(font)));
    Vec2::new(shaped.iter().map(|s| s.size.x).fold(0.0, f32::max), shaped.iter().map(|s| s.size.y.max(rh)).sum())
}

/// Size of the block [`paint_lines`] paints.
pub fn measure_lines(painter: &egui::Painter, lines: &[String], font: FontId) -> Vec2 {
    if !lines.iter().any(|l| needs(&line_segs(l), Fmt::now())) {
        return painter.layout_no_wrap(lines.join("\n"), font, Color32::PLACEHOLDER).size();
    }
    let shaped: Vec<Shaped> = lines.iter().map(|l| shape(painter, &line_segs(l), &font, Color32::PLACEHOLDER)).collect();
    lines_size(painter, &shaped, &font)
}

/// Segments of a column header or series name: `name`, `name [unit]` or
/// `name (...)` get the name typeset; anything else is plain text.
pub fn head_segs(s: &str) -> Vec<Seg<'_>> {
    let ident = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || "_|[],#$".contains(c));
    let end = s.find(" [").or_else(|| s.find(" (")).unwrap_or(s.len());
    let mut v = if ident(&s[..end]) { vec![Seg::Name(&s[..end])] } else { vec![Seg::Text(&s[..end])] };
    let rest = &s[end..];
    // " [unit]" and what follows
    match rest.strip_prefix(" [").and_then(|r| r.find(']').map(|k| (k, r))) {
        Some((k, r)) => {
            v.push(Seg::Text(" ["));
            v.push(Seg::Unit(&r[..k]));
            v.push(Seg::Text(&r[k..]));
        }
        None if !rest.is_empty() => v.push(Seg::Text(rest)),
        None => {}
    }
    v.retain(|x| !matches!(x, Seg::Text("")));
    v
}

/// Text for a widget that takes [`egui::WidgetText`] (plot axis labels):
/// the plain text in raw mode, else one galley holding the typeset line
/// (in the widget's text colour, accents included).
pub fn widget_text(ctx: &egui::Context, segs: &[Seg], font: FontId) -> egui::WidgetText {
    if !needs(segs, Fmt::now()) {
        return concat(segs).into();
    }
    let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
    egui::WidgetText::Galley(galley(&painter, segs, &font))
}

/// One galley holding a typeset line: the rows of every run's galley at
/// their places, and a row with the accents' mesh. Its colour is the
/// painter's fallback colour (the text colour of the widget showing it).
pub fn galley(painter: &egui::Painter, segs: &[Seg], font: &FontId) -> Arc<Galley> {
    let sh = shape_with(painter, segs, font, Color32::PLACEHOLDER, Fmt::now());
    let first = sh.galleys[0].1.clone();
    let mut rows = Vec::new();
    let mut bounds = Rect::NOTHING;
    let (mut nv, mut ni) = (0, 0);
    for (o, g) in &sh.galleys {
        for r in &g.rows {
            rows.push(egui::epaint::text::PlacedRow { pos: r.pos + *o, row: r.row.clone(), ends_with_newline: false });
            bounds = bounds.union(r.row.visuals.mesh_bounds.translate(r.pos.to_vec2() + *o));
            nv += r.row.visuals.mesh.vertices.len();
            ni += r.row.visuals.mesh.indices.len();
        }
    }
    if !sh.marks.is_empty() {
        let mut mesh = egui::Mesh::default();
        for m in &sh.marks {
            mark_mesh(&mut mesh, m, 0.0, sh.baseline);
        }
        let mut row = (*first.rows[0].row).clone();
        row.glyphs.clear();
        let mb = Rect::from_points(&mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
        row.visuals = egui::epaint::text::RowVisuals { glyph_index_start: 0, glyph_vertex_range: 0..mesh.vertices.len(), mesh_bounds: mb, mesh };
        nv += row.visuals.mesh.vertices.len();
        ni += row.visuals.mesh.indices.len();
        bounds = bounds.union(mb);
        rows.push(egui::epaint::text::PlacedRow { pos: Pos2::ZERO, row: Arc::new(row), ends_with_newline: false });
    }
    let mut g = (*first).clone();
    g.rows = rows;
    g.elided = false;
    g.rect = Rect::from_min_size(Pos2::ZERO, sh.size);
    g.mesh_bounds = bounds;
    g.num_vertices = nv;
    g.num_indices = ni;
    Arc::new(g)
}

/// Triangles of an accent (vertex colour: placeholder) for [`galley`].
fn mark_mesh(mesh: &mut egui::Mesh, m: &Mark, x: f32, base: f32) {
    let col = Color32::PLACEHOLDER;
    let s = m.size;
    let (x0, x1) = (x + m.x0, x + m.x1);
    let mid = (x0 + x1) / 2.0;
    let y = base - m.y;
    let w = (0.07 * s).max(1.0);
    let disk = |mesh: &mut egui::Mesh, c: Pos2, r: f32| {
        let i0 = mesh.vertices.len() as u32;
        mesh.colored_vertex(c, col);
        for k in 0..=12 {
            let t = k as f32 / 12.0 * std::f32::consts::TAU;
            mesh.colored_vertex(c + Vec2::new(t.cos(), t.sin()) * r, col);
            if k > 0 {
                mesh.add_triangle(i0, i0 + k, i0 + k + 1);
            }
        }
    };
    let seg = |mesh: &mut egui::Mesh, a: Pos2, b: Pos2| {
        let n = (b - a).normalized().rot90() * (w / 2.0);
        let i = mesh.vertices.len() as u32;
        for p in [a + n, a - n, b - n, b + n] {
            mesh.colored_vertex(p, col);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    };
    let poly = |mesh: &mut egui::Mesh, pts: &[Pos2]| {
        for p in pts.windows(2) {
            seg(mesh, p[0], p[1]);
        }
    };
    match m.accent {
        Accent::Dot => disk(mesh, Pos2::new(mid, y - 0.07 * s), (0.07 * s).max(1.1)),
        Accent::DDot => {
            for dx in [-0.14, 0.14] {
                disk(mesh, Pos2::new(mid + dx * s, y - 0.07 * s), (0.065 * s).max(1.0));
            }
        }
        Accent::Bar => seg(mesh, Pos2::new(x0 + 0.04 * s, y - 0.04 * s), Pos2::new(x1 - 0.04 * s, y - 0.04 * s)),
        Accent::Hat => poly(mesh, &[Pos2::new(mid - 0.18 * s, y), Pos2::new(mid, y - 0.16 * s), Pos2::new(mid + 0.18 * s, y)]),
        Accent::Tilde => {
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let t = i as f32 / 12.0;
                    Pos2::new(mid - 0.2 * s + 0.4 * s * t, y - 0.07 * s - 0.05 * s * (t * std::f32::consts::TAU).sin())
                })
                .collect();
            poly(mesh, &pts);
        }
        Accent::Vec => {
            let (a, b) = (Pos2::new(x0, y - 0.07 * s), Pos2::new(x1.max(x0 + 0.4 * s), y - 0.07 * s));
            seg(mesh, a, b);
            poly(mesh, &[Pos2::new(b.x - 0.14 * s, b.y - 0.09 * s), b, Pos2::new(b.x - 0.14 * s, b.y + 0.09 * s)]);
        }
    }
}

/// The segments as plain text.
pub fn concat(segs: &[Seg]) -> String {
    segs.iter()
        .map(|s| match s {
            Seg::Name(t) | Seg::Text(t) | Seg::Unit(t) => *t,
        })
        .collect()
}

/// A clickable, selectable variable name, like
/// [`egui::Ui::selectable_label`] (which is used as is in raw mode).
pub fn selectable(ui: &mut egui::Ui, selected: bool, raw: &str, rich: RichText) -> Response {
    if !formatted() {
        return ui.selectable_label(selected, rich);
    }
    let font = FontId::proportional(egui::TextStyle::Body.resolve(ui.style()).size);
    let shaped = shape(ui.painter(), &[Seg::Name(raw)], &font, Color32::PLACEHOLDER);
    let pad = ui.spacing().button_padding;
    let (size, dy) = shaped.in_line(ui.painter(), &font);
    let (rect, resp) = ui.allocate_exact_size(size + 2.0 * pad, Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&resp, selected);
        if selected || resp.hovered() || resp.highlighted() || resp.has_focus() {
            ui.painter().rect(rect.expand(v.expansion), v.corner_radius, v.weak_bg_fill, v.bg_stroke, egui::StrokeKind::Inside);
        }
        shaped.paint(ui.painter(), rect.min + pad + Vec2::new(0.0, dy), v.text_color());
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Sym {
        Sym::leaf(t.into())
    }

    #[test]
    fn greek_letters() {
        assert_eq!(greek("eta"), Some('η'));
        assert_eq!(greek("Delta"), Some('Δ'));
        assert_eq!(greek("DELTA"), None);
        assert_eq!(greek("Nu"), None, "capital Nu looks like N: keep Nusselt's number");
        assert_eq!(greek("etap"), None);
        assert_eq!(parse("eta").unwrap(), s("η"));
    }

    #[test]
    fn subscripts_same_level_and_nested() {
        let p = parse("eta_is_t").unwrap();
        assert_eq!(p.text, "η");
        assert_eq!(p.sub, vec![s("is"), s("t")]);
        let p = parse("T_c__1").unwrap();
        assert_eq!(p.sub.len(), 1);
        assert_eq!(p.sub[0].text, "c");
        assert_eq!(p.sub[0].sub, vec![s("1")]);
        // back to level one after a nested part
        let p = parse("T_c__1_out").unwrap();
        assert_eq!(p.sub.len(), 2);
        assert_eq!(p.sub[1], s("out"));
    }

    #[test]
    fn superscripts_attach_to_the_base() {
        let p = parse("h|o_f").unwrap();
        assert_eq!(p.text, "h");
        assert_eq!(p.sup, vec![s("°")]);
        assert_eq!(p.sub, vec![s("f")]);
        let p = parse("x|2").unwrap();
        assert_eq!(p.sup, vec![s("2")]);
    }

    #[test]
    fn accents_and_specials() {
        let p = parse("m_dot").unwrap();
        assert_eq!(p.accent, Some(Accent::Dot));
        assert!(p.sub.is_empty());
        let p = parse("m_dot_in").unwrap();
        assert_eq!(p.accent, Some(Accent::Dot));
        assert_eq!(p.sub, vec![s("in")]);
        assert_eq!(parse("T_infinity").unwrap().sub, vec![s("∞")]);
        assert_eq!(parse("T_star").unwrap().sup, vec![s("*")]);
        assert_eq!(parse("x_prime").unwrap().text, "x′");
        // accent of a nested part
        let p = parse("W_c__dot").unwrap();
        assert_eq!(p.sub[0].accent, Some(Accent::Dot));
    }

    #[test]
    fn arrays() {
        let p = parse("T[1]").unwrap();
        assert_eq!(p.sub, vec![s("1")]);
        let p = parse("T_in[1, 2]").unwrap();
        assert_eq!(p.sub, vec![s("in"), s("1,2")]);
    }

    #[test]
    fn malformed_names_stay_raw() {
        assert_eq!(parse("_x"), None);
        assert_eq!(parse("x_"), None);
        assert_eq!(parse("a__"), None);
        assert!(parse("abc").unwrap().is_plain());
    }

    #[test]
    fn layout_is_raw_when_off() {
        let w = |t: &str, s: f32| t.chars().count() as f32 * s * 0.5;
        let t = typeset(&[Seg::Name("eta_t"), Seg::Text(" = 0.3")], 10.0, Fmt { names: false, units: false }, &w);
        assert_eq!(t.runs.len(), 1);
        assert_eq!(t.runs[0].text, "eta_t = 0.3");
        let t = typeset(&[Seg::Name("eta_t"), Seg::Text(" = 0.3")], 10.0, Fmt { names: true, units: true }, &w);
        let texts: Vec<&str> = t.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["η", "t", " = 0.3"]);
        assert!(t.runs[1].rise < 0.0 && t.runs[1].size < 10.0);
        // stacked scripts start at the same x
        let t = typeset(&[Seg::Name("h|o_f")], 10.0, Fmt { names: true, units: true }, &w);
        assert_eq!(t.runs[1].x, t.runs[2].x);
    }

    #[test]
    fn label_lines() {
        let n = |l: &str| match line_segs(l).first() {
            Some(Seg::Name(n)) => Some(n.to_string()),
            _ => None,
        };
        assert_eq!(n("eta_s = 0.85"), Some("eta_s".into()));
        assert_eq!(n("T[3] = 550 C"), Some("T[3]".into()));
        assert_eq!(n("Turbine"), None);
        assert_eq!(n("Heat input = 3 MW"), None);
        let h = |l: &str| match head_segs(l).first() {
            Some(Seg::Name(n)) => Some(n.to_string()),
            _ => None,
        };
        assert_eq!(h("eta_th [-]"), Some("eta_th".into()));
        assert_eq!(h("T[3]"), Some("T[3]".into()));
        assert_eq!(h("W_net (right axis)"), Some("W_net".into()));
        assert_eq!(h("Turbine power"), None);
        let u = |l: &'static str| head_segs(l).iter().find_map(|s| if let Seg::Unit(u) = s { Some(u.to_string()) } else { None });
        assert_eq!(u("w_spec [kJ/kg] (right axis)"), Some("kJ/kg".into()));
        assert_eq!(concat(&head_segs("w_spec [kJ/kg] (right axis)")), "w_spec [kJ/kg] (right axis)");
        assert_eq!(concat(&line_segs("T = 550 kJ/kg")), "T = 550 kJ/kg");
        assert!(matches!(line_segs("T = 550 kJ/kg")[..], [Seg::Name("T"), Seg::Text(" = 550 "), Seg::Unit("kJ/kg")]));
        assert!(matches!(line_segs("eta = 0.85")[..], [Seg::Name("eta"), Seg::Text(" = 0.85")]));
    }

    #[test]
    fn unit_exponents() {
        let f = |u: &str| unit_factors(u).map(|v| v.iter().map(|(b, e)| format!("{b}{e}")).collect::<Vec<_>>().join(" "));
        assert_eq!(f("kJ/kg"), Some("kJ1 kg-1".into()));
        assert_eq!(f("kJ/kg-K"), Some("kJ1 kg-1 K-1".into()));
        assert_eq!(f("W/(m^2*K)"), Some("W1 m-2 K-1".into()));
        assert_eq!(f("kg/m^3"), Some("kg1 m-3".into()));
        assert_eq!(f("m3/kg"), Some("m3 kg-1".into()));
        assert_eq!(f("1/s"), Some("s-1".into()));
        assert_eq!(f("m^-1"), Some("m-1".into()));
        assert_eq!(f("kg-m"), Some("kg1 m1".into()));
        assert_eq!(f("W/m**2-K"), Some("W1 m-2 K-1".into()));
        assert_eq!(f("m**-1"), Some("m-1".into()));
        assert_eq!(f("kJ/kgK"), Some("kJ1 kg-1 K-1".into()));
        assert_eq!(f("kJ/kg/K"), Some("kJ1 kg-1 K-1".into()));
        assert_eq!(f("W/m2K"), Some("W1 m-2 K-1".into()));
        for plain in ["kW", "C", "-", "", "bar", "dK"] {
            assert_eq!(f(plain), None, "{plain}");
        }
        let w = |t: &str, s: f32| t.chars().count() as f32 * s * 0.5;
        let both = Fmt { names: true, units: true };
        let t = typeset(&[Seg::Unit("kJ/kg")], 10.0, both, &w);
        let texts: Vec<&str> = t.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["kJ", "kg", "\u{2212}1"]);
        assert!(t.runs[2].rise > 0.0);
        let t = typeset(&[Seg::Unit("kJ/kg")], 10.0, Fmt { names: true, units: false }, &w);
        assert_eq!(t.runs[0].text, "kJ/kg");
        assert!(!needs(&[Seg::Unit("kW")], both));
        assert!(needs(&[Seg::Unit("kW/K")], both));
    }
}
