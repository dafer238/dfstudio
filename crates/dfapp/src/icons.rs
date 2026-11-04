//! Monochrome SVG icons (white strokes, tinted with the theme's text colour
//! when drawn, so they follow the light and dark themes).

use eframe::egui::{self, ImageSource, Vec2};

/// Names of all icons.
#[cfg(test)]
pub const NAMES: [&str; 36] = [
    "turbine",
    "compressor",
    "pump",
    "heat_exchanger",
    "hrsg",
    "condenser",
    "valve",
    "source_sink",
    "electrical",
    "new",
    "open",
    "save",
    "save_as",
    "files",
    "example",
    "undo",
    "redo",
    "find",
    "replace",
    "format",
    "solve",
    "units",
    "reset",
    "font_larger",
    "font_smaller",
    "book",
    "bug",
    "download",
    "sun",
    "moon",
    "monitor",
    "text",
    "function",
    "fluid",
    "braces",
    "heart",
];

/// Icon by name; unknown names give the text icon.
pub fn source(name: &str) -> ImageSource<'static> {
    match name {
        "turbine" => egui::include_image!("../assets/icons/turbine.svg"),
        "compressor" => egui::include_image!("../assets/icons/compressor.svg"),
        "pump" => egui::include_image!("../assets/icons/pump.svg"),
        "heat_exchanger" => egui::include_image!("../assets/icons/heat_exchanger.svg"),
        "hrsg" => egui::include_image!("../assets/icons/hrsg.svg"),
        "condenser" => egui::include_image!("../assets/icons/condenser.svg"),
        "valve" => egui::include_image!("../assets/icons/valve.svg"),
        "source_sink" => egui::include_image!("../assets/icons/source_sink.svg"),
        "electrical" => egui::include_image!("../assets/icons/electrical.svg"),
        "new" => egui::include_image!("../assets/icons/new.svg"),
        "open" => egui::include_image!("../assets/icons/open.svg"),
        "save" => egui::include_image!("../assets/icons/save.svg"),
        "save_as" => egui::include_image!("../assets/icons/save_as.svg"),
        "files" => egui::include_image!("../assets/icons/files.svg"),
        "example" => egui::include_image!("../assets/icons/example.svg"),
        "undo" => egui::include_image!("../assets/icons/undo.svg"),
        "redo" => egui::include_image!("../assets/icons/redo.svg"),
        "find" => egui::include_image!("../assets/icons/find.svg"),
        "replace" => egui::include_image!("../assets/icons/replace.svg"),
        "format" => egui::include_image!("../assets/icons/format.svg"),
        "solve" => egui::include_image!("../assets/icons/solve.svg"),
        "units" => egui::include_image!("../assets/icons/units.svg"),
        "reset" => egui::include_image!("../assets/icons/reset.svg"),
        "font_larger" => egui::include_image!("../assets/icons/font_larger.svg"),
        "font_smaller" => egui::include_image!("../assets/icons/font_smaller.svg"),
        "book" => egui::include_image!("../assets/icons/book.svg"),
        "bug" => egui::include_image!("../assets/icons/bug.svg"),
        "download" => egui::include_image!("../assets/icons/download.svg"),
        "sun" => egui::include_image!("../assets/icons/sun.svg"),
        "moon" => egui::include_image!("../assets/icons/moon.svg"),
        "monitor" => egui::include_image!("../assets/icons/monitor.svg"),
        "function" => egui::include_image!("../assets/icons/function.svg"),
        "fluid" => egui::include_image!("../assets/icons/fluid.svg"),
        "braces" => egui::include_image!("../assets/icons/braces.svg"),
        "heart" => egui::include_image!("../assets/icons/heart.svg"),
        _ => egui::include_image!("../assets/icons/text.svg"),
    }
}

/// Icon image of `size` points in the current text colour.
pub fn image(ui: &egui::Ui, name: &str, size: f32) -> egui::Image<'static> {
    egui::Image::new(source(name)).tint(ui.visuals().text_color()).fit_to_exact_size(Vec2::splat(size))
}

/// Menu entry with an icon and, right-aligned, its keyboard shortcut.
pub fn menu_item(ui: &mut egui::Ui, icon: &str, text: &str, shortcut: &str) -> egui::Response {
    let mut b = egui::Button::image_and_text(image(ui, icon, 14.0), text);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    ui.add(b)
}

#[cfg(test)]
mod tests {
    /// Every icon is a monochrome SVG drawn with white strokes only.
    #[test]
    fn icons_are_monochrome_svg() {
        for n in super::NAMES {
            let p = format!("{}/assets/icons/{n}.svg", env!("CARGO_MANIFEST_DIR"));
            let s = std::fs::read_to_string(&p).unwrap();
            assert!(s.starts_with("<svg") && s.contains("viewBox=\"0 0 24 24\""), "{n}");
            let colours: Vec<&str> = s.split('#').skip(1).map(|c| &c[..6]).collect();
            assert!(colours.iter().all(|c| *c == "ffffff"), "{n}: {colours:?}");
            assert!(matches!(super::source(n).uri(), Some(u) if u.ends_with(&format!("/{n}.svg"))), "{n}");
        }
    }
}
