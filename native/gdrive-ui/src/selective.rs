//! Selective sync page — choose which remote folders to sync.
//!
//! Rust port of `gdrive/pages/selective.py`. Lists the remote's top-level
//! directories (`rclone lsd`) with a switch per folder; toggling persists the
//! selection to `settings.json` (`selected_folders`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::config::{load_config, save_config, selected_folders, set_selected_folders};
use crate::helpers::{rclone_lsd, RCLONE_REMOTE};
use crate::i18n::{t, Lang};

/// The Selective sync page widget.
pub struct SelectiveSyncPage {
    root: gtk::Box,
    list: gtk::ListBox,
}

impl SelectiveSyncPage {
    /// Builds the page and triggers an initial folder listing.
    pub fn new(lang: Lang) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();

        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .margin_top(20)
            .margin_bottom(10)
            .margin_start(24)
            .margin_end(24)
            .spacing(8)
            .build();
        root.append(&header);

        let title = gtk::Label::builder().xalign(0.0).hexpand(true).build();
        title.set_markup(&format!(
            "<span size=\"xx-large\" weight=\"bold\">{}</span>",
            t(lang, "selective.title")
        ));
        header.append(&title);

        let refresh_btn = gtk::Button::builder()
            .label(t(lang, "selective.refresh"))
            .css_classes(["flat"])
            .build();
        header.append(&refresh_btn);

        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        root.append(&scroll);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        scroll.set_child(Some(&list));

        // Load the current selection from config.
        let cfg = load_config();
        let mut selected = HashMap::new();
        for f in selected_folders(&cfg) {
            selected.insert(f, true);
        }
        let selected = Rc::new(RefCell::new(selected));

        let page = SelectiveSyncPage { root, list };

        // Refresh button relists folders.
        {
            let page_list = page.list.clone();
            let page_selected = selected.clone();
            refresh_btn.connect_clicked(move |_| {
                trigger_listing(page_list.clone(), page_selected.clone(), lang);
            });
        }

        trigger_listing(page.list.clone(), selected, lang);

        page
    }

    /// Returns the root widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

/// Persists the current `selected` map to `settings.json`.
fn persist_selection(selected: &HashMap<String, bool>) {
    let mut cfg = load_config();
    let folders: Vec<String> = selected
        .iter()
        .filter(|(_, on)| **on)
        .map(|(name, _)| name.clone())
        .collect();
    set_selected_folders(&mut cfg, &folders);
    save_config(&cfg);
}

/// Lists remote folders off-thread and applies them on the main loop.
///
/// GTK widgets and `Rc` are not `Send`, so the thread only computes the plain
/// `Vec<String>` of folders and ships it back through an `mpsc` channel; a
/// `glib::idle_add_local` source drains it and builds the rows on the main loop.
fn trigger_listing(list: gtk::ListBox, selected: Rc<RefCell<HashMap<String, bool>>>, lang: Lang) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let dirs = rclone_lsd(RCLONE_REMOTE);
        let _ = tx.send(dirs);
    });

    glib::idle_add_local(move || {
        let dirs = match rx.try_recv() {
            Ok(dirs) => dirs,
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
        };

        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        if dirs.is_empty() {
            list.append(&no_folders_row(lang));
            return glib::ControlFlow::Break;
        }
        for dir in dirs {
            let on = selected.borrow().get(&dir).copied().unwrap_or(false);
            let row = adw::ActionRow::builder().title(&dir).build();
            row.add_prefix(&gtk::Image::from_icon_name("folder-symbolic"));
            let sw = gtk::Switch::builder()
                .active(on)
                .valign(gtk::Align::Center)
                .build();
            let selected = selected.clone();
            let dir_clone = dir.clone();
            sw.connect_active_notify(move |switch| {
                let active = switch.is_active();
                selected.borrow_mut().insert(dir_clone.clone(), active);
                persist_selection(&selected.borrow());
            });
            row.add_suffix(&sw);
            list.append(&row);
        }
        glib::ControlFlow::Break
    });
}

/// Returns the row shown when the remote has no folders.
fn no_folders_row(lang: Lang) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(t(lang, "selective.no_folders"))
        .build()
}
