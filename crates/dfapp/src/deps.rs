//! Dependency diagnosis of a model: how a variable is computed and what is
//! computed from it.
//!
//! The structural analysis orders the equations into blocks (strongly
//! connected components of the matched equation–variable graph, i.e. the
//! block-triangular form of the incidence matrix). Each block is solved in
//! turn, explicitly when it holds a single equation in a single unknown and by
//! Newton's method otherwise. A block *reads* every variable that appears in
//! its equations but is computed by an earlier block (or fixed). The
//! dependency graph used here has an edge `u -> v` whenever the block that
//! computes `v` reads `u`; variables of the same block are coupled (solved
//! simultaneously).

use crate::analysis::Analysis;
use crate::editor::status_color;
use dfcore::VarStatus;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// Dependency tables of one analysis.
pub struct Deps {
    /// Identity of the analysis these tables were built from.
    src: usize,
    /// Variables read by each block (computed elsewhere).
    pub block_inputs: Vec<Vec<u32>>,
    /// Blocks reading each variable.
    pub var_users: Vec<Vec<u32>>,
    /// Equations mentioning each variable.
    pub var_eqs: Vec<Vec<u32>>,
    /// Longest path from a source (graph column).
    pub level: Vec<u32>,
}

/// Variables related to a focused one.
#[derive(Default)]
pub struct Related {
    /// Upstream variables with their distance (1 = read directly).
    pub up: HashMap<u32, usize>,
    /// Downstream variables with their distance (1 = reads it directly).
    pub down: HashMap<u32, usize>,
    /// Variables solved simultaneously with the focus.
    pub coupled: Vec<u32>,
    /// Blocks needed to compute the focus, in solve order (its own last).
    pub path: Vec<u32>,
}

impl Deps {
    pub fn new(a: &Arc<Analysis>) -> Deps {
        let m = &a.model;
        let st = &a.plan.structure;
        let n = m.vars.len();
        let mut var_eqs = vec![Vec::new(); n];
        for (e, eq) in m.eqs.iter().enumerate() {
            for &v in &eq.vars {
                let l: &mut Vec<u32> = &mut var_eqs[v as usize];
                if l.last() != Some(&(e as u32)) {
                    l.push(e as u32);
                }
            }
        }
        let mut block_inputs = Vec::with_capacity(st.blocks.len());
        let mut var_users = vec![Vec::new(); n];
        let mut level = vec![0u32; n];
        for (bi, b) in st.blocks.iter().enumerate() {
            let own: HashSet<u32> = b.vars.iter().copied().collect();
            let mut inp: Vec<u32> = Vec::new();
            for &e in &b.eqs {
                for &v in &m.eqs[e as usize].vars {
                    if !own.contains(&v) && !inp.contains(&v) {
                        inp.push(v);
                    }
                }
            }
            // blocks come in solve order, so the inputs' levels are final
            let lv = inp.iter().map(|&u| level[u as usize] + 1).max().unwrap_or(0);
            for &v in &b.vars {
                level[v as usize] = lv;
            }
            for &u in &inp {
                var_users[u as usize].push(bi as u32);
            }
            block_inputs.push(inp);
        }
        Deps { src: Arc::as_ptr(a) as usize, block_inputs, var_users, var_eqs, level }
    }

    pub fn is_for(&self, a: &Arc<Analysis>) -> bool {
        self.src == Arc::as_ptr(a) as usize
    }

    /// Breadth-first search up and down the block graph from `v`; `depth`
    /// limits the distance (`None` = transitive closure).
    pub fn related(&self, a: &Analysis, v: u32, depth: Option<usize>) -> Related {
        let st = &a.plan.structure;
        let lim = depth.unwrap_or(usize::MAX);
        let own = st.var_block[v as usize];
        let mut r = Related::default();
        if let Some(b) = own {
            r.coupled = st.blocks[b as usize].vars.iter().copied().filter(|&x| x != v).collect();
        }
        // upstream
        let mut seen: HashSet<u32> = HashSet::new();
        let mut q: VecDeque<(u32, usize)> = VecDeque::new();
        if let Some(b) = own {
            seen.insert(b);
            q.push_back((b, 0));
        }
        while let Some((b, d)) = q.pop_front() {
            if d >= lim {
                continue;
            }
            for &u in &self.block_inputs[b as usize] {
                match st.var_block[u as usize] {
                    Some(c) if seen.insert(c) => {
                        for &x in &st.blocks[c as usize].vars {
                            r.up.entry(x).or_insert(d + 1);
                        }
                        q.push_back((c, d + 1));
                    }
                    Some(_) => {}
                    None => {
                        r.up.entry(u).or_insert(d + 1);
                    }
                }
            }
        }
        r.path = seen.into_iter().collect();
        r.path.sort_unstable();
        // downstream
        let mut seen: HashSet<u32> = HashSet::new();
        let mut q: VecDeque<(u32, usize)> = VecDeque::new();
        let start: Vec<u32> = match own {
            Some(b) => st.blocks[b as usize].vars.clone(),
            None => vec![v],
        };
        if let Some(b) = own {
            seen.insert(b);
        }
        q.push_back((u32::MAX, 0));
        while let Some((b, d)) = q.pop_front() {
            if d >= lim {
                continue;
            }
            let vars = if b == u32::MAX { &start } else { &st.blocks[b as usize].vars };
            for &x in vars {
                for &c in &self.var_users[x as usize] {
                    if seen.insert(c) {
                        for &y in &st.blocks[c as usize].vars {
                            r.down.entry(y).or_insert(d + 1);
                        }
                        q.push_back((c, d + 1));
                    }
                }
            }
        }
        r
    }
}

/// What the user asked for in the dependency view.
pub enum DepAction {
    Focus(Option<u32>),
    Jump(u32),
}

/// Graph view settings and cached layout.
pub struct GraphState {
    pub zoom: f32,
    /// Show the whole model even when a variable is focused.
    pub show_all: bool,
    /// Distance limit up and down from the focus (0 = no limit).
    pub depth: usize,
    layout: Option<(LayoutKey, Layout)>,
    last_focus: Option<u32>,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState { zoom: 1.0, show_all: false, depth: 0, layout: None, last_focus: None }
    }
}

#[derive(PartialEq, Clone)]
struct LayoutKey {
    src: usize,
    focus: Option<u32>,
    show_all: bool,
    depth: usize,
}

struct Layout {
    /// Node rectangles in unscaled graph coordinates.
    nodes: Vec<(u32, Rect)>,
    pos: HashMap<u32, usize>,
    /// Boxes around coupled blocks: (block, rect).
    groups: Vec<(u32, Rect)>,
    size: Vec2,
}

const ROW_H: f32 = 26.0;
const NODE_H: f32 = 20.0;
const COL_GAP: f32 = 70.0;
const FONT: f32 = 12.0;

fn label_w(s: &str) -> f32 {
    s.chars().count() as f32 * FONT * 0.62 + 14.0
}

fn layout(a: &Analysis, d: &Deps, nodes: &[u32]) -> Layout {
    let m = &a.model;
    let st = &a.plan.structure;
    let in_set: HashSet<u32> = nodes.iter().copied().collect();
    // columns = distinct levels present
    let mut levels: Vec<u32> = nodes.iter().map(|&v| d.level[v as usize]).collect();
    levels.sort_unstable();
    levels.dedup();
    let col_of: HashMap<u32, usize> = levels.iter().enumerate().map(|(i, l)| (*l, i)).collect();
    // groups (one per block, or a single unblocked variable) per column
    let mut cols: Vec<Vec<Vec<u32>>> = vec![Vec::new(); levels.len()];
    let mut gidx: HashMap<u32, (usize, usize)> = HashMap::new(); // block -> (col, group)
    for &v in nodes {
        let c = col_of[&d.level[v as usize]];
        match st.var_block[v as usize] {
            Some(b) => match gidx.get(&b) {
                Some(&(c, g)) => cols[c][g].push(v),
                None => {
                    gidx.insert(b, (c, cols[c].len()));
                    cols[c].push(vec![v]);
                }
            },
            None => cols[c].push(vec![v]),
        }
    }
    for col in cols.iter_mut() {
        for g in col.iter_mut() {
            g.sort_by(|x, y| crate::app::natord(&m.vars[*x as usize].name, &m.vars[*y as usize].name));
        }
        col.sort_by_key(|g| (st.var_block[g[0] as usize].unwrap_or(0), g[0]));
    }
    let parents = |v: u32| -> Vec<u32> {
        match st.var_block[v as usize] {
            Some(b) => d.block_inputs[b as usize].iter().copied().filter(|u| in_set.contains(u)).collect(),
            None => Vec::new(),
        }
    };
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &v in nodes {
        for u in parents(v) {
            children.entry(u).or_default().push(v);
        }
    }
    // barycentre ordering (Sugiyama-style crossing reduction), a few sweeps
    let mut row: HashMap<u32, f32> = HashMap::new();
    let assign = |cols: &Vec<Vec<Vec<u32>>>, row: &mut HashMap<u32, f32>| {
        for col in cols {
            let mut r = 0.0;
            for g in col {
                for &v in g {
                    row.insert(v, r);
                    r += 1.0;
                }
            }
        }
    };
    assign(&cols, &mut row);
    for sweep in 0..4 {
        let forward = sweep % 2 == 0;
        let order: Vec<usize> = if forward { (1..cols.len()).collect() } else { (0..cols.len().saturating_sub(1)).rev().collect() };
        for c in order {
            let mut keyed: Vec<(f32, Vec<u32>)> = cols[c]
                .drain(..)
                .map(|g| {
                    let nb: Vec<f32> =
                        g.iter().flat_map(|&v| if forward { parents(v) } else { children.get(&v).cloned().unwrap_or_default() }).filter_map(|u| row.get(&u).copied()).collect();
                    let k = if nb.is_empty() { row[&g[0]] } else { nb.iter().sum::<f32>() / nb.len() as f32 };
                    (k, g)
                })
                .collect();
            keyed.sort_by(|x, y| x.0.total_cmp(&y.0));
            cols[c] = keyed.into_iter().map(|x| x.1).collect();
            let mut r = 0.0;
            for g in &cols[c] {
                for &v in g {
                    row.insert(v, r);
                    r += 1.0;
                }
            }
        }
    }
    // coordinates; columns are centred vertically
    let heights: Vec<f32> = cols.iter().map(|c| c.iter().map(|g| g.len() as f32 * ROW_H + if g.len() > 1 { 12.0 } else { 0.0 }).sum()).collect();
    let hmax = heights.iter().copied().fold(0.0, f32::max);
    let mut out = Layout { nodes: Vec::new(), pos: HashMap::new(), groups: Vec::new(), size: Vec2::ZERO };
    let mut x = 10.0;
    for (ci, col) in cols.iter().enumerate() {
        let w = col.iter().flatten().map(|&v| label_w(&m.vars[v as usize].display)).fold(40.0, f32::max);
        let mut y = 10.0 + (hmax - heights[ci]) / 2.0;
        for g in col {
            let pad = if g.len() > 1 { 6.0 } else { 0.0 };
            let y0 = y;
            y += pad;
            for &v in g {
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, NODE_H));
                out.pos.insert(v, out.nodes.len());
                out.nodes.push((v, r));
                y += ROW_H;
            }
            y += pad;
            if g.len() > 1 {
                if let Some(b) = st.var_block[g[0] as usize] {
                    out.groups.push((b, Rect::from_min_max(Pos2::new(x - 5.0, y0), Pos2::new(x + w + 5.0, y - pad - ROW_H + NODE_H + pad))));
                }
            }
        }
        x += w + COL_GAP;
    }
    out.size = Vec2::new(x - COL_GAP + 20.0, hmax + 30.0);
    out
}

/// Draw the dependency graph. Click a node to focus it, double-click to open
/// its defining equation in the editor.
pub fn graph(ui: &mut egui::Ui, a: &Arc<Analysis>, d: &Deps, focus: Option<u32>, gs: &mut GraphState, value: &dyn Fn(u32) -> String) -> Option<DepAction> {
    let m = &a.model;
    let st = &a.plan.structure;
    let dark = ui.visuals().dark_mode;
    let rel = focus.map(|f| d.related(a, f, if gs.depth == 0 { None } else { Some(gs.depth) }));
    let key = LayoutKey { src: Arc::as_ptr(a) as usize, focus: if gs.show_all { None } else { focus }, show_all: gs.show_all, depth: gs.depth };
    if gs.layout.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
        let nodes: Vec<u32> = match (&rel, gs.show_all) {
            (Some(r), false) => {
                let f = focus.unwrap();
                std::iter::once(f).chain(r.coupled.iter().copied()).chain(r.up.keys().copied()).chain(r.down.keys().copied()).collect::<HashSet<_>>().into_iter().collect()
            }
            _ => (0..m.vars.len() as u32).filter(|&v| st.var_status[v as usize] != VarStatus::Unused).collect(),
        };
        gs.layout = Some((key, layout(a, d, &nodes)));
    }
    let lay = &gs.layout.as_ref().unwrap().1;
    if lay.nodes.is_empty() {
        ui.label("No variables to show.");
        return None;
    }
    let z = gs.zoom;
    let mut act = None;
    let mut scroll_to: Option<Rect> = None;
    egui::ScrollArea::both().id_salt("dep_graph").auto_shrink([false, false]).show(ui, |ui| {
        let (resp, painter) = ui.allocate_painter(lay.size * z, Sense::click());
        let o = resp.rect.min.to_vec2();
        let tr = |r: Rect| Rect::from_min_max(r.min * z + o, r.max * z + o);
        let font = FontId::monospace(FONT * z);
        let weak = ui.visuals().weak_text_color();
        let up_c = Color32::from_rgb(70, 140, 230);
        let down_c = Color32::from_rgb(235, 140, 40);
        let focus_c = Color32::from_rgb(250, 200, 60);
        let role = |v: u32| -> u8 {
            match (&rel, focus) {
                (Some(r), Some(f)) => {
                    if v == f {
                        3
                    } else if r.coupled.contains(&v) {
                        4
                    } else if r.up.contains_key(&v) {
                        1
                    } else if r.down.contains_key(&v) {
                        2
                    } else {
                        0
                    }
                }
                _ => 5,
            }
        };
        for (b, r) in &lay.groups {
            let r = tr(*r);
            painter.rect_stroke(r, 4.0 * z, Stroke::new(1.0, weak), egui::StrokeKind::Outside);
            painter.text(
                r.left_top() + Vec2::new(2.0, -1.0),
                egui::Align2::LEFT_BOTTOM,
                format!("block {} ({} eq.)", b + 1, st.blocks[*b as usize].eqs.len()),
                FontId::proportional(10.0 * z),
                weak,
            );
        }
        // edges
        for &(v, rv) in &lay.nodes {
            let Some(b) = st.var_block[v as usize] else {
                continue;
            };
            for &u in &d.block_inputs[b as usize] {
                let Some(&k) = lay.pos.get(&u) else { continue };
                let ru = tr(lay.nodes[k].1);
                let rv = tr(rv);
                let (ro, rt) = (role(u), role(v));
                let (col, w) = match (ro, rt) {
                    (1, 1 | 3 | 4) => (up_c, 1.6),
                    (2 | 3 | 4, 2) => (down_c, 1.6),
                    (5, _) => (weak.gamma_multiply(0.6), 1.0),
                    _ => (weak.gamma_multiply(0.25), 0.8),
                };
                let p0 = ru.right_center();
                let p3 = rv.left_center();
                let dx = ((p3.x - p0.x) * 0.5).max(20.0 * z);
                let pts = [p0, p0 + Vec2::new(dx, 0.0), p3 - Vec2::new(dx, 0.0), p3];
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(w * z.max(0.5), col)));
            }
        }
        // nodes
        let hover = resp.hover_pos();
        let bg = ui.visuals().panel_fill;
        let mut hovered: Option<u32> = None;
        for &(v, r) in &lay.nodes {
            let rr = tr(r);
            let s = st.var_status[v as usize];
            let sc = status_color(s, dark);
            let ro = role(v);
            let (fill, stroke) = match ro {
                3 => (focus_c.gamma_multiply(0.45), Stroke::new(2.0, focus_c)),
                4 => (focus_c.gamma_multiply(0.25), Stroke::new(1.5, focus_c)),
                1 => (up_c.gamma_multiply(0.30), Stroke::new(1.0, up_c)),
                2 => (down_c.gamma_multiply(0.30), Stroke::new(1.0, down_c)),
                0 => (sc.gamma_multiply(0.06), Stroke::new(0.5, sc.gamma_multiply(0.3))),
                _ => (sc.gamma_multiply(0.18), Stroke::new(1.0, sc)),
            };
            painter.rect_filled(rr, 3.0 * z, bg);
            painter.rect(rr, 3.0 * z, fill, stroke, egui::StrokeKind::Inside);
            let tc = if ro == 0 { weak.gamma_multiply(0.5) } else { ui.visuals().text_color() };
            painter.text(rr.center(), egui::Align2::CENTER_CENTER, &m.vars[v as usize].display, font.clone(), tc);
            if hover.is_some_and(|h| rr.contains(h)) {
                hovered = Some(v);
            }
            if ro == 3 && gs.last_focus != focus {
                scroll_to = Some(rr);
            }
        }
        if let Some(v) = hovered {
            let vi = &m.vars[v as usize];
            let how = match st.var_block[v as usize] {
                Some(b) => {
                    let bl = &st.blocks[b as usize];
                    if bl.eqs.len() == 1 {
                        format!("block {} · line {}: {}", b + 1, m.eqs[bl.eqs[0] as usize].span.line, eq_text(a, bl.eqs[0]))
                    } else {
                        format!("block {} · {} equations solved simultaneously", b + 1, bl.eqs.len())
                    }
                }
                None => format!("{:?}", st.var_status[v as usize]),
            };
            let resp = resp.clone().on_hover_text_at_pointer(format!("{} = {}\n{how}\nclick: focus · double-click: show in editor", vi.display, value(v)));
            if resp.double_clicked() {
                act = Some(DepAction::Jump(def_line(a, v)));
            } else if resp.clicked() {
                act = Some(DepAction::Focus(if focus == Some(v) { None } else { Some(v) }));
            }
        }
        if resp.hovered() {
            let zd = ui.input(|i| i.zoom_delta());
            if zd != 1.0 {
                gs.zoom = (gs.zoom * zd).clamp(0.2, 3.0);
            }
        }
        if let Some(r) = scroll_to {
            ui.scroll_to_rect(r, Some(egui::Align::Center));
        }
    });
    gs.last_focus = focus;
    act
}

/// Line of the equation that computes `v` (its first appearance otherwise).
pub fn def_line(a: &Analysis, v: u32) -> u32 {
    match a.plan.structure.var_match[v as usize] {
        Some(e) => a.model.eqs[e as usize].span.line,
        None => a.model.vars[v as usize].line,
    }
}

/// Equation text as shown to the user (loop indices substituted).
pub fn eq_text(a: &Analysis, e: u32) -> String {
    let eq = &a.model.eqs[e as usize];
    let t = if eq.label.trim().is_empty() { eq.text.trim() } else { eq.label.trim() };
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > 90 {
        format!("{}…", t.chars().take(90).collect::<String>())
    } else {
        t
    }
}

/// Text panel: how the focused variable is computed.
pub fn info(ui: &mut egui::Ui, a: &Arc<Analysis>, d: &Deps, v: u32, value: &dyn Fn(u32) -> String) -> Option<DepAction> {
    let m = &a.model;
    let st = &a.plan.structure;
    let dark = ui.visuals().dark_mode;
    let mut act = None;
    let vi = &m.vars[v as usize];
    let s = st.var_status[v as usize];
    let rel = d.related(a, v, None);
    let var_link = |ui: &mut egui::Ui, x: u32, act: &mut Option<DepAction>| {
        let xs = st.var_status[x as usize];
        ui.horizontal(|ui| {
            crate::app::dot(ui, status_color(xs, dark));
            if ui.link(RichText::new(&m.vars[x as usize].display).monospace()).on_hover_text("focus this variable").clicked() {
                *act = Some(DepAction::Focus(Some(x)));
            }
            ui.label(RichText::new(value(x)).monospace().weak());
        });
    };
    let eq_link = |ui: &mut egui::Ui, e: u32, act: &mut Option<DepAction>| {
        let eq = &m.eqs[e as usize];
        let res = a.solution.as_ref().map(|s| s.residuals[e as usize]).unwrap_or(f64::NAN);
        ui.horizontal(|ui| {
            if ui.link(RichText::new(format!("line {}", eq.span.line)).monospace()).clicked() {
                *act = Some(DepAction::Jump(eq.span.line));
            }
            ui.label(RichText::new(eq_text(a, e)).monospace());
            if res.is_finite() {
                ui.label(RichText::new(format!("res {res:.1e}")).small().weak());
            }
        });
    };
    ui.horizontal(|ui| {
        ui.heading(RichText::new(&vi.display).monospace());
        ui.label(RichText::new(value(v)).monospace().strong());
    });
    ui.horizontal(|ui| {
        crate::app::dot(ui, status_color(s, dark));
        ui.label(format!("{s:?}"));
        if ui.small_button("Show in editor").clicked() {
            act = Some(DepAction::Jump(def_line(a, v)));
        }
        if ui.small_button("Clear focus").clicked() {
            act = Some(DepAction::Focus(None));
        }
    });
    ui.separator();
    match st.var_block[v as usize] {
        Some(b) => {
            let bl = &st.blocks[b as usize];
            let rep = a.solution.as_ref().and_then(|s| s.blocks.get(b as usize));
            ui.strong(format!("Computed in block {} of {} (solve order)", b + 1, st.blocks.len()));
            match rep {
                Some(r) => {
                    let how = if r.explicit { "explicit assignment".to_string() } else { format!("Newton, {} unknown(s), {} iteration(s)", r.size, r.iterations) };
                    let (c, ok) = if r.converged { (Color32::from_rgb(80, 170, 90), "converged") } else { (Color32::from_rgb(230, 100, 60), "FAILED") };
                    ui.label(RichText::new(format!("{how} · {ok} · residual {:.2e}", r.residual)).color(c));
                    if let Some(msg) = &r.message {
                        ui.label(RichText::new(msg).color(c).small());
                    }
                }
                None => {
                    ui.label(RichText::new("not solved yet (press Solve)").weak());
                }
            }
            if let Some(e) = st.var_match[v as usize] {
                ui.label(RichText::new("Matched equation (the one assigned to this unknown):").small().weak());
                eq_link(ui, e, &mut act);
            }
            if bl.eqs.len() > 1 {
                egui::CollapsingHeader::new(format!("Block equations ({})", bl.eqs.len())).default_open(bl.eqs.len() <= 8).show(ui, |ui| {
                    for &e in &bl.eqs {
                        eq_link(ui, e, &mut act);
                    }
                });
                egui::CollapsingHeader::new(format!("Solved simultaneously with ({})", rel.coupled.len())).default_open(rel.coupled.len() <= 8).show(ui, |ui| {
                    for &x in &rel.coupled {
                        var_link(ui, x, &mut act);
                    }
                });
            }
            let inp = &d.block_inputs[b as usize];
            egui::CollapsingHeader::new(format!("Reads ({} direct inputs)", inp.len())).default_open(true).show(ui, |ui| {
                if inp.is_empty() {
                    ui.label(RichText::new("nothing: a constant").weak());
                }
                let mut inp = inp.clone();
                inp.sort_by(|x, y| crate::app::natord(&m.vars[*x as usize].name, &m.vars[*y as usize].name));
                for x in inp {
                    var_link(ui, x, &mut act);
                }
            });
        }
        None => {
            let why = match s {
                VarStatus::Fixed => "Fixed by the study (not computed).",
                VarStatus::UnderDetermined => "Not determined: the equations it appears in need more specifications (see degrees of freedom).",
                VarStatus::OverDetermined => "Over-specified.",
                VarStatus::Unused => "Not used by any equation.",
                VarStatus::Determined => "Determined.",
            };
            ui.label(RichText::new(why).color(status_color(s, dark)));
        }
    }
    let mut users: Vec<u32> = rel.down.iter().filter(|(_, &dd)| dd == 1).map(|(x, _)| *x).collect();
    users.sort_by(|x, y| crate::app::natord(&m.vars[*x as usize].name, &m.vars[*y as usize].name));
    egui::CollapsingHeader::new(format!("Read by ({} variables computed directly from it)", users.len())).default_open(true).show(ui, |ui| {
        if users.is_empty() {
            ui.label(RichText::new("nothing: a result").weak());
        }
        for x in users {
            var_link(ui, x, &mut act);
        }
    });
    egui::CollapsingHeader::new(format!("Appears in ({} equations)", d.var_eqs[v as usize].len())).default_open(false).show(ui, |ui| {
        for &e in &d.var_eqs[v as usize] {
            eq_link(ui, e, &mut act);
        }
    });
    if !rel.path.is_empty() {
        egui::CollapsingHeader::new(format!("Solve path ({} blocks, in order)", rel.path.len())).default_open(rel.path.len() <= 12).show(ui, |ui| {
            ui.label(RichText::new("Every block that must be solved before this variable is known.").small().weak());
            for &b in &rel.path {
                let bl = &st.blocks[b as usize];
                let rep = a.solution.as_ref().and_then(|s| s.blocks.get(b as usize));
                let bad = rep.is_some_and(|r| !r.converged);
                let names: Vec<&str> = bl.vars.iter().take(4).map(|x| m.vars[*x as usize].display.as_str()).collect();
                let more = if bl.vars.len() > 4 { format!(" +{}", bl.vars.len() - 4) } else { String::new() };
                ui.horizontal(|ui| {
                    let t = RichText::new(format!("#{:<4}", b + 1)).monospace();
                    ui.label(if bad { t.color(Color32::from_rgb(230, 100, 60)) } else { t.weak() });
                    if ui.link(RichText::new(format!("line {}", m.eqs[bl.eqs[0] as usize].span.line)).monospace()).clicked() {
                        act = Some(DepAction::Jump(m.eqs[bl.eqs[0] as usize].span.line));
                    }
                    for x in &bl.vars {
                        if bl.vars.len() > 4 {
                            break;
                        }
                        if ui.link(RichText::new(&m.vars[*x as usize].display).monospace()).clicked() {
                            act = Some(DepAction::Focus(Some(*x)));
                        }
                    }
                    if bl.vars.len() > 4 {
                        if ui.link(RichText::new(format!("{}{more}", names.join(", "))).monospace()).clicked() {
                            act = Some(DepAction::Focus(Some(bl.vars[0])));
                        }
                    }
                    if bl.eqs.len() == 1 {
                        ui.label(RichText::new(eq_text(a, bl.eqs[0])).monospace().weak());
                    } else {
                        ui.label(RichText::new(format!("{} equations", bl.eqs.len())).weak());
                    }
                    if let Some(r) = rep {
                        if !r.explicit {
                            ui.label(RichText::new(format!("{} it.", r.iterations)).small().weak());
                        }
                    }
                });
            }
        });
    }
    ui.label(RichText::new(format!("{} upstream and {} downstream variables in total", rel.up.len(), rel.down.len())).small().weak());
    act
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{run, Request};
    use dfcore::{CompileOptions, SolveOptions};

    fn analyse(src: &str) -> Arc<Analysis> {
        Arc::new(run(Request { gen: 1, text: src.into(), opts: CompileOptions::default(), solve: true, solve_opts: SolveOptions::default(), warm: None, init: Default::default() }))
    }

    #[test]
    fn chain_and_loop() {
        // a -> b -> (c, d coupled) -> e
        let a = analyse("a = 2\nb = a + 1\nc + d = b\nc - d = 1\ne = c * 2\n");
        let d = Deps::new(&a);
        let id = |n: &str| a.model.var(n).unwrap();
        let r = d.related(&a, id("c"), None);
        assert_eq!(r.coupled, vec![id("d")]);
        assert_eq!(r.up.get(&id("b")), Some(&1));
        assert_eq!(r.up.get(&id("a")), Some(&2));
        assert_eq!(r.down.get(&id("e")), Some(&1));
        assert!(!r.up.contains_key(&id("e")));
        assert_eq!(r.path.len(), 3);
        // limited depth
        let r1 = d.related(&a, id("c"), Some(1));
        assert!(r1.up.contains_key(&id("b")) && !r1.up.contains_key(&id("a")));
        assert!(d.level[id("e") as usize] > d.level[id("c") as usize]);
        assert_eq!(d.level[id("c") as usize], d.level[id("d") as usize]);
        let r = d.related(&a, id("a"), None);
        assert_eq!(r.down.len(), 4);
        assert!(r.up.is_empty());
    }
}
