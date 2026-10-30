//! Account page — linked account email, storage usage, and actions.
//!
//! Rust port of `gdrive/pages/account.py`. The `rclone` queries run on a
//! background thread and marshal results back to the main loop via
//! `glib::idle_add` (GTK widgets are `Send` but must be touched on the main
//! thread, which `idle_add` guarantees).

use adw::prelude::*;
use gtk4 as gtk;

use crate::helpers::{format_size, open_url, rclone_about, rclone_user_email};
use crate::i18n::{t, tf, Lang};

/// The Account page widget.
pub struct AccountPage {
    root: gtk::Box,
    email: gtk::Label,
    bar: gtk::ProgressBar,
    used: gtk::Label,
    detail: gtk::Label,
}

impl AccountPage {
    /// Builds the page and triggers an initial `rclone` refresh.
    pub fn new(lang: Lang) -> Self {
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

        let title = gtk::Label::builder().xalign(0.0).build();
        title.set_markup(&format!(
            "<span size=\"xx-large\" weight=\"bold\">{}</span>",
            t(lang, "account.title")
        ));
        col.append(&title);

        let email = gtk::Label::builder().xalign(0.0).build();
        email.set_markup(&format!(
            "<span size=\"large\" weight=\"bold\">{}</span>",
            t(lang, "account.not_signed_in")
        ));
        email.add_css_class("dim-label");
        col.append(&email);

        let grp = adw::PreferencesGroup::builder()
            .title(t(lang, "account.storage"))
            .build();
        col.append(&grp);

        let bar = gtk::ProgressBar::builder()
            .show_text(true)
            .height_request(32)
            .build();
        bar.add_css_class("osd");
        bar.add_css_class("storage-bar");
        grp.add(&bar);

        let used = gtk::Label::builder().xalign(0.0).build();
        grp.add(&used);

        let detail = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        detail.add_css_class("dim-label");
        grp.add(&detail);

        // ── Actions ─────────────────────────────────────────────────────────
        let g2 = adw::PreferencesGroup::builder()
            .title(t(lang, "account.actions"))
            .build();
        col.append(&g2);

        let r1 = adw::ActionRow::builder()
            .title(t(lang, "account.open_drive"))
            .build();
        r1.add_prefix(&info_icon("globe-symbolic"));
        let b1 = gtk::Button::builder()
            .label(t(lang, "account.open"))
            .css_classes(["flat"])
            .build();
        b1.connect_clicked(|_| open_url("https://drive.google.com"));
        r1.add_suffix(&b1);
        g2.add(&r1);

        let r2 = adw::ActionRow::builder()
            .title(t(lang, "account.manage_account"))
            .build();
        r2.add_prefix(&info_icon("user-symbolic"));
        let b2 = gtk::Button::builder()
            .label(t(lang, "account.open"))
            .css_classes(["flat"])
            .build();
        b2.connect_clicked(|_| open_url("https://myaccount.google.com"));
        r2.add_suffix(&b2);
        g2.add(&r2);

        let refresh_btn = gtk::Button::builder()
            .label(t(lang, "account.refresh_info"))
            .css_classes(["flat", "pill"])
            .halign(gtk::Align::Start)
            .build();
        g2.add(&refresh_btn);

        let page = AccountPage {
            root,
            email,
            bar,
            used,
            detail,
        };

        // Refresh button re-runs the queries.
        {
            let page = page.clone_refs();
            refresh_btn.connect_clicked(move |_| page.refresh(lang));
        }

        page.refresh(lang);

        page
    }

    /// Returns the root widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Runs the `rclone` queries off-thread and applies results on the main loop.
    ///
    /// GTK widgets are not `Send`, so the thread only computes plain data
    /// (`Option<Value>`, `Option<String>`) and ships it back through an
    /// `mpsc` channel; a `glib::idle_add_local` source on the main loop drains
    /// the channel and updates the widgets.
    fn refresh(&self, lang: Lang) {
        let email = self.email.clone();
        let bar = self.bar.clone();
        let used = self.used.clone();
        let detail = self.detail.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let about = rclone_about();
            let user_email = rclone_user_email();
            let _ = tx.send((about, user_email));
        });

        glib::idle_add_local(move || match rx.try_recv() {
            Ok((about, user_email)) => {
                apply_account(about, user_email, lang, &email, &bar, &used, &detail);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });
    }

    /// Returns clones of the mutable presentation widgets for `refresh`.
    fn clone_refs(&self) -> AccountPage {
        AccountPage {
            root: self.root.clone(),
            email: self.email.clone(),
            bar: self.bar.clone(),
            used: self.used.clone(),
            detail: self.detail.clone(),
        }
    }
}

/// Applies the result of `rclone about`/`rclone user` to the presentation.
fn apply_account(
    about: Option<serde_json::Value>,
    user_email: Option<String>,
    lang: Lang,
    email: &gtk::Label,
    bar: &gtk::ProgressBar,
    used: &gtk::Label,
    detail: &gtk::Label,
) {
    if let Some(e) = user_email {
        email.set_markup(&format!("<span size=\"large\" weight=\"bold\">{e}</span>"));
    } else if about.is_some() {
        email.set_markup(&format!(
            "<span size=\"large\" weight=\"bold\">{}</span>",
            t(lang, "account.google_drive_account")
        ));
    }

    let Some(about) = about else {
        bar.set_fraction(0.0);
        bar.set_text(Some("—"));
        used.set_text(t(lang, "account.no_storage_info"));
        detail.set_text(t(lang, "account.no_storage_info"));
        return;
    };

    let (mut used_bytes, mut total_bytes) = (0u64, 0u64);
    if let Some(totals) = about.get("totals").and_then(|v| v.as_array()) {
        for t in totals {
            used_bytes += t.get("used").and_then(|v| v.as_u64()).unwrap_or(0);
            total_bytes += t.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        }
    }

    if total_bytes > 0 {
        let frac = used_bytes as f64 / total_bytes as f64;
        bar.set_fraction(frac);
        bar.set_text(Some(&format!("{:.1}%", frac * 100.0)));
        used.set_text(&tf(
            lang,
            "account.storage_used",
            &[
                ("used", &format_size(used_bytes)),
                ("total", &format_size(total_bytes)),
            ],
        ));
    } else {
        bar.set_fraction(0.0);
        bar.set_text(Some("∞"));
        used.set_text(t(lang, "account.no_limit"));
    }
}

/// Creates a small symbolic icon image for an `ActionRow` prefix.
fn info_icon(name: &str) -> gtk::Image {
    gtk::Image::from_icon_name(name)
}
