"""Página de Estado."""

import time

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk

from ..config import save_config
from ..i18n import t


STATUS_MAP = {
    "idle": ("status.idle", "status.detail.idle"),
    "syncing": ("status.syncing", "status.detail.syncing"),
    "paused": ("status.paused", "status.detail.paused"),
    "error": ("status.error", "status.detail.error"),
}


class StatusPage(Gtk.Box):
    def __init__(self, sync_mgr, config=None):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.mgr = sync_mgr
        self._config = config or {}

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        clamp = Adw.Clamp(hexpand=True, margin_top=20, margin_bottom=20,
                          margin_start=24, margin_end=24)
        scroll.set_child(clamp)

        col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
        clamp.set_child(col)

        from ..widgets import StatusBadge
        self._badge = StatusBadge()
        col.append(self._badge)

        self._title = Gtk.Label(halign=Gtk.Align.CENTER)
        col.append(self._title)

        self._detail = Gtk.Label(halign=Gtk.Align.CENTER, wrap=True,
                                 justify=Gtk.Justification.CENTER, width_chars=40)
        self._detail.add_css_class("dim-label")
        col.append(self._detail)

        self._progress = Gtk.ProgressBar(margin_top=6, margin_start=40, margin_end=40)
        self._progress.set_visible(False)
        col.append(self._progress)

        info = Adw.PreferencesGroup(title=t("status.info_group"))
        col.append(info)

        self._last = Adw.ActionRow(title=t("status.last_sync"),
                                   subtitle=t("status.last_sync_value"),
                                   icon_name="clock-pause-symbolic")
        info.add(self._last)

        self._files_row = Adw.ActionRow(title=t("status.files_synced"),
                                        subtitle="—",
                                        icon_name="check-circle-symbolic")
        info.add(self._files_row)

        self._errors_row = Adw.ActionRow(title=t("status.errors"),
                                         subtitle="0",
                                         icon_name="alert-circle-symbolic")
        info.add(self._errors_row)

        self._speed_row = Adw.ActionRow(title=t("status.speed"),
                                        subtitle="—",
                                        icon_name="globe-symbolic")
        info.add(self._speed_row)

        btns = Gtk.Box(spacing=12, halign=Gtk.Align.CENTER, margin_top=10)
        col.append(btns)

        self._sync_btn = Gtk.Button(label=t("status.sync_now"),
                                    css_classes=["suggested-action", "pill"])
        self._sync_btn.set_size_request(180, 40)
        self._sync_btn.connect("clicked", lambda _: self._do_sync())
        btns.append(self._sync_btn)

        self._pause_btn = Gtk.Button(label=t("status.pause"), css_classes=["pill"])
        self._pause_btn.set_size_request(120, 40)
        self._pause_btn.connect("clicked", lambda _: self._do_pause())
        btns.append(self._pause_btn)

        self._load_last_sync()
        self._apply(self.mgr.status)

    def _load_last_sync(self):
        """Carga el último sync desde config."""
        last = self._config.get("last_sync")
        if last:
            self._last.set_subtitle(last)

    def _do_sync(self):
        self.mgr.config["paused"] = False
        save_config(self.mgr.config)
        self.mgr.resume()
        self.mgr.start_sync()

    def _do_pause(self):
        if self.mgr.status == "syncing":
            return
        if self.mgr.config.get("paused"):
            self.mgr.resume()
        else:
            self.mgr.pause()

    def _apply(self, status):
        self._badge.set_status(status)
        title_key, detail_key = STATUS_MAP.get(status, ("status.idle", "status.detail.idle"))
        self._title.set_markup(
            f'<span size="x-large" weight="bold">{t(title_key)}</span>')
        self._detail.set_text(t(detail_key))
        self._pause_btn.set_label(
            t("status.resume") if self.mgr.config.get("paused") else t("status.pause"))
        self._sync_btn.set_sensitive(status != "syncing")
        self._progress.set_visible(status == "syncing")

    def on_status(self, s):
        self._apply(s)

    def on_progress(self, text, transferred=0, total=0):
        self._detail.set_text(text)
        if total and total > 0:
            self._progress.set_visible(True)
            frac = min(transferred / total, 1.0)
            self._progress.set_fraction(frac)
            self._progress.set_text(f"{transferred}/{total}")
            self._progress.set_show_text(True)
        else:
            self._progress.set_visible(False)

    def on_finished(self, ok, msg):
        now = time.strftime("%d/%m/%Y %H:%M")
        self._last.set_subtitle(now)
        self._speed_row.set_subtitle("—")

    def on_speed(self, speed):
        self._speed_row.set_subtitle(speed)
