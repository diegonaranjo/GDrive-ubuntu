//! Synchronization settings page — sync folder, mode, interval, bandwidth,
//! advanced options and shortcuts.
//!
//! Rust port of `gdrive/pages/sync_settings.py`. Changes are persisted to
//! `settings.json` and, where the daemon needs to react (pause/resume, interval,
//! watch mode), pushed over D-Bus via [`DaemonClient`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use serde_json::Value;

use crate::config::{
    get_bool, get_string, get_u64, load_config, save_config, set_bool, set_string, set_u64,
    sync_dir_default,
};
use crate::dbus_client::DaemonClient;
use crate::helpers::open_url;
use crate::i18n::{t, Lang};

/// Toast callback shared with the window (shows `adw::Toast`).
pub type ToastFn = Rc<dyn Fn(&str)>;

/// The Synchronization page widget.
pub struct SyncSettingsPage {
    root: gtk::Box,
    config: Rc<RefCell<HashMap<String, Value>>>,
    client: DaemonClient,
    on_toast: ToastFn,
    lang: Lang,
}

impl SyncSettingsPage {
    /// Builds the page; `client` drives daemon side-effects (pause/resume/
    /// config push), `on_toast` reports saved/cleared feedback.
    pub fn new(client: DaemonClient, lang: Lang, on_toast: ToastFn) -> Self {
        let config = Rc::new(RefCell::new(load_config()));

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
            t(lang, "sync.title")
        ));
        col.append(&title);

        // Location group.
        let g1 = adw::PreferencesGroup::builder()
            .title(t(lang, "sync.location"))
            .build();
        col.append(&g1);

        let folder_row = adw::ActionRow::builder()
            .title(t(lang, "sync.sync_folder"))
            .subtitle(get_string(&config.borrow(), "sync_dir"))
            .build();
        folder_row.add_prefix(&gtk::Image::from_icon_name("folder-symbolic"));
        let change = gtk::Button::builder()
            .label(t(lang, "sync.change"))
            .css_classes(["flat"])
            .build();
        folder_row.add_suffix(&change);
        g1.add(&folder_row);

        let mut page = SyncSettingsPage {
            root,
            config: config.clone(),
            client,
            on_toast,
            lang,
        };

        // Change folder via FileDialog.
        {
            let page_folder = folder_row.clone();
            let page_config = config.clone();
            let parent = root.clone();
            change.connect_clicked(move |_| {
                let dialog = gtk::FileDialog::builder()
                    .title(t(lang, "sync.select_folder"))
                    .build();
                let parent = parent.clone();
                dialog.select_folder(Some(&parent), gio::Cancellable::NONE, move |result| {
                    let Ok(gfile) = dialog.select_folder_finish(result) else {
                        return;
                    };
                    if let Some(path) = gfile.path() {
                        let p = path.display().to_string();
                        set_string(&mut page_config.borrow_mut(), "sync_dir", &p);
                        save_config(&page_config.borrow());
                        page_folder.set_subtitle(&p);
                    }
                });
            });
        }

        // Mode group (ComboRow).
        let g2 = adw::PreferencesGroup::builder().title(t(lang, "sync.mode")).build();
        col.append(&g2);
        let modes = ["mirror", "selective"];
        let mode_labels: &[&str] = &[
            t(lang, "sync.mode_mirror"),
            t(lang, "sync.mode_selective"),
        ];
        let model = gtk::StringList::new(mode_labels);
        let mode_row = adw::ComboRow::builder()
            .title(t(lang, "sync.file_mode"))
            .subtitle(t(lang, "sync.file_mode_subtitle"))
            .model(&model)
            .build();
        let current_mode = get_string(&config.borrow(), "sync_mode");
        let sel = modes
            .iter()
            .position(|m| *m == current_mode)
            .unwrap_or(0) as u32;
        mode_row.set_selected(sel);
        g2.add(&mode_row);
        {
            let page_config = config.clone();
            let on_toast = page.on_toast.clone();
            mode_row.connect_selected_notify(move |row| {
                let idx = row.selected() as usize;
                let val = modes.get(idx).copied().unwrap_or("mirror");
                set_string(&mut page_config.borrow_mut(), "sync_mode", val);
                save_config(&page_config.borrow());
                on_toast(t(lang, "sync.config_saved"));
            });
        }

        // Scheduling group.
        let g3 = adw::PreferencesGroup::builder()
            .title(t(lang, "sync.scheduling"))
            .build();
        col.append(&g3);
        let intervals = ["manual", "5min", "15min", "30min", "1hr"];
        let interval_labels: &[&str] = &[
            t(lang, "sync.interval_manual"),
            t(lang, "sync.interval_5min"),
            t(lang, "sync.interval_15min"),
            t(lang, "sync.interval_30min"),
            t(lang, "sync.interval_1hr"),
        ];
        let model_i = gtk::StringList::new(interval_labels);
        let interval_row = adw::ComboRow::builder()
            .title(t(lang, "sync.interval"))
            .subtitle(t(lang, "sync.interval_subtitle"))
            .model(&model_i)
            .build();
        let current_iv = get_string(&config.borrow(), "sync_interval");
        let sel = intervals
            .iter()
            .position(|m| *m == current_iv)
            .unwrap_or(0) as u32;
        interval_row.set_selected(sel);
        g3.add(&interval_row);
        {
            let page_config = config.clone();
            let page_client = page.client.clone();
            interval_row.connect_selected_notify(move |row| {
                let idx = row.selected() as usize;
                let val = intervals.get(idx).copied().unwrap_or("manual");
                set_string(&mut page_config.borrow_mut(), "sync_interval", val);
                save_config(&page_config.borrow());
                // Notify the daemon so it restarts its periodic scheduler.
                let mut changes = HashMap::new();
                changes.insert("sync_interval".to_string(), val.to_string());
                let _ = page_client.update_config(changes);
            });
        }

        // Pause switch (drives daemon pause/resume).
        let pause_sw = adw::SwitchRow::builder()
            .title(t(lang, "sync.pause_sync"))
            .subtitle(t(lang, "sync.pause_subtitle"))
            .active(get_bool(&config.borrow(), "paused"))
            .build();
        g3.add(&pause_sw);
        {
            let page_config = config.clone();
            let page_client = page.client.clone();
            pause_sw.connect_active_notify(move |row| {
                let active = row.is_active();
                set_bool(&mut page_config.borrow_mut(), "paused", active);
                save_config(&page_config.borrow());
                if active {
                    let _ = page_client.pause();
                } else {
                    let _ = page_client.resume();
                }
            });
        }

        // Watch changes switch.
        let watch_sw = adw::SwitchRow::builder()
            .title(t(lang, "sync.watch_changes"))
            .subtitle(t(lang, "sync.watch_subtitle"))
            .active(get_bool(&config.borrow(), "watch_changes"))
            .build();
        g3.add(&watch_sw);
        {
            let page_config = config.clone();
            let page_client = page.client.clone();
            watch_sw.connect_active_notify(move |row| {
                let active = row.is_active();
                set_bool(&mut page_config.borrow_mut(), "watch_changes", active);
                save_config(&page_config.borrow());
                let mut changes = HashMap::new();
                changes.insert("watch_changes".to_string(), active.to_string());
                let _ = page_client.update_config(changes);
            });
        }

        // Bandwidth group.
        let g4 = adw::PreferencesGroup::builder()
            .title(t(lang, "sync.bandwidth"))
            .build();
        col.append(&g4);
        let bw_row = adw::ActionRow::builder()
            .title(t(lang, "sync.global_limit"))
            .subtitle(t(lang, "sync.no_limit"))
            .build();
        let bw_entry = gtk::Entry::builder()
            .placeholder_text(t(lang, "sync.placeholder_bw"))
            .text(get_string(&config.borrow(), "bandwidth_limit"))
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        bw_row.add_suffix(&bw_entry);
        g4.add(&bw_row);
        {
            let page_config = config.clone();
            bw_entry.connect_changed(move |entry| {
                set_string(
                    &mut page_config.borrow_mut(),
                    "bandwidth_limit",
                    &entry.text(),
                );
                save_config(&page_config.borrow());
            });
        }

        // Advanced group.
        let g5 = adw::PreferencesGroup::builder()
            .title(t(lang, "sync.advanced"))
            .build();
        col.append(&g5);
        let skip_sw = adw::SwitchRow::builder()
            .title(t(lang, "sync.skip_hidden"))
            .active(get_bool(&config.borrow(), "skip_hidden"))
            .build();
        g5.add(&skip_sw);
        {
            let page_config = config.clone();
            let on_toast = page.on_toast.clone();
            skip_sw.connect_active_notify(move |row| {
                set_bool(&mut page_config.borrow_mut(), "skip_hidden", row.is_active());
                save_config(&page_config.borrow());
                on_toast(t(lang, "sync.config_saved"));
            });
        }
        let ts_sw = adw::SwitchRow::builder()
            .title(t(lang, "sync.store_metadata"))
            .active(get_bool(&config.borrow(), "store_metadata"))
            .build();
        g5.add(&ts_sw);
        {
            let page_config = config.clone();
            let on_toast = page.on_toast.clone();
            ts_sw.connect_active_notify(move |row| {
                set_bool(
                    &mut page_config.borrow_mut(),
                    "store_metadata",
                    row.is_active(),
                );
                save_config(&page_config.borrow());
                on_toast(t(lang, "sync.config_saved"));
            });
        }
        let threads_row = adw::ActionRow::builder()
            .title(t(lang, "sync.transfer_threads"))
            .build();
        let spin = gtk::SpinButton::with_range(1.0, 16.0, 1.0);
        spin.set_value(get_u64(&config.borrow(), "num_threads") as f64);
        threads_row.add_suffix(&spin);
        g5.add(&threads_row);
        {
            let page_config = config.clone();
            let on_toast = page.on_toast.clone();
            spin.connect_value_changed(move |s| {
                set_u64(
                    &mut page_config.borrow_mut(),
                    "num_threads",
                    s.value() as u64,
                );
                save_config(&page_config.borrow());
                on_toast(t(lang, "sync.config_saved"));
            });
        }

        // Shortcuts group.
        let g6 = adw::PreferencesGroup::builder()
            .title(t(lang, "sync.shortcuts"))
            .build();
        col.append(&g6);
        let o1 = adw::ActionRow::builder().title(t(lang, "sync.open_local")).build();
        o1.add_prefix(&gtk::Image::from_icon_name("folder-open-symbolic"));
        let b1 = gtk::Button::builder()
            .label(t(lang, "sync.open"))
            .css_classes(["flat"])
            .build();
        let sync_dir = get_string(&config.borrow(), "sync_dir");
        let sync_dir2 = sync_dir.clone();
        b1.connect_clicked(move |_| open_url(&format!("file://{sync_dir2}")));
        o1.add_suffix(&b1);
        g6.add(&o1);

        let o2 = adw::ActionRow::builder()
            .title(t(lang, "sync.open_browser"))
            .build();
        o2.add_prefix(&gtk::Image::from_icon_name("globe-symbolic"));
        let b2 = gtk::Button::builder()
            .label(t(lang, "sync.open"))
            .css_classes(["flat"])
            .build();
        b2.connect_clicked(|_| open_url("https://drive.google.com"));
        o2.add_suffix(&b2);
        g6.add(&o2);

        let _ = sync_dir;

        page
    }

    /// Returns the root widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}