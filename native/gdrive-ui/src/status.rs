//! Status page — sync status, info and controls.
//!
//! Rust port of `gdrive/pages/status.py`, driven by [`DaemonClient`] over
//! D-Bus. It shows the state badge, last-sync/errors/speed rows, a progress bar
//! while syncing, and Sync/Pause buttons.

use adw::prelude::*;
use gtk4 as gtk;

use crate::dbus_client::DaemonClient;
use crate::i18n::{t, Lang};
use crate::widgets::StatusBadge;

/// Maps a status string to its title/detail i18n keys (mirrors `STATUS_MAP`).
fn status_keys(status: &str) -> (&'static str, &'static str) {
    match status {
        "syncing" => ("status.syncing", "status.detail.syncing"),
        "paused" => ("status.paused", "status.detail.paused"),
        "error" => ("status.error", "status.detail.error"),
        _ => ("status.idle", "status.detail.idle"),
    }
}

/// Creates a small symbolic icon image for an `ActionRow` prefix.
fn info_icon(name: &str) -> gtk::Image {
    gtk::Image::from_icon_name(name)
}

/// The Status page widget.
pub struct StatusPage {
    root: gtk::Box,
    lang: Lang,
    title: gtk::Label,
    detail: gtk::Label,
    progress: gtk::ProgressBar,
    last_sync: adw::ActionRow,
    /// Files/errors rows are rendered now but populated later (the daemon does
    /// not currently emit per-sync file/error counts).
    #[allow(dead_code)]
    files_row: adw::ActionRow,
    #[allow(dead_code)]
    errors_row: adw::ActionRow,
    speed_row: adw::ActionRow,
    sync_btn: gtk::Button,
    pause_btn: gtk::Button,
    badge: StatusBadge,
}

impl StatusPage {
    /// Builds the page, subscribes to the daemon's signals, and does an initial
    /// status refresh.
    pub fn new(client: DaemonClient, lang: Lang) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();

        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        root.append(&scroll);

        let clamp = adw::Clamp::builder()
            .hexpand(true)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(24)
            .margin_end(24)
            .build();
        scroll.set_child(Some(&clamp));

        let col = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(20)
            .build();
        clamp.set_child(Some(&col));

        let badge = StatusBadge::new();
        col.append(badge.widget());

        let title = gtk::Label::builder().halign(gtk::Align::Center).build();
        col.append(&title);

        let detail = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .width_chars(40)
            .build();
        detail.add_css_class("dim-label");
        col.append(&detail);

        let progress = gtk::ProgressBar::builder()
            .margin_top(6)
            .margin_start(40)
            .margin_end(40)
            .visible(false)
            .build();
        col.append(&progress);

        // ── Info group ──────────────────────────────────────────────────────
        let info = adw::PreferencesGroup::builder()
            .title(t(lang, "status.info_group"))
            .build();
        col.append(&info);

        let last_sync = adw::ActionRow::builder()
            .title(t(lang, "status.last_sync"))
            .subtitle(t(lang, "status.last_sync_value"))
            .build();
        last_sync.add_prefix(&info_icon("clock-pause-symbolic"));
        info.add(&last_sync);

        let files_row = adw::ActionRow::builder()
            .title(t(lang, "status.files_synced"))
            .subtitle("—")
            .build();
        files_row.add_prefix(&info_icon("check-circle-symbolic"));
        info.add(&files_row);

        let errors_row = adw::ActionRow::builder()
            .title(t(lang, "status.errors"))
            .subtitle("0")
            .build();
        errors_row.add_prefix(&info_icon("alert-circle-symbolic"));
        info.add(&errors_row);

        let speed_row = adw::ActionRow::builder()
            .title(t(lang, "status.speed"))
            .subtitle("—")
            .build();
        speed_row.add_prefix(&info_icon("globe-symbolic"));
        info.add(&speed_row);

        // ── Buttons ─────────────────────────────────────────────────────────
        let btns = gtk::Box::builder()
            .spacing(12)
            .halign(gtk::Align::Center)
            .margin_top(10)
            .build();
        col.append(&btns);

        let sync_btn = gtk::Button::builder()
            .label(t(lang, "status.sync_now"))
            .css_classes(["suggested-action", "pill"])
            .width_request(180)
            .height_request(40)
            .build();
        btns.append(&sync_btn);

        let pause_btn = gtk::Button::builder()
            .label(t(lang, "status.pause"))
            .css_classes(["pill"])
            .width_request(120)
            .height_request(40)
            .build();
        btns.append(&pause_btn);

        let page = StatusPage {
            root,
            lang,
            title,
            detail,
            progress,
            last_sync,
            files_row,
            errors_row,
            speed_row,
            sync_btn,
            pause_btn,
            badge,
        };

        page.wire_buttons(&client);
        page.wire_signals(&client);
        page.refresh(&client);

        page
    }

    /// Returns the root widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Connects the Sync/Pause buttons to the daemon.
    fn wire_buttons(&self, client: &DaemonClient) {
        let c = client.clone();
        self.sync_btn.connect_clicked(move |_| {
            let _ = c.resume();
            let _ = c.sync_now();
        });

        let c = client.clone();
        self.pause_btn.connect_clicked(move |_| {
            let _ = c.pause();
        });
    }

    /// Subscribes to the daemon's live signals and updates the presentation.
    fn wire_signals(&self, client: &DaemonClient) {
        let lang = self.lang;
        let pres = self.presentation();

        client.on_status_changed(move |status, paused| {
            pres.apply(lang, &status, paused);
        });

        let detail = self.detail.clone();
        let progress = self.progress.clone();
        client.on_progress(move |text, transferred, total| {
            detail.set_text(&text);
            if total > 0 {
                let frac = (transferred as f64 / total as f64).min(1.0);
                progress.set_visible(true);
                progress.set_fraction(frac);
                progress.set_text(Some(&format!("{transferred}/{total}")));
                progress.set_show_text(true);
            } else {
                progress.set_visible(false);
            }
        });

        let speed_row = self.speed_row.clone();
        client.on_speed_changed(move |speed| {
            speed_row.set_subtitle(&speed);
        });

        let speed_row = self.speed_row.clone();
        client.on_sync_finished(move |_ok, _msg| {
            speed_row.set_subtitle("—");
        });
    }

    /// Re-fetches the daemon status and applies it (initial render).
    fn refresh(&self, client: &DaemonClient) {
        if let Ok(status) = client.get_status() {
            self.presentation()
                .apply(self.lang, &status.state, status.paused);
            if !status.last_sync.is_empty() {
                self.last_sync.set_subtitle(&status.last_sync);
            }
        }
    }

    /// Builds a [`StatusPresentation`] snapshot of the state-driven widgets.
    fn presentation(&self) -> StatusPresentation {
        StatusPresentation {
            badge: self.badge.clone(),
            title: self.title.clone(),
            detail: self.detail.clone(),
            progress: self.progress.clone(),
            sync_btn: self.sync_btn.clone(),
            pause_btn: self.pause_btn.clone(),
        }
    }
}

/// The set of widgets driven by the current sync status. Cloned into signal
/// closures (all GTK objects are refcounted) so a status change updates them
/// without borrowing `StatusPage` mutably.
#[derive(Clone)]
struct StatusPresentation {
    badge: StatusBadge,
    title: gtk::Label,
    detail: gtk::Label,
    progress: gtk::ProgressBar,
    sync_btn: gtk::Button,
    pause_btn: gtk::Button,
}

impl StatusPresentation {
    /// Applies a status snapshot: badge, title, detail, progress, buttons.
    fn apply(&self, lang: Lang, status: &str, paused: bool) {
        self.badge.set_status(status);
        let (tk, dk) = status_keys(status);
        self.title.set_markup(&format!(
            "<span size=\"x-large\" weight=\"bold\">{}</span>",
            t(lang, tk)
        ));
        self.detail.set_text(t(lang, dk));
        self.pause_btn.set_label(if paused {
            t(lang, "status.resume")
        } else {
            t(lang, "status.pause")
        });
        self.sync_btn.set_sensitive(status != "syncing");
        self.progress.set_visible(status == "syncing");
    }
}
