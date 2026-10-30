//! Activity page — shows the recent activity log with search and clear.
//!
//! Rust port of `gdrive/pages/activity.py`. Reads `~/.config/gdrive/activity.log`
//! (plain `timestamp | component | detail` lines) and renders the last 100
//! entries, newest first.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::i18n::{t, Lang};

/// Returns the GDrive config directory (`~/.config/gdrive`).
fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is not set");
    PathBuf::from(home).join(".config").join("gdrive")
}

/// The Activity page widget.
pub struct ActivityPage {
    root: gtk::Box,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    /// Filter text cache (lowercased) shared with the list filter.
    filter: Rc<RefCell<String>>,
    lang: Lang,
}

impl ActivityPage {
    /// Builds the page and loads the activity log.
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
            t(lang, "activity.title")
        ));
        header.append(&title);

        let clear_btn = gtk::Button::builder()
            .label(t(lang, "activity.clear"))
            .css_classes(["flat"])
            .build();
        header.append(&clear_btn);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(t(lang, "activity.search_placeholder")));
        root.append(&search);

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

        let filter = Rc::new(RefCell::new(String::new()));

        let page = ActivityPage {
            root,
            search,
            list,
            filter,
            lang,
        };

        // Filter: repopulates nothing; each row is filtered via set_filter_func.
        {
            let list = page.list.clone();
            let filter = page.filter.clone();
            list.set_filter_func(move |row| {
                let text = filter.borrow().clone();
                if text.is_empty() {
                    return true;
                }
                let hay = search_text(row);
                hay.contains(&text)
            });
        }

        // Search text updates the filter cache and invalidates the filter.
        {
            let page_search = page.search.clone();
            let page_filter = page.filter.clone();
            let page_list = page.list.clone();
            page_search.connect_search_changed(move |entry| {
                *page_filter.borrow_mut() = entry.text().to_lowercase();
                page_list.invalidate_filter();
            });
        }

        // Clear button removes the log file and empties the list.
        {
            let page_list = page.list.clone();
            let lang = page.lang;
            clear_btn.connect_clicked(move |_| {
                let log = config_dir().join("activity.log");
                let _ = fs::remove_file(&log);
                while let Some(child) = page_list.first_child() {
                    page_list.remove(&child);
                }
                page_list.append(&no_activity_row(lang));
            });
        }

        page.load();

        page
    }

    /// Returns the root widget (owned by the caller's layout).
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Loads the activity log (last 100 lines, newest first).
    fn load(&self) {
        let log = config_dir().join("activity.log");
        let Ok(content) = fs::read_to_string(&log) else {
            self.list.append(&no_activity_row(self.lang));
            return;
        };
        let lines: Vec<&str> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(100)
            .collect();
        if lines.is_empty() {
            self.list.append(&no_activity_row(self.lang));
            return;
        }
        for line in lines {
            if let Some(row) = parse_activity_line(line) {
                self.list.append(&row);
            }
        }
    }
}

/// Builds an `ActionRow` from an activity log line (`timestamp | component | detail`).
fn parse_activity_line(line: &str) -> Option<adw::ActionRow> {
    let mut parts = line.splitn(3, " | ");
    let _timestamp = parts.next()?;
    let component = parts.next()?;
    let detail = parts.next().unwrap_or("");
    Some(
        adw::ActionRow::builder()
            .title(component)
            .subtitle(detail)
            .build(),
    )
}

/// Concatenates a row's title and subtitle for case-insensitive search.
fn search_text(row: &gtk::ListBoxRow) -> String {
    let mut hay = String::new();
    if let Some(child) = row.child() {
        if let Ok(action) = child.downcast::<adw::ActionRow>() {
            hay.push_str(&action.title().to_lowercase());
            hay.push(' ');
            hay.push_str(&action.subtitle().unwrap_or_default().to_lowercase());
        }
    }
    hay
}

/// Returns an `ActionRow` shown when there is no activity.
fn no_activity_row(lang: Lang) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(t(lang, "activity.no_activity"))
        .build()
}
