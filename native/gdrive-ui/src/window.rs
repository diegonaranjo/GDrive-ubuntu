//! Main window shell: sidebar navigation + page container.
//!
//! Rust port of `gdrive/window.py`. The shell builds the sidebar (logo + six
//! navigation items), the page container and the toast overlay. The Status page
//! is implemented; the remaining pages are placeholders until the next phases.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use glib::object::Cast;
use gtk::{Box as GtkBox, Image, Label, Orientation, Separator};
use gtk4 as gtk;

use crate::dbus_client::DaemonClient;
use crate::i18n::{t, Lang};
use crate::status::StatusPage;

/// Sidebar item: (tag, i18n label key, icon name).
const NAV_ITEMS: [(&str, &str, &str); 6] = [
    ("status", "sidebar.status", "info-symbolic"),
    ("account", "sidebar.account", "user-symbolic"),
    ("sync", "sidebar.sync", "refresh-symbolic"),
    ("selective", "sidebar.selective", "folder-symbolic"),
    ("settings", "sidebar.settings", "settings-symbolic"),
    ("activity", "sidebar.activity", "clock-pause-symbolic"),
];

/// Sidebar button: icon + label, tracks the `active` navigation state.
struct SidebarButton {
    button: gtk::Button,
}

impl SidebarButton {
    /// Creates a sidebar navigation button.
    fn new(label: &str, icon_name: &str) -> Self {
        let button = gtk::Button::builder()
            .css_classes(["sidebar-item", "flat"])
            .halign(gtk::Align::Fill)
            .build();

        let content = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        button.set_child(Some(&content));

        let icon = Image::from_icon_name(icon_name);
        icon.set_pixel_size(20);
        content.append(&icon);

        let lbl = Label::builder()
            .label(label)
            .hexpand(true)
            .xalign(0.0)
            .build();
        content.append(&lbl);

        SidebarButton { button }
    }

    /// Marks this button as the active navigation item.
    fn set_active(&self, active: bool) {
        if active {
            self.button.add_css_class("active");
        } else {
            self.button.remove_css_class("active");
        }
    }
}

/// The main application window. Shared as `Rc<RefCell<…>>` so button callbacks
/// can drive navigation without self-referential borrows.
pub struct DriveWindow {
    window: adw::ApplicationWindow,
    content_box: GtkBox,
    buttons: HashMap<&'static str, SidebarButton>,
    pages: HashMap<&'static str, gtk::Widget>,
    current: RefCell<Option<&'static str>>,
    /// Toast overlay; `toast()` is wired to page events in a later phase.
    #[allow(dead_code)]
    toast_overlay: adw::ToastOverlay,
    /// Kept alive for the window's lifetime; holds the live signal
    /// subscriptions and drives the Status page. Exposed via `client()` for
    /// pages beyond Status (later phases).
    #[allow(dead_code)]
    client: DaemonClient,
}

impl DriveWindow {
    /// Builds the window shell, wires navigation, and returns a shared handle.
    ///
    /// `client` drives the real Status page; `lang` localizes the sidebar.
    pub fn new(app: &adw::Application, client: DaemonClient, lang: Lang) -> Rc<RefCell<Self>> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Google Drive")
            .default_width(980)
            .default_height(640)
            .resizable(true)
            .build();

        let toast_overlay = adw::ToastOverlay::new();
        window.set_content(Some(&toast_overlay));

        let toolbar = adw::ToolbarView::new();
        toast_overlay.set_child(Some(&toolbar));

        let header = adw::HeaderBar::new();
        toolbar.add_top_bar(&header);

        let main_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .build();
        toolbar.set_content(Some(&main_box));

        // ── Sidebar ─────────────────────────────────────────────────────────
        let sidebar = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .width_request(170)
            .hexpand(false)
            .css_classes(["sidebar"])
            .build();
        main_box.append(&sidebar);

        let logo_row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .margin_top(10)
            .margin_bottom(16)
            .margin_start(10)
            .margin_end(10)
            .build();
        sidebar.append(&logo_row);

        let logo = Image::from_icon_name("google-drive");
        logo.set_pixel_size(32);
        logo_row.append(&logo);

        let title = Label::builder().xalign(0.0).build();
        title.set_markup("<span size=\"large\" weight=\"bold\">Google Drive</span>");
        logo_row.append(&title);

        let sep = Separator::builder().margin_start(8).margin_end(8).build();
        sidebar.append(&sep);

        // ── Content area ────────────────────────────────────────────────────
        let mut buttons = HashMap::new();
        let mut pages: HashMap<&'static str, gtk::Widget> = HashMap::new();

        for (tag, label_key, icon) in NAV_ITEMS {
            let label = t(lang, label_key);
            let btn = SidebarButton::new(label, icon);
            sidebar.append(&btn.button);
            buttons.insert(tag, btn);

            // Real pages; Synchronization/Settings remain placeholders.
            let page: gtk::Widget = match tag {
                "status" => StatusPage::new(client.clone(), lang)
                    .widget()
                    .clone()
                    .upcast(),
                "account" => crate::account::AccountPage::new(lang)
                    .widget()
                    .clone()
                    .upcast(),
                "activity" => crate::activity::ActivityPage::new(lang)
                    .widget()
                    .clone()
                    .upcast(),
                "selective" => crate::selective::SelectiveSyncPage::new(lang)
                    .widget()
                    .clone()
                    .upcast(),
                _ => placeholder_page(label),
            };
            pages.insert(tag, page);
        }

        let vsep = Separator::builder()
            .orientation(Orientation::Vertical)
            .build();
        main_box.append(&vsep);

        let content_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        main_box.append(&content_box);

        let me = Rc::new(RefCell::new(DriveWindow {
            window,
            content_box,
            buttons,
            pages,
            current: RefCell::new(None),
            toast_overlay,
            client,
        }));

        // Wire navigation: each button switches to its tag.
        for (tag, btn) in &me.borrow().buttons {
            let tag = *tag;
            btn.button.connect_clicked(glib::clone!(
                #[weak]
                me,
                move |_| {
                    me.borrow_mut().switch_page(tag);
                }
            ));
        }

        me.borrow_mut().switch_page("status");
        me
    }

    /// Presents the window.
    pub fn present(&self) {
        self.window.present();
    }

    /// Switches the visible page by tag.
    pub fn switch_page(&mut self, tag: &'static str) {
        if *self.current.borrow() == Some(tag) {
            return;
        }
        if let Some(cur) = *self.current.borrow() {
            if let Some(btn) = self.buttons.get(cur) {
                btn.set_active(false);
            }
        }
        *self.current.borrow_mut() = Some(tag);
        if let Some(btn) = self.buttons.get(tag) {
            btn.set_active(true);
        }

        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }
        if let Some(page) = self.pages.get(tag) {
            self.content_box.append(page);
        }
    }

    /// Returns a clone of the daemon client (for pages that need extra calls).
    #[allow(dead_code)]
    pub fn client(&self) -> DaemonClient {
        self.client.clone()
    }

    /// Shows a toast notification.
    #[allow(dead_code)]
    pub fn toast(&self, msg: &str) {
        let t = adw::Toast::builder().title(msg).timeout(2).build();
        self.toast_overlay.add_toast(t);
    }
}

/// Creates a placeholder page with a heading label.
fn placeholder_page(label: &str) -> gtk::Widget {
    let page = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .hexpand(true)
        .vexpand(true)
        .build();
    let heading = Label::builder().label(label).build();
    heading.add_css_class("title-1");
    page.append(&heading);

    let hint = Label::builder()
        .label("Page implementation pending")
        .build();
    hint.add_css_class("dim-label");
    page.append(&hint);

    page.upcast()
}
