"""Main window."""

from .config import save_config
from .i18n import t
from .pages.status import StatusPage
from .pages.account import AccountPage
from .pages.sync_settings import SyncSettingsPage
from .pages.selective import SelectiveSyncPage
from .pages.settings import SettingsPage
from .pages.activity import ActivityPage
from .widgets import SidebarButton

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("GLib", "2.0")
from gi.repository import Adw, GLib, Gtk


class DriveWindow(Adw.ApplicationWindow):
    def __init__(self, app, config, sync_mgr):
        super().__init__(application=app, title="Google Drive",
                         default_width=config["window_width"],
                         default_height=config["window_height"], resizable=True)

        self._toast_overlay = Adw.ToastOverlay()
        self.set_content(self._toast_overlay)

        toolbar = Adw.ToolbarView()
        self._toast_overlay.set_child(toolbar)

        header = Adw.HeaderBar()
        toolbar.add_top_bar(header)

        main_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        toolbar.set_content(main_box)

        sidebar_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        sidebar_box.set_size_request(170, -1)
        sidebar_box.set_hexpand(False)
        sidebar_box.add_css_class("sidebar")
        main_box.append(sidebar_box)

        logo_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12,
                           margin_top=10, margin_bottom=16, margin_start=10,
                           margin_end=10)
        sidebar_box.append(logo_box)

        logo = Gtk.Image.new_from_icon_name("google-drive")
        logo.set_pixel_size(32)
        logo_box.append(logo)

        lbl = Gtk.Label(xalign=0)
        lbl.set_markup('<span size="large" weight="bold">Google Drive</span>')
        logo_box.append(lbl)

        sep = Gtk.Separator(margin_start=8, margin_end=8)
        sidebar_box.append(sep)

        self._pages = {}
        self._buttons = {}
        self._current = None

        nav_items = [
            ("status", t("sidebar.status"), "info-symbolic"),
            ("account", t("sidebar.account"), "user-symbolic"),
            ("sync", t("sidebar.sync"), "refresh-symbolic"),
            ("selective", t("sidebar.selective"), "folder-symbolic"),
            ("settings", t("sidebar.settings"), "settings-symbolic"),
            ("activity", t("sidebar.activity"), "clock-pause-symbolic"),
        ]

        self._status_pg = StatusPage(sync_mgr, config)
        self._account_pg = AccountPage()
        self._sync_pg = SyncSettingsPage(config, sync_mgr, on_toast=self._toast)
        self._selective_pg = SelectiveSyncPage(config)
        self._settings_pg = SettingsPage(
            config, on_toast=self._toast, on_config_reset=self._reload_after_reset)
        self._activity_pg = ActivityPage()

        pages_map = {
            "status": self._status_pg,
            "account": self._account_pg,
            "sync": self._sync_pg,
            "selective": self._selective_pg,
            "settings": self._settings_pg,
            "activity": self._activity_pg,
        }

        for tag, label, icon in nav_items:
            btn = SidebarButton(label, icon, tag)
            btn.connect("clicked", lambda b, t=tag: self._switch(t))
            sidebar_box.append(btn)
            self._buttons[tag] = btn
            self._pages[tag] = pages_map[tag]

        sep = Gtk.Separator(orientation=Gtk.Orientation.VERTICAL)
        main_box.append(sep)

        self._content_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, hexpand=True)
        main_box.append(self._content_box)

        self._switch("status")

        sync_mgr.connect("status-changed", lambda _, s: self._status_pg.on_status(s))
        sync_mgr.connect("progress", lambda _, t, tr, to: self._status_pg.on_progress(t, tr, to))
        sync_mgr.connect("speed-changed", lambda _, sp: self._status_pg.on_speed(sp))
        sync_mgr.connect("finished", self._on_sync_finished)

        sync_mgr.connect("status-changed", lambda _, s: self._update_sidebar_badge(s))
        self._update_sidebar_badge(sync_mgr.status)

        self.connect("close-request", self._on_close)
        self.config = config

        self._auth_checked = False
        import threading
        threading.Thread(target=self._check_auth_async, daemon=True).start()

    def _check_auth_async(self):
        if self._auth_checked:
            return
        self._auth_checked = True
        from .helpers import rclone_is_authorized
        import logging
        logging.getLogger(__name__).debug("Checking rclone auth...")
        ok = rclone_is_authorized()
        logging.getLogger(__name__).debug(f"Auth check result: {ok}")
        if not ok:
            GLib.idle_add(self._show_signin_dialog)

    def _show_signin_dialog(self):
        from .widgets import ConfirmDialog
        dlg = ConfirmDialog(
            self,
            t("signin.title"),
            t("signin.body"),
            confirm_label=t("signin.confirm"),
            destructive=False,
        )
        dlg.connect("response", self._on_signin_response)
        dlg.present()

    def _on_signin_response(self, dlg, response):
        if response != "confirm":
            return
        self._toast(t("signin.opening_browser"))
        import threading
        threading.Thread(target=self._do_signin, daemon=True).start()

    def _do_signin(self):
        from .helpers import rclone_sign_in
        try:
            ok = rclone_sign_in()
        except Exception as e:
            ok = False
            print(f"[signin] error: {e}")
        title = t("signin.success") if ok else t("signin.failed")
        GLib.idle_add(lambda: self._toast(title))

    def _on_sync_finished(self, mgr, ok, msg):
        self._status_pg.on_finished(ok, msg)
        self._activity_pg.log("Sync" if ok else "Error", msg)
        title = t("sync.finished_ok") if ok else t("sync.finished_error")
        if self.config.get("notifications_enabled", True):
            from .helpers import send_notification
            send_notification(self.get_application(), title, msg)
        self._toast(title)

    def _toast(self, msg):
        toast = Adw.Toast(title=msg, timeout=2)
        self._toast_overlay.add_toast(toast)

    def _update_sidebar_badge(self, status):
        colors = {"idle": "#1E8E3E", "syncing": "#1A73E8",
                  "paused": "#F9AB00", "error": "#D93025"}
        css_classes = {"idle": "idle", "syncing": "syncing",
                       "paused": "paused", "error": "error"}
        btn = self._buttons.get("status")
        if not btn:
            return
        dot = btn._badge
        for c in ("idle", "syncing", "paused", "error"):
            dot.remove_css_class(c)
        dot.add_css_class(css_classes.get(status, "idle"))
        btn.set_badge_color(colors.get(status, "#1E8E3E"))

    def _switch(self, tag):
        if self._current == tag:
            return
        if self._current and self._current in self._buttons:
            self._buttons[self._current].remove_css_class("active")
        self._current = tag
        self._buttons[tag].add_css_class("active")

        while child := self._content_box.get_first_child():
            self._content_box.remove(child)
        self._content_box.append(self._pages[tag])

    def _reload_after_reset(self):
        self._switch("settings")
        self._toast(t("settings.reload_hint"))

    def _on_close(self, *a):
        size = self.get_default_size()
        self.config["window_width"] = size[0]
        self.config["window_height"] = size[1]
        save_config(self.config)
        self.hide()
        return True
