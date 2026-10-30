//! GDrive native GTK4/libadwaita UI.
//!
//! Rust port of the Python `gdrive-app.py` window. It presents the same
//! six-page libadwaita shell and talks to `gdrive-core`'s daemon over the
//! existing D-Bus interface (`com.gdrive.Daemon1`), via [`DaemonClient`].
//!
//! Pages implemented: Status, Account, Activity, Selective. Synchronization
//! and Settings are placeholders. The tray remains the Python GTK3/AppIndicator
//! process during the transition.

#![warn(missing_docs)]

mod account;
mod activity;
mod config;
mod dbus_client;
mod helpers;
mod i18n;
mod selective;
mod status;
mod widgets;
mod window;

use std::process::ExitCode;

use adw::prelude::*;

use crate::i18n::Lang;

/// Application ID for the GTK4 window (mirrors `APP_ID` in `gdrive/constants.py`).
const APP_ID: &str = "com.gdrive.ClientUI";

/// Application entry-point. Builds the `Adw::Application` and runs it.
fn main() -> ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| load_theme());
    app.connect_activate(build_ui);

    app.run();
    ExitCode::SUCCESS
}

/// Builds the main window on activation: connects to the daemon, creates the
/// window (singleton per app), and presents it. The [`DaemonClient`] is kept
/// alive by the window for its lifetime.
fn build_ui(app: &adw::Application) {
    let client = dbus_client::DaemonClient::connect().expect("session bus unavailable");

    let window = window::DriveWindow::new(app, client, default_lang());
    window.borrow().present();
}

/// Resolves the UI language. Phase 2 defaults to Spanish (same as the Python
/// UI); config-driven language selection is ported in a later phase.
fn default_lang() -> Lang {
    Lang::Es
}

/// Loads the app theme (CSS). Port of `gdrive/theme.py`; reads `theme/theme.css`
/// from the repo root (same file the Python UI uses).
fn load_theme() {
    use gtk4::gdk;

    let css = gtk4::CssProvider::new();
    css.load_from_path("theme/theme.css");

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
