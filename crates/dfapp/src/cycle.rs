//! Cycle diagram tab: a flowsheet of components connected by
//! streams. Each stream is bound to a state number, and its labels read the
//! state arrays (`T[k]`, `P[k]`, `h[k]`, ...) of the current solution, so
//! the diagram updates whenever the model is solved. A KPI table shows
//! chosen variables (gross/net power, efficiencies...). Diagram, state table
//! and KPIs export to a vector PDF report.

use crate::analysis::Analysis;
use crate::pdf::{self, Align, Pdf};
use crate::studies_ui::conv;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::HashMap;

// ------------------------------------------------------------------ model

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Turbine,
    Compressor,
    Pump,
    HeatExchanger,
    Heater,
    Cooler,
    Mixer,
    Splitter,
    Valve,
    Source,
    Sink,
    Generator,
    Label,
    Motor,
    CoolingTower,
    Pipe,
}

impl Kind {
    pub const ALL: [Kind; 16] = [
        Kind::Turbine,
        Kind::Compressor,
        Kind::Pump,
        Kind::HeatExchanger,
        Kind::Heater,
        Kind::Cooler,
        Kind::Mixer,
        Kind::Splitter,
        Kind::Valve,
        Kind::Source,
        Kind::Sink,
        Kind::Generator,
        Kind::Motor,
        Kind::CoolingTower,
        Kind::Pipe,
        Kind::Label,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Kind::Turbine => "Turbine",
            Kind::Compressor => "Compressor",
            Kind::Pump => "Pump",
            Kind::HeatExchanger => "Heat exchanger",
            Kind::Heater => "Heater / boiler",
            Kind::Cooler => "Cooler / condenser",
            Kind::Mixer => "Mixer",
            Kind::Splitter => "Splitter",
            Kind::Valve => "Valve",
            Kind::Source => "Source",
            Kind::Sink => "Sink",
            Kind::Generator => "Generator",
            Kind::Label => "Text",
            Kind::Motor => "Motor",
            Kind::CoolingTower => "Cooling tower",
            Kind::Pipe => "Pipe",
        }
    }
}

/// Drawing primitive in component-local coordinates (px at zoom 1, y down).
#[derive(Clone, Debug)]
enum Prim {
    Poly {
        pts: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<[u8; 3]>,
    },
    Circle {
        c: [f32; 2],
        r: f32,
        fill: Option<[u8; 3]>,
    },
    Text {
        p: [f32; 2],
        s: String,
        size: f32,
    },
    /// Convex filled area without outline (pieces of non-convex bodies).
    Fill {
        pts: Vec<[f32; 2]>,
        fill: [u8; 3],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Port {
    pub name: &'static str,
    pub pos: [f32; 2],
    /// outward direction
    pub dir: [f32; 2],
    pub inlet: bool,
}

const TURB: [u8; 3] = [214, 120, 110];
const COMP: [u8; 3] = [110, 160, 214];
const HX: [u8; 3] = [222, 214, 170];
const HOT: [u8; 3] = [236, 160, 110];
const COLD: [u8; 3] = [140, 196, 230];

fn shape(kind: Kind) -> Vec<Prim> {
    use Prim::*;
    let poly = |pts: &[[f32; 2]], fill: Option<[u8; 3]>| Poly { pts: pts.to_vec(), closed: true, fill };
    let line = |pts: &[[f32; 2]]| Poly { pts: pts.to_vec(), closed: false, fill: None };
    match kind {
        Kind::Turbine => vec![poly(&[[-25.0, -12.0], [25.0, -26.0], [25.0, 26.0], [-25.0, 12.0]], Some(TURB)), line(&[[25.0, 0.0], [38.0, 0.0]])],
        Kind::Compressor => vec![poly(&[[-25.0, -26.0], [25.0, -12.0], [25.0, 12.0], [-25.0, 26.0]], Some(COMP)), line(&[[-38.0, 0.0], [-25.0, 0.0]])],
        Kind::Pump => vec![Circle { c: [0.0, 0.0], r: 17.0, fill: Some(COMP) }, poly(&[[-8.0, 9.0], [8.0, 9.0], [0.0, -10.0]], None)],
        Kind::HeatExchanger => vec![
            poly(&[[-32.0, -22.0], [32.0, -22.0], [32.0, 22.0], [-32.0, 22.0]], Some(HX)),
            line(&[[-32.0, -10.0], [-16.0, -10.0], [-8.0, 4.0], [0.0, -10.0], [8.0, 4.0], [16.0, -10.0], [32.0, -10.0]]),
            line(&[[-32.0, 10.0], [32.0, 10.0]]),
        ],
        Kind::Heater => vec![
            poly(&[[-20.0, -28.0], [20.0, -28.0], [20.0, 28.0], [-20.0, 28.0]], Some(HOT)),
            line(&[[0.0, 28.0], [0.0, 18.0], [-10.0, 12.0], [10.0, 4.0], [-10.0, -4.0], [10.0, -12.0], [0.0, -18.0], [0.0, -28.0]]),
        ],
        Kind::Cooler => vec![
            poly(&[[-28.0, -20.0], [28.0, -20.0], [28.0, 20.0], [-28.0, 20.0]], Some(COLD)),
            line(&[[0.0, -20.0], [0.0, -12.0], [-12.0, -6.0], [12.0, 0.0], [-12.0, 6.0], [0.0, 12.0], [0.0, 20.0]]),
        ],
        Kind::Mixer | Kind::Splitter => vec![Circle { c: [0.0, 0.0], r: 7.0, fill: Some([200, 200, 200]) }],
        // bow tie as two triangles (egui fills convex polygons only)
        Kind::Valve => vec![poly(&[[-14.0, -9.0], [0.0, 0.0], [-14.0, 9.0]], Some([200, 200, 200])), poly(&[[14.0, -9.0], [0.0, 0.0], [14.0, 9.0]], Some([200, 200, 200]))],
        Kind::Source => vec![Circle { c: [0.0, 0.0], r: 9.0, fill: Some([160, 210, 160]) }, line(&[[9.0, 0.0], [16.0, 0.0]])],
        Kind::Sink => vec![Circle { c: [0.0, 0.0], r: 9.0, fill: Some([220, 170, 170]) }, line(&[[-16.0, 0.0], [-9.0, 0.0]])],
        Kind::Generator => {
            vec![Circle { c: [0.0, 0.0], r: 16.0, fill: Some([235, 235, 235]) }, Text { p: [0.0, 0.0], s: "G".into(), size: 14.0 }, line(&[[-30.0, 0.0], [-16.0, 0.0]])]
        }
        Kind::Motor => vec![Circle { c: [0.0, 0.0], r: 16.0, fill: Some([235, 235, 235]) }, Text { p: [0.0, 0.0], s: "M".into(), size: 14.0 }, line(&[[16.0, 0.0], [30.0, 0.0]])],
        Kind::CoolingTower => vec![
            // hyperbolic shell: two convex fills and the outline
            Fill { pts: vec![[-24.0, -32.0], [24.0, -32.0], [18.0, 0.0], [-18.0, 0.0]], fill: COLD },
            Fill { pts: vec![[-18.0, 0.0], [18.0, 0.0], [32.0, 32.0], [-32.0, 32.0]], fill: COLD },
            poly(&[[-24.0, -32.0], [24.0, -32.0], [18.0, 0.0], [32.0, 32.0], [-32.0, 32.0], [-18.0, 0.0]], None),
            line(&[[-14.0, -14.0], [14.0, -14.0]]),
            line(&[[-8.0, -20.0], [-8.0, -8.0]]),
            line(&[[0.0, -20.0], [0.0, -8.0]]),
            line(&[[8.0, -20.0], [8.0, -8.0]]),
        ],
        Kind::Pipe => vec![poly(&[[-18.0, -5.0], [18.0, -5.0], [18.0, 5.0], [-18.0, 5.0]], Some([150, 200, 120]))],
        Kind::Label => vec![],
    }
}

/// Port whose stream colours primitive `i` of a shape: the internal passes
/// of exchangers, heaters, coolers and towers, and the outline of the
/// machines and flow devices the stream goes through.
fn inner_port(kind: Kind, i: usize) -> Option<usize> {
    match (kind, i) {
        (Kind::HeatExchanger, 1) => Some(0),
        (Kind::HeatExchanger, 2) => Some(2),
        (Kind::Heater | Kind::Cooler, 1) => Some(0),
        (Kind::CoolingTower, 3..=6) => Some(0),
        (Kind::Turbine | Kind::Compressor, 0) => Some(0),
        (Kind::Pump, 0 | 1) => Some(0),
        (Kind::Valve, 0 | 1) => Some(0),
        (Kind::Pipe | Kind::Mixer | Kind::Splitter | Kind::Source | Kind::Sink, _) => Some(0),
        _ => None,
    }
}

/// Colour of the stream at port `k` of component `id` (entering or leaving).
fn inlet_color(d: &CycleDiagram, id: u32, k: usize) -> Option<[u8; 3]> {
    d.links.iter().find(|l| l.to == (id, k) || l.from == (id, k)).and_then(|l| l.color)
}

/// Component fill for the theme: the light-mode colour, or a darker shade
/// of it on the dark canvas.
fn theme_fill(f: [u8; 3], dark: bool) -> Color32 {
    if dark {
        let bg = 28.0;
        let m = |x: u8| (bg + (x as f32 - bg) * 0.42).round() as u8;
        Color32::from_rgb(m(f[0]), m(f[1]), m(f[2]))
    } else {
        c32(f)
    }
}

/// Alternative connection points on the contour of the turbomachines:
/// (name, local position, outward direction). Index 0 of each port is its
/// default position from [`ports`]; these follow.
pub fn anchor_options(kind: Kind) -> Vec<(String, [f32; 2], [f32; 2])> {
    let mut v = Vec::new();
    match kind {
        Kind::Turbine | Kind::Compressor => {
            // small end at x = s·25 (turbine: left), large end at -s·25
            let s = if kind == Kind::Turbine { -1.0 } else { 1.0 };
            let (small, large) = if kind == Kind::Turbine { ("inlet face", "exhaust face") } else { ("discharge face", "suction face") };
            for (k, y) in [-8.0, 0.0, 8.0].into_iter().enumerate() {
                v.push((format!("axial, {small} {}", k + 1), [s * 25.0, y], [s, 0.0]));
            }
            // the shaft leaves the large end at y = 0
            for (k, y) in [-18.0, -9.0, 9.0, 18.0].into_iter().enumerate() {
                v.push((format!("axial, {large} {}", k + 1), [-s * 25.0, y], [-s, 0.0]));
            }
            for (side, sy) in [("upper", -1.0), ("lower", 1.0)] {
                for (k, t) in [0.15, 0.5, 0.85].into_iter().enumerate() {
                    // casing edge from the small end (half height 12) to the large end (26)
                    let x = s * 25.0 - s * 50.0 * t;
                    let y = sy * (12.0 + 14.0 * t);
                    v.push((format!("radial, {side} casing {}", k + 1), [x, y], [0.0, sy]));
                }
            }
        }
        Kind::Pump => {
            let r: f32 = 17.0;
            for (side, ax) in [("top", [0.0, -1.0]), ("bottom", [0.0, 1.0]), ("left", [-1.0, 0.0]), ("right", [1.0, 0.0])] {
                for (k, o) in [-10.0f32, 0.0, 10.0].into_iter().enumerate() {
                    let n = (r * r - o * o).sqrt();
                    let pos = if ax[0] == 0.0 { [o, ax[1] * n] } else { [ax[0] * n, o] };
                    let tag = ["offset -", "centre", "offset +"][k];
                    v.push((format!("{side}, {tag}"), pos, ax));
                }
            }
        }
        _ => {}
    }
    v
}

pub fn ports(kind: Kind) -> Vec<Port> {
    let p = |name, pos: [f32; 2], dir: [f32; 2], inlet| Port { name, pos, dir, inlet };
    match kind {
        Kind::Turbine => vec![p("in", [-25.0, 0.0], [-1.0, 0.0], true), p("out", [25.0, 18.0], [0.0, 1.0], false)],
        Kind::Compressor => vec![p("in", [-25.0, 18.0], [0.0, 1.0], true), p("out", [25.0, 0.0], [1.0, 0.0], false)],
        Kind::Pump => vec![p("in", [-17.0, 0.0], [-1.0, 0.0], true), p("out", [0.0, -17.0], [0.0, -1.0], false)],
        Kind::HeatExchanger => vec![
            p("hot in", [-32.0, -10.0], [-1.0, 0.0], true),
            p("hot out", [32.0, -10.0], [1.0, 0.0], false),
            p("cold in", [32.0, 10.0], [1.0, 0.0], true),
            p("cold out", [-32.0, 10.0], [-1.0, 0.0], false),
        ],
        Kind::Heater => vec![p("in", [0.0, 28.0], [0.0, 1.0], true), p("out", [0.0, -28.0], [0.0, -1.0], false)],
        Kind::Cooler => vec![p("in", [0.0, -20.0], [0.0, -1.0], true), p("out", [0.0, 20.0], [0.0, 1.0], false)],
        Kind::Mixer => vec![p("in 1", [-7.0, 0.0], [-1.0, 0.0], true), p("in 2", [0.0, 7.0], [0.0, 1.0], true), p("out", [7.0, 0.0], [1.0, 0.0], false)],
        Kind::Splitter => vec![p("in", [-7.0, 0.0], [-1.0, 0.0], true), p("out 1", [7.0, 0.0], [1.0, 0.0], false), p("out 2", [0.0, 7.0], [0.0, 1.0], false)],
        Kind::Valve => vec![p("in", [-14.0, 0.0], [-1.0, 0.0], true), p("out", [14.0, 0.0], [1.0, 0.0], false)],
        Kind::Source => vec![p("out", [16.0, 0.0], [1.0, 0.0], false)],
        Kind::Sink => vec![p("in", [-16.0, 0.0], [-1.0, 0.0], true)],
        Kind::CoolingTower => vec![
            p("water in", [21.0, -14.0], [1.0, 0.0], true),
            p("water out", [0.0, 32.0], [0.0, 1.0], false),
            p("air in", [-25.0, 16.0], [-1.0, 0.0], true),
            p("air out", [0.0, -32.0], [0.0, -1.0], false),
        ],
        Kind::Pipe => vec![p("in", [-18.0, 0.0], [-1.0, 0.0], true), p("out", [18.0, 0.0], [1.0, 0.0], false)],
        Kind::Generator | Kind::Motor | Kind::Label => vec![],
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Comp {
    pub id: u32,
    pub kind: Kind,
    pub pos: [f32; 2],
    /// quarter turns, clockwise
    pub rot: u8,
    pub flip: bool,
    pub label: String,
    /// scalar variables shown next to the component (e.g. `W_t`)
    pub vars: Vec<String>,
    /// Machines on the shaft of a generator or motor (component ids).
    #[serde(default)]
    pub shaft: Vec<u32>,
    /// Component parameters for the results (number in SI or variable
    /// name): `U` of a heat exchanger, `eta_mech`/`eta_elec` of a generator...
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, String>,
    /// Connection point chosen for each port (index into the port's
    /// default followed by [`anchor_options`]; missing or 0 = default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<u8>,
}

impl Comp {
    /// Local -> world transform (mirror in x, then rotate).
    fn tf(&self, p: [f32; 2]) -> [f32; 2] {
        let x = if self.flip { -p[0] } else { p[0] };
        let (x, y) = match self.rot % 4 {
            0 => (x, p[1]),
            1 => (-p[1], x),
            2 => (-x, -p[1]),
            _ => (p[1], -x),
        };
        [self.pos[0] + x, self.pos[1] + y]
    }
    fn tdir(&self, d: [f32; 2]) -> [f32; 2] {
        let t = self.tf(d);
        [t[0] - self.pos[0], t[1] - self.pos[1]]
    }
    fn port(&self, k: usize) -> Option<(Port, [f32; 2], [f32; 2])> {
        let mut p = *ports(self.kind).get(k)?;
        let a = self.anchors.get(k).copied().unwrap_or(0) as usize;
        if a > 0 {
            if let Some((_, pos, dir)) = anchor_options(self.kind).get(a - 1) {
                p.pos = *pos;
                p.dir = *dir;
            }
        }
        Some((p, self.tf(p.pos), self.tdir(p.dir)))
    }
    fn bbox(&self) -> ([f32; 2], [f32; 2]) {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        let mut add = |p: [f32; 2]| {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        };
        for pr in shape(self.kind) {
            match pr {
                Prim::Poly { pts, .. } => pts.iter().for_each(|p| add(self.tf(*p))),
                Prim::Circle { c, r, .. } => {
                    add(self.tf([c[0] - r, c[1] - r]));
                    add(self.tf([c[0] + r, c[1] + r]));
                }
                Prim::Text { p, .. } => add(self.tf(p)),
                Prim::Fill { pts, .. } => pts.iter().for_each(|p| add(self.tf(*p))),
            }
        }
        if lo[0] > hi[0] {
            let w = (self.label.len() as f32 * 3.5).max(20.0);
            return ([self.pos[0] - w, self.pos[1] - 8.0], [self.pos[0] + w, self.pos[1] + 8.0]);
        }
        (lo, hi)
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub id: u32,
    pub from: (u32, usize),
    pub to: (u32, usize),
    /// State number: the stream shows `base[state]` for every state variable.
    pub state: Option<i64>,
    /// Line colour (e.g. by fluid); `None` uses the default line colour.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// CoolProp fluid of the stream ("" = from the colour preset or the
    /// model's main fluid, "-" = none / mixture).
    #[serde(default)]
    pub fluid: String,
    /// Position of the stream label along the route, as a fraction of its
    /// length (`None`: middle).
    #[serde(default)]
    pub label_t: Option<f32>,
    /// Offset of the value label from its point on the route (world units;
    /// `None`: beside the stream).
    #[serde(default)]
    pub label_off: Option<[f32; 2]>,
}

/// How state values are written on the streams.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum LabelStyle {
    /// `T = 550 C` lines in a box
    Box,
    /// Cross label: P | T over m | h, numbers only, units in a legend
    #[default]
    Cross,
}

/// Size of the stream labels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum LabelSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl LabelSize {
    fn factor(self) -> f32 {
        match self {
            LabelSize::Small => 0.82,
            LabelSize::Medium => 1.0,
            LabelSize::Large => 1.3,
        }
    }
}

/// Quadrants of the cross label: top left, top right, bottom left, bottom right.
pub const CROSS_SLOTS: [(&str, &[&str]); 4] = [("P", &["p", "pres"]), ("T", &["t", "temp"]), ("m", &["m", "m_dot", "mdot"]), ("h", &["h", "enth"])];

/// Stream colour presets (common heat-balance convention).
pub const STREAM_COLORS: [(&str, [u8; 3]); 6] = [
    ("CO2 / working fluid", [200, 0, 200]),
    ("Water", [0, 60, 220]),
    ("Exhaust gas / fuel", [220, 0, 0]),
    ("Air", [0, 150, 70]),
    ("Steam", [230, 120, 0]),
    ("Grey", [120, 120, 120]),
];

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Kpi {
    pub label: String,
    pub var: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CycleDiagram {
    pub title: String,
    pub comps: Vec<Comp>,
    pub links: Vec<Link>,
    /// Array bases displayed on every stream (e.g. T, P, h, m).
    pub state_vars: Vec<String>,
    pub kpis: Vec<Kpi>,
    pub show_values: bool,
    pub offset: [f32; 2],
    pub zoom: f32,
    pub next_id: u32,
    /// Fluid properties also shown on the streams (`rho`, `v`, `cp`).
    pub derived: Vec<String>,
    /// Dead-state temperature for exergy results (K).
    pub t0: f64,
    /// Property diagram for the component process paths.
    pub path_diagram: crate::components::PathDiagram,
    /// Draw the KPI box over the canvas.
    pub kpi_box: bool,
    pub label_style: LabelStyle,
    /// Array bases in the four cross quadrants ("" = detect P, T, m, h).
    pub cross_vars: [String; 4],
    /// Show the state number of each stream.
    pub show_numbers: bool,
    /// Locks against moving or deleting by mistake.
    pub lock_comps: bool,
    pub lock_text: bool,
    pub lock_streams: bool,
    pub lock_labels: bool,
    /// Stream labels grow and shrink with the zoom (off: fixed screen size).
    pub label_zoom: bool,
    /// Cross labels centred on the stream line instead of beside it.
    pub cross_on_line: bool,
    pub label_size: LabelSize,
}

impl Default for CycleDiagram {
    fn default() -> Self {
        CycleDiagram {
            title: String::new(),
            comps: Vec::new(),
            links: Vec::new(),
            state_vars: Vec::new(),
            kpis: Vec::new(),
            show_values: true,
            offset: [60.0, 60.0],
            zoom: 1.0,
            next_id: 1,
            derived: Vec::new(),
            t0: 298.15,
            path_diagram: Default::default(),
            kpi_box: true,
            label_style: LabelStyle::Cross,
            cross_vars: Default::default(),
            show_numbers: false,
            lock_comps: true,
            lock_text: true,
            lock_streams: true,
            lock_labels: true,
            label_zoom: true,
            cross_on_line: true,
            label_size: LabelSize::Small,
        }
    }
}

impl CycleDiagram {
    fn comp(&self, id: u32) -> Option<&Comp> {
        self.comps.iter().find(|c| c.id == id)
    }
    /// Whether a component may be moved or deleted.
    fn comp_locked(&self, id: u32) -> bool {
        self.comp(id).is_some_and(|c| if c.kind == Kind::Label { self.lock_text } else { self.lock_comps })
    }
    fn new_id(&mut self) -> u32 {
        self.next_id = self.next_id.max(self.comps.iter().map(|c| c.id).chain(self.links.iter().map(|l| l.id)).max().unwrap_or(0) + 1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    /// World polyline of a link: stubs out of both ports and an orthogonal
    /// (Manhattan) route between them.
    fn route(&self, l: &Link) -> Option<Vec<[f32; 2]>> {
        let (_, a, da) = self.comp(l.from.0)?.port(l.from.1)?;
        let (_, b, db) = self.comp(l.to.0)?.port(l.to.1)?;
        let boxes: Vec<Bx> = self.comps.iter().filter(|c| c.kind != Kind::Label).map(|c| c.bbox()).collect();
        Some(route(a, da, b, db, &boxes))
    }
    fn bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for c in &self.comps {
            let (a, b) = c.bbox();
            lo = [lo[0].min(a[0]), lo[1].min(a[1])];
            hi = [hi[0].max(b[0]), hi[1].max(b[1])];
        }
        for l in &self.links {
            for p in self.route(l).unwrap_or_default() {
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }
}

type Bx = ([f32; 2], [f32; 2]);

/// Does the axis-aligned segment p-q pass through the interior of box `b`?
fn seg_hits_box(p: [f32; 2], q: [f32; 2], b: &Bx) -> bool {
    let (lo, hi) = (([b.0[0] + 2.0, b.0[1] + 2.0]), [b.1[0] - 2.0, b.1[1] - 2.0]);
    let (x0, x1) = (p[0].min(q[0]), p[0].max(q[0]));
    let (y0, y1) = (p[1].min(q[1]), p[1].max(q[1]));
    x1 > lo[0] && x0 < hi[0] && y1 > lo[1] && y0 < hi[1]
}

/// Orthogonal route from port a (direction da) to port b (direction db).
///
/// Candidates: a straight segment when the ports are aligned, one-bend (L)
/// routes, two-bend (Z) routes through the midpoint, and routes with short
/// stubs out of both ports through the stub coordinates, the midpoint or
/// around all components. The score is length + a bend penalty large enough
/// to prefer fewer turns over a slightly shorter path + penalties for
/// crossing component boxes and for leaving or entering a port sideways or
/// backwards.
fn route(a: [f32; 2], da: [f32; 2], b: [f32; 2], db: [f32; 2], boxes: &[Bx]) -> Vec<[f32; 2]> {
    const STUB: f32 = 14.0;
    const BEND: f32 = 70.0;
    let a1 = [a[0] + da[0] * STUB, a[1] + da[1] * STUB];
    let b1 = [b[0] + db[0] * STUB, b[1] + db[1] * STUB];
    let (mut gx0, mut gy0, mut gx1, mut gy1) = (a1[0].min(b1[0]), a1[1].min(b1[1]), a1[0].max(b1[0]), a1[1].max(b1[1]));
    for bx in boxes {
        gx0 = gx0.min(bx.0[0]);
        gy0 = gy0.min(bx.0[1]);
        gx1 = gx1.max(bx.1[0]);
        gy1 = gy1.max(bx.1[1]);
    }
    // inner points of each candidate (a and b are added around them)
    let mut cands: Vec<Vec<[f32; 2]>> = vec![Vec::new(), vec![[b[0], a[1]]], vec![[a[0], b[1]]]];
    let (mx, my) = (0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]));
    cands.push(vec![[mx, a[1]], [mx, b[1]]]);
    cands.push(vec![[a[0], my], [b[0], my]]);
    cands.push(vec![a1, [b1[0], a1[1]], b1]);
    cands.push(vec![a1, [a1[0], b1[1]], b1]);
    for y in [a1[1], b1[1], 0.5 * (a1[1] + b1[1]), gy0 - 20.0, gy1 + 20.0] {
        cands.push(vec![a1, [a1[0], y], [b1[0], y], b1]);
    }
    for x in [a1[0], b1[0], 0.5 * (a1[0] + b1[0]), gx0 - 20.0, gx1 + 20.0] {
        cands.push(vec![a1, [x, a1[1]], [x, b1[1]], b1]);
    }
    // the boxes of the two end components: their own stubs touch them
    let own = |p: [f32; 2]| boxes.iter().position(|bx| p[0] >= bx.0[0] - 1.0 && p[0] <= bx.1[0] + 1.0 && p[1] >= bx.0[1] - 1.0 && p[1] <= bx.1[1] + 1.0);
    let (own_a, own_b) = (own(a), own(b));
    let full = |c: &[[f32; 2]]| -> Vec<[f32; 2]> {
        let mut f = vec![a];
        f.extend_from_slice(c);
        f.push(b);
        f.dedup_by(|p, q| (p[0] - q[0]).abs() < 0.01 && (p[1] - q[1]).abs() < 0.01);
        simplify(f)
    };
    let score = |f: &[[f32; 2]]| -> f32 {
        let n = f.len() - 1;
        // only orthogonal routes
        if f.windows(2).any(|w| (w[0][0] - w[1][0]).abs() > 0.01 && (w[0][1] - w[1][1]).abs() > 0.01) {
            return f32::MAX;
        }
        let mut len = 0.0;
        let mut hits = 0;
        for (i, w) in f.windows(2).enumerate() {
            len += (w[1][0] - w[0][0]).abs() + (w[1][1] - w[0][1]).abs();
            for (k, bx) in boxes.iter().enumerate() {
                // a port may sit inside its own component's box: ignore the
                // first STUB length of the segment leaving (or entering) it
                let (mut p, mut q) = (w[0], w[1]);
                let l = (q[0] - p[0]).abs() + (q[1] - p[1]).abs();
                if i == 0 && Some(k) == own_a {
                    if l <= STUB {
                        continue;
                    }
                    let t = STUB / l;
                    p = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
                }
                if i + 1 == n && Some(k) == own_b {
                    if l <= STUB {
                        continue;
                    }
                    let t = STUB / l;
                    q = [q[0] + (p[0] - q[0]) * t, q[1] + (p[1] - q[1]) * t];
                }
                if seg_hits_box(p, q, bx) {
                    hits += 1;
                }
            }
        }
        // leave a port along its direction and enter the other against its direction
        let dir = |p: [f32; 2], q: [f32; 2]| {
            let l = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt().max(1e-6);
            [(q[0] - p[0]) / l, (q[1] - p[1]) / l]
        };
        let d0 = dir(f[0], f[1]);
        let d1 = dir(f[n - 1], f[n]);
        let bad_out = (d0[0] * da[0] + d0[1] * da[1]) < 0.5;
        let bad_in = (d1[0] * db[0] + d1[1] * db[1]) > -0.5;
        len + BEND * (n - 1) as f32 + 2000.0 * hits as f32 + 400.0 * (bad_out as u32 + bad_in as u32) as f32
    };
    cands.iter().map(|c| full(c)).min_by(|x, y| score(x).total_cmp(&score(y))).unwrap()
}

/// Drop collinear middle points of a polyline.
fn simplify(pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.len() >= 2 {
            let (u, v) = (out[out.len() - 2], out[out.len() - 1]);
            if ((u[0] - v[0]).abs() < 0.01 && (v[0] - p[0]).abs() < 0.01) || ((u[1] - v[1]).abs() < 0.01 && (v[1] - p[1]).abs() < 0.01) {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// Snap a moved component so that one of its connected ports lines up with
/// the port at the other end of its stream (straight streams need no
/// bends). Only small corrections (< `tol`) are applied, the smallest first.
fn align_ports(d: &mut CycleDiagram, id: u32, tol: f32) {
    let Some(c) = d.comp(id) else { return };
    let mut best: Option<(f32, usize, f32)> = None; // (|shift|, axis, shift)
    for l in &d.links {
        let (mine, other) = if l.from.0 == id {
            (l.from, l.to)
        } else if l.to.0 == id {
            (l.to, l.from)
        } else {
            continue;
        };
        let (Some((_, p, dp)), Some((_, q, _))) = (c.port(mine.1), d.comp(other.0).and_then(|o| o.port(other.1))) else { continue };
        // a horizontal port lines up in y, a vertical one in x
        let axis = if dp[0].abs() > dp[1].abs() { 1 } else { 0 };
        let shift = q[axis] - p[axis];
        if shift.abs() > 0.01 && shift.abs() < tol && best.is_none_or(|b| shift.abs() < b.0) {
            best = Some((shift.abs(), axis, shift));
        }
    }
    if let Some((_, axis, shift)) = best {
        if let Some(c) = d.comps.iter_mut().find(|c| c.id == id) {
            c.pos[axis] += shift;
        }
    }
}

/// Side of a component for its label (0 below, 1 above, 2 right, 3 left):
/// the side whose label box (w × h, world units) crosses the fewest streams
/// and other components (first side wins ties).
fn label_side(d: &CycleDiagram, c: &Comp, w: f32, h: f32) -> (usize, [f32; 2]) {
    let (lo, hi) = c.bbox();
    let mx = 0.5 * (lo[0] + hi[0]);
    let my = 0.5 * (lo[1] + hi[1]);
    let cands = [
        (0, [mx, hi[1] + 3.0], ([mx - w / 2.0, hi[1] + 3.0], [mx + w / 2.0, hi[1] + 3.0 + h])),
        (1, [mx, lo[1] - 3.0], ([mx - w / 2.0, lo[1] - 3.0 - h], [mx + w / 2.0, lo[1] - 3.0])),
        (2, [hi[0] + 5.0, my], ([hi[0] + 5.0, my - h / 2.0], [hi[0] + 5.0 + w, my + h / 2.0])),
        (3, [lo[0] - 5.0, my], ([lo[0] - 5.0 - w, my - h / 2.0], [lo[0] - 5.0, my + h / 2.0])),
    ];
    let routes: Vec<Vec<[f32; 2]>> = d.links.iter().filter_map(|l| d.route(l)).collect();
    let others: Vec<Bx> = d.comps.iter().filter(|o| o.id != c.id && o.kind != Kind::Label).map(|o| o.bbox()).collect();
    let overlap = |a: &Bx, b: &Bx| a.0[0] < b.1[0] && a.1[0] > b.0[0] && a.0[1] < b.1[1] && a.1[1] > b.0[1];
    let mut best = (usize::MAX, 0, cands[0].1);
    for (side, anchor, bx) in cands {
        let bx = ([bx.0[0] - 2.0, bx.0[1] - 2.0], [bx.1[0] + 2.0, bx.1[1] + 2.0]);
        let hits = routes.iter().filter(|r| r.windows(2).any(|s| seg_hits_box(s[0], s[1], &bx))).count() + 2 * others.iter().filter(|o| overlap(o, &bx)).count();
        if hits < best.0 {
            best = (hits, side, anchor);
            if hits == 0 {
                break;
            }
        }
    }
    (best.1, best.2)
}

/// Point at the fraction `f` of the length of a polyline and the direction there.
fn point_at(pts: &[[f32; 2]], f: f32) -> ([f32; 2], [f32; 2]) {
    let len: f32 = pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
    let target = len * f.clamp(0.0, 1.0);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        if acc + d >= target && d > 0.0 {
            let t = (target - acc) / d;
            return ([w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1])], [(w[1][0] - w[0][0]) / d, (w[1][1] - w[0][1]) / d]);
        }
        acc += d;
    }
    (pts[0], [1.0, 0.0])
}

/// Fraction of the length of a polyline at the point closest to `q`.
fn project_on(pts: &[[f32; 2]], q: [f32; 2]) -> f32 {
    let (mut best, mut best_f) = (f32::MAX, 0.5);
    let len: f32 = pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let d2 = dx * dx + dy * dy;
        let t = if d2 > 0.0 { (((q[0] - w[0][0]) * dx + (q[1] - w[0][1]) * dy) / d2).clamp(0.0, 1.0) } else { 0.0 };
        let (px, py) = (w[0][0] + t * dx, w[0][1] + t * dy);
        let dist = (q[0] - px).powi(2) + (q[1] - py).powi(2);
        if dist < best {
            best = dist;
            best_f = if len > 0.0 { (acc + t * d2.sqrt()) / len } else { 0.5 };
        }
        acc += d2.sqrt();
    }
    best_f
}

// ------------------------------------------------------------------ values

pub struct CycleInputs<'a> {
    pub analysis: &'a Analysis,
    pub display_units: &'a HashMap<String, String>,
    /// Script text (main fluid detection).
    pub script: &'a str,
}

/// Significant-figure formatting for labels.
pub fn fmt_sig(x: f64) -> String {
    if !x.is_finite() {
        return "—".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    if !(1e-3..1e6).contains(&a) {
        return format!("{x:.3e}");
    }
    let d = (3 - a.log10().floor() as i32).clamp(0, 6) as usize;
    let s = format!("{x:.d$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

impl CycleInputs<'_> {
    /// (display name, value, unit) of a variable in display units.
    fn value(&self, name: &str) -> Option<(String, f64, String)> {
        let m = &self.analysis.model;
        let v = m.var(name)?;
        let sol = self.analysis.solution.as_ref()?;
        if !sol.known[v as usize] {
            return None;
        }
        let (mu, du) = conv(m, self.display_units, v);
        let x = mu.convert(sol.x[v as usize], &du);
        let unit = if du.symbol == "-" { String::new() } else { du.symbol };
        Some((m.vars[v as usize].display.clone(), x, unit))
    }
    fn base_display(&self, base: &str) -> String {
        self.analysis
            .model
            .vars
            .iter()
            .find(|v| v.base == base && v.index.len() == 1)
            .map(|v| v.display.split('[').next().unwrap_or(base).to_string())
            .unwrap_or_else(|| base.to_string())
    }
    /// Lines `T = 550 C` of a state.
    pub(crate) fn state_lines(&self, d: &CycleDiagram, k: i64) -> Vec<String> {
        d.state_vars
            .iter()
            .filter_map(|b| {
                let (_, x, u) = self.value(&format!("{b}[{k}]"))?;
                Some(format!("{} = {} {}", self.base_display(b), fmt_sig(x), u).trim_end().to_string())
            })
            .collect()
    }
    /// Array bases of the four cross quadrants (explicit or detected).
    fn cross_bases(&self, d: &CycleDiagram) -> [Option<String>; 4] {
        let bases = self.array_bases();
        std::array::from_fn(|i| {
            let set = d.cross_vars[i].trim();
            if !set.is_empty() {
                return Some(set.to_lowercase());
            }
            CROSS_SLOTS[i].1.iter().find(|c| bases.iter().any(|b| b == *c)).map(|c| c.to_string())
        })
    }
    /// The four numbers of a cross label (no names, no units).
    fn cross_values(&self, d: &CycleDiagram, k: i64) -> [String; 4] {
        let b = self.cross_bases(d);
        std::array::from_fn(|i| b[i].as_ref().and_then(|b| self.value(&format!("{b}[{k}]"))).map(|v| fmt_sig(v.1)).unwrap_or_default())
    }
    /// Units of the four quadrants (legend).
    fn cross_units(&self, d: &CycleDiagram) -> [String; 4] {
        let b = self.cross_bases(d);
        std::array::from_fn(|i| {
            let Some(b) = b[i].as_ref() else { return String::new() };
            d.links.iter().filter_map(|l| l.state).find_map(|k| self.value(&format!("{b}[{k}]"))).map(|v| if v.2.is_empty() { "-".into() } else { v.2 }).unwrap_or_default()
        })
    }
    fn array_bases(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for v in &self.analysis.model.vars {
            if v.index.len() == 1 && !out.contains(&v.base) {
                out.push(v.base.clone());
            }
        }
        out
    }
    fn scalars(&self) -> Vec<String> {
        let mut v: Vec<String> = self.analysis.model.vars.iter().filter(|v| v.index.is_empty()).map(|v| v.display.clone()).collect();
        v.sort_by(|a, b| crate::app::natord(&a.to_lowercase(), &b.to_lowercase()));
        v
    }
}

// ------------------------------------------------------------------ state table

/// Table of the diagram's states: the chosen stream variables, then mass
/// flow and entropy from the model, then quality, superheating/subcooling,
/// density and cp from CoolProp at (P, h) of each stream's fluid.
#[derive(Clone, Debug, Default)]
pub struct StateTable {
    /// (name, unit) of each column after the state number
    pub heads: Vec<(String, String)>,
    pub rows: Vec<(i64, Vec<String>)>,
}

impl StateTable {
    /// Tab-separated text with a header line of names and units.
    pub fn tsv(&self) -> String {
        let mut s = String::from("State");
        for (h, u) in &self.heads {
            s.push('\t');
            s.push_str(h);
            if !u.is_empty() {
                s.push_str(&format!(" [{u}]"));
            }
        }
        s.push('\n');
        for (k, r) in &self.rows {
            s.push_str(&k.to_string());
            for v in r {
                s.push('\t');
                s.push_str(v);
            }
            s.push('\n');
        }
        s
    }
}

pub fn state_table(d: &CycleDiagram, inp: &CycleInputs) -> StateTable {
    use crate::components::{bases, main_fluid, state, stream_fluid_ex, Phase};
    use dfcore::props::Key;
    use dfcore::units::Dim;
    let mut states: Vec<i64> = d.links.iter().filter_map(|l| l.state).collect();
    states.sort_unstable();
    states.dedup();
    if states.is_empty() {
        return StateTable::default();
    }
    let b = bases(inp);
    let main = main_fluid(inp.script);
    let sts: Vec<_> = states
        .iter()
        .map(|k| {
            let l = d.links.iter().find(|l| l.state == Some(*k)).unwrap();
            state(inp, &b, *k, stream_fluid_ex(l, &main))
        })
        .collect();
    let us = &inp.analysis.model.unit_system;
    let shown = |base: &str| d.state_vars.iter().any(|v| v.eq_ignore_ascii_case(base));
    let model_col = |base: &str| -> (String, String, Vec<String>) {
        let unit = states.iter().find_map(|k| inp.value(&format!("{base}[{k}]"))).map(|v| v.2).unwrap_or_default();
        let vals = states.iter().map(|k| inp.value(&format!("{base}[{k}]")).map(|v| fmt_sig(v.1)).unwrap_or_else(|| "—".into())).collect();
        (inp.base_display(base), unit, vals)
    };
    let mut cols: Vec<(String, String, Vec<String>)> = d.state_vars.iter().map(|v| model_col(v)).collect();
    if let Some(m) = b.m.as_ref().filter(|m| !shown(m)) {
        cols.push(model_col(m));
    }
    let any_fluid = sts.iter().any(|s| s.fluid.is_some());
    let cp_col = |name: &str, key: Key, f: &dyn Fn(&crate::components::St) -> f64| -> (String, String, Vec<String>) {
        let u = us.unit_for(key.dim());
        let vals = sts.iter().map(|s| if s.fluid.is_some() { f(s) } else { f64::NAN }).map(|x| if x.is_finite() { fmt_sig(u.from_si(x)) } else { "—".into() }).collect();
        (name.to_string(), if u.symbol == "-" { String::new() } else { u.symbol.clone() }, vals)
    };
    match b.s.as_ref() {
        Some(sb) if !shown(sb) => {
            // states the model leaves without s: CoolProp, when its reference matches the model's
            let mut c = model_col(sb);
            let u = us.unit_for(Dim::SPEC_ENTROPY);
            for (i, st) in sts.iter().enumerate() {
                if c.2[i] == "—" && st.fluid.is_some() && st.h_off == 0.0 && c.1 == u.symbol {
                    if let Some(x) = st.s_cp() {
                        c.2[i] = fmt_sig(u.from_si(x));
                    }
                }
            }
            cols.push(c);
        }
        None if any_fluid => cols.push(cp_col("s", Key::S, &|s| s.s_cp().unwrap_or(f64::NAN))),
        _ => {}
    }
    if any_fluid {
        let phases: Vec<Phase> = sts.iter().map(|s| s.phase()).collect();
        if !shown("x") {
            let vals = phases
                .iter()
                .map(|p| match p {
                    Phase::TwoPhase(x) => format!("{x:.4}"),
                    Phase::Subcooled(_) => "liquid".into(),
                    Phase::Superheated(_) => "vapour".into(),
                    Phase::Supercritical => "supercritical".into(),
                    Phase::Unknown => "—".into(),
                })
                .collect();
            cols.push(("x".into(), String::new(), vals));
        }
        // temperature difference in the unit system's degree size
        let tu = us.unit_for(Dim::TEMP);
        let (f, unit) = if (tu.factor - 1.0).abs() < 1e-12 { (1.0, "K".to_string()) } else { (1.0 / tu.factor, tu.symbol.clone()) };
        let vals = phases
            .iter()
            .map(|p| match p {
                Phase::Superheated(dt) => format!("+{}", fmt_sig(dt * f)),
                Phase::Subcooled(dt) => format!("-{}", fmt_sig(dt * f)),
                Phase::TwoPhase(_) => "sat".into(),
                _ => "—".into(),
            })
            .collect();
        cols.push(("dT_sat".into(), unit, vals));
        if !shown("rho") && !shown("d") {
            cols.push(cp_col("rho", Key::D, &|s| s.derived(Key::D)));
        }
        if !shown("cp") {
            cols.push(cp_col("cp", Key::Cp, &|s| s.derived(Key::Cp)));
        }
        cols.push(("fluid".into(), String::new(), sts.iter().map(|s| s.fluid.clone().unwrap_or_else(|| "—".into())).collect()));
    }
    StateTable {
        heads: cols.iter().map(|c| (c.0.clone(), c.1.clone())).collect(),
        rows: states.iter().enumerate().map(|(i, k)| (*k, cols.iter().map(|c| c.2[i].clone()).collect())).collect(),
    }
}

// ------------------------------------------------------------------ UI

#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Comp(u32),
    Link(u32),
}

enum Drag {
    Move(u32),
    /// stream label: along the route, or free (Shift) from `off0`
    Label {
        id: u32,
        free: bool,
        off: [f32; 2],
    },
    Wire {
        from: (u32, usize),
        a: [f32; 2],
    },
    Pan,
}

#[derive(Default)]
pub struct CycleUi {
    sel: Option<Sel>,
    drag: Option<Drag>,
    /// canvas rectangle of the previous frame (drop target of the palette)
    canvas: Option<Rect>,
    message: String,
    /// Component whose results window is open.
    pub popup: Option<u32>,
    /// Component selected in the Components tab, and the view shown there.
    pub tab_sel: Option<u32>,
    pub tab_view: usize,
    tab_filter: String,
    cache: Option<(u64, u32, std::sync::Arc<crate::components::Report>)>,
    /// Screen rectangles of the stream labels drawn in the last frame, with
    /// the label offset in use (world units).
    label_rects: Vec<(u32, Rect, [f32; 2])>,
    /// Width of the KPI box drawn over the canvas (kept clear by Fit).
    kpi_w: f32,
    /// Label drag mode: free placement instead of sliding along the stream.
    free_drag: bool,
    /// Undo / redo history of the diagram.
    hist: History,
    /// Equations of an inserted template, for an empty script (taken by the app).
    pub script_request: Option<String>,
    /// Tab of the right sidebar.
    side_tab: SideTab,
    /// State table, cached on the solution and the diagram streams.
    table_cache: Option<(u64, std::sync::Arc<StateTable>)>,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum SideTab {
    #[default]
    Kpi,
    States,
}

/// Undo history: snapshots of the diagram (JSON, without the pan and zoom,
/// which are not edits). A change is committed once no mouse button is held
/// and no text field has the keyboard, so a drag or a typed name is one step.
#[derive(Default)]
struct History {
    undo: Vec<String>,
    redo: Vec<String>,
    /// The committed state.
    cur: Option<String>,
}

const HISTORY_MAX: usize = 200;

fn snapshot(d: &CycleDiagram) -> String {
    let mut c = d.clone();
    c.offset = [0.0, 0.0];
    c.zoom = 1.0;
    serde_json::to_string(&c).unwrap_or_default()
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

struct View {
    origin: Pos2,
    zoom: f32,
}

impl View {
    fn s(&self, p: [f32; 2]) -> Pos2 {
        self.origin + Vec2::new(p[0], p[1]) * self.zoom
    }
    fn w(&self, p: Pos2) -> [f32; 2] {
        let v = (p - self.origin) / self.zoom;
        [v.x, v.y]
    }
}

/// Draw a component; `inner(k)` is the colour of the stream entering port
/// `k`, used for the passes through the component.
fn paint_comp(painter: &egui::Painter, c: &Comp, view: &View, line: Color32, selected: bool, dark: bool, inner: &dyn Fn(usize) -> Option<Color32>) {
    let z = view.zoom;
    let stroke = Stroke::new(1.6 * z, if selected { Color32::from_rgb(255, 170, 0) } else { line });
    for (i, pr) in shape(c.kind).into_iter().enumerate() {
        let st = match inner_port(c.kind, i).and_then(inner) {
            Some(col) if !selected => Stroke::new(2.0 * z, col),
            _ => stroke,
        };
        match pr {
            Prim::Poly { pts, closed, fill } => {
                let p: Vec<Pos2> = pts.iter().map(|q| view.s(c.tf(*q))).collect();
                if closed {
                    let mut path = egui::epaint::PathShape::closed_line(p, st);
                    path.fill = fill.map(|f| theme_fill(f, dark)).unwrap_or(Color32::TRANSPARENT);
                    painter.add(path);
                } else {
                    painter.add(egui::Shape::line(p, st));
                }
            }
            Prim::Circle { c: cc, r, fill } => {
                painter.circle(view.s(c.tf(cc)), r * z, fill.map(|f| theme_fill(f, dark)).unwrap_or(Color32::TRANSPARENT), st);
            }
            Prim::Text { p, s, size } => {
                painter.text(view.s(c.tf(p)), egui::Align2::CENTER_CENTER, s, egui::FontId::proportional(size * z), line);
            }
            Prim::Fill { pts, fill } => {
                let p: Vec<Pos2> = pts.iter().map(|q| view.s(c.tf(*q))).collect();
                painter.add(egui::Shape::convex_polygon(p, theme_fill(fill, dark), Stroke::NONE));
            }
        }
    }
}

impl CycleUi {
    /// Forget the undo history (another project was opened).
    pub fn clear_history(&mut self) {
        self.hist = History::default();
    }

    /// Commit the current diagram to the history when it changed and no
    /// edit is in progress.
    fn track(&mut self, ctx: &egui::Context, d: &CycleDiagram) {
        if ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input() {
            return;
        }
        let s = snapshot(d);
        match self.hist.cur.take() {
            Some(c) if c != s => {
                self.hist.undo.push(c);
                if self.hist.undo.len() > HISTORY_MAX {
                    self.hist.undo.remove(0);
                }
                self.hist.redo.clear();
            }
            _ => {}
        }
        self.hist.cur = Some(s);
    }

    /// Undo (`redo = false`) or redo the last diagram edit. Returns true when
    /// the diagram changed. The view (pan and zoom) is kept.
    pub fn undo_redo(&mut self, d: &mut CycleDiagram, redo: bool) -> bool {
        // an edit not yet committed (e.g. typed into a field) counts first
        let now = snapshot(d);
        match self.hist.cur.take() {
            Some(c) if c != now => {
                self.hist.undo.push(c);
                self.hist.redo.clear();
            }
            _ => {}
        }
        let h = &mut self.hist;
        let (from, to) = if redo { (&mut h.redo, &mut h.undo) } else { (&mut h.undo, &mut h.redo) };
        let Some(prev) = from.pop() else {
            h.cur = Some(now);
            return false;
        };
        let Ok(mut nd) = serde_json::from_str::<CycleDiagram>(&prev) else {
            h.cur = Some(now);
            return false;
        };
        to.push(now);
        nd.offset = d.offset;
        nd.zoom = d.zoom;
        *d = nd;
        h.cur = Some(prev);
        self.sel = None;
        self.drag = None;
        self.cache = None;
        true
    }

    /// Ctrl+Z, Ctrl+Y and Ctrl+Shift+Z on the diagram (not while typing in a
    /// field, which has its own undo).
    fn history_keys(&mut self, ui: &egui::Ui, d: &mut CycleDiagram) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let (redo, undo) = ui.input_mut(|i| {
            let r = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            (r, i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
        });
        if undo || redo {
            let changed = self.undo_redo(d, redo);
            self.message = match (changed, redo) {
                (true, false) => "Undone".into(),
                (true, true) => "Redone".into(),
                (false, false) => "Nothing to undo".into(),
                (false, true) => "Nothing to redo".into(),
            };
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        self.history_keys(ui, d);
        self.track(ui.ctx(), d);
        egui::Panel::left("cycle_side").resizable(true).default_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side(ui, d, inp));
        });
        egui::Panel::right("cycle_kpi").resizable(true).default_size(330.0).show(ui, |ui| self.right(ui, d, inp));
        self.canvas_ui(ui, d, inp);
        if let Some(id) = self.popup {
            let mut open = true;
            let title = d.comp(id).map(|c| format!("{} — results", c.label)).unwrap_or_default();
            egui::Window::new(title).id(egui::Id::NULL.with(("comp_popup", id))).open(&mut open).default_size([640.0, 560.0]).resizable(true).show(ui.ctx(), |ui| {
                let r = self.report(d, inp, id);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if crate::components::show_report(ui, &r, None, &mut d.path_diagram, ("popup", id)) {
                        self.cache = None;
                    }
                });
            });
            if !open || d.comp(id).is_none() {
                self.popup = None;
            }
        }
    }

    /// Component report, cached on the analysis and the diagram content.
    pub fn report(&mut self, d: &CycleDiagram, inp: &CycleInputs, id: u32) -> std::sync::Arc<crate::components::Report> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (inp.analysis as *const Analysis as usize).hash(&mut h);
        inp.analysis.gen.hash(&mut h);
        inp.analysis.solution.as_ref().map(|s| s.elapsed_ms.to_bits()).hash(&mut h);
        serde_json::to_string(&(&d.comps, &d.links, d.t0, d.path_diagram)).unwrap_or_default().hash(&mut h);
        let key = h.finish();
        if let Some((k, i, r)) = &self.cache {
            if *k == key && *i == id {
                return r.clone();
            }
        }
        let r = std::sync::Arc::new(crate::components::report(d, inp, id, d.path_diagram));
        self.cache = Some((key, id, r.clone()));
        r
    }

    /// Components tab: list of the diagram's components and their results.
    pub fn components_tab(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        self.history_keys(ui, d);
        self.track(ui.ctx(), d);
        let mut items: Vec<(Kind, String, u32)> = d.comps.iter().filter(|c| c.kind != Kind::Label).map(|c| (c.kind, c.label.clone(), c.id)).collect();
        items.sort_by(|a, b| a.0.name().cmp(b.0.name()).then(crate::app::natord(&a.1, &b.1)));
        if items.is_empty() {
            ui.label(RichText::new("No components yet: draw the flowsheet in the Cycle diagram tab and bind its streams to state numbers.").weak());
            return;
        }
        if self.tab_sel.is_none_or(|s| !items.iter().any(|i| i.2 == s)) {
            self.tab_sel = Some(items[0].2);
        }
        egui::Panel::left("comp_list").resizable(true).default_size(230.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.tab_filter);
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let f = self.tab_filter.to_lowercase();
                let mut last: Option<Kind> = None;
                for (k, label, id) in &items {
                    if !f.is_empty() && !label.to_lowercase().contains(&f) && !k.name().to_lowercase().contains(&f) {
                        continue;
                    }
                    if last != Some(*k) {
                        ui.add_space(4.0);
                        ui.label(RichText::new(k.name()).small().strong());
                        last = Some(*k);
                    }
                    if ui.selectable_label(self.tab_sel == Some(*id), label).clicked() {
                        self.tab_sel = Some(*id);
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Dead state T0");
                let mut t = d.t0 - 273.15;
                if ui.add(egui::DragValue::new(&mut t).speed(0.5).suffix(" C")).on_hover_text("Reference temperature of the exergy results").changed() {
                    d.t0 = t + 273.15;
                }
            });
        });
        let Some(id) = self.tab_sel else { return };
        egui::CentralPanel::default().show(ui, |ui| {
            let r = self.report(d, inp, id);
            ui.horizontal_wrapped(|ui| {
                let mut views = vec!["Summary".to_string(), "Streams".to_string()];
                views.extend(r.sections.iter().map(|s| s.title.clone()));
                views.extend(r.charts.iter().map(|c| c.title.split(" —").next().unwrap_or("").to_string()));
                views.push("Parameters".into());
                if self.tab_view >= views.len() {
                    self.tab_view = 0;
                }
                for (k, v) in views.iter().enumerate() {
                    ui.selectable_value(&mut self.tab_view, k, v);
                }
            });
            ui.separator();
            let n_tables = 2 + r.sections.len();
            let n_all = n_tables + r.charts.len();
            let mut body = |ui: &mut egui::Ui| {
                if self.tab_view == n_all {
                    crate::components::params_ui(ui, d, id, inp);
                } else if crate::components::show_report(ui, &r, if self.tab_view == 0 { None } else { Some(self.tab_view - 1) }, &mut d.path_diagram, ("tab", id)) {
                    self.cache = None;
                }
            };
            // a chart on its own fills the panel; tables scroll
            if self.tab_view >= n_tables && self.tab_view < n_all {
                body(ui);
            } else {
                egui::ScrollArea::vertical().show(ui, body);
            }
        });
    }

    fn side(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        ui.strong("Components");
        ui.label(RichText::new("Drag onto the canvas. Drag from a port (dot) to another port to connect.").small().weak());
        egui::Grid::new("palette").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, k) in Kind::ALL.iter().enumerate() {
                let r = ui.add(egui::Button::new(k.name()).min_size(Vec2::new(125.0, 22.0)).sense(Sense::click_and_drag()));
                if r.dragged() {
                    if let Some(p) = ui.ctx().pointer_interact_pos() {
                        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("cycle_ghost")));
                        let view = View { origin: p, zoom: d.zoom };
                        let ghost = Comp {
                            id: 0,
                            kind: *k,
                            pos: [0.0, 0.0],
                            rot: 0,
                            flip: false,
                            label: k.name().into(),
                            vars: Vec::new(),
                            shaft: Vec::new(),
                            params: Default::default(),
                            anchors: Vec::new(),
                        };
                        paint_comp(&painter, &ghost, &view, Color32::GRAY, false, ui.visuals().dark_mode, &|_| None);
                    }
                }
                let drop_at = if r.drag_stopped() { ui.ctx().pointer_interact_pos().filter(|p| self.canvas.is_some_and(|c| c.contains(*p))) } else { None };
                if r.clicked() || drop_at.is_some() {
                    let canvas = self.canvas.unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0)));
                    let view = View { origin: canvas.min + Vec2::from(d.offset), zoom: d.zoom };
                    let w = view.w(drop_at.unwrap_or(canvas.center()));
                    let id = d.new_id();
                    let n = d.comps.iter().filter(|c| c.kind == *k).count() + 1;
                    let label = if *k == Kind::Label { "Text".into() } else { format!("{} {n}", k.name().split(' ').next().unwrap_or("")) };
                    d.comps.push(Comp {
                        id,
                        kind: *k,
                        pos: [(w[0] / 10.0).round() * 10.0, (w[1] / 10.0).round() * 10.0],
                        rot: 0,
                        flip: false,
                        label,
                        vars: Vec::new(),
                        shaft: Vec::new(),
                        params: Default::default(),
                        anchors: Vec::new(),
                    });
                    self.sel = Some(Sel::Comp(id));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Template").on_hover_text("Insert a ready-made flowsheet. In an empty diagram it starts the drawing; with an empty script its equations are added too.");
            for t in &crate::templates::ALL {
                if ui.button(t.name).on_hover_text(t.about).clicked() {
                    let shift = crate::templates::insert(d, &(t.diagram)());
                    let script_empty = inp.script.trim().is_empty();
                    if script_empty && shift == 0 {
                        self.script_request = Some(t.script.to_string());
                    }
                    self.message = match (shift, script_empty) {
                        (0, true) => format!("{}: diagram and equations inserted.", t.name),
                        (0, false) => {
                            format!("{}: diagram inserted (states 1 to 4). The script already has equations, so the template's were not added; they are in examples/.", t.name)
                        }
                        (k, _) => format!("{}: diagram inserted with states {} to {} (renumber its streams or the equations to match).", t.name, k + 1, k + 4),
                    };
                    self.sel = None;
                    self.fit(d);
                }
            }
        });
        ui.separator();
        match self.sel {
            Some(Sel::Comp(id)) => {
                let scalars = inp.scalars();
                if let Some(c) = d.comps.iter_mut().find(|c| c.id == id) {
                    ui.strong(c.kind.name());
                    ui.horizontal(|ui| {
                        ui.label("Label");
                        ui.text_edit_singleline(&mut c.label);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("rotate 90°").on_hover_text("R").clicked() {
                            c.rot = (c.rot + 1) % 4;
                        }
                        if ui.button("mirror").on_hover_text("M").clicked() {
                            c.flip = !c.flip;
                        }
                    });
                    let opts = anchor_options(c.kind);
                    if !opts.is_empty() {
                        ui.add_space(4.0);
                        ui.label("Connection points").on_hover_text("Where each stream meets the casing; combine with rotate and mirror for the orientation");
                        let pl = ports(c.kind);
                        if c.anchors.len() < pl.len() {
                            c.anchors.resize(pl.len(), 0);
                        }
                        egui::Grid::new(("anchors", id)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                            for (k, p) in pl.iter().enumerate() {
                                ui.label(p.name);
                                let cur = c.anchors[k] as usize;
                                let text = if cur == 0 { "default".to_string() } else { opts.get(cur - 1).map(|o| o.0.clone()).unwrap_or_default() };
                                egui::ComboBox::from_id_salt(("anchor", id, k)).selected_text(text).width(190.0).show_ui(ui, |ui| {
                                    ui.selectable_value(&mut c.anchors[k], 0, "default");
                                    for (j, o) in opts.iter().enumerate() {
                                        ui.selectable_value(&mut c.anchors[k], (j + 1) as u8, &o.0);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                        if c.anchors.iter().all(|a| *a == 0) {
                            c.anchors.clear();
                        }
                    }
                    ui.add_space(4.0);
                    ui.label("Variables shown (e.g. power, duty):");
                    let mut remove = None;
                    for (k, v) in c.vars.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            var_combo(ui, ("cv", id, k), v, &scalars);
                            if ui.small_button("x").clicked() {
                                remove = Some(k);
                            }
                        });
                    }
                    if let Some(k) = remove {
                        c.vars.remove(k);
                    }
                    if ui.small_button("+ variable").clicked() {
                        c.vars.push(String::new());
                    }
                    ui.add_space(4.0);
                    if ui.button("Delete component").on_hover_text("Del").clicked() {
                        self.delete(d);
                    }
                }
            }
            Some(Sel::Link(id)) => {
                if let Some(l) = d.links.iter_mut().find(|l| l.id == id) {
                    ui.strong("Stream");
                    ui.horizontal(|ui| {
                        ui.label("State number");
                        let mut s = l.state.map(|s| s.to_string()).unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut s).desired_width(60.0)).changed() {
                            l.state = s.trim().parse().ok();
                        }
                    });
                    ui.label(RichText::new("The stream shows T[k], P[k], ... of state k from the solution.").small().weak());
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Colour");
                        if ui.selectable_label(l.color.is_none(), "default").clicked() {
                            l.color = None;
                        }
                        for (name, c) in STREAM_COLORS {
                            let on = l.color == Some(c);
                            let stroke = if on { Stroke::new(2.0, ui.visuals().strong_text_color()) } else { Stroke::NONE };
                            if ui.add(egui::Button::new("").fill(c32(c)).stroke(stroke).min_size(Vec2::new(16.0, 16.0))).on_hover_text(name).clicked() {
                                l.color = Some(c);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Fluid");
                        let auto = crate::components::stream_fluid(&Link { fluid: String::new(), ..l.clone() }, &crate::components::main_fluid(inp.script));
                        let shown = match l.fluid.as_str() {
                            "" => format!("auto ({})", auto.clone().unwrap_or_else(|| "none".into())),
                            "-" => "none / mixture".into(),
                            f => f.to_string(),
                        };
                        egui::ComboBox::from_id_salt(("fluid", id)).selected_text(shown).height(300.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut l.fluid, String::new(), "auto (colour / main fluid)");
                            ui.selectable_value(&mut l.fluid, "-".to_string(), "none / mixture");
                            for f in ["Water", "CO2", "Air", "Nitrogen", "R134a", "R245fa", "Toluene", "Ammonia", "Helium", "Methane", "Propane"] {
                                ui.selectable_value(&mut l.fluid, f.to_string(), f);
                            }
                        });
                    });
                    ui.label(RichText::new("Used for the component results and the fluid properties on the labels.").small().weak());
                    if (l.label_t.is_some() || l.label_off.is_some()) && ui.small_button("Reset label position").clicked() {
                        l.label_t = None;
                        l.label_off = None;
                    }
                    if ui.button("Delete stream").on_hover_text("Del").clicked() {
                        self.delete(d);
                    }
                }
            }
            None => {
                ui.label(RichText::new("Select a component or stream to edit it.").weak());
                ui.label(RichText::new("Double-click a component for its results (also in the Components tab). The diagram settings are in the right sidebar.").small().weak());
            }
        }
        if let Some(Sel::Comp(id)) = self.sel {
            if d.comp(id).is_some_and(|c| c.kind != Kind::Label) {
                ui.separator();
                crate::components::params_ui(ui, d, id, inp);
                if ui.button("Results…").on_hover_text("Double-click the component on the canvas").clicked() {
                    self.popup = Some(id);
                }
            }
        }
        if !self.message.is_empty() {
            ui.separator();
            ui.label(RichText::new(&self.message).small().weak());
        }
    }

    /// Right sidebar: KPI and States tabs, then the diagram settings.
    fn right(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.side_tab, SideTab::Kpi, RichText::new("KPI").strong()).on_hover_text("Key performance indicators");
            ui.selectable_value(&mut self.side_tab, SideTab::States, RichText::new("States").strong()).on_hover_text("State table of the streams");
        });
        ui.separator();
        let h = (ui.available_height() * 0.45).max(120.0);
        egui::ScrollArea::vertical().id_salt("cycle_tab").max_height(h).auto_shrink([false, true]).show(ui, |ui| match self.side_tab {
            SideTab::Kpi => self.kpi_ui(ui, d, inp),
            SideTab::States => self.states_ui(ui, d, inp),
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("cycle_settings").show(ui, |ui| self.settings(ui, d, inp));
    }

    fn kpi_ui(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        let scalars = inp.scalars();
        let mut remove = None;
        for (k, kp) in d.kpis.iter_mut().enumerate() {
            ui.push_id(("kpi", k), |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut kp.label).desired_width(190.0).hint_text("label"));
                    if ui.small_button("x").clicked() {
                        remove = Some(k);
                    }
                });
                ui.horizontal(|ui| {
                    var_combo(ui, "var", &mut kp.var, &scalars);
                    match inp.value(&kp.var) {
                        Some((_, x, u)) => ui.label(RichText::new(format!("{} {u}", fmt_sig(x))).strong().monospace()),
                        None => ui.label(RichText::new("—").weak()),
                    };
                });
                ui.add_space(4.0);
            });
        }
        if let Some(k) = remove {
            d.kpis.remove(k);
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ KPI").clicked() {
                d.kpis.push(Kpi { label: String::new(), var: String::new() });
            }
            if ui.small_button("Suggest").on_hover_text("Add power, heat and efficiency variables of the model").clicked() {
                for v in &scalars {
                    let l = v.to_lowercase();
                    let hit = l.starts_with("eta")
                        || l.starts_with("w_")
                        || l.starts_with("w_dot")
                        || l.starts_with("q_")
                        || l.starts_with("cop")
                        || l.starts_with("p_net")
                        || l.starts_with("p_gross");
                    if hit && !d.kpis.iter().any(|k| k.var.eq_ignore_ascii_case(v)) {
                        d.kpis.push(Kpi { label: kpi_label(v), var: v.clone() });
                    }
                }
            }
        });
    }

    /// State table, recomputed when the solution or the streams change.
    fn state_table_cached(&mut self, d: &CycleDiagram, inp: &CycleInputs) -> std::sync::Arc<StateTable> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (inp.analysis as *const Analysis as usize).hash(&mut h);
        inp.analysis.gen.hash(&mut h);
        inp.analysis.solution.as_ref().map(|s| s.elapsed_ms.to_bits()).hash(&mut h);
        serde_json::to_string(&(&d.links, &d.state_vars)).unwrap_or_default().hash(&mut h);
        let mut du: Vec<_> = inp.display_units.iter().collect();
        du.sort();
        du.hash(&mut h);
        inp.script.hash(&mut h);
        let key = h.finish();
        if let Some((k, t)) = &self.table_cache {
            if *k == key {
                return t.clone();
            }
        }
        let t = std::sync::Arc::new(state_table(d, inp));
        self.table_cache = Some((key, t.clone()));
        t
    }

    fn states_ui(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        let t = self.state_table_cached(d, inp);
        if t.rows.is_empty() || t.heads.is_empty() {
            ui.label(RichText::new("Connect components and number their streams; the columns are the stream variables chosen below.").weak());
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Copy table (TSV)").on_hover_text("Paste into Excel or a text editor").clicked() {
                ui.ctx().copy_text(t.tsv());
            }
            ui.label(RichText::new("x, dT_sat, rho, cp: CoolProp at (P, h)").small().weak())
                .on_hover_text("dT_sat > 0: degree of superheating, T - T_dew; < 0: degree of subcooling, T_bubble - T (at the stream pressure)");
        });
        egui::ScrollArea::horizontal().id_salt("states_h").show(ui, |ui| {
            egui::Grid::new("states").striped(true).show(ui, |ui| {
                ui.strong("k");
                for (h, u) in &t.heads {
                    ui.vertical(|ui| {
                        ui.strong(h);
                        ui.label(RichText::new(u).small().weak());
                    });
                }
                ui.end_row();
                for (k, r) in &t.rows {
                    ui.label(k.to_string());
                    for v in r {
                        ui.label(RichText::new(v).monospace());
                    }
                    ui.end_row();
                }
            });
        });
    }

    /// Diagram-wide settings: locks, stream labels, export.
    fn settings(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        ui.strong("Lock");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.lock_comps, "components").on_hover_text("Components cannot be moved, rotated or deleted");
            ui.checkbox(&mut d.lock_streams, "streams").on_hover_text("No new connections; streams cannot be deleted");
            ui.checkbox(&mut d.lock_labels, "stream labels").on_hover_text("Unlock to drag stream labels: along the stream, or freely with Shift held");
            ui.checkbox(&mut d.lock_text, "text").on_hover_text("Text items cannot be moved or deleted");
        });
        if !d.lock_labels {
            ui.horizontal(|ui| {
                ui.label("Drag labels");
                ui.selectable_value(&mut self.free_drag, false, "along the stream");
                ui.selectable_value(&mut self.free_drag, true, "freely");
            });
            ui.label(RichText::new("Shift while dragging also places a label freely.").small().weak());
        }
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);
        ui.strong("Stream labels");
        ui.checkbox(&mut d.show_values, "Show state values on the diagram");
        ui.checkbox(&mut d.show_numbers, "Show state numbers");
        ui.horizontal(|ui| {
            ui.label("Size");
            for (z, n) in [(LabelSize::Small, "small"), (LabelSize::Medium, "medium"), (LabelSize::Large, "large")] {
                ui.selectable_value(&mut d.label_size, z, n);
            }
        });
        ui.checkbox(&mut d.label_zoom, "Resize labels with the view").on_hover_text("Off: labels keep the same size on screen at any zoom");
        ui.horizontal(|ui| {
            ui.label("Style");
            ui.selectable_value(&mut d.label_style, LabelStyle::Cross, "cross (P T / m h)").on_hover_text("Four numbers around a cross, units in the legend (top left)");
            ui.selectable_value(&mut d.label_style, LabelStyle::Box, "box");
        });
        if d.label_style == LabelStyle::Cross {
            ui.horizontal(|ui| {
                ui.label("Cross");
                ui.selectable_value(&mut d.cross_on_line, false, "beside the stream");
                ui.selectable_value(&mut d.cross_on_line, true, "on the stream").on_hover_text("The stream line passes through the middle of the cross");
            });
        }
        if d.label_style == LabelStyle::Cross {
            let bases = inp.array_bases();
            let auto = inp.cross_bases(&CycleDiagram::default());
            egui::Grid::new("cross_vars").num_columns(2).show(ui, |ui| {
                for i in 0..4 {
                    let pos = ["top left", "top right", "bottom left", "bottom right"][i];
                    ui.label(format!("{pos} ({})", CROSS_SLOTS[i].0));
                    let shown = if d.cross_vars[i].is_empty() {
                        format!("auto ({})", auto[i].as_deref().map(|b| inp.base_display(b)).unwrap_or_else(|| "none".into()))
                    } else {
                        inp.base_display(&d.cross_vars[i])
                    };
                    egui::ComboBox::from_id_salt(("cross", i)).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.cross_vars[i], String::new(), "auto");
                        for b in &bases {
                            ui.selectable_value(&mut d.cross_vars[i], b.clone(), inp.base_display(b));
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if d.links.iter().any(|l| l.label_t.is_some() || l.label_off.is_some()) && ui.small_button("Reset all label positions").clicked() {
            for l in &mut d.links {
                l.label_t = None;
                l.label_off = None;
            }
        }
        ui.label(RichText::new("Stream variables (labels and States table):").small());
        ui.horizontal_wrapped(|ui| {
            for b in inp.array_bases() {
                let mut on = d.state_vars.contains(&b);
                if ui.checkbox(&mut on, inp.base_display(&b)).changed() {
                    if on {
                        d.state_vars.push(b.clone());
                    } else {
                        d.state_vars.retain(|x| *x != b);
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Fluid properties:").small());
            for (n, label, _) in crate::components::DERIVED {
                let mut on = d.derived.iter().any(|x| x == n);
                if ui.checkbox(&mut on, label).on_hover_text("From CoolProp at (P, h) of the stream; needs the stream fluid").changed() {
                    if on {
                        d.derived.push(n.to_string());
                    } else {
                        d.derived.retain(|x| x != n);
                    }
                }
            }
        });
        if ui.small_button("Renumber states in drawing order").on_hover_text("Assign 1, 2, 3 ... to streams without a state number").clicked() {
            let mut next = d.links.iter().filter_map(|l| l.state).max().unwrap_or(0) + 1;
            for l in d.links.iter_mut().filter(|l| l.state.is_none()) {
                l.state = Some(next);
                next += 1;
            }
        }
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);
        ui.strong("Diagram");
        ui.checkbox(&mut d.kpi_box, "Show the KPI box on the canvas");
        ui.horizontal(|ui| {
            ui.label("Title");
            ui.text_edit_singleline(&mut d.title);
        });
        ui.horizontal(|ui| {
            if ui.button("Export PDF report…").clicked() {
                let bytes = report_pdf(d, inp);
                let name =
                    if d.title.is_empty() { "cycle.pdf".to_string() } else { format!("{}.pdf", d.title.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_")) };
                if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(name).save_file() {
                    self.message = match std::fs::write(&p, bytes) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => format!("Cannot write {}: {e}", p.display()),
                    };
                }
            }
            if ui.button("Fit view").clicked() {
                self.fit(d);
            }
        });
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).small().weak());
        }
    }

    fn delete(&mut self, d: &mut CycleDiagram) {
        match self.sel {
            Some(Sel::Comp(id)) if d.comp_locked(id) => {
                self.message = "Locked: unlock components / text (Lock section) to delete.".into();
                return;
            }
            Some(Sel::Link(_)) if d.lock_streams => {
                self.message = "Streams are locked (Lock section).".into();
                return;
            }
            _ => {}
        }
        match self.sel.take() {
            Some(Sel::Comp(id)) => {
                d.comps.retain(|c| c.id != id);
                d.links.retain(|l| l.from.0 != id && l.to.0 != id);
            }
            Some(Sel::Link(id)) => d.links.retain(|l| l.id != id),
            None => {}
        }
    }

    fn fit(&mut self, d: &mut CycleDiagram) {
        let (Some((lo, hi)), Some(c)) = (d.bounds(), self.canvas) else { return };
        let (w, h) = (hi[0] - lo[0] + 160.0, hi[1] - lo[1] + 120.0);
        // keep the drawing clear of the KPI box (top right)
        // (unless that would shrink the drawing by more than 15 %)
        let full = (c.width() / w).min(c.height() / h);
        let cw = if d.kpi_box && (c.width() - self.kpi_w) / w >= 0.85 * full { c.width() - self.kpi_w } else { c.width() };
        d.zoom = (cw / w).min(c.height() / h).clamp(0.3, 3.0);
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        d.offset = [cw / 2.0 - mid[0] * d.zoom, c.height() / 2.0 - mid[1] * d.zoom];
    }

    fn canvas_ui(&mut self, ui: &mut egui::Ui, d: &mut CycleDiagram, inp: &CycleInputs) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.canvas = Some(rect);
        let painter = ui.painter_at(rect);
        let dark = ui.visuals().dark_mode;
        painter.rect_filled(rect, 0.0, if dark { Color32::from_gray(28) } else { Color32::from_gray(250) });
        let line = if dark { Color32::from_gray(210) } else { Color32::from_gray(30) };
        let text = ui.visuals().text_color();
        let view = View { origin: rect.min + Vec2::from(d.offset), zoom: d.zoom };
        // grid
        let step = 20.0 * d.zoom;
        if step > 6.0 {
            let gcol = if dark { Color32::from_gray(38) } else { Color32::from_gray(235) };
            let x0 = rect.min.x + (view.origin.x - rect.min.x).rem_euclid(step);
            let y0 = rect.min.y + (view.origin.y - rect.min.y).rem_euclid(step);
            let mut x = x0;
            while x < rect.max.x {
                painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(1.0, gcol));
                x += step;
            }
            let mut y = y0;
            while y < rect.max.y {
                painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(1.0, gcol));
                y += step;
            }
        }
        if d.comps.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drag components from the left panel onto the canvas",
                egui::FontId::proportional(15.0),
                text.gamma_multiply(0.6),
            );
        }

        let pointer = ui.ctx().pointer_interact_pos();
        // ---- interaction
        if resp.drag_started() {
            // use the press position: drag_started fires only after the pointer has moved
            if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                let label = if d.lock_labels { None } else { self.label_rects.iter().rev().find(|r| r.1.contains(p)).map(|r| (r.0, r.2)) };
                self.drag = if ui.input(|i| i.pointer.middle_down() || i.pointer.secondary_down()) {
                    Some(Drag::Pan)
                } else if let Some((id, off)) = label {
                    self.sel = Some(Sel::Link(id));
                    Some(Drag::Label { id, free: self.free_drag || ui.input(|i| i.modifiers.shift), off })
                } else if let Some((cid, k, a)) = hover_port(d, &view, p).filter(|_| !d.lock_streams) {
                    Some(Drag::Wire { from: (cid, k), a })
                } else if let Some(id) = hit_comp(d, &view, p) {
                    self.sel = Some(Sel::Comp(id));
                    if d.comp_locked(id) {
                        Some(Drag::Pan)
                    } else {
                        Some(Drag::Move(id))
                    }
                } else {
                    Some(Drag::Pan)
                };
            }
        }
        if resp.dragged() {
            let delta = resp.drag_delta();
            match &self.drag {
                Some(Drag::Move(id)) => {
                    if let Some(c) = d.comps.iter_mut().find(|c| c.id == *id) {
                        c.pos[0] += delta.x / d.zoom;
                        c.pos[1] += delta.y / d.zoom;
                    }
                }
                Some(Drag::Pan) => {
                    d.offset[0] += delta.x;
                    d.offset[1] += delta.y;
                }
                Some(Drag::Label { id, free, off }) => {
                    let (id, free) = (*id, *free);
                    if free {
                        let o = [off[0] + delta.x / d.zoom, off[1] + delta.y / d.zoom];
                        self.drag = Some(Drag::Label { id, free, off: o });
                        if let Some(l) = d.links.iter_mut().find(|l| l.id == id) {
                            l.label_off = Some(o);
                        }
                    } else if let (Some(q), Some(pts)) = (pointer, d.links.iter().find(|l| l.id == id).and_then(|l| d.route(l))) {
                        let f = project_on(&pts, view.w(q));
                        if let Some(l) = d.links.iter_mut().find(|l| l.id == id) {
                            l.label_t = Some(f);
                        }
                    }
                }
                _ => {}
            }
        }
        if resp.drag_stopped() {
            match self.drag.take() {
                Some(Drag::Move(id)) => {
                    if let Some(c) = d.comps.iter_mut().find(|c| c.id == id) {
                        c.pos = [(c.pos[0] / 10.0).round() * 10.0, (c.pos[1] / 10.0).round() * 10.0];
                    }
                    // then line a port up with its partner (straight stream)
                    align_ports(d, id, 15.0);
                }
                Some(Drag::Wire { from, .. }) => {
                    if let Some((cid, k, _)) = pointer.and_then(|q| hover_port(d, &view, q)) {
                        self.connect(d, from, (cid, k));
                    }
                }
                _ => {}
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.sel = hit_comp(d, &view, p)
                    .map(Sel::Comp)
                    .or_else(|| hit_link(d, &view, p).map(Sel::Link))
                    .or_else(|| self.label_rects.iter().rev().find(|r| r.1.contains(p)).map(|r| Sel::Link(r.0)));
            }
        }
        if resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                match hit_comp(d, &view, p) {
                    Some(id) => {
                        if d.comp(id).is_some_and(|c| c.kind != Kind::Label) {
                            self.popup = Some(id);
                        }
                    }
                    // empty canvas: fit the drawing to the view
                    None if hit_link(d, &view, p).is_none() && !self.label_rects.iter().any(|r| r.1.contains(p)) => self.fit(d),
                    None => {}
                }
            }
        }
        // stream tooltip: every state value
        if let Some(p) = pointer.filter(|_| resp.hovered() && self.drag.is_none()) {
            if let Some(lid) = hit_link(d, &view, p) {
                if let Some(l) = d.links.iter().find(|l| l.id == lid) {
                    if let Some(k) = l.state {
                        let mut lines: Vec<String> = inp
                            .array_bases()
                            .iter()
                            .filter_map(|b| inp.value(&format!("{b}[{k}]")).map(|(_, x, u)| format!("{} = {} {u}", inp.base_display(b), fmt_sig(x))))
                            .collect();
                        lines.extend(crate::components::derived_lines(
                            &CycleDiagram { derived: crate::components::DERIVED.iter().map(|x| x.0.to_string()).collect(), ..Default::default() },
                            inp,
                            l,
                        ));
                        if !lines.is_empty() {
                            resp.clone().on_hover_text_at_pointer(format!("Stream {k}\n{}", lines.join("\n")));
                        }
                    }
                }
            }
        }
        if resp.hovered() {
            let (scroll, zoom_delta) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = if zoom_delta != 1.0 { zoom_delta } else { (scroll * 0.0015).exp() };
            if f != 1.0 {
                if let Some(p) = pointer {
                    let w = view.w(p);
                    d.zoom = (d.zoom * f).clamp(0.25, 4.0);
                    d.offset = [p.x - rect.min.x - w[0] * d.zoom, p.y - rect.min.y - w[1] * d.zoom];
                }
            }
            if !ui.ctx().egui_wants_keyboard_input() {
                let (del, r, m) = ui.input(|i| (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::R), i.key_pressed(egui::Key::M)));
                if del {
                    self.delete(d);
                }
                if let Some(Sel::Comp(id)) = self.sel.filter(|s| !matches!(s, Sel::Comp(id) if d.comp_locked(*id))) {
                    if let Some(c) = d.comps.iter_mut().find(|c| c.id == id) {
                        if r {
                            c.rot = (c.rot + 1) % 4;
                        }
                        if m {
                            c.flip = !c.flip;
                        }
                    }
                }
            }
        }
        let view = View { origin: rect.min + Vec2::from(d.offset), zoom: d.zoom };

        // ---- links
        let mut rects: Vec<(u32, Rect, [f32; 2])> = Vec::new();
        // screen boxes of the components (with their name labels), avoided by
        // the default label placement
        let boxes: Vec<Rect> = d
            .comps
            .iter()
            .filter(|c| c.kind != Kind::Label)
            .map(|c| {
                let (lo, hi) = c.bbox();
                Rect::from_two_pos(view.s(lo), view.s(hi)).expand(2.0)
            })
            .collect();
        for l in &d.links {
            let Some(pts) = d.route(l) else { continue };
            let sel = self.sel == Some(Sel::Link(l.id));
            let col = if sel { Color32::from_rgb(255, 170, 0) } else { l.color.map(c32).unwrap_or(line) };
            let sp: Vec<Pos2> = pts.iter().map(|p| view.s(*p)).collect();
            painter.add(egui::Shape::line(sp.clone(), Stroke::new(1.6 * d.zoom, col)));
            // arrow head at the inlet
            if sp.len() >= 2 {
                let (b, a) = (sp[sp.len() - 1], sp[sp.len() - 2]);
                let dir = (b - a).normalized();
                let n = Vec2::new(-dir.y, dir.x);
                let s = 6.0 * d.zoom;
                painter.add(egui::Shape::convex_polygon(vec![b, b - dir * s * 1.6 + n * s * 0.7, b - dir * s * 1.6 - n * s * 0.7], col, Stroke::NONE));
            }
            if let Some(k) = l.state {
                let (m, dir) = point_at(&pts, l.label_t.unwrap_or(0.5));
                let c = view.s(m);
                let horizontal = dir[0].abs() > dir[1].abs();
                // label scale: fixed screen size unless it follows the zoom
                let ls = d.label_size.factor() * if d.label_zoom { d.zoom.clamp(0.5, 2.0) } else { 1.0 };
                let r = 8.0 * ls;
                let lcol = l.color.map(c32).unwrap_or(text);
                let bg = if dark { Color32::from_black_alpha(150) } else { Color32::from_white_alpha(200) };
                let badge = |at: Pos2| {
                    painter.circle(at, r, Color32::WHITE, Stroke::new(1.2, col));
                    painter.text(at, egui::Align2::CENTER_CENTER, k.to_string(), egui::FontId::proportional(10.5 * ls), Color32::BLACK);
                };
                let mut hit = Rect::from_center_size(c, Vec2::splat(2.0 * r));
                let mut off_used = [0.0, 0.0];
                let cross = if d.show_values && d.label_style == LabelStyle::Cross { Some(inp.cross_values(d, k)) } else { None };
                if let Some(v) = cross.filter(|v| v.iter().any(|x| !x.is_empty())) {
                    // cross label: P | T over m | h
                    let font = egui::FontId::proportional(11.0 * ls);
                    let g: Vec<_> = v.iter().map(|x| painter.layout_no_wrap(x.clone(), font.clone(), lcol)).collect();
                    let pad = 3.0 * ls;
                    let wl = g[0].size().x.max(g[2].size().x);
                    let wr = g[1].size().x.max(g[3].size().x);
                    let rh = g.iter().map(|g| g.size().y).fold(0.0, f32::max);
                    let size = Vec2::new(wl + wr + 4.0 * pad, 2.0 * rh + 2.0 * pad);
                    let num = if d.show_numbers { 2.0 * r + 2.0 } else { 0.0 };
                    // default side: the first of above/below (horizontal stream) or
                    // right/left (vertical) that keeps clear of components and labels
                    let cands = if horizontal {
                        [Vec2::new(0.0, -(size.y / 2.0 + 4.0)), Vec2::new(0.0, size.y / 2.0 + 4.0 + num)]
                    } else {
                        [Vec2::new(size.x / 2.0 + 6.0, num / 2.0), Vec2::new(-(size.x / 2.0 + 6.0), num / 2.0)]
                    };
                    let clear = |o: Vec2| {
                        let b = Rect::from_center_size(c + o, size).union(Rect::from_center_size(c + o - Vec2::new(0.0, size.y / 2.0 + num / 2.0), Vec2::new(2.0 * r, num)));
                        !boxes.iter().any(|x| x.intersects(b)) && !rects.iter().any(|x: &(u32, Rect, [f32; 2])| x.1.intersects(b))
                    };
                    let def = if d.cross_on_line {
                        // the stream runs along the cross's horizontal bar (horizontal
                        // stream) or its vertical bar (vertical stream)
                        if horizontal {
                            Vec2::ZERO
                        } else {
                            Vec2::new((g[1].size().x.max(g[3].size().x) - g[0].size().x.max(g[2].size().x)) / 2.0, 0.0)
                        }
                    } else {
                        cands.iter().copied().find(|o| clear(*o)).unwrap_or(cands[0])
                    };
                    let off = l.label_off.map(|o| Vec2::new(o[0], o[1]) * d.zoom).unwrap_or(def);
                    off_used = [off.x / d.zoom, off.y / d.zoom];
                    let ctr = c + off;
                    let bx = Rect::from_center_size(ctr, size);
                    painter.rect_filled(bx, 2.0, bg);
                    let xv = bx.min.x + wl + 2.0 * pad;
                    let st = Stroke::new(1.0, lcol);
                    painter.line_segment([Pos2::new(bx.min.x, ctr.y), Pos2::new(bx.max.x, ctr.y)], st);
                    painter.line_segment([Pos2::new(xv, bx.min.y), Pos2::new(xv, bx.max.y)], st);
                    let (y0, y1) = (ctr.y - pad * 0.5, ctr.y + pad * 0.5);
                    for (i, gal) in g.into_iter().enumerate() {
                        let w = gal.size();
                        let x = if i % 2 == 0 { xv - pad - w.x } else { xv + pad };
                        let y = if i < 2 { y0 - w.y } else { y1 };
                        painter.galley(Pos2::new(x, y), gal, lcol);
                    }
                    hit = bx;
                    if d.show_numbers {
                        let at = Pos2::new(xv, bx.min.y - r - 2.0);
                        badge(at);
                        hit = hit.union(Rect::from_center_size(at, Vec2::splat(2.0 * r)));
                    }
                } else {
                    if d.show_numbers {
                        badge(c);
                    }
                    if d.show_values && d.label_style == LabelStyle::Box {
                        let mut lines = inp.state_lines(d, k);
                        lines.extend(crate::components::derived_lines(d, inp, l));
                        if !lines.is_empty() {
                            // beside the stream (above horizontal segments, right of vertical ones)
                            let font = egui::FontId::monospace(10.0 * ls);
                            let galley = painter.layout_no_wrap(lines.join("\n"), font, text);
                            let size = galley.size() + Vec2::splat(6.0);
                            let def = if horizontal { Vec2::new(-size.x / 2.0, -r - 3.0 - size.y) } else { Vec2::new(r + 4.0, -size.y / 2.0) };
                            let off = l.label_off.map(|o| Vec2::new(o[0], o[1]) * d.zoom).unwrap_or(def);
                            off_used = [off.x / d.zoom, off.y / d.zoom];
                            let bx = Rect::from_min_size(c + off, size);
                            painter.rect_filled(bx, 3.0, if dark { Color32::from_black_alpha(170) } else { Color32::from_white_alpha(220) });
                            painter.galley(bx.min + Vec2::splat(3.0), galley, text);
                            hit = hit.union(bx);
                        }
                    }
                }
                if !d.lock_labels {
                    let hov = pointer.is_some_and(|q| hit.contains(q));
                    if hov || sel {
                        painter.rect_stroke(
                            hit.expand(2.0),
                            2.0,
                            Stroke::new(1.0, Color32::from_rgb(255, 170, 0).gamma_multiply(if hov { 1.0 } else { 0.5 })),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
                rects.push((l.id, hit, off_used));
            }
        }
        self.label_rects = rects;
        if !d.lock_labels {
            if let Some(q) = pointer.filter(|q| rect.contains(*q)) {
                if self.label_rects.iter().any(|r| r.1.contains(q)) || matches!(self.drag, Some(Drag::Label { .. })) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }
        // wire being drawn
        if let (Some(Drag::Wire { a, .. }), Some(p)) = (&self.drag, pointer) {
            painter.line_segment([view.s(*a), p], Stroke::new(1.5, Color32::from_rgb(255, 170, 0)));
        }

        // ---- components
        for c in &d.comps {
            let sel = self.sel == Some(Sel::Comp(c.id));
            paint_comp(&painter, c, &view, line, sel, dark, &|k| inlet_color(d, c.id, k).map(c32));
            let (lo, hi) = c.bbox();
            let font = egui::FontId::proportional(11.5 * d.zoom.clamp(0.7, 1.5));
            if c.kind == Kind::Label {
                painter.text(view.s(c.pos), egui::Align2::CENTER_CENTER, &c.label, font, if sel { Color32::from_rgb(255, 170, 0) } else { text });
                continue;
            }
            let mut lines = vec![c.label.clone()];
            for v in &c.vars {
                if let Some((n, x, u)) = inp.value(v) {
                    lines.push(format!("{n} = {} {u}", fmt_sig(x)).trim_end().to_string());
                }
            }
            let _ = (lo, hi);
            let (side, anchor) = label_side(d, c, lines.iter().map(|l| l.len()).max().unwrap_or(0) as f32 * 6.2, lines.len() as f32 * 13.5);
            let align = [egui::Align2::CENTER_TOP, egui::Align2::CENTER_BOTTOM, egui::Align2::LEFT_CENTER, egui::Align2::RIGHT_CENTER][side];
            painter.text(view.s(anchor), align, lines.join("\n"), font, text);
            // ports
            for k in 0..ports(c.kind).len() {
                let (p, w, _) = c.port(k).unwrap();
                let connected = d.links.iter().any(|l| l.from == (c.id, k) || l.to == (c.id, k));
                let hov = pointer.is_some_and(|q| view.s(w).distance(q) < 9.0);
                if !connected || hov || sel {
                    let col = if p.inlet { Color32::from_rgb(70, 140, 230) } else { Color32::from_rgb(230, 90, 70) };
                    painter.circle_filled(view.s(w), if hov { 5.0 } else { 3.2 }, col);
                    if hov {
                        painter.text(view.s(w) + Vec2::new(6.0, -6.0), egui::Align2::LEFT_BOTTOM, p.name, egui::FontId::proportional(11.0), text);
                    }
                }
            }
        }

        // ---- KPI box (top right)
        let kp: Vec<(String, String)> = d
            .kpis
            .iter()
            .filter_map(|k| inp.value(&k.var).map(|(n, x, u)| (if k.label.is_empty() { n } else { k.label.clone() }, format!("{} {u}", fmt_sig(x)).trim_end().to_string())))
            .collect();
        if !kp.is_empty() && d.kpi_box {
            let font = egui::FontId::proportional(12.0);
            let lw = kp.iter().map(|(l, _)| painter.layout_no_wrap(l.clone(), font.clone(), text).size().x).fold(0.0, f32::max);
            let vw = kp.iter().map(|(_, v)| painter.layout_no_wrap(v.clone(), font.clone(), text).size().x).fold(0.0, f32::max);
            let h = 18.0;
            let bx = Rect::from_min_size(Pos2::new(rect.max.x - lw - vw - 36.0, rect.min.y + 10.0), Vec2::new(lw + vw + 26.0, h * kp.len() as f32 + 10.0));
            self.kpi_w = bx.width() + 20.0;
            painter.rect(
                bx,
                4.0,
                if dark { Color32::from_black_alpha(190) } else { Color32::from_white_alpha(235) },
                Stroke::new(1.0, text.gamma_multiply(0.4)),
                egui::StrokeKind::Inside,
            );
            for (i, (l, v)) in kp.iter().enumerate() {
                let y = bx.min.y + 5.0 + h * i as f32 + h / 2.0;
                painter.text(Pos2::new(bx.min.x + 8.0, y), egui::Align2::LEFT_CENTER, l, font.clone(), text);
                painter.text(Pos2::new(bx.max.x - 8.0, y), egui::Align2::RIGHT_CENTER, v, font.clone(), text);
            }
        }
        if !d.title.is_empty() {
            painter.text(rect.min + Vec2::new(12.0, 10.0), egui::Align2::LEFT_TOP, &d.title, egui::FontId::proportional(17.0), text);
        }
        // ---- unit legend of the cross labels (top left)
        if d.show_values && d.label_style == LabelStyle::Cross && d.links.iter().any(|l| l.state.is_some()) {
            let u = inp.cross_units(d);
            if u.iter().any(|x| !x.is_empty()) {
                let font = egui::FontId::proportional(11.0);
                let g: Vec<_> = u.iter().map(|x| painter.layout_no_wrap(x.clone(), font.clone(), text)).collect();
                let pad = 4.0;
                let wl = g[0].size().x.max(g[2].size().x);
                let wr = g[1].size().x.max(g[3].size().x);
                let rh = g.iter().map(|g| g.size().y).fold(0.0, f32::max);
                let top = rect.min.y + if d.title.is_empty() { 10.0 } else { 36.0 };
                let bx = Rect::from_min_size(Pos2::new(rect.min.x + 12.0, top), Vec2::new(wl + wr + 4.0 * pad, 2.0 * rh + 2.0 * pad));
                painter.rect(
                    bx.expand(3.0),
                    3.0,
                    if dark { Color32::from_black_alpha(150) } else { Color32::from_white_alpha(220) },
                    Stroke::new(1.0, text.gamma_multiply(0.3)),
                    egui::StrokeKind::Outside,
                );
                let (xv, yc) = (bx.min.x + wl + 2.0 * pad, bx.center().y);
                painter.line_segment([Pos2::new(bx.min.x, yc), Pos2::new(bx.max.x, yc)], Stroke::new(1.0, text));
                painter.line_segment([Pos2::new(xv, bx.min.y), Pos2::new(xv, bx.max.y)], Stroke::new(1.0, text));
                for (i, gal) in g.into_iter().enumerate() {
                    let w = gal.size();
                    let x = if i % 2 == 0 { xv - pad - w.x } else { xv + pad };
                    let y = if i < 2 { yc - pad * 0.5 - w.y } else { yc + pad * 0.5 };
                    painter.galley(Pos2::new(x, y), gal, text);
                }
            }
        }
    }

    fn connect(&mut self, d: &mut CycleDiagram, a: (u32, usize), b: (u32, usize)) {
        if a == b || a.0 == b.0 {
            return;
        }
        let inlet = |p: (u32, usize)| d.comp(p.0).and_then(|c| ports(c.kind).get(p.1).copied()).map(|p| p.inlet);
        let (Some(ia), Some(ib)) = (inlet(a), inlet(b)) else { return };
        if ia == ib {
            self.message = "Connect an outlet (red) to an inlet (blue).".into();
            return;
        }
        let (from, to) = if ia { (b, a) } else { (a, b) };
        // one stream per port
        d.links.retain(|l| l.from != from && l.to != to);
        let state = d.links.iter().filter_map(|l| l.state).max().unwrap_or(0) + 1;
        let id = d.new_id();
        // a new stream takes the colour of a stream already touching either component
        let color = d.links.iter().find(|l| l.to.0 == from.0 || l.from.0 == to.0).and_then(|l| l.color);
        d.links.push(Link { id, from, to, state: Some(state), color, fluid: String::new(), label_t: None, label_off: None });
        self.sel = Some(Sel::Link(id));
        self.message.clear();
    }
}

fn kpi_label(v: &str) -> String {
    let l = v.to_lowercase();
    if l.starts_with("eta") {
        format!("Efficiency ({v})")
    } else if l.starts_with("w_") || l.starts_with("p_") {
        format!("Power ({v})")
    } else if l.starts_with("q_") {
        format!("Heat ({v})")
    } else {
        v.to_string()
    }
}

fn hover_port(d: &CycleDiagram, view: &View, pos: Pos2) -> Option<(u32, usize, [f32; 2])> {
    let mut best = None;
    let mut bd = 9.0f32.max(6.0 * d.zoom);
    for c in &d.comps {
        for k in 0..ports(c.kind).len() {
            let (_, p, _) = c.port(k).unwrap();
            let dd = view.s(p).distance(pos);
            if dd < bd {
                bd = dd;
                best = Some((c.id, k, p));
            }
        }
    }
    best
}

fn hit_comp(d: &CycleDiagram, view: &View, pos: Pos2) -> Option<u32> {
    d.comps
        .iter()
        .rev()
        .find(|c| {
            let (lo, hi) = c.bbox();
            Rect::from_min_max(view.s(lo), view.s(hi)).expand(3.0).contains(pos)
        })
        .map(|c| c.id)
}

fn hit_link(d: &CycleDiagram, view: &View, pos: Pos2) -> Option<u32> {
    d.links.iter().find(|l| d.route(l).is_some_and(|pts| pts.windows(2).any(|w| seg_dist(pos, view.s(w[0]), view.s(w[1])) < 5.0))).map(|l| l.id)
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn var_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, v: &mut String, options: &[String]) {
    egui::ComboBox::from_id_salt(id).width(130.0).selected_text(if v.is_empty() { "select…" } else { v.as_str() }).height(320.0).show_ui(ui, |ui| {
        for o in options {
            ui.selectable_value(v, o.clone(), o);
        }
    });
}

// ------------------------------------------------------------------ PDF report

/// A4 landscape report: diagram with stream labels, KPI table and state
/// table (continued on further pages when long).
pub fn report_pdf(d: &CycleDiagram, inp: &CycleInputs) -> Vec<u8> {
    // A4 landscape; A3 for larger flowsheets so stream labels stay legible
    let (pw, ph) = if d.links.len() > 8 || d.comps.len() > 10 { (1190.55f32, 841.89f32) } else { (842.0f32, 595.0f32) };
    let title = if d.title.is_empty() { "Cycle diagram".to_string() } else { d.title.clone() };
    let mut pdf = Pdf::new(pw, ph, &title);
    let black = [0u8, 0, 0];
    let grey = [110u8, 110, 110];
    let margin = 36.0;
    let page = pdf.page();
    page.text(margin, margin + 6.0, 16.0, &title, true, Align::Left, black);
    let sol = inp.analysis.solution.as_ref();
    let sub = format!(
        "Unit system: {}   ·   {}",
        inp.analysis.model.unit_system.name,
        match sol {
            Some(s) if s.converged => format!("solved, max. residual {:.1e}", s.max_residual),
            Some(_) => "solution NOT converged".into(),
            None => "not solved".into(),
        }
    );
    page.text(margin, margin + 22.0, 8.5, &sub, false, Align::Left, grey);

    // diagram area: top 62 % of the page, left of the KPI table
    let kpis: Vec<(String, String)> = d
        .kpis
        .iter()
        .filter_map(|k| inp.value(&k.var).map(|(n, x, u)| (if k.label.is_empty() { n } else { k.label.clone() }, format!("{} {u}", fmt_sig(x)).trim_end().to_string())))
        .collect();
    let kpi_w = if kpis.is_empty() { 0.0 } else { kpis.iter().map(|(l, v)| pdf::text_width(l, 8.5, false) + pdf::text_width(v, 8.5, true)).fold(0.0, f32::max) + 30.0 };
    // unit legend of the cross labels, top left of the diagram
    let legend = if d.show_values && d.label_style == LabelStyle::Cross && d.links.iter().any(|l| l.state.is_some()) {
        Some(inp.cross_units(d)).filter(|u| u.iter().any(|x| !x.is_empty()))
    } else {
        None
    };
    let mut top = margin + 34.0;
    if let Some(u) = &legend {
        let fs = 8.0;
        let pad = 3.0;
        let w: Vec<f32> = u.iter().map(|x| pdf::text_width(x, fs, false)).collect();
        let (wl, wr, rh) = (w[0].max(w[2]), w[1].max(w[3]), fs * 1.15);
        let (x0, y0) = (margin, top);
        let size = [wl + wr + 4.0 * pad, 2.0 * rh + 2.0 * pad];
        page.rect(x0, y0, size[0], size[1], 0.4, Some([170, 170, 170]), None);
        let (xv, yc) = (x0 + wl + 2.0 * pad, y0 + size[1] / 2.0);
        page.line([x0, yc], [x0 + size[0], yc], 0.6, black);
        page.line([xv, y0], [xv, y0 + size[1]], 0.6, black);
        for (i, x) in u.iter().enumerate() {
            let (px, al) = if i % 2 == 0 { (xv - pad, Align::Right) } else { (xv + pad, Align::Left) };
            let py = if i < 2 { yc - pad * 0.5 - fs * 0.25 } else { yc + pad * 0.5 + fs * 0.8 };
            page.text(px, py, fs, x, false, al, black);
        }
        top += size[1] + 6.0;
    }
    let area = [margin, top, pw - margin - kpi_w - if kpi_w > 0.0 { 12.0 } else { 0.0 }, ph * 0.64];
    if let Some((lo, hi)) = d.bounds() {
        // leave room for the value boxes around the streams
        let pad = if d.show_values && !d.state_vars.is_empty() { 60.0 } else { 15.0 };
        let (lo, hi) = ([lo[0] - pad, lo[1] - pad * 1.6], [hi[0] + pad, hi[1] + pad]);
        let s = ((area[2] - area[0]) / (hi[0] - lo[0])).min((area[3] - area[1]) / (hi[1] - lo[1])).min(1.4);
        let ox = area[0] + ((area[2] - area[0]) - (hi[0] - lo[0]) * s) / 2.0;
        let oy = area[1] + ((area[3] - area[1]) - (hi[1] - lo[1]) * s) / 2.0;
        let t = |p: [f32; 2]| [ox + (p[0] - lo[0]) * s, oy + (p[1] - lo[1]) * s];
        let lw = (1.1 * s).clamp(0.5, 1.4);
        let mut labels = Vec::new();
        for l in &d.links {
            let Some(pts) = d.route(l) else { continue };
            let tp: Vec<[f32; 2]> = pts.iter().map(|p| t(*p)).collect();
            let lc = l.color.unwrap_or(black);
            page.polyline(&tp, false, lw, Some(lc), None, None);
            if tp.len() >= 2 {
                let (b, a) = (tp[tp.len() - 1], tp[tp.len() - 2]);
                let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt().max(1e-3);
                let dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
                let n = [-dir[1], dir[0]];
                let k = 4.0 * s.max(0.6);
                page.polyline(
                    &[
                        b,
                        [b[0] - dir[0] * k * 1.6 + n[0] * k * 0.7, b[1] - dir[1] * k * 1.6 + n[1] * k * 0.7],
                        [b[0] - dir[0] * k * 1.6 - n[0] * k * 0.7, b[1] - dir[1] * k * 1.6 - n[1] * k * 0.7],
                    ],
                    true,
                    0.1,
                    None,
                    Some(lc),
                    None,
                );
            }
            if l.state.is_some() {
                labels.push((l, pts));
            }
        }
        // stream labels as on the canvas (style, numbers, size, positions and
        // colours), in world units scaled with the diagram
        let comp_boxes: Vec<([f32; 2], [f32; 2])> = d.comps.iter().filter(|c| c.kind != Kind::Label).map(|c| c.bbox()).collect();
        let overlaps = |lo: [f32; 2], hi: [f32; 2]| comp_boxes.iter().any(|(a, b)| lo[0] < b[0] && hi[0] > a[0] && lo[1] < b[1] && hi[1] > a[1]);
        let f = d.label_size.factor();
        let white = [255u8, 255, 255];
        for (l, pts) in &labels {
            let k = l.state.unwrap_or_default();
            let (m, dir) = point_at(pts, l.label_t.unwrap_or(0.5));
            let horizontal = dir[0].abs() > dir[1].abs();
            let lc = l.color.unwrap_or(black);
            let r = 8.0 * f;
            let badge = |page: &mut pdf::Page, at: [f32; 2]| {
                let c = t(at);
                page.circle(c, r * s, 0.6, Some(lc), Some(white));
                page.text(c[0], c[1] + r * s * 0.38, 10.5 * f * s, &k.to_string(), false, Align::Center, black);
            };
            let cross = if d.show_values && d.label_style == LabelStyle::Cross { Some(inp.cross_values(d, k)) } else { None };
            if let Some(v) = cross.filter(|v| v.iter().any(|x| !x.is_empty())) {
                let fs = 11.0 * f;
                let pad = 3.0 * f;
                let w: Vec<f32> = v.iter().map(|x| pdf::text_width(x, fs, false)).collect();
                let (wl, wr, rh) = (w[0].max(w[2]), w[1].max(w[3]), fs * 1.15);
                let size = [wl + wr + 4.0 * pad, 2.0 * rh + 2.0 * pad];
                let num = if d.show_numbers { 2.0 * r + 2.0 } else { 0.0 };
                let def = if d.cross_on_line {
                    if horizontal {
                        [0.0, 0.0]
                    } else {
                        [(wr - wl) / 2.0, 0.0]
                    }
                } else {
                    let cands = if horizontal {
                        [[0.0, -(size[1] / 2.0 + 4.0)], [0.0, size[1] / 2.0 + 4.0 + num]]
                    } else {
                        [[size[0] / 2.0 + 6.0, num / 2.0], [-(size[0] / 2.0 + 6.0), num / 2.0]]
                    };
                    cands
                        .iter()
                        .copied()
                        .find(|o| {
                            let c = [m[0] + o[0], m[1] + o[1]];
                            !overlaps([c[0] - size[0] / 2.0, c[1] - size[1] / 2.0 - num], [c[0] + size[0] / 2.0, c[1] + size[1] / 2.0])
                        })
                        .unwrap_or(cands[0])
                };
                let off = l.label_off.unwrap_or(def);
                let ctr = [m[0] + off[0], m[1] + off[1]];
                let tl = [ctr[0] - size[0] / 2.0, ctr[1] - size[1] / 2.0];
                let p0 = t(tl);
                page.rect(p0[0], p0[1], size[0] * s, size[1] * s, 0.0, None, Some(white));
                let xv = tl[0] + wl + 2.0 * pad;
                let lw = (0.8 * s).clamp(0.3, 1.0);
                page.line(t([tl[0], ctr[1]]), t([tl[0] + size[0], ctr[1]]), lw, lc);
                page.line(t([xv, tl[1]]), t([xv, tl[1] + size[1]]), lw, lc);
                for (i, x) in v.iter().enumerate() {
                    let (px, al) = if i % 2 == 0 { (xv - pad, Align::Right) } else { (xv + pad, Align::Left) };
                    let py = if i < 2 { ctr[1] - pad * 0.5 - fs * 0.25 } else { ctr[1] + pad * 0.5 + fs * 0.8 };
                    let q = t([px, py]);
                    page.text(q[0], q[1], fs * s, x, false, al, lc);
                }
                if d.show_numbers {
                    badge(page, [xv, tl[1] - r - 2.0]);
                }
            } else {
                if d.show_numbers {
                    badge(page, m);
                }
                if d.show_values && d.label_style == LabelStyle::Box {
                    let mut lines = inp.state_lines(d, k);
                    lines.extend(crate::components::derived_lines(d, inp, l));
                    if !lines.is_empty() {
                        let fs = 10.0 * f;
                        let lh = fs * 1.2;
                        let size = [lines.iter().map(|l| pdf::text_width(l, fs, false)).fold(0.0, f32::max) + 6.0, lines.len() as f32 * lh + 6.0];
                        let def = if horizontal { [-size[0] / 2.0, -r - 3.0 - size[1]] } else { [r + 4.0, -size[1] / 2.0] };
                        let off = l.label_off.unwrap_or(def);
                        let tl = [m[0] + off[0], m[1] + off[1]];
                        let p0 = t(tl);
                        page.rect(p0[0], p0[1], size[0] * s, size[1] * s, 0.3, Some([180, 180, 180]), Some(white));
                        for (i, line) in lines.iter().enumerate() {
                            let q = t([tl[0] + 3.0, tl[1] + 3.0 + lh * i as f32 + fs * 0.9]);
                            page.text(q[0], q[1], fs * s, line, false, Align::Left, black);
                        }
                    }
                }
            }
        }
        for c in &d.comps {
            for (i, pr) in shape(c.kind).into_iter().enumerate() {
                let inner = inner_port(c.kind, i).and_then(|k| inlet_color(d, c.id, k));
                match pr {
                    Prim::Poly { pts, closed, fill } => {
                        let tp: Vec<[f32; 2]> = pts.iter().map(|p| t(c.tf(*p))).collect();
                        page.polyline(&tp, closed, if inner.is_some() { lw * 1.3 } else { lw }, Some(inner.unwrap_or(black)), fill, None);
                    }
                    Prim::Circle { c: cc, r, fill } => page.circle(t(c.tf(cc)), r * s, lw, Some(inner.unwrap_or(black)), fill),
                    Prim::Text { p, s: txt, size } => {
                        let q = t(c.tf(p));
                        page.text(q[0], q[1] + size * s * 0.35, size * s, &txt, true, Align::Center, black);
                    }
                    Prim::Fill { pts, fill } => {
                        let tp: Vec<[f32; 2]> = pts.iter().map(|p| t(c.tf(*p))).collect();
                        page.polyline(&tp, true, 0.0, None, Some(fill), None);
                    }
                }
            }
            let fs = (8.0 * s).clamp(6.0, 9.5);
            if c.kind == Kind::Label {
                let q = t(c.pos);
                page.text(q[0], q[1] + fs * 0.35, fs, &c.label, false, Align::Center, black);
                continue;
            }
            let mut lines = vec![c.label.clone()];
            for v in &c.vars {
                if let Some((n, val, u)) = inp.value(v) {
                    lines.push(format!("{n} = {} {u}", fmt_sig(val)).trim_end().to_string());
                }
            }
            let (side, anchor) = label_side(d, c, lines.iter().map(|l| l.len()).max().unwrap_or(0) as f32 * 6.2, lines.len() as f32 * 13.5);
            let q = t(anchor);
            let lh = fs * 1.15;
            let total = lh * lines.len() as f32;
            let (x, y0, align) = match side {
                0 => (q[0], q[1] + fs, Align::Center),
                1 => (q[0], q[1] - total + fs * 0.85, Align::Center),
                2 => (q[0], q[1] - total / 2.0 + fs * 0.85, Align::Left),
                _ => (q[0], q[1] - total / 2.0 + fs * 0.85, Align::Right),
            };
            for (i, l) in lines.iter().enumerate() {
                page.text(x, y0 + lh * i as f32, if i == 0 { fs } else { fs * 0.92 }, l, i == 0, align, black);
            }
        }
    }

    // KPI table (right of the diagram)
    if !kpis.is_empty() {
        let x0 = pw - margin - kpi_w;
        let mut y = margin + 44.0;
        page.text(x0, y, 10.0, "Key performance indicators", true, Align::Left, black);
        y += 6.0;
        page.line([x0, y], [pw - margin, y], 0.6, black);
        for (l, v) in &kpis {
            y += 12.5;
            page.text(x0, y, 8.5, l, false, Align::Left, black);
            page.text(pw - margin, y, 8.5, v, true, Align::Right, black);
        }
        page.line([x0, y + 4.0], [pw - margin, y + 4.0], 0.6, black);
    }

    // state table (the same columns as the States tab)
    let tab = state_table(d, inp);
    if !tab.rows.is_empty() && !tab.heads.is_empty() {
        let heads = &tab.heads;
        let col_w = ((pw - 2.0 * margin - 40.0) / heads.len() as f32).min(90.0);
        let fs = if col_w < 60.0 { 7.0 } else { 8.0 };
        let row_h = 12.0;
        let mut y = ph * 0.64 + 22.0;
        let mut page = &mut *page;
        let header = |page: &mut pdf::Page, y: &mut f32| {
            page.text(margin, *y, 10.0, "States", true, Align::Left, black);
            *y += 14.0;
            page.text(margin + 20.0, *y, fs, "State", true, Align::Right, black);
            for (i, (h, u)) in heads.iter().enumerate() {
                let x = margin + 40.0 + col_w * (i as f32 + 1.0) - 6.0;
                page.text(x, *y, fs, h, true, Align::Right, black);
                page.text(x, *y + 9.0, 6.5, &if u.is_empty() { "-".to_string() } else { format!("[{u}]") }, false, Align::Right, grey);
            }
            *y += 13.0;
            page.line([margin, *y], [margin + 40.0 + col_w * heads.len() as f32, *y], 0.6, black);
        };
        header(page, &mut y);
        let mut new_page_needed = false;
        let mut remaining: &[(i64, Vec<String>)] = &tab.rows;
        while !remaining.is_empty() {
            if new_page_needed {
                page = pdf_page_hack(&mut pdf);
                y = margin + 10.0;
                header(page, &mut y);
            }
            let fit = (((ph - margin) - y) / row_h).floor().max(1.0) as usize;
            let (now, rest) = remaining.split_at(fit.min(remaining.len()));
            for (k, vals) in now {
                y += row_h;
                page.text(margin + 20.0, y, fs, &k.to_string(), true, Align::Right, black);
                for (i, v) in vals.iter().enumerate() {
                    let x = margin + 40.0 + col_w * (i as f32 + 1.0) - 6.0;
                    page.text(x, y, fs, if v == "—" { "–" } else { v }, false, Align::Right, black);
                }
            }
            remaining = rest;
            new_page_needed = true;
        }
    }
    pdf.finish()
}

/// New page of the report (helper so the borrow of the previous page ends).
fn pdf_page_hack(pdf: &mut Pdf) -> &mut pdf::Page {
    pdf.page()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};

    /// Rankine flowsheet used by the tests and the documentation.
    pub fn rankine_diagram() -> CycleDiagram {
        crate::templates::rankine()
    }

    #[test]
    fn turbomachine_anchor_points() {
        for kind in [Kind::Turbine, Kind::Compressor] {
            let o = anchor_options(kind);
            // at least three per contour side: two end faces and two casings
            for side in ["face", "upper casing", "lower casing"] {
                assert!(o.iter().filter(|a| a.0.contains(side)).count() >= 3, "{side}");
            }
            // every point lies on the outline (|y| within the half height at x)
            for (_, p, _) in &o {
                let half = if kind == Kind::Turbine { 12.0 + 14.0 * (p[0] + 25.0) / 50.0 } else { 26.0 - 14.0 * (p[0] + 25.0) / 50.0 };
                assert!(p[1].abs() <= half + 1e-3 && p[0].abs() <= 25.0 + 1e-3);
            }
        }
        assert!(anchor_options(Kind::Pump).len() >= 12);
        let mut c = Comp {
            id: 1,
            kind: Kind::Turbine,
            pos: [0.0, 0.0],
            rot: 0,
            flip: false,
            label: String::new(),
            vars: Vec::new(),
            shaft: Vec::new(),
            params: Default::default(),
            anchors: Vec::new(),
        };
        assert_eq!(c.port(0).unwrap().1, [-25.0, 0.0]);
        let j = anchor_options(Kind::Turbine).iter().position(|a| a.0 == "radial, upper casing 1").unwrap();
        c.anchors = vec![(j + 1) as u8];
        let (_, pos, dir) = c.port(0).unwrap();
        assert!(dir == [0.0, -1.0] && pos[1] < -12.0);
        let d = CycleDiagram::default();
        assert!(d.lock_comps && d.lock_streams && d.lock_labels && d.lock_text && d.cross_on_line && d.label_zoom && !d.show_numbers && d.label_size == LabelSize::Small);
    }

    #[test]
    fn undo_and_redo_diagram_edits() {
        let ctx = egui::Context::default();
        let mut ui = CycleUi::default();
        let mut d = rankine_diagram();
        ui.track(&ctx, &d);
        let p0 = d.comps[0].pos;
        let c0 = d.links[0].color;
        d.comps[0].pos = [500.0, 500.0];
        ui.track(&ctx, &d);
        d.links[0].color = Some([1, 2, 3]);
        d.links[1].state = Some(9);
        // the view is not an edit
        d.zoom = 2.0;
        ui.track(&ctx, &d);
        assert!(ui.undo_redo(&mut d, false));
        assert!(d.links[0].color == c0 && d.links[1].state == Some(2) && d.comps[0].pos == [500.0, 500.0]);
        assert_eq!(d.zoom, 2.0);
        assert!(ui.undo_redo(&mut d, false));
        assert_eq!(d.comps[0].pos, p0);
        assert!(!ui.undo_redo(&mut d, false));
        assert!(ui.undo_redo(&mut d, true));
        assert_eq!(d.comps[0].pos, [500.0, 500.0]);
        // an edit after an undo drops the redo branch
        d.comps.pop();
        ui.track(&ctx, &d);
        assert!(!ui.undo_redo(&mut d, true));
        assert!(ui.undo_redo(&mut d, false));
        assert_eq!(d.comps.len(), 4);
    }

    #[test]
    fn diagram_states_and_pdf() {
        let d = rankine_diagram();
        assert_eq!(d.links.iter().map(|l| l.state.unwrap()).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        // outlet->inlet normalisation: connecting inlet first gives the same stream
        let mut d2 = d.clone();
        let mut ui = CycleUi::default();
        let (pump, cond) = (d2.comps[0].id, d2.comps[3].id);
        ui.connect(&mut d2, (pump, 0), (cond, 1));
        assert_eq!(d2.links.len(), 4);
        assert!(d2.links.iter().any(|l| l.from == (cond, 1) && l.to == (pump, 0)));
        // inlet to inlet is refused
        let boil = d2.comps[1].id;
        ui.connect(&mut d2, (pump, 0), (boil, 0));
        assert_eq!(d2.links.len(), 4);

        let a =
            run(Request { gen: 1, text: include_str!("../../../examples/rankine.dfs").into(), opts: Default::default(), solve: true, solve_opts: Default::default(), warm: None });
        let du = HashMap::new();
        let inp = CycleInputs { analysis: &a, display_units: &du, script: "" };
        let l3 = inp.state_lines(&d, 3);
        assert_eq!(l3, vec!["T = 550 C", "P = 150 bar", "h = 3450 kJ/kg"]);
        // state table: the chosen variables, then s, x, dT_sat, rho, cp from CoolProp
        let tab = state_table(&d, &inp);
        eprintln!("{}", tab.tsv());
        let col = |n: &str| tab.heads.iter().position(|h| h.0 == n).unwrap_or_else(|| panic!("{n}"));
        let row = |k: i64| &tab.rows.iter().find(|r| r.0 == k).unwrap().1;
        assert_eq!(tab.rows.len(), 4);
        assert_eq!(&tab.heads[..3].iter().map(|h| h.0.as_str()).collect::<Vec<_>>(), &["T", "P", "h"]);
        assert_eq!(row(3)[col("x")], "vapour");
        assert!(row(3)[col("dT_sat")].starts_with('+'));
        assert!(row(2)[col("dT_sat")].starts_with('-'));
        assert_eq!(row(3)[col("fluid")], "Water");
        let rho1: f64 = row(1)[col("rho")].parse().unwrap();
        assert!((900.0..1000.0).contains(&rho1), "{rho1}");
        assert!(tab.tsv().starts_with("State\tT [C]\tP [bar]"));
        let pdf = report_pdf(&d, &inp);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        // the PDF follows the label settings: no state numbers, box style
        let mut d2 = d.clone();
        d2.show_numbers = false;
        d2.label_style = LabelStyle::Box;
        assert!(report_pdf(&d2, &inp).starts_with(b"%PDF-1.4"));
        if let Ok(p) = std::env::var("DF_PDF_OUT") {
            std::fs::write(p, &pdf).unwrap();
        }
        assert_eq!(fmt_sig(0.370623), "0.3706");
        assert_eq!(fmt_sig(60944.39), "60944");
        assert_eq!(fmt_sig(150.0), "150");
    }
}
