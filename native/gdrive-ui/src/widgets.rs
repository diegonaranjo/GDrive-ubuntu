//! Reusable widgets (GTK4/libadwaita), Rust ports of `gdrive/widgets.py`.

use adw::prelude::*;
use gtk4 as gtk;

/// Status badge colors, mirroring `STATUS_BADGE_COLORS` in `widgets.py`.
const STATUS_BADGE_COLORS: [(&str, &str); 5] = [
    ("idle", "#1E8E3E"),
    ("syncing", "#1A73E8"),
    ("paused", "#F9AB00"),
    ("error", "#D93025"),
    ("unknown", "#888888"),
];

/// Icon per status state (symbolic names installed by the project).
const STATUS_ICONS: [(&str, &str); 5] = [
    ("idle", "gdrive-status-symbolic"),
    ("syncing", "gdrive-syncing-symbolic"),
    ("paused", "gdrive-paused-symbolic"),
    ("error", "gdrive-error-symbolic"),
    ("unknown", "dialog-information-symbolic"),
];

/// Circular status badge shown at the top of the Status page.
///
/// A `Gtk::Box` wrapping a 48px `Gtk::Image`, tinted and animated via CSS
/// (`status-badge` + per-state classes), identical to the Python `StatusBadge`.
#[derive(Clone)]
pub struct StatusBadge {
    container: gtk::Box,
    icon: gtk::Image,
}

impl StatusBadge {
    /// Creates a badge in the `idle` state.
    pub fn new() -> Self {
        let container = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .margin_top(10)
            .build();
        let icon = gtk::Image::builder().pixel_size(48).build();
        icon.add_css_class("status-badge-icon");
        container.append(&icon);

        let badge = StatusBadge { container, icon };
        badge.set_status("idle");
        badge
    }

    /// Returns the underlying widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Applies the state: tint color + icon + CSS class.
    pub fn set_status(&self, status: &str) {
        let color = STATUS_BADGE_COLORS
            .iter()
            .find(|(s, _)| *s == status)
            .map_or("#888888", |(_, c)| c);
        let icon = STATUS_ICONS
            .iter()
            .find(|(s, _)| *s == status)
            .map_or("dialog-information-symbolic", |(_, i)| i);

        self.container.add_css_class("status-badge");
        for (s, _) in STATUS_BADGE_COLORS {
            if s != "unknown" {
                self.container.remove_css_class(s);
            }
        }
        // Every real state gets its own class; "unknown" is not emitted.
        if status != "unknown" {
            self.container.add_css_class(status);
        }

        // Tint via an inline CSS provider (mirrors the Python approach).
        let css = format!(".status-badge {{ color: {color}; }}");
        let provider = gtk::CssProvider::new();
        provider.load_from_data(&css);
        self.icon
            .style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);

        self.icon.set_icon_name(Some(icon));
    }
}

impl Default for StatusBadge {
    fn default() -> Self {
        Self::new()
    }
}
