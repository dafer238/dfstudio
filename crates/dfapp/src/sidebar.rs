//! Side panels that can be hidden (Ctrl+B left, Ctrl+Shift+B right) to give
//! the canvas or plot the whole window.

use egui::{Id, Panel, Pos2, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Width of a hidden panel: a strip with the button that shows it again.
const STRIP: f32 = 16.0;

/// Ctrl+B / Ctrl+Shift+B pressed this frame: (toggle left, toggle right).
pub fn keys(ui: &Ui) -> (bool, bool) {
    ui.input_mut(|i| {
        // the shifted shortcut first: Ctrl+B also matches with Shift held
        let right = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::B);
        let left = i.consume_key(egui::Modifiers::COMMAND, egui::Key::B);
        (left, right)
    })
}

/// Whether the panel `id` is shown (remembered across sessions).
pub fn is_open(ui: &Ui, id: &str) -> bool {
    ui.data_mut(|d| *d.get_persisted_mut_or(key(id), true))
}

pub fn toggle(ui: &Ui, id: &str) {
    ui.data_mut(|d| {
        let o = d.get_persisted_mut_or(key(id), true);
        *o = !*o;
    });
}

/// A resizable side panel `id` that can be hidden: the chevron of its
/// [`header`] row hides it, and the thin strip left in its place shows it
/// again.
pub fn panel(ui: &mut Ui, id: &'static str, side: Side, default: f32, add: impl FnOnce(&mut Ui)) {
    let mut open = is_open(ui, id);
    let was = open;
    let shortcut = if side == Side::Left { "Ctrl+B" } else { "Ctrl+Shift+B" };
    let (collapsed, expanded) = match side {
        Side::Left => (Panel::left(format!("{id}_hidden")), Panel::left(id)),
        Side::Right => (Panel::right(format!("{id}_hidden")), Panel::right(id)),
    };
    let mut flip = false;
    let thin = egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(1, 6));
    Panel::show_switched(ui, &mut open, collapsed.resizable(false).exact_size(STRIP).frame(thin), expanded.resizable(true).default_size(default), |ui, expanded| {
        if expanded {
            add(ui);
        } else {
            // In the second half of the closing slide this runs in the
            // expanded panel, which slides towards its fixed edge: only its
            // inner edge stays visible. Put the chevron where the strip will
            // have it, measured from that inner edge, so it does not vanish
            // off-screen (left panel) and jump back when the strip appears.
            let r = ui.max_rect();
            // Fill the panel: egui sizes the slide from the contents, and
            // contents at the far end of a left panel would leave nothing
            // visible, letting the canvas jump under it.
            ui.expand_to_include_rect(r);
            let m = if r.width() > STRIP { egui::Frame::side_top_panel(ui.style()).inner_margin } else { egui::Margin::symmetric(1, 6) };
            let x = match side {
                Side::Left => r.right() + m.right as f32 - 0.5 * STRIP,
                Side::Right => r.left() - m.left as f32 + 0.5 * STRIP,
            };
            let y = r.top() - m.top as f32 + 6.0 + 7.0;
            let rect = egui::Rect::from_center_size(Pos2::new(x, y), Vec2::splat(14.0));
            let resp = ui.interact(rect, Id::NULL.with(("sidebar_show", id)), Sense::click());
            if chevron_paint(ui, resp, side == Side::Right, &format!("Show the panel ({shortcut})")) {
                flip = true;
            }
        }
    });
    if flip {
        open = !open;
    }
    if open != was {
        ui.data_mut(|d| d.insert_persisted(key(id), open));
    }
}

/// First row of the panel `id`: `add` with the hide chevron at the panel's
/// inner edge (right of the content for a left panel, left of it for a
/// right panel).
pub fn header(ui: &mut Ui, id: &'static str, side: Side, add: impl FnOnce(&mut Ui)) {
    let tip = format!("Hide this panel ({})", if side == Side::Left { "Ctrl+B" } else { "Ctrl+Shift+B" });
    match side {
        Side::Left => {
            // content wraps short of the chevron, which sits at the right
            // edge, centred on the first row
            const W: f32 = 14.0;
            let (top, w) = (ui.cursor().min, ui.available_width());
            ui.scope(|ui| {
                ui.set_max_width(w - W - ui.spacing().item_spacing.x);
                ui.horizontal_wrapped(add);
            });
            let row = ui.spacing().interact_size.y;
            let rect = egui::Rect::from_min_size(Pos2::new(top.x + w - W, top.y + 0.5 * (row - W)), Vec2::splat(W));
            let r = ui.interact(rect, Id::NULL.with(("sidebar_hide", id)), Sense::click());
            if chevron_paint(ui, r, true, &tip) {
                toggle(ui, id);
            }
        }
        Side::Right => {
            ui.horizontal(|ui| {
                if chevron(ui, false, &tip) {
                    toggle(ui, id);
                }
                add(ui);
            });
        }
    }
}

/// A small, discreet chevron button pointing left or right.
fn chevron(ui: &mut Ui, left: bool, tip: &str) -> bool {
    let (_, r) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::click());
    chevron_paint(ui, r, left, tip)
}

fn chevron_paint(ui: &Ui, r: egui::Response, left: bool, tip: &str) -> bool {
    let rect = r.rect;
    let r = r.on_hover_text(tip);
    let vis = ui.visuals();
    let col = if r.hovered() { vis.strong_text_color() } else { vis.weak_text_color() };
    if r.hovered() {
        ui.painter().rect_filled(rect, 3.0, vis.widgets.hovered.weak_bg_fill);
    }
    let c = rect.center();
    let (dx, dy) = (2.5, 4.5);
    let s = if left { 1.0 } else { -1.0 };
    let pts = [Pos2::new(c.x + s * dx, c.y - dy), Pos2::new(c.x - s * dx, c.y), Pos2::new(c.x + s * dx, c.y + dy)];
    ui.painter().add(egui::Shape::line(pts.to_vec(), Stroke::new(1.5, col)));
    r.clicked()
}

fn key(id: &str) -> Id {
    Id::NULL.with(("sidebar_open", id))
}
