//! Monochrome SVG icons (white strokes, tinted with the theme's text colour
//! when drawn, so they follow the light and dark themes).

use eframe::egui::{self, ImageSource, Vec2};

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
        _ => egui::include_image!("../assets/icons/text.svg"),
    }
}

/// Icon image of `size` points in the current text colour.
pub fn image(ui: &egui::Ui, name: &str, size: f32) -> egui::Image<'static> {
    egui::Image::new(source(name)).tint(ui.visuals().text_color()).fit_to_exact_size(Vec2::splat(size))
}
