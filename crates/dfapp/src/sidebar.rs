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

/// A resizable side panel `id` that can be hidden: a small chevron at its
/// top hides it, and the thin strip left in its place shows it again.
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
            // header strip with the hide button at the panel's outer edge
            ui.horizontal(|ui| {
                ui.set_height(14.0);
                let layout = if side == Side::Left { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
                ui.with_layout(layout, |ui| {
                    if chevron(ui, side == Side::Left, &format!("Hide this panel ({shortcut})")) {
                        flip = true;
                    }
                });
            });
            add(ui);
        } else if chevron(ui, side == Side::Right, &format!("Show the panel ({shortcut})")) {
            flip = true;
        }
    });
    if flip {
        open = !open;
    }
    if open != was {
        ui.data_mut(|d| d.insert_persisted(key(id), open));
    }
}

/// A small, discreet chevron button pointing left or right.
fn chevron(ui: &mut Ui, left: bool, tip: &str) -> bool {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::click());
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
