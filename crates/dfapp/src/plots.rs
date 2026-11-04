//! Plots tab: 2D XY plots of tables and arrays, thermodynamic property
//! diagrams (T-s, P-h, P-v, h-s, T-h) with the saturation dome and the cycle
//! states, and 3D surface / scatter plots. Every plot can be exported as a
//! self-contained Python (matplotlib) script for publication-quality output.

use crate::analysis::Analysis;
use crate::editor::fmt_num;
use crate::studies_ui::{conv, ParamTable, StudiesUi};
use dfcore::diagrams;
use dfcore::props::{backend, coolprop_fluid, Key, PROPERTY_FUNCTIONS};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};
use egui_plot::{AxisHints, GridMark, HPlacement, HoverPosition, Legend, Line, LineStyle, MarkerShape, Plot, PlotPoint, PlotUi, Points, Text, VLine};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::RangeInclusive;
use std::sync::Arc;

// ------------------------------------------------------------------ definitions

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum PlotKind {
    XY,
    Property,
    Surface,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Source {
    Parametric,
    Arrays,
    OptHistory,
}

impl Source {
    const ALL: [Source; 3] = [Source::Parametric, Source::Arrays, Source::OptHistory];
    fn label(self) -> &'static str {
        match self {
            Source::Parametric => "Parametric table",
            Source::Arrays => "Arrays (by index)",
            Source::OptHistory => "Optimisation history",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum LineKind {
    Solid,
    Dashed,
    Dotted,
    None,
}

impl LineKind {
    const ALL: [LineKind; 4] = [LineKind::Solid, LineKind::Dashed, LineKind::Dotted, LineKind::None];
    fn egui(self) -> LineStyle {
        match self {
            LineKind::Dashed => LineStyle::Dashed { length: 8.0 },
            LineKind::Dotted => LineStyle::Dotted { spacing: 5.0 },
            _ => LineStyle::Solid,
        }
    }
    fn py(self) -> &'static str {
        match self {
            LineKind::Solid => "-",
            LineKind::Dashed => "--",
            LineKind::Dotted => ":",
            LineKind::None => "none",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Marker {
    None,
    Circle,
    Square,
    Diamond,
    Up,
    Down,
    Cross,
    Plus,
}

impl Marker {
    const ALL: [Marker; 8] = [Marker::None, Marker::Circle, Marker::Square, Marker::Diamond, Marker::Up, Marker::Down, Marker::Cross, Marker::Plus];
    fn egui(self) -> Option<MarkerShape> {
        Some(match self {
            Marker::None => return None,
            Marker::Circle => MarkerShape::Circle,
            Marker::Square => MarkerShape::Square,
            Marker::Diamond => MarkerShape::Diamond,
            Marker::Up => MarkerShape::Up,
            Marker::Down => MarkerShape::Down,
            Marker::Cross => MarkerShape::Cross,
            Marker::Plus => MarkerShape::Plus,
        })
    }
    fn py(self) -> &'static str {
        match self {
            Marker::None => "None",
            Marker::Circle => "'o'",
            Marker::Square => "'s'",
            Marker::Diamond => "'D'",
            Marker::Up => "'^'",
            Marker::Down => "'v'",
            Marker::Cross => "'x'",
            Marker::Plus => "'+'",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Diagram {
    Ts,
    Ph,
    Pv,
    Hs,
    Th,
}

impl Diagram {
    const ALL: [Diagram; 5] = [Diagram::Ts, Diagram::Ph, Diagram::Pv, Diagram::Hs, Diagram::Th];
    fn label(self) -> &'static str {
        match self {
            Diagram::Ts => "T-s",
            Diagram::Ph => "P-h",
            Diagram::Pv => "P-v",
            Diagram::Hs => "h-s (Mollier)",
            Diagram::Th => "T-h",
        }
    }
    /// (x, y) properties.
    fn axes(self) -> (Key, Key) {
        match self {
            Diagram::Ts => (Key::S, Key::T),
            Diagram::Ph => (Key::H, Key::P),
            Diagram::Pv => (Key::V, Key::P),
            Diagram::Hs => (Key::S, Key::H),
            Diagram::Th => (Key::H, Key::T),
        }
    }
    /// Conventional logarithmic axes (x, y).
    pub(crate) fn default_log(self) -> (bool, bool) {
        match self {
            Diagram::Ph => (false, true),
            Diagram::Pv => (true, true),
            _ => (false, false),
        }
    }
}

fn key_name(k: Key) -> &'static str {
    match k {
        Key::T => "T",
        Key::P => "P",
        Key::H => "h",
        Key::S => "s",
        Key::V => "v",
        _ => "?",
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Series {
    pub y: String,
    pub label: String,
    /// Line colour (also the markers' when `marker_color` is unset).
    pub color: [u8; 3],
    /// Marker colour, when different from the line's (absent in files
    /// before 0.3.0).
    #[serde(default)]
    pub marker_color: Option<[u8; 3]>,
    pub width: f32,
    pub line: LineKind,
    pub marker: Marker,
    pub size: f32,
    /// Plot against the secondary (right) y axis.
    #[serde(default)]
    pub y2: bool,
}

/// Colour cycle (Okabe & Ito 2008, colour-blind safe).
const PALETTE: [[u8; 3]; 8] = [[0, 114, 178], [213, 94, 0], [0, 158, 115], [204, 121, 167], [230, 159, 0], [86, 180, 233], [240, 228, 66], [0, 0, 0]];

impl Series {
    pub(crate) fn new(y: &str, k: usize) -> Self {
        Series {
            y: y.into(),
            label: String::new(),
            color: PALETTE[k % PALETTE.len()],
            marker_color: None,
            width: 1.8,
            line: LineKind::Solid,
            marker: Marker::Circle,
            size: 3.0,
            y2: false,
        }
    }
    fn color32(&self) -> Color32 {
        Color32::from_rgb(self.color[0], self.color[1], self.color[2])
    }
    /// Colour of the markers.
    fn marker_rgb(&self) -> [u8; 3] {
        self.marker_color.unwrap_or(self.color)
    }
    fn marker32(&self) -> Color32 {
        let [r, g, b] = self.marker_rgb();
        Color32::from_rgb(r, g, b)
    }
    /// Colour that stands for the series (legend, hover): the line's, or the
    /// markers' when there is no line.
    fn key32(&self) -> Color32 {
        if self.line == LineKind::None {
            self.marker32()
        } else {
            self.color32()
        }
    }
}

/// Automatic label of a y axis from its columns: the single column's header,
/// or the names separated by spaces followed by the unit when they share it
/// (`W_net W_c [kW]`), else each name with its unit (`eta_th [-] W_net [MW]`).
fn axis_label(cols: &[&Column]) -> String {
    match cols {
        [] => String::new(),
        [one] => one.header(),
        _ => {
            let unit = |c: &Column| if c.unit == "-" { String::new() } else { c.unit.clone() };
            let shared = cols.iter().all(|c| unit(c) == unit(cols[0]));
            if shared {
                let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
                let u = Column { name: names.join(" "), unit: unit(cols[0]), values: Vec::new() };
                u.header()
            } else {
                // dimensionless columns get "[-]" so no unit reads as another's
                cols.iter().map(|c| if c.unit.is_empty() || c.unit == "-" { format!("{} [-]", c.name) } else { c.header() }).collect::<Vec<_>>().join(" ")
            }
        }
    }
}

/// Property-diagram state mapping: array bases holding T, P, h, s, v.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StateMap {
    pub t: String,
    pub p: String,
    pub h: String,
    pub s: String,
    pub v: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PlotDef {
    pub name: String,
    pub kind: PlotKind,
    pub source: Source,
    /// Parametric table read when `source` is Parametric (index).
    #[serde(default)]
    pub table: usize,
    pub x: String,
    pub series: Vec<Series>,
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    /// Label of the secondary y axis (empty: automatic).
    #[serde(default)]
    pub y2_label: String,
    pub legend: bool,
    pub grid: bool,
    pub log_x: bool,
    pub log_y: bool,
    // property diagram
    pub fluid: String,
    pub diagram: Diagram,
    pub map: StateMap,
    /// State order, e.g. `1,2,3,4,1; 5,6` (empty: ascending, closed).
    pub order: String,
    pub isobars: bool,
    pub labels: bool,
    // 3D
    pub y3: String,
    pub z3: String,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl PlotDef {
    pub(crate) fn new(name: String, kind: PlotKind) -> Self {
        PlotDef {
            name,
            kind,
            source: Source::Parametric,
            table: 0,
            x: String::new(),
            series: Vec::new(),
            title: String::new(),
            x_label: String::new(),
            y_label: String::new(),
            y2_label: String::new(),
            legend: true,
            grid: true,
            log_x: false,
            log_y: false,
            fluid: String::new(),
            diagram: Diagram::Ts,
            map: StateMap::default(),
            order: String::new(),
            isobars: true,
            labels: true,
            y3: String::new(),
            z3: String::new(),
            yaw: 0.6,
            pitch: 0.45,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlotsState {
    pub plots: Vec<PlotDef>,
    pub sel: usize,
}

// ------------------------------------------------------------------ data

/// A named column of values (display units).
#[derive(Clone)]
pub struct Column {
    pub name: String,
    pub unit: String,
    pub values: Vec<f64>,
}

impl Column {
    fn header(&self) -> String {
        if self.unit.is_empty() || self.unit == "-" {
            self.name.clone()
        } else {
            format!("{} [{}]", self.name, self.unit)
        }
    }
}

/// Everything a plot may read.
pub struct PlotInputs<'a> {
    pub analysis: &'a Analysis,
    /// Parametric tables of the project.
    pub tables: &'a [ParamTable],
    pub studies: &'a StudiesUi,
    pub display_units: &'a HashMap<String, String>,
    pub script: &'a str,
}

fn columns(src: Source, table: usize, inp: &PlotInputs) -> Vec<Column> {
    let m = &inp.analysis.model;
    let du = inp.display_units;
    let unit_of = |name: &str| m.var(name).map(|v| conv(m, du, v).1.symbol).unwrap_or_default();
    match src {
        Source::Parametric => {
            let Some(t) = inp.tables.get(table).or(inp.tables.first()) else {
                return Vec::new();
            };
            let mut cols = Vec::new();
            for (k, n) in t.inputs.iter().enumerate() {
                let values = t.cells.iter().map(|r| r.get(k).and_then(|c| c.trim().parse::<f64>().ok()).unwrap_or(f64::NAN)).collect();
                cols.push(Column { name: n.clone(), unit: unit_of(n), values });
            }
            for (j, n) in t.outputs.iter().enumerate() {
                let values = (0..t.rows()).map(|r| t.results.get(r).and_then(|x| x.as_ref()).and_then(|x| x.get(j).copied()).unwrap_or(f64::NAN)).collect();
                cols.push(Column { name: n.clone(), unit: unit_of(n), values });
            }
            cols
        }
        Source::Arrays => {
            let Some(sol) = &inp.analysis.solution else {
                return Vec::new();
            };
            let mut idx: Vec<i64> = m.vars.iter().filter(|v| v.index.len() == 1).map(|v| v.index[0]).collect();
            idx.sort_unstable();
            idx.dedup();
            let mut bases: Vec<(String, String)> = Vec::new();
            for v in &m.vars {
                if v.index.len() == 1 && !bases.iter().any(|b| b.0 == v.base) {
                    bases.push((v.base.clone(), v.display.split('[').next().unwrap_or("").to_string()));
                }
            }
            let mut cols = vec![Column { name: "i".into(), unit: String::new(), values: idx.iter().map(|&k| k as f64).collect() }];
            for (b, disp) in bases {
                let mut unit = String::new();
                let values = idx
                    .iter()
                    .map(|k| {
                        let Some(v) = m.var(&format!("{b}[{k}]")) else {
                            return f64::NAN;
                        };
                        if !sol.known[v as usize] {
                            return f64::NAN;
                        }
                        let (mu, d) = conv(m, du, v);
                        if unit.is_empty() {
                            unit = d.symbol.clone();
                        }
                        mu.convert(sol.x[v as usize], &d)
                    })
                    .collect();
                cols.push(Column { name: disp, unit, values });
            }
            cols
        }
        Source::OptHistory => {
            let Some(r) = &inp.studies.opt_result else {
                return Vec::new();
            };
            vec![
                Column { name: "evaluation".into(), unit: String::new(), values: (1..=r.history.len()).map(|k| k as f64).collect() },
                Column { name: "best objective".into(), unit: String::new(), values: r.history.clone() },
            ]
        }
    }
}

fn find<'a>(cols: &'a [Column], name: &str) -> Option<&'a Column> {
    cols.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// First fluid named in a property call of the script, e.g. `h(Water, ...)`.
pub fn detect_fluid(script: &str) -> Option<String> {
    let low = script.to_lowercase();
    let mut best: Option<(usize, String)> = None;
    for (f, _) in PROPERTY_FUNCTIONS {
        let pat = format!("{f}(");
        let mut from = 0;
        while let Some(p) = low[from..].find(&pat) {
            let at = from + p;
            from = at + pat.len();
            // whole-word match only
            if at > 0 && low.as_bytes()[at - 1].is_ascii_alphanumeric() {
                continue;
            }
            let arg = script[from..].split([',', ')']).next().unwrap_or("").trim().trim_matches(['\'', '"', '$']).to_string();
            if !arg.is_empty() && !arg.contains('=') && best.as_ref().is_none_or(|b| at < b.0) {
                best = Some((at, arg));
            }
            break;
        }
    }
    best.map(|b| b.1)
}

// ------------------------------------------------------------------ property diagrams

/// Curves of a property diagram, in display units (not log-transformed).
#[derive(Default)]
pub struct PropData {
    x_unit: String,
    y_unit: String,
    liquid: Vec<[f64; 2]>,
    vapor: Vec<[f64; 2]>,
    critical: Option<[f64; 2]>,
    isobars: Vec<(String, Vec<[f64; 2]>)>,
    paths: Vec<Vec<[f64; 2]>>,
    states: Vec<(i64, [f64; 2])>,
    warnings: Vec<String>,
}

/// SI state of one array index: (T, P, h, s, v).
fn state_si(fluid: &str, known: &[(Key, f64)]) -> Option<[f64; 5]> {
    let get = |k: Key| known.iter().find(|(kk, _)| *kk == k).map(|x| x.1);
    let pairs = [(Key::P, Key::H), (Key::P, Key::S), (Key::T, Key::S), (Key::H, Key::S), (Key::P, Key::V), (Key::T, Key::V), (Key::P, Key::T)];
    for (a, b) in pairs {
        let (Some(va), Some(vb)) = (get(a), get(b)) else {
            continue;
        };
        // CoolProp takes density, not specific volume, as an input
        let (ka, va) = if a == Key::V { (Key::D, 1.0 / va) } else { (a, va) };
        let (kb, vb) = if b == Key::V { (Key::D, 1.0 / vb) } else { (b, vb) };
        if let Ok(r) = backend::state_point(fluid, [ka, kb], [va, vb], &[Key::T, Key::P, Key::H, Key::S, Key::V]) {
            if r.iter().all(|x| x.is_finite()) {
                return Some([r[0], r[1], r[2], r[3], r[4]]);
            }
        }
    }
    None
}

fn parse_order(order: &str, available: &[i64]) -> Vec<Vec<i64>> {
    let chains: Vec<Vec<i64>> =
        order.split([';', '\n']).map(|c| c.split([',', '-', ' ', '>']).filter_map(|t| t.trim().parse::<i64>().ok()).collect::<Vec<_>>()).filter(|c| c.len() >= 2).collect();
    if !chains.is_empty() {
        return chains;
    }
    let mut c = available.to_vec();
    if c.len() >= 3 {
        c.push(c[0]);
    }
    if c.len() >= 2 {
        vec![c]
    } else {
        vec![]
    }
}

fn compute_property(def: &PlotDef, inp: &PlotInputs) -> PropData {
    let m = &inp.analysis.model;
    let mut out = PropData::default();
    let fluid = coolprop_fluid(&def.fluid);
    let (xk, yk) = def.diagram.axes();
    let xu = m.unit_system.unit_for(xk.dim());
    let yu = m.unit_system.unit_for(yk.dim());
    out.x_unit = xu.symbol.clone();
    out.y_unit = yu.symbol.clone();
    let to_disp = |c: &[f64; 2]| [xu.from_si(c[0]), yu.from_si(c[1])];

    if let Err(e) = backend::check_fluid(&fluid) {
        out.warnings.push(format!("Fluid '{}': {e}", def.fluid));
        return out;
    }
    match diagrams::dome(&fluid, xk, yk, 90) {
        Ok(d) => {
            out.liquid = d.liquid.iter().map(to_disp).collect();
            out.vapor = d.vapor.iter().map(to_disp).collect();
            out.critical = d.critical.as_ref().map(to_disp);
        }
        Err(e) => out.warnings.push(format!("Saturation dome: {e}")),
    }

    // states from arrays (model units -> SI)
    let Some(sol) = &inp.analysis.solution else {
        out.warnings.push("No solution: solve the model to overlay states.".into());
        return out;
    };
    let mut idx: Vec<i64> = m.vars.iter().filter(|v| v.index.len() == 1).map(|v| v.index[0]).collect();
    idx.sort_unstable();
    idx.dedup();
    let mapped = [(Key::T, &def.map.t), (Key::P, &def.map.p), (Key::H, &def.map.h), (Key::S, &def.map.s), (Key::V, &def.map.v)];
    let mut states: HashMap<i64, [f64; 5]> = HashMap::new();
    let mut avail = Vec::new();
    for &k in &idx {
        let mut known = Vec::new();
        for (key, base) in mapped {
            if base.is_empty() {
                continue;
            }
            if let Some(v) = m.var(&format!("{base}[{k}]")) {
                if sol.known[v as usize] {
                    known.push((key, m.unit_system.unit_for(key.dim()).to_si(sol.x[v as usize])));
                }
            }
        }
        if known.len() < 2 {
            continue;
        }
        match state_si(&fluid, &known) {
            Some(st) => {
                states.insert(k, st);
                avail.push(k);
            }
            None => out.warnings.push(format!("State {k}: could not evaluate from the mapped arrays.")),
        }
    }
    let pick = |st: &[f64; 5], key: Key| match key {
        Key::T => st[0],
        Key::P => st[1],
        Key::H => st[2],
        Key::S => st[3],
        _ => st[4],
    };
    let xy = |st: &[f64; 5]| [pick(st, xk), pick(st, yk)];
    out.states = avail.iter().map(|k| (*k, to_disp(&xy(&states[k])))).collect();

    // isobars through the distinct state pressures
    if def.isobars && !states.is_empty() {
        let (tmin, _, _, tmax, _) = backend::limits(&fluid).unwrap_or((250.0, 0.0, 0.0, 1000.0, 0.0));
        let thi = states.values().map(|s| s[0]).fold(f64::MIN, f64::max);
        let t1 = (thi * 1.12).min(tmax);
        let mut ps: Vec<f64> = states.values().map(|s| s[1]).collect();
        ps.sort_by(|a, b| a.total_cmp(b));
        let mut distinct: Vec<f64> = Vec::new();
        for p in ps {
            if distinct.last().is_none_or(|q| (p - q).abs() > 0.01 * p.abs().max(q.abs())) {
                distinct.push(p);
            }
        }
        let pu = m.unit_system.unit_for(Key::P.dim());
        for p in distinct.into_iter().take(12) {
            let c = diagrams::isoline(&fluid, Key::P, p, xk, yk, tmin + 0.01, t1, 90);
            out.isobars.push((format!("{} {}", fmt_num(pu.from_si(p)), pu.symbol), c.iter().map(to_disp).collect()));
        }
    }

    // process paths: isobaric when the pressures agree within 0.5 %, else straight
    for chain in parse_order(&def.order, &avail) {
        let mut path: Vec<[f64; 2]> = Vec::new();
        for w in chain.windows(2) {
            let (Some(a), Some(b)) = (states.get(&w[0]), states.get(&w[1])) else {
                out.warnings.push(format!("Path {}→{}: state not available.", w[0], w[1]));
                continue;
            };
            let seg: Vec<[f64; 2]> = if (a[1] - b[1]).abs() <= 0.005 * a[1].abs().max(b[1].abs()) {
                let mut c = diagrams::isobaric_path(&fluid, 0.5 * (a[1] + b[1]), a[3], b[3], xk, yk, 50);
                if c.len() < 2 {
                    c = vec![xy(a), xy(b)];
                }
                c
            } else {
                vec![xy(a), xy(b)]
            };
            path.extend(seg.iter().map(to_disp));
        }
        if path.len() >= 2 {
            out.paths.push(path);
        }
    }
    out
}

fn auto_map(inp: &PlotInputs) -> StateMap {
    let m = &inp.analysis.model;
    let bases: Vec<&str> = m.vars.iter().filter(|v| v.index.len() == 1).map(|v| v.base.as_str()).collect();
    let pick = |cands: &[&str]| cands.iter().find(|c| bases.contains(c)).map(|s| s.to_string()).unwrap_or_default();
    StateMap { t: pick(&["t", "temp"]), p: pick(&["p", "pres"]), h: pick(&["h", "enth"]), s: pick(&["s", "entr"]), v: pick(&["v", "vol"]) }
}

// ------------------------------------------------------------------ UI

#[derive(Default)]
pub struct PlotsUi {
    cache: HashMap<String, Arc<PropData>>,
    message: String,
}

fn tx(v: f64, log: bool) -> f64 {
    if log {
        if v > 0.0 {
            v.log10()
        } else {
            f64::NAN
        }
    } else {
        v
    }
}

fn pts(c: &[[f64; 2]], lx: bool, ly: bool) -> Vec<[f64; 2]> {
    c.iter().map(|p| [tx(p[0], lx), tx(p[1], ly)]).filter(|p| p[0].is_finite() && p[1].is_finite()).collect()
}

/// Which parametric table a plot reads, when there are several.
fn table_combo(ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
    if def.source != Source::Parametric || inp.tables.len() < 2 {
        return;
    }
    let name = |k: usize| inp.tables.get(k).map(|t| t.title(k)).unwrap_or_default();
    egui::ComboBox::from_label("Table").selected_text(name(def.table)).show_ui(ui, |ui| {
        for k in 0..inp.tables.len() {
            ui.selectable_value(&mut def.table, k, name(k));
        }
    });
}

/// Hover position, prefixed with the item name when near a data point.
fn hover(h: &HoverPosition) -> (String, PlotPoint) {
    match h {
        HoverPosition::NearDataPoint { plot_name, position, .. } => (if plot_name.is_empty() { String::new() } else { format!("{plot_name}\n") }, *position),
        HoverPosition::Elsewhere { position } => (String::new(), *position),
    }
}

fn axis_fmt(log: bool) -> impl Fn(GridMark, &RangeInclusive<f64>) -> String {
    move |g: GridMark, r: &RangeInclusive<f64>| {
        if log {
            // decades always; 2 and 5 within a few decades; every mark when
            // less than about a decade is visible
            let span = r.end() - r.start();
            let v = 10f64.powf(g.value);
            let mant = v / 10f64.powf(g.value.floor());
            let near = |k: f64| (mant - k).abs() < 1e-6 * k;
            let show = g.step_size >= 1.0 || span < 1.2 || (span < 3.5 && (near(2.0) || near(5.0)));
            if show {
                fmt_num(round_sig(v, 6))
            } else {
                String::new()
            }
        } else {
            fmt_num(g.value)
        }
    }
}

fn round_sig(v: f64, n: i32) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let p = 10f64.powi(n - 1 - v.abs().log10().floor() as i32);
    (v * p).round() / p
}

/// Grid of a logarithmic axis (coordinates are log10 of the values): lines
/// at every decade, at 2..9 times each decade, and at 1.1, 1.2, ... when
/// less than a decade is visible, so the lines crowd towards the next decade
/// as on log paper. The step size tells egui_plot how strong each line is.
fn log_grid(input: egui_plot::GridInput) -> Vec<GridMark> {
    let (lo, hi) = input.bounds;
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
        return Vec::new();
    }
    let span = hi - lo;
    let d0 = lo.floor() as i64;
    let d1 = hi.ceil() as i64;
    // very wide ranges: decades only, every n-th
    let every = ((span / 40.0).ceil() as i64).max(1);
    let mut out = Vec::new();
    for d in d0..=d1 {
        if d % every == 0 {
            out.push(GridMark { value: d as f64, step_size: every as f64 });
        }
        if span > 12.0 {
            continue;
        }
        for k in 2..=9 {
            let v = d as f64 + (k as f64).log10();
            if v >= lo && v <= hi {
                out.push(GridMark { value: v, step_size: 0.1 });
            }
        }
        if span < 1.2 {
            for k in 11..100 {
                if k % 10 == 0 {
                    continue;
                }
                let v = d as f64 + (k as f64 / 10.0).log10();
                if v >= lo && v <= hi {
                    out.push(GridMark { value: v, step_size: 0.01 });
                }
            }
        }
    }
    out
}

/// Logarithmic grid spacer on the axes that use a log scale.
fn with_log_grids(mut p: Plot<'_>, lx: bool, ly: bool) -> Plot<'_> {
    if lx {
        p = p.x_grid_spacer(log_grid);
    }
    if ly {
        p = p.y_grid_spacer(log_grid);
    }
    p
}

fn combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut String, options: &[String], allow_empty: bool) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(id).width(150.0).selected_text(if value.is_empty() { "—".to_string() } else { value.clone() }).show_ui(ui, |ui| {
        if allow_empty {
            ui.selectable_value(value, String::new(), "—");
        }
        for o in options {
            ui.selectable_value(value, o.clone(), o);
        }
    });
    *value != before
}

impl PlotsUi {
    pub fn show(&mut self, ui: &mut egui::Ui, st: &mut PlotsState, inp: &PlotInputs) {
        egui::Panel::left("plots_side").resizable(true).default_size(320.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side(ui, st, inp));
        });
        let Some(def) = st.plots.get_mut(st.sel) else {
            ui.centered_and_justified(|ui| ui.label("Add a plot with the buttons on the left."));
            return;
        };
        match def.kind {
            PlotKind::XY => self.xy(ui, def, inp),
            PlotKind::Property => self.property(ui, def, inp),
            PlotKind::Surface => self.surface(ui, def, inp),
        }
    }

    fn side(&mut self, ui: &mut egui::Ui, st: &mut PlotsState, inp: &PlotInputs) {
        ui.horizontal_wrapped(|ui| {
            let n = st.plots.len() + 1;
            if ui.button("+ XY plot").clicked() {
                let mut d = PlotDef::new(format!("Plot {n}"), PlotKind::XY);
                if inp.tables.iter().all(|t| t.rows() == 0) {
                    d.source = Source::Arrays;
                }
                st.plots.push(d);
                st.sel = st.plots.len() - 1;
            }
            if ui.button("+ Property diagram").clicked() {
                let mut d = PlotDef::new(format!("Diagram {n}"), PlotKind::Property);
                d.fluid = detect_fluid(inp.script).unwrap_or_else(|| "Water".into());
                d.map = auto_map(inp);
                st.plots.push(d);
                st.sel = st.plots.len() - 1;
            }
            if ui.button("+ 3D plot").clicked() {
                st.plots.push(PlotDef::new(format!("Surface {n}"), PlotKind::Surface));
                st.sel = st.plots.len() - 1;
            }
        });
        ui.separator();
        let mut remove = None;
        let mut dup = None;
        for (i, p) in st.plots.iter().enumerate() {
            ui.horizontal(|ui| {
                let kind = match p.kind {
                    PlotKind::XY => "XY",
                    PlotKind::Property => p.diagram.label(),
                    PlotKind::Surface => "3D",
                };
                ui.selectable_value(&mut st.sel, i, format!("{}  ({kind})", p.name));
                if ui.small_button("copy").on_hover_text("Duplicate").clicked() {
                    dup = Some(i);
                }
                if ui.small_button("del").on_hover_text("Delete").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = dup {
            let mut d = st.plots[i].clone();
            d.name += " (copy)";
            st.plots.insert(i + 1, d);
            st.sel = i + 1;
        }
        if let Some(i) = remove {
            st.plots.remove(i);
            st.sel = st.sel.min(st.plots.len().saturating_sub(1));
        }
        let Some(def) = st.plots.get_mut(st.sel) else {
            return;
        };
        ui.separator();
        egui::Grid::new("plot_common").num_columns(2).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut def.name);
            ui.end_row();
            ui.label("Title");
            ui.text_edit_singleline(&mut def.title);
            ui.end_row();
            ui.label("x label");
            ui.add(egui::TextEdit::singleline(&mut def.x_label).hint_text("auto"));
            ui.end_row();
            ui.label("y label");
            ui.add(egui::TextEdit::singleline(&mut def.y_label).hint_text("auto"));
            ui.end_row();
            if def.kind == PlotKind::XY && def.series.iter().any(|s| s.y2) {
                ui.label("y2 label");
                ui.add(egui::TextEdit::singleline(&mut def.y2_label).hint_text("auto (right axis)"));
                ui.end_row();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if def.kind != PlotKind::Surface {
                ui.checkbox(&mut def.grid, "Grid");
                ui.checkbox(&mut def.legend, "Legend");
                ui.checkbox(&mut def.log_x, "log x");
                ui.checkbox(&mut def.log_y, "log y");
            }
        });
        ui.separator();
        match def.kind {
            PlotKind::XY => self.xy_side(ui, def, inp),
            PlotKind::Property => self.property_side(ui, def, inp),
            PlotKind::Surface => self.surface_side(ui, def, inp),
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Export Python…").on_hover_text("Self-contained matplotlib script with the data embedded").clicked() {
                let code = self.python(def, inp);
                let name = format!("{}.py", def.name.replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "_"));
                if let Some(p) = rfd::FileDialog::new().add_filter("Python", &["py"]).set_file_name(&name).save_file() {
                    self.message = match std::fs::write(&p, code) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => format!("Could not save: {e}"),
                    };
                }
            }
            if ui.button("Copy Python").clicked() {
                ui.ctx().copy_text(self.python(def, inp));
                self.message = "Python script copied to the clipboard.".into();
            }
        });
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).weak());
        }
    }

    // ---------------------------------------------------------- XY

    fn xy_side(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        egui::ComboBox::from_label("Data").selected_text(def.source.label()).show_ui(ui, |ui| {
            for s in Source::ALL {
                ui.selectable_value(&mut def.source, s, s.label());
            }
        });
        table_combo(ui, def, inp);
        let cols = columns(def.source, def.table, inp);
        let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
        if names.is_empty() {
            ui.label(RichText::new("No data yet: run a parametric table, solve a model with arrays, or run an optimisation.").weak());
            return;
        }
        if def.x.is_empty() || find(&cols, &def.x).is_none() {
            def.x = names[0].clone();
        }
        ui.horizontal(|ui| {
            ui.label("x");
            combo(ui, "xy_x", &mut def.x, &names, false);
        });
        ui.strong("Series");
        let mut remove = None;
        for (k, s) in def.series.iter_mut().enumerate() {
            ui.push_id(k, |ui| {
                ui.horizontal(|ui| {
                    combo(ui, "y", &mut s.y, &names, false);
                    if ui.small_button("del").clicked() {
                        remove = Some(k);
                    }
                });
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut s.color).on_hover_text("Line colour");
                    egui::ComboBox::from_id_salt("line").width(70.0).selected_text(format!("{:?}", s.line)).show_ui(ui, |ui| {
                        for l in LineKind::ALL {
                            ui.selectable_value(&mut s.line, l, format!("{l:?}"));
                        }
                    });
                    ui.add(egui::DragValue::new(&mut s.width).range(0.5..=6.0).speed(0.1).prefix("w "));
                });
                ui.horizontal(|ui| {
                    let mut mc = s.marker_rgb();
                    let r = ui.color_edit_button_srgb(&mut mc).on_hover_text(if s.marker_color.is_some() {
                        "Marker colour (right-click: same as the line)"
                    } else {
                        "Marker colour (now the line's)"
                    });
                    if r.changed() {
                        s.marker_color = Some(mc);
                    }
                    if r.secondary_clicked() {
                        s.marker_color = None;
                    }
                    egui::ComboBox::from_id_salt("marker").width(70.0).selected_text(format!("{:?}", s.marker)).show_ui(ui, |ui| {
                        for mk in Marker::ALL {
                            ui.selectable_value(&mut s.marker, mk, format!("{mk:?}"));
                        }
                    });
                    ui.add(egui::DragValue::new(&mut s.size).range(1.0..=10.0).speed(0.1).prefix("size "));
                    ui.add(egui::TextEdit::singleline(&mut s.label).hint_text("legend").desired_width(80.0));
                });
                ui.checkbox(&mut s.y2, "Right y axis").on_hover_text("Plot this series against a secondary y axis (e.g. efficiencies next to powers)");
                ui.add_space(4.0);
            });
        }
        if let Some(k) = remove {
            def.series.remove(k);
        }
        if ui.button("+ series").clicked() {
            let next = names.iter().find(|n| **n != def.x && !def.series.iter().any(|s| &s.y == *n)).cloned().unwrap_or_else(|| names[0].clone());
            def.series.push(Series::new(&next, def.series.len()));
        }
    }

    fn xy(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        let cols = columns(def.source, def.table, inp);
        let xc = find(&cols, &def.x);
        let (lx, ly) = (def.log_x, def.log_y);
        let x_label = if def.x_label.is_empty() { xc.map(|c| c.header()).unwrap_or_default() } else { def.x_label.clone() };
        let y_label = if def.y_label.is_empty() { axis_label(&axis_cols(def, &cols, false)) } else { def.y_label.clone() };
        if !def.title.is_empty() {
            ui.vertical_centered(|ui| ui.heading(&def.title));
        }
        // secondary axis: its data are mapped linearly onto the primary
        // coordinates, y_plot = a + b*y, with a nice-number frame so that the
        // primary grid lines fall on round secondary values
        let range = |sec: bool, log: bool| -> Option<(f64, f64)> {
            let v: Vec<f64> = def
                .series
                .iter()
                .filter(|s| s.y2 == sec)
                .filter_map(|s| find(&cols, &s.y))
                .flat_map(|c| c.values.iter().map(|y| if log { y.log10() } else { *y }))
                .filter(|y| y.is_finite())
                .collect();
            if v.is_empty() {
                None
            } else {
                Some((v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max)))
            }
        };
        let (a2, b2) = match range(true, false) {
            Some(sr) => y2_map(range(false, ly), sr),
            None => (0.0, 1.0),
        };
        let has_y2 = def.series.iter().any(|s| s.y2);
        let y2_names: Vec<String> = def.series.iter().filter(|s| s.y2).filter_map(|s| find(&cols, &s.y).map(|c| series_name(s, c, true))).collect();
        let y2_label = if def.y2_label.is_empty() { axis_label(&axis_cols(def, &cols, true)) } else { def.y2_label.clone() };
        let x_name = x_label.split(" [").next().unwrap_or("x").to_string();
        // the hover read-out replaces egui_plot's coordinate label
        let mut plot = Plot::new(("xy", def.name.as_str())).x_axis_label(x_label).show_grid(def.grid).x_axis_formatter(axis_fmt(lx)).show_x(false).show_y(false);
        plot = with_log_grids(plot, lx, ly);
        if has_y2 {
            let right = move |g: GridMark, _r: &RangeInclusive<f64>| {
                let st = g.step_size / b2;
                let v = (g.value - a2) / b2;
                fmt_num(if st > 0.0 { (v / st).round() * st } else { v })
            };
            plot = plot
                .custom_y_axes(vec![AxisHints::new_y().label(y_label).formatter(axis_fmt(ly)), AxisHints::new_y().label(y2_label).formatter(right).placement(HPlacement::Right)]);
        } else {
            plot = plot.y_axis_label(y_label).y_axis_formatter(axis_fmt(ly));
        }
        if def.legend {
            plot = plot.legend(Legend::default());
        }
        plot = plot.label_formatter(move |h| {
            let (name, p) = hover(h);
            let on_y2 = y2_names.iter().any(|n| name.trim_end() == n);
            let x = if lx { 10f64.powf(p.x) } else { p.x };
            let y = if on_y2 {
                (p.y - a2) / b2
            } else if ly {
                10f64.powf(p.y)
            } else {
                p.y
            };
            Some(format!("{name}{}, {}", fmt_num(x), fmt_num(y)))
        });
        let r = plot.show(ui, |pu| {
            let xc = xc?;
            let mut hov: Vec<(String, Color32, Vec<[f64; 2]>, bool)> = Vec::new();
            for s in &def.series {
                let Some(yc) = find(&cols, &s.y) else {
                    continue;
                };
                let raw: Vec<[f64; 2]> = xc.values.iter().zip(&yc.values).map(|(x, y)| [*x, if s.y2 { a2 + b2 * *y } else { *y }]).collect();
                let p = pts(&raw, lx, ly && !s.y2);
                let name = series_name(s, yc, has_y2);
                if s.line != LineKind::None {
                    pu.line(Line::new(name.clone(), p.clone()).color(s.color32()).width(s.width).style(s.line.egui()));
                }
                if let Some(shape) = s.marker.egui() {
                    pu.points(Points::new(name.clone(), p.clone()).shape(shape).radius(s.size).color(s.marker32()).filled(true));
                }
                hov.push((name, s.key32(), p, s.y2));
            }
            let series: Vec<HoverSeries> = hov.iter().map(|h| (h.0.clone(), h.1, h.2.as_slice())).collect();
            let (x, _, rows) = hover_readout(pu, &series, &[])?;
            // back to data values (log axes, secondary axis)
            let rows: Vec<(String, Color32, String)> = rows
                .into_iter()
                .map(|(n, c, y)| {
                    let y2 = hov.iter().any(|h| h.0 == n && h.3);
                    let v = if y2 {
                        (y - a2) / b2
                    } else if ly {
                        10f64.powf(y)
                    } else {
                        y
                    };
                    (n, c, fmt_num(v))
                })
                .collect();
            Some((fmt_num(if lx { 10f64.powf(x) } else { x }), rows))
        });
        if let Some((x, rows)) = r.inner {
            if r.response.hovered() {
                hover_tooltip(r.response, format!("{} = {x}", x_name), &rows);
            }
        }
    }

    // ---------------------------------------------------------- property diagram

    fn property_side(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        ui.horizontal(|ui| {
            ui.label("Fluid");
            ui.add(egui::TextEdit::singleline(&mut def.fluid).desired_width(140.0));
            if ui.small_button("detect").on_hover_text("First fluid used in a property call of the script").clicked() {
                if let Some(f) = detect_fluid(inp.script) {
                    def.fluid = f;
                }
            }
        });
        let before = def.diagram;
        egui::ComboBox::from_label("Diagram").selected_text(def.diagram.label()).show_ui(ui, |ui| {
            for d in Diagram::ALL {
                ui.selectable_value(&mut def.diagram, d, d.label());
            }
        });
        if before != def.diagram {
            (def.log_x, def.log_y) = def.diagram.default_log();
        }
        ui.strong("States (arrays)");
        let m = &inp.analysis.model;
        let mut bases: Vec<String> = Vec::new();
        for v in &m.vars {
            if v.index.len() == 1 && !bases.contains(&v.base) {
                bases.push(v.base.clone());
            }
        }
        egui::Grid::new("statemap").num_columns(2).show(ui, |ui| {
            for (label, field) in [("T", &mut def.map.t), ("P", &mut def.map.p), ("h", &mut def.map.h), ("s", &mut def.map.s), ("v", &mut def.map.v)] {
                ui.label(label);
                combo(ui, ("map", label), field, &bases, true);
                ui.end_row();
            }
        });
        if ui.small_button("Auto-map").on_hover_text("Map arrays named T, P, h, s, v").clicked() {
            def.map = auto_map(inp);
        }
        ui.label(RichText::new("Any two known properties define a state (P-h, P-s, T-s, h-s, P-v, T-v or P-T).").small().weak());
        ui.horizontal(|ui| {
            ui.label("Order");
            ui.add(egui::TextEdit::singleline(&mut def.order).hint_text("1,2,3,4,1; 5,6").desired_width(150.0));
        });
        ui.label(RichText::new("Consecutive states at equal pressure are joined along the isobar; others by a straight line.").small().weak());
        ui.checkbox(&mut def.isobars, "Isobars through the states");
        ui.checkbox(&mut def.labels, "State numbers");
    }

    fn prop_data(&mut self, def: &PlotDef, inp: &PlotInputs) -> Arc<PropData> {
        let key =
            format!("{}|{}|{:?}|{}|{}|{}|{}|{}|{}|{}", inp.analysis.gen, def.fluid, def.diagram, def.map.t, def.map.p, def.map.h, def.map.s, def.map.v, def.order, def.isobars);
        if let Some(d) = self.cache.get(&key) {
            return d.clone();
        }
        if self.cache.len() > 24 {
            self.cache.clear();
        }
        let d = Arc::new(compute_property(def, inp));
        self.cache.insert(key, d.clone());
        d
    }

    fn property(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        let data = self.prop_data(def, inp);
        let (xk, yk) = def.diagram.axes();
        let (lx, ly) = (def.log_x, def.log_y);
        for w in &data.warnings {
            ui.label(RichText::new(w).color(Color32::from_rgb(200, 120, 0)).small());
        }
        let title = if def.title.is_empty() { format!("{} — {}", def.diagram.label(), def.fluid) } else { def.title.clone() };
        ui.vertical_centered(|ui| ui.heading(title));
        let xl = if def.x_label.is_empty() { format!("{} [{}]", key_name(xk), data.x_unit) } else { def.x_label.clone() };
        let yl = if def.y_label.is_empty() { format!("{} [{}]", key_name(yk), data.y_unit) } else { def.y_label.clone() };
        let mut plot = Plot::new(("prop", def.name.as_str())).x_axis_label(xl).y_axis_label(yl).show_grid(def.grid).x_axis_formatter(axis_fmt(lx)).y_axis_formatter(axis_fmt(ly));
        plot = with_log_grids(plot, lx, ly);
        if def.legend {
            plot = plot.legend(Legend::default());
        }
        let (xs, ys) = (key_name(xk), key_name(yk));
        plot = plot.label_formatter(move |h| {
            let (name, p) = hover(h);
            let (x, y) = (if lx { 10f64.powf(p.x) } else { p.x }, if ly { 10f64.powf(p.y) } else { p.y });
            Some(format!("{name}{xs} = {}\n{ys} = {}", fmt_num(x), fmt_num(y)))
        });
        let labels = def.labels;
        let plot = plot.show_x(false).show_y(false);
        let r = plot.show(ui, |pu| {
            let dome = Color32::from_gray(90);
            pu.line(Line::new("Saturated liquid", pts(&data.liquid, lx, ly)).color(dome).width(1.6));
            pu.line(Line::new("Saturated vapour", pts(&data.vapor, lx, ly)).color(dome).width(1.6));
            if let Some(c) = data.critical {
                pu.points(Points::new("Critical point", pts(&[c], lx, ly)).shape(MarkerShape::Diamond).radius(3.5).color(dome).filled(true));
            }
            for (label, c) in data.isobars.iter() {
                let p = pts(c, lx, ly);
                let col = Color32::from_rgb(120, 150, 190);
                pu.line(Line::new(format!("P = {label}"), p.clone()).color(col).width(0.9).style(LineStyle::Dashed { length: 6.0 }));
                if let Some(last) = p.last() {
                    pu.text(Text::new("", PlotPoint::new(last[0], last[1]), RichText::new(label).small()).color(col).anchor(egui::Align2::LEFT_BOTTOM));
                }
            }
            let cyc = Color32::from_rgb(213, 94, 0);
            for (k, p) in data.paths.iter().enumerate() {
                pu.line(Line::new(if k == 0 { "Cycle".to_string() } else { format!("Path {}", k + 1) }, pts(p, lx, ly)).color(cyc).width(2.2));
            }
            let sp: Vec<[f64; 2]> = pts(&data.states.iter().map(|s| s.1).collect::<Vec<_>>(), lx, ly);
            pu.points(Points::new("States", sp).shape(MarkerShape::Circle).radius(4.0).color(cyc).filled(true));
            if labels {
                for (k, s) in &data.states {
                    let p = [tx(s[0], lx), tx(s[1], ly)];
                    if p[0].is_finite() && p[1].is_finite() {
                        pu.text(Text::new("", PlotPoint::new(p[0], p[1]), RichText::new(format!(" {k}")).strong()).anchor(egui::Align2::LEFT_BOTTOM));
                    }
                }
            }
            // hover read-out: computed cycle paths only (no dome or isobars),
            // snapping to the state points
            let hs: Vec<(String, Color32, Vec<[f64; 2]>)> =
                data.paths.iter().enumerate().map(|(k, p)| (if k == 0 { "Cycle".to_string() } else { format!("Path {}", k + 1) }, cyc, pts(p, lx, ly))).collect();
            let keys: Vec<f64> = data.states.iter().map(|s| tx(s.1[0], lx)).collect();
            let series: Vec<HoverSeries> = hs.iter().map(|h| (h.0.clone(), h.1, h.2.as_slice())).collect();
            let (x, snapped, rows) = hover_readout(pu, &series, &keys)?;
            let state = if snapped { data.states.iter().find(|s| tx(s.1[0], lx) == x).map(|s| s.0.clone()) } else { None };
            let back = |v: f64, log: bool| if log { 10f64.powf(v) } else { v };
            let xs = fmt_num(back(x, lx));
            let head = match state {
                Some(k) => format!("State {k}: {} = {xs}  ({})", key_name(xk), key_name(yk)),
                None => format!("{} = {xs}  ({})", key_name(xk), key_name(yk)),
            };
            Some((head, rows.into_iter().map(|(n, c, y)| (n, c, fmt_num(back(y, ly)))).collect::<Vec<_>>()))
        });
        if let Some((head, rows)) = r.inner {
            if r.response.hovered() {
                hover_tooltip(r.response, head, &rows);
            }
        }
    }

    // ---------------------------------------------------------- 3D

    fn surface_side(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        egui::ComboBox::from_label("Data").selected_text(def.source.label()).show_ui(ui, |ui| {
            for s in [Source::Parametric, Source::Arrays] {
                ui.selectable_value(&mut def.source, s, s.label());
            }
        });
        table_combo(ui, def, inp);
        let cols = columns(def.source, def.table, inp);
        let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
        if names.is_empty() {
            ui.label(RichText::new("No data yet.").weak());
            return;
        }
        for (label, field, k) in [("x", &mut def.x, 0usize), ("y", &mut def.y3, 1), ("z", &mut def.z3, 2)] {
            if field.is_empty() || find(&cols, field).is_none() {
                *field = names[k.min(names.len() - 1)].clone();
            }
            ui.horizontal(|ui| {
                ui.label(label);
                combo(ui, ("s3", label), field, &names, false);
            });
        }
        ui.label(
            RichText::new("A full x × y grid (e.g. a two-input parametric table) is drawn as a surface; other data as a scatter. Drag to rotate, scroll to zoom.").small().weak(),
        );
        if ui.button("Reset view").clicked() {
            (def.yaw, def.pitch, def.zoom) = (0.6, 0.45, 1.0);
        }
    }

    fn surface(&mut self, ui: &mut egui::Ui, def: &mut PlotDef, inp: &PlotInputs) {
        let cols = columns(def.source, def.table, inp);
        let (Some(xc), Some(yc), Some(zc)) = (find(&cols, &def.x), find(&cols, &def.y3), find(&cols, &def.z3)) else {
            ui.label("Choose x, y and z.");
            return;
        };
        let data: Vec<[f64; 3]> = (0..xc.values.len().min(yc.values.len()).min(zc.values.len()))
            .map(|i| [xc.values[i], yc.values[i], zc.values[i]])
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .collect();
        if !def.title.is_empty() {
            ui.vertical_centered(|ui| ui.heading(&def.title));
        }
        let labels = [if def.x_label.is_empty() { xc.header() } else { def.x_label.clone() }, if def.y_label.is_empty() { yc.header() } else { def.y_label.clone() }, zc.header()];
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            def.yaw += d.x * 0.01;
            def.pitch = (def.pitch + d.y * 0.01).clamp(-1.5, 1.5);
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                def.zoom = (def.zoom * (1.0 + s * 0.002)).clamp(0.2, 5.0);
            }
        }
        draw3d(ui.painter_at(rect), rect, &data, &labels, def, ui.visuals().text_color());
    }

    // ---------------------------------------------------------- Python export

    pub fn python(&mut self, def: &PlotDef, inp: &PlotInputs) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "# Generated by DeltaF Studio: plot '{}'", def.name);
        s += PY_HEADER;
        let q = |t: &str| format!("tex({:?})", t);
        match def.kind {
            PlotKind::XY => {
                let cols = columns(def.source, def.table, inp);
                s += "fig, ax = plt.subplots(figsize=FIGSIZE)\n";
                let xc = find(&cols, &def.x);
                let has_y2 = def.series.iter().any(|s| s.y2);
                if has_y2 {
                    s += "ax2 = ax.twinx()\n";
                }
                if let Some(xc) = xc {
                    let _ = writeln!(s, "x = np.array({})", py_list(&xc.values));
                    for (k, se) in def.series.iter().enumerate() {
                        let Some(yc) = find(&cols, &se.y) else {
                            continue;
                        };
                        let label = if se.label.is_empty() { yc.header() } else { se.label.clone() };
                        let _ = writeln!(s, "y{k} = np.array({})", py_list(&yc.values));
                        let _ = writeln!(
                            s,
                            "{}.plot(x, y{k}, color='{}', mfc='{m}', mec='{m}', lw={}, ls='{}', marker={}, ms={}, label={})",
                            if se.y2 { "ax2" } else { "ax" },
                            hex(se.color),
                            se.width,
                            se.line.py(),
                            se.marker.py(),
                            2.0 * se.size,
                            q(&label),
                            m = hex(se.marker_rgb()),
                        );
                    }
                    let xl = if def.x_label.is_empty() { xc.header() } else { def.x_label.clone() };
                    let _ = writeln!(s, "ax.set_xlabel({})", q(&xl));
                }
                let yl = if def.y_label.is_empty() { axis_label(&axis_cols(def, &cols, false)) } else { def.y_label.clone() };
                if !yl.is_empty() {
                    let _ = writeln!(s, "ax.set_ylabel({})", q(&yl));
                }
                if has_y2 {
                    let yl = if def.y2_label.is_empty() { axis_label(&axis_cols(def, &cols, true)) } else { def.y2_label.clone() };
                    if !yl.is_empty() {
                        let _ = writeln!(s, "ax2.set_ylabel({})", q(&yl));
                    }
                    // one legend for both axes
                    let mut d2 = def.clone();
                    d2.legend = false;
                    if def.legend {
                        s += "h1, l1 = ax.get_legend_handles_labels()\nh2, l2 = ax2.get_legend_handles_labels()\nax.legend(h1 + h2, l1 + l2)\n";
                    }
                    axes_common(&mut s, &d2, q);
                } else {
                    axes_common(&mut s, def, q);
                }
            }
            PlotKind::Property => {
                let d = self.prop_data(def, inp);
                let (xk, yk) = def.diagram.axes();
                s += "fig, ax = plt.subplots(figsize=FIGSIZE)\n";
                let _ = writeln!(s, "liq = np.array({})\nvap = np.array({})", py_pts(&d.liquid), py_pts(&d.vapor));
                s += "ax.plot(liq[:, 0], liq[:, 1], color='0.35', lw=1.4, label='Saturated liquid')\n";
                s += "ax.plot(vap[:, 0], vap[:, 1], color='0.35', lw=1.4, label='Saturated vapour')\n";
                if let Some(c) = d.critical {
                    let _ = writeln!(s, "ax.plot([{}], [{}], 'D', color='0.35', ms=5)", c[0], c[1]);
                }
                for (k, (label, c)) in d.isobars.iter().enumerate() {
                    if c.len() < 2 {
                        continue;
                    }
                    let _ = writeln!(s, "iso{k} = np.array({})", py_pts(c));
                    let _ = writeln!(s, "ax.plot(iso{k}[:, 0], iso{k}[:, 1], color='#7896be', lw=0.8, ls='--')");
                    let _ = writeln!(s, "ax.annotate({}, iso{k}[-1], fontsize=8, color='#7896be')", q(label));
                }
                for (k, p) in d.paths.iter().enumerate() {
                    let _ = writeln!(s, "path{k} = np.array({})", py_pts(p));
                    let _ = writeln!(s, "ax.plot(path{k}[:, 0], path{k}[:, 1], color='#d55e00', lw=2{})", if k == 0 { ", label='Cycle'" } else { "" });
                }
                if !d.states.is_empty() {
                    let _ = writeln!(s, "states = {{{}}}", d.states.iter().map(|(k, p)| format!("{k}: ({}, {})", p[0], p[1])).collect::<Vec<_>>().join(", "));
                    s += "for k, (sx, sy) in states.items():\n    ax.plot(sx, sy, 'o', color='#d55e00', ms=6)\n";
                    if def.labels {
                        s += "    ax.annotate(f' {k}', (sx, sy), fontsize=10, fontweight='bold')\n";
                    }
                }
                let xl = if def.x_label.is_empty() { format!("{} [{}]", key_name(xk), d.x_unit) } else { def.x_label.clone() };
                let yl = if def.y_label.is_empty() { format!("{} [{}]", key_name(yk), d.y_unit) } else { def.y_label.clone() };
                let _ = writeln!(s, "ax.set_xlabel({})\nax.set_ylabel({})", q(&xl), q(&yl));
                let mut d2 = def.clone();
                if d2.title.is_empty() {
                    d2.title = format!("{} — {}", def.diagram.label(), def.fluid);
                }
                axes_common(&mut s, &d2, q);
            }
            PlotKind::Surface => {
                let cols = columns(def.source, def.table, inp);
                let (Some(xc), Some(yc), Some(zc)) = (find(&cols, &def.x), find(&cols, &def.y3), find(&cols, &def.z3)) else {
                    return s + "# x, y or z column missing\n";
                };
                let _ = writeln!(s, "x = np.array({})\ny = np.array({})\nz = np.array({})", py_list(&xc.values), py_list(&yc.values), py_list(&zc.values));
                s += "fig = plt.figure(figsize=(FIGSIZE[0], FIGSIZE[0]*0.85))\nax = fig.add_subplot(projection='3d')\nok = np.isfinite(x) & np.isfinite(y) & np.isfinite(z)\n";
                s += "ux, uy = np.unique(x[ok]), np.unique(y[ok])\n";
                s += "if ux.size * uy.size == ok.sum() and ux.size > 1 and uy.size > 1:\n";
                s += "    order = np.lexsort((x[ok], y[ok]))\n";
                s += "    Z = z[ok][order].reshape(uy.size, ux.size)\n";
                s += "    X, Y = np.meshgrid(ux, uy)\n";
                s += "    surf = ax.plot_surface(X, Y, Z, cmap='viridis', edgecolor='k', linewidth=0.3)\n";
                s += "else:\n    surf = ax.scatter(x[ok], y[ok], z[ok], c=z[ok], cmap='viridis')\n";
                s += "fig.colorbar(surf, ax=ax, shrink=0.6)\n";
                let xl = if def.x_label.is_empty() { xc.header() } else { def.x_label.clone() };
                let yl = if def.y_label.is_empty() { yc.header() } else { def.y_label.clone() };
                let _ = writeln!(s, "ax.set_xlabel({})\nax.set_ylabel({})\nax.set_zlabel({})", q(&xl), q(&yl), q(&zc.header()));
                let _ = writeln!(s, "ax.view_init(elev={:.1}, azim={:.1})", def.pitch.to_degrees(), -90.0 + def.yaw.to_degrees());
                if !def.title.is_empty() {
                    let _ = writeln!(s, "ax.set_title({})", q(&def.title));
                }
                s += "plt.tight_layout()\nplt.show()\n";
            }
        }
        s
    }
}

/// Legend name of an XY series; secondary-axis series are marked.
/// Columns of the series on the left (`y2` false) or right axis, each once.
fn axis_cols<'c>(def: &PlotDef, cols: &'c [Column], y2: bool) -> Vec<&'c Column> {
    let mut out: Vec<&Column> = Vec::new();
    for c in def.series.iter().filter(|s| s.y2 == y2).filter_map(|s| find(cols, &s.y)) {
        if !out.iter().any(|o| o.name == c.name) {
            out.push(c);
        }
    }
    out
}

/// `#rrggbb` of a colour, for matplotlib.
fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn series_name(s: &Series, c: &Column, mark: bool) -> String {
    let n = if s.label.is_empty() { c.header() } else { s.label.clone() };
    if mark && s.y2 {
        format!("{n} (right axis)")
    } else {
        n
    }
}

/// Round step 1, 2 or 5 × 10^k not smaller than `x`.
fn nice_step(x: f64) -> f64 {
    let e = 10f64.powf(x.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * e).find(|s| *s >= x * (1.0 - 1e-12)).unwrap_or(10.0 * e)
}

/// Linear map `y_plot = a + b*y` that places the secondary data range `sec`
/// over the primary range `prim` (both `(min, max)`, primary in plot
/// coordinates). Both axes get a frame of the same number of round steps, so
/// that every primary grid line corresponds to a round secondary value.
fn y2_map(prim: Option<(f64, f64)>, sec: (f64, f64)) -> (f64, f64) {
    let widen = |(lo, hi): (f64, f64)| {
        if hi - lo > 1e-12 * (lo.abs() + hi.abs()).max(1e-300) {
            (lo, hi)
        } else {
            let d = (lo.abs() * 0.1).max(1.0);
            (lo - d, hi + d)
        }
    };
    let (s0, s1) = widen(sec);
    let Some(prim) = prim else { return (0.0, 1.0) };
    let (p0, p1) = widen(prim);
    let n = 5.0;
    let sp = nice_step((p1 - p0) / n);
    let pz = (p0 / sp).floor() * sp;
    let np = ((p1 - pz) / sp - 1e-9).ceil().max(1.0);
    let mut ss = nice_step((s1 - s0) / np);
    let mut sz = (s0 / ss).floor() * ss;
    for _ in 0..8 {
        if sz + np * ss >= s1 - 1e-9 * ss {
            break;
        }
        ss = nice_step(ss * 1.0001);
        sz = (s0 / ss).floor() * ss;
    }
    let b = sp / ss;
    (pz - sz * b, b)
}

/// Start of every exported script: SciencePlots style (Garrett, 2021,
/// https://github.com/garrettj403/SciencePlots) when installed, with LaTeX
/// text only when a LaTeX installation is found; `tex()` escapes the
/// characters LaTeX treats specially in labels.
const PY_HEADER: &str = r#"import shutil
import numpy as np
import matplotlib.pyplot as plt

try:
    import scienceplots  # noqa: F401  (pip install SciencePlots)
    plt.style.use(['science'] if shutil.which('latex') else ['science', 'no-latex'])
except ImportError:
    print('SciencePlots is not installed (pip install SciencePlots): using the matplotlib style')

# Figure size in inches: (3.5, 2.6) fits one journal column, (7.0, 4.3) a full page width
FIGSIZE = (3.5, 2.6)
USETEX = plt.rcParams['text.usetex']


_TEX = {'\\': r'\textbackslash{}', '_': r'\_', '%': r'\%', '&': r'\&', '#': r'\#', '$': r'\$',
        '^': r'\^{}', '~': r'\~{}', '{': r'\{', '}': r'\}',
        '\u2014': '---', '\u2013': '--', '\u00b0': r'$^\circ$'}


def tex(s):
    """Escape text for LaTeX when matplotlib renders text with LaTeX."""
    return ''.join(_TEX.get(c, c) for c in s) if USETEX else s


nan = np.nan
"#;

fn axes_common(s: &mut String, def: &PlotDef, q: impl Fn(&str) -> String) {
    if !def.title.is_empty() {
        let _ = writeln!(s, "ax.set_title({})", q(&def.title));
    }
    if def.log_x {
        *s += "ax.set_xscale('log')\n";
    }
    if def.log_y {
        *s += "ax.set_yscale('log')\n";
    }
    if def.grid {
        *s += "ax.grid(True, which='major', alpha=0.35)\n";
        if def.log_x || def.log_y {
            *s += "ax.grid(True, which='minor', alpha=0.12)\n";
        }
    }
    if def.legend {
        *s += "ax.legend()\n";
    }
    *s += "plt.tight_layout()\n# plt.savefig('figure.pdf')\nplt.show()\n";
}

fn py_num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "nan".into()
    }
}

fn py_list(v: &[f64]) -> String {
    format!("[{}]", v.iter().map(|x| py_num(*x)).collect::<Vec<_>>().join(", "))
}

fn py_pts(c: &[[f64; 2]]) -> String {
    if c.is_empty() {
        return "np.empty((0, 2))".into();
    }
    format!("[{}]", c.iter().map(|p| format!("[{}, {}]", py_num(p[0]), py_num(p[1]))).collect::<Vec<_>>().join(", "))
}

// ------------------------------------------------------------------ 3D painter

/// Viridis colour map (van der Walt & Smith 2015), piecewise-linear.
fn viridis(t: f32) -> Color32 {
    const C: [[f32; 3]; 5] = [[68.0, 1.0, 84.0], [59.0, 82.0, 139.0], [33.0, 145.0, 140.0], [94.0, 201.0, 98.0], [253.0, 231.0, 37.0]];
    let t = t.clamp(0.0, 1.0) * 4.0;
    let i = (t.floor() as usize).min(3);
    let f = t - i as f32;
    let c = |k: usize| (C[i][k] + (C[i + 1][k] - C[i][k]) * f) as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

fn nice_ticks(lo: f64, hi: f64) -> Vec<f64> {
    if !(hi > lo) {
        return vec![lo];
    }
    let raw = (hi - lo) / 4.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(raw);
    let mut t = (lo / step).ceil() * step;
    let mut v = Vec::new();
    while t <= hi + 1e-9 * step {
        v.push(t);
        t += step;
    }
    v
}

fn draw3d(painter: egui::Painter, rect: egui::Rect, data: &[[f64; 3]], labels: &[String; 3], def: &PlotDef, text: Color32) {
    if data.is_empty() {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "No finite data", egui::FontId::proportional(14.0), text);
        return;
    }
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in data {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    for k in 0..3 {
        if hi[k] - lo[k] < 1e-300 {
            hi[k] = lo[k] + 1.0;
        }
    }
    let norm = |p: [f64; 3]| -> [f32; 3] { std::array::from_fn(|k| (2.0 * (p[k] - lo[k]) / (hi[k] - lo[k]) - 1.0) as f32) };
    let (sy, cy, sp, cp) = (def.yaw.sin(), def.yaw.cos(), def.pitch.sin(), def.pitch.cos());
    let scale = rect.width().min(rect.height()) * 0.28 * def.zoom;
    let center = rect.center() + Vec2::new(-30.0, 10.0);
    // returns (screen position, depth: larger = farther)
    let proj = |q: [f32; 3]| -> (Pos2, f32) {
        let x1 = q[0] * cy - q[1] * sy;
        let y1 = q[0] * sy + q[1] * cy;
        let up = q[2] * cp + y1 * sp;
        let depth = y1 * cp - q[2] * sp;
        (center + Vec2::new(x1 * scale, -up * scale), depth)
    };
    let font = egui::FontId::proportional(11.0);
    let axis_col = text.gamma_multiply(0.6);

    // bounding box: draw the back faces' edges first (grid), front edges later
    let corners: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
    for i in 0..8usize {
        for b in [1usize, 2, 4] {
            let j = i | b;
            if j != i {
                let (a, _) = proj(corners[i]);
                let (c, _) = proj(corners[j]);
                painter.line_segment([a, c], Stroke::new(0.7, axis_col.gamma_multiply(0.6)));
            }
        }
    }
    let zc = |z: f64| viridis(((z - lo[2]) / (hi[2] - lo[2])) as f32);
    // grid detection
    let uniq = |k: usize| {
        let mut v: Vec<f64> = data.iter().map(|p| p[k]).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * (a.abs() + b.abs() + 1e-300));
        v
    };
    let (ux, uy) = (uniq(0), uniq(1));
    let pos = |v: &[f64], x: f64| v.iter().position(|u| (u - x).abs() <= 1e-9 * (u.abs() + x.abs() + 1e-300));
    let mut grid: Vec<Vec<Option<f64>>> = vec![vec![None; ux.len()]; uy.len()];
    let mut filled = 0;
    for p in data {
        if let (Some(i), Some(j)) = (pos(&ux, p[0]), pos(&uy, p[1])) {
            if grid[j][i].is_none() {
                filled += 1;
            }
            grid[j][i] = Some(p[2]);
        }
    }
    if ux.len() >= 2 && uy.len() >= 2 && filled == ux.len() * uy.len() {
        let mut quads: Vec<(f32, [Pos2; 4], Color32)> = Vec::new();
        for j in 0..uy.len() - 1 {
            for i in 0..ux.len() - 1 {
                let c = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let mut ps = [Pos2::ZERO; 4];
                let mut depth = 0.0;
                let mut zm = 0.0;
                for (n, (a, b)) in c.iter().enumerate() {
                    let z = grid[*b][*a].unwrap();
                    let (p, d) = proj(norm([ux[*a], uy[*b], z]));
                    ps[n] = p;
                    depth += d / 4.0;
                    zm += z / 4.0;
                }
                quads.push((depth, ps, zc(zm)));
            }
        }
        // painter's algorithm: far to near
        quads.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, ps, col) in quads {
            painter.add(egui::Shape::convex_polygon(ps.to_vec(), col.gamma_multiply(0.92), Stroke::new(0.5, Color32::from_black_alpha(140))));
        }
    } else {
        let mut ps: Vec<(f32, Pos2, Color32)> = data
            .iter()
            .map(|p| {
                let (s, d) = proj(norm(*p));
                (d, s, zc(p[2]))
            })
            .collect();
        ps.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, p, c) in ps {
            painter.circle(p, 4.0, c, Stroke::new(0.6, Color32::from_black_alpha(160)));
        }
    }
    // axis ticks along three edges, offset outwards from the box so that
    // labels of edges sharing a corner do not overlap
    // x and y on the nearest bottom edges, z on the left-most vertical edge
    let near = |a: [f32; 3], b: [f32; 3]| if proj(a).1 <= proj(b).1 { a } else { b };
    let ex = near([0.0, -1.0, -1.0], [0.0, 1.0, -1.0]);
    let ey = near([-1.0, 0.0, -1.0], [1.0, 0.0, -1.0]);
    let ez = [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, 1.0, 0.0], [1.0, 1.0, 0.0f32]].into_iter().min_by(|a, b| proj(*a).0.x.total_cmp(&proj(*b).0.x)).unwrap();
    let edges: [([f32; 3], usize); 3] = [(ex, 0), (ey, 1), (ez, 2)];
    for (base, k) in edges {
        let outward = |q: [f32; 3], f: f32| {
            let mut o = q;
            for (j, c) in o.iter_mut().enumerate() {
                if j != k {
                    *c += f * base[j];
                }
            }
            proj(o).0 - proj(q).0
        };
        for t in nice_ticks(lo[k], hi[k]) {
            let mut q = base;
            q[k] = (2.0 * (t - lo[k]) / (hi[k] - lo[k]) - 1.0) as f32;
            let (p, _) = proj(q);
            painter.line_segment([p, p + outward(q, 0.04)], Stroke::new(1.0, axis_col));
            painter.text(p + outward(q, 0.13), egui::Align2::CENTER_CENTER, fmt_num(t), font.clone(), axis_col);
        }
        let mut q = base;
        q[k] = 0.0;
        painter.text(proj(q).0 + outward(q, 0.32), egui::Align2::CENTER_CENTER, &labels[k], egui::FontId::proportional(12.5), text);
    }
    // colour bar
    let bar = egui::Rect::from_min_size(egui::pos2(rect.right() - 110.0, rect.top() + 40.0), Vec2::new(14.0, rect.height() * 0.5));
    let n = 32;
    for k in 0..n {
        let t0 = k as f32 / n as f32;
        let r = egui::Rect::from_min_max(egui::pos2(bar.left(), bar.bottom() - bar.height() * (t0 + 1.0 / n as f32)), egui::pos2(bar.right(), bar.bottom() - bar.height() * t0));
        painter.rect_filled(r, 0.0, viridis(t0 + 0.5 / n as f32));
    }
    painter.text(bar.right_top() + Vec2::new(4.0, 0.0), egui::Align2::LEFT_CENTER, fmt_num(hi[2]), font.clone(), text);
    painter.text(bar.right_bottom() + Vec2::new(4.0, 0.0), egui::Align2::LEFT_CENTER, fmt_num(lo[2]), font, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};

    fn rankine() -> Analysis {
        run(Request {
            gen: 1,
            text: include_str!("../../../examples/rankine.dfs").into(),
            opts: dfcore::CompileOptions::default(),
            solve: true,
            solve_opts: dfcore::SolveOptions::default(),
            warm: None,
            init: Default::default(),
        })
    }

    /// Builds every plot kind from the Rankine example and checks the
    /// generated Python. With `DF_PY_OUT=<dir>` the scripts are written there
    /// so that they can be run with matplotlib.
    #[test]
    fn plots_and_python_export() {
        let a = rankine();
        assert!(a.solution.as_ref().is_some_and(|s| s.converged));
        let table = ParamTable::default();
        let studies = StudiesUi::default();
        let du = HashMap::new();
        let script = include_str!("../../../examples/rankine.dfs");
        let inp = PlotInputs { analysis: &a, tables: std::slice::from_ref(&table), studies: &studies, display_units: &du, script };
        assert_eq!(detect_fluid(script).as_deref(), Some("Water"));
        let mut ui = PlotsUi::default();

        let mut xy = PlotDef::new("xy".into(), PlotKind::XY);
        xy.source = Source::Arrays;
        xy.x = "i".into();
        xy.series.push(Series::new("h", 0));
        // black dashed line with red square markers
        let mut hs = Series::new("h_s", 1);
        (hs.color, hs.marker_color, hs.line, hs.marker) = ([0, 0, 0], Some([255, 0, 0]), LineKind::Dashed, Marker::Square);
        xy.series.push(hs);
        let py = ui.python(&xy, &inp);
        assert!(py.contains("color='#000000', mfc='#ff0000', mec='#ff0000'"), "{py}");
        assert!(py.contains("ax.set_ylabel(tex(\"h h_s [kJ/kg]\"))"), "{py}");
        let cols = columns(Source::Arrays, 0, &inp);
        let h = find(&cols, "h").unwrap();
        assert_eq!(h.unit, "kJ/kg");
        assert!((h.values[2] - 3450.4).abs() < 0.5, "h[3] = {}", h.values[2]);

        let mut ts = PlotDef::new("ts".into(), PlotKind::Property);
        ts.fluid = "Water".into();
        ts.map = auto_map(&inp);
        assert_eq!(ts.map.p, "p");
        let d = compute_property(&ts, &inp);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        assert_eq!(d.states.len(), 4);
        assert_eq!(d.isobars.len(), 2);
        assert_eq!(d.paths.len(), 1);
        // T-s in engineering units: T3 = 550 C, s3 = 6.52 kJ/kg-K
        let s3 = d.states.iter().find(|s| s.0 == 3).unwrap().1;
        assert!((s3[1] - 550.0).abs() < 1e-6 && (s3[0] - 6.523).abs() < 1e-2, "{s3:?}");
        let c = d.critical.unwrap();
        assert!((c[1] - 373.946).abs() < 0.05, "critical {c:?}");

        let mut ph = ts.clone();
        ph.name = "ph".into();
        ph.diagram = Diagram::Ph;
        (ph.log_x, ph.log_y) = Diagram::Ph.default_log();

        for dg in [Diagram::Pv, Diagram::Hs, Diagram::Th] {
            let mut d2 = ts.clone();
            d2.diagram = dg;
            let r = compute_property(&d2, &inp);
            assert!(r.warnings.is_empty() && r.states.len() == 4 && r.liquid.len() > 50, "{dg:?}: {:?}", r.warnings);
        }

        let mut s3d = PlotDef::new("surface".into(), PlotKind::Surface);
        s3d.source = Source::Arrays;
        s3d.x = "s".into();
        s3d.y3 = "h".into();
        s3d.z3 = "T".into();

        for def in [&xy, &ts, &ph, &s3d] {
            let py = ui.python(def, &inp);
            assert!(py.contains("plt.show()") && !py.contains("NaN"), "{py}");
            if let Ok(dir) = std::env::var("DF_PY_OUT") {
                std::fs::write(format!("{dir}/{}.py", def.name), py).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod y2_tests {
    use super::*;

    #[test]
    fn axis_labels() {
        let c = |n: &str, u: &str| Column { name: n.into(), unit: u.into(), values: Vec::new() };
        let (w, q, eta) = (c("W_net", "kW"), c("Q_in", "kW"), c("eta_th", "-"));
        assert_eq!(axis_label(&[]), "");
        assert_eq!(axis_label(&[&w]), "W_net [kW]");
        assert_eq!(axis_label(&[&w, &q]), "W_net Q_in [kW]");
        assert_eq!(axis_label(&[&eta, &w]), "eta_th [-] W_net [kW]");
        assert_eq!(axis_label(&[&eta, &c("x", "")]), "eta_th x");
    }

    /// Series written before separate marker colours use the line colour.
    #[test]
    fn old_series_marker_colour() {
        let s: Series = serde_json::from_str(r#"{"y":"h","label":"","color":[1,2,3],"width":1.0,"line":"Solid","marker":"Circle","size":3.0}"#).unwrap();
        assert_eq!(s.marker_color, None);
        assert_eq!(s.marker_rgb(), [1, 2, 3]);
    }

    #[test]
    fn secondary_axis_map() {
        // power 0..1200 kW on the left, efficiency 0.31..0.42 on the right
        let (a, b) = y2_map(Some((0.0, 1200.0)), (0.31, 0.42));
        let f = |y: f64| a + b * y;
        // the secondary data lie inside the primary frame
        assert!(f(0.31) >= -1e-9 && f(0.42) <= 1400.0 + 1e-9, "{} {}", f(0.31), f(0.42));
        // primary grid lines (multiples of 200) fall on round secondary values
        for k in 0..=7 {
            let s = (200.0 * k as f64 - a) / b;
            let r = (s * 100.0).round() / 100.0;
            assert!((s - r).abs() < 1e-9, "{s}");
        }
        assert_eq!(nice_step(0.03), 0.05);
        assert_eq!(nice_step(200.0), 200.0);
        // constant secondary series and missing primary do not blow up
        let (a, b) = y2_map(Some((0.0, 10.0)), (0.5, 0.5));
        assert!(a.is_finite() && b.is_finite() && b > 0.0);
        assert_eq!(y2_map(None, (0.0, 1.0)), (0.0, 1.0));
    }
}

/// A series for the hover read-out: name, colour and points in plot
/// coordinates.
pub type HoverSeries<'a> = (String, Color32, &'a [[f64; 2]]);

/// Values of every series at the hovered x (plot coordinates). The x snaps
/// to a computed point when the pointer is near one: first to a key point
/// (`keys`, e.g. state points) or a series end within 10 px, otherwise to
/// any vertex within 6 px; between vertices the value is interpolated on the
/// segment that spans x. Draws a vertical guide and a marker on each series.
/// Returns (x, snapped, [(name, colour, y)]).
pub fn hover_readout(pu: &mut PlotUi, series: &[HoverSeries], keys: &[f64]) -> Option<(f64, bool, Vec<(String, Color32, f64)>)> {
    if !pu.response().contains_pointer() {
        return None;
    }
    let p = pu.pointer_coordinate()?;
    let b = pu.plot_bounds();
    let (x0, x1) = (b.min()[0].min(b.max()[0]), b.min()[0].max(b.max()[0]));
    if !(p.x >= x0 && p.x <= x1) {
        return None;
    }
    let px = pu.transform().dpos_dvalue_x().abs().max(1e-12);
    let near = |cands: &mut dyn Iterator<Item = f64>, lim: f64| -> Option<f64> {
        cands.filter(|c| c.is_finite() && ((c - p.x).abs() * px) <= lim).min_by(|a, c| (a - p.x).abs().total_cmp(&(c - p.x).abs()))
    };
    let ends = series.iter().filter(|s| !s.0.is_empty()).flat_map(|s| s.2.first().into_iter().chain(s.2.last()).map(|q| q[0]));
    let snap = near(&mut keys.iter().copied().chain(ends), 10.0).or_else(|| near(&mut series.iter().filter(|s| !s.0.is_empty()).flat_map(|s| s.2.iter().map(|q| q[0])), 6.0));
    let x = snap.unwrap_or(p.x);
    let tol = 6.0 / px;
    let mut rows = Vec::new();
    let mut marks = Vec::new();
    for (name, col, pts) in series {
        if name.is_empty() || pts.is_empty() {
            continue;
        }
        // every crossing of the vertical line x (a closed cycle has two)
        let mut ys: Vec<f64> = Vec::new();
        let mut push = |y: f64, span: f64| {
            if y.is_finite() && !ys.iter().any(|v| (v - y).abs() <= 1e-6 * span.max(y.abs()).max(1e-12)) {
                ys.push(y);
            }
        };
        for (k, w) in pts.windows(2).enumerate() {
            let (a, c) = (w[0], w[1]);
            let span = (c[1] - a[1]).abs();
            if a[0] == x {
                push(a[1], span);
            }
            if k + 2 == pts.len() && c[0] == x {
                push(c[1], span);
            }
            if (a[0] - x) * (c[0] - x) < 0.0 {
                push(a[1] + (c[1] - a[1]) * (x - a[0]) / (c[0] - a[0]), span);
            }
        }
        if pts.len() == 1 && pts[0][0] == x {
            push(pts[0][1], 0.0);
        }
        if ys.is_empty() {
            if let Some(q) = pts.iter().filter(|q| (q[0] - x).abs() <= tol).min_by(|a, c| (a[0] - x).abs().total_cmp(&(c[0] - x).abs())) {
                ys.push(q[1]);
            }
        }
        ys.sort_by(|a, b| b.total_cmp(a));
        let n = ys.len();
        for (k, y) in ys.into_iter().enumerate().take(4) {
            rows.push((if n > 1 { format!("{name} ({})", k + 1) } else { name.clone() }, *col, y));
            marks.push(([x, y], *col));
        }
    }
    if rows.is_empty() {
        return None;
    }
    let snapped = snap.is_some();
    pu.vline(VLine::new("", x).color(Color32::from_gray(140)).width(if snapped { 1.2 } else { 0.8 }).style(LineStyle::dashed_dense()));
    for (m, c) in marks {
        pu.points(Points::new("", vec![m]).radius(if snapped { 5.0 } else { 4.0 }).color(c).filled(true));
    }
    Some((x, snapped, rows))
}

/// Tooltip for a hover read-out; `fx`/`fy` format x and a series value.
pub fn hover_tooltip(resp: egui::Response, x: String, rows: &[(String, Color32, String)]) {
    resp.on_hover_ui_at_pointer(|ui| {
        ui.strong(x);
        egui::Grid::new("hover_rows").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (n, c, v) in rows {
                ui.label(RichText::new(n).color(*c));
                ui.label(RichText::new(v).monospace());
                ui.end_row();
            }
        });
    });
}

#[cfg(test)]
mod log_axis_tests {
    use super::*;

    fn input(lo: f64, hi: f64) -> egui_plot::GridInput {
        egui_plot::GridInput { bounds: (lo, hi), base_step_size: 0.001 }
    }

    #[test]
    fn decades_and_crowding_minor_lines() {
        // 0.1 .. 100: decades at -1, 0, 1, 2 and 2..9 in between
        let g = log_grid(input(-1.0, 2.0));
        let majors: Vec<f64> = g.iter().filter(|m| m.step_size >= 1.0).map(|m| m.value).collect();
        assert_eq!(majors, vec![-1.0, 0.0, 1.0, 2.0]);
        let mut minors: Vec<f64> = g.iter().filter(|m| m.step_size < 1.0 && m.value > 0.0 && m.value < 1.0).map(|m| 10f64.powf(m.value)).collect();
        minors.sort_by(f64::total_cmp);
        assert_eq!(minors.len(), 8);
        for (k, v) in minors.iter().enumerate() {
            assert!((v - (k + 2) as f64).abs() < 1e-9);
        }
        // gaps shrink towards the next decade (log paper)
        let pos: Vec<f64> = minors.iter().map(|v| v.log10()).collect();
        assert!(pos.windows(3).all(|w| w[2] - w[1] < w[1] - w[0]));
        // labels: decades only over three decades
        let f = axis_fmt(true);
        let r = -1.0..=2.0;
        assert_eq!(f(GridMark { value: 1.0, step_size: 1.0 }, &r), "10");
        assert_eq!(f(GridMark { value: 3f64.log10(), step_size: 0.1 }, &r), "");
        // zoomed in: finer lines and every label
        let g = log_grid(input(0.0, 0.5));
        assert!(g.iter().any(|m| m.step_size == 0.01));
        assert_eq!(f(GridMark { value: 2f64.log10(), step_size: 0.1 }, &(0.0..=0.5)), "2");
    }
}
