//! *Help > Support DeltaF Studio*: how the project is funded and where to pay.
//!
//! The links go through short links on the project site, so the payment
//! pages can change without a new release of the app.

use eframe::egui::{self, RichText};

/// Overview of the ways to support the project.
pub const SUPPORT_PAGE: &str = "https://dfstudio.neodafer.com/#support";

/// (label, hover text, URL) of each payment option.
pub const OPTIONS: [(&str, &str, &str); 4] = [
    ("GitHub Sponsors", "Monthly or one-time, through your GitHub account", "https://github.com/sponsors/dafer238"),
    ("One-time payment", "Card or other methods through Stripe; you choose the amount", "https://dfstudio.neodafer.com/pay/once"),
    ("Monthly subscription", "Through Stripe; cancel at any time", "https://dfstudio.neodafer.com/pay/monthly"),
    ("Yearly subscription", "Through Stripe; invoices in the customer portal; cancel at any time", "https://dfstudio.neodafer.com/pay/yearly"),
];

/// Stripe customer portal: invoices, payment method, cancelling.
pub const MANAGE: &str = "https://dfstudio.neodafer.com/pay/manage";

/// Draw the window while `open` is true.
pub fn window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Support DeltaF Studio").open(open).resizable(false).default_width(460.0).show(ctx, |ui| {
        ui.label(
            "DeltaF Studio is free software under the GNU AGPL-3.0, and it will stay free software. \
             Personal, academic and educational use is and will remain free of charge.",
        );
        ui.add_space(6.0);
        ui.label(
            "Commercial use is not restricted in any way. If you or your organisation use DeltaF Studio \
             for paid work, please consider paying for it, once or by subscription: it funds \
             maintenance, verification and new features.",
        );
        ui.add_space(4.0);
        ui.label(RichText::new("This is a request, not a condition of the licence.").weak());
        ui.add_space(8.0);
        for (label, hover, url) in OPTIONS {
            if ui.add(egui::Button::new(label).min_size(egui::vec2(220.0, 0.0))).on_hover_text(format!("{hover}\n{url}")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.hyperlink_to("Manage a subscription", MANAGE);
            ui.label("·");
            ui.hyperlink_to("All options", SUPPORT_PAGE);
        });
    });
}

#[cfg(test)]
mod tests {
    /// Every link is an https URL on the project site or GitHub.
    #[test]
    fn links_are_https() {
        let urls = super::OPTIONS.iter().map(|o| o.2).chain([super::MANAGE, super::SUPPORT_PAGE]);
        for u in urls {
            assert!(u.starts_with("https://dfstudio.neodafer.com/") || u.starts_with("https://github.com/"), "{u}");
        }
    }
}
