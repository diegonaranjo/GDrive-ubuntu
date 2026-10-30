"""Sync Settings page."""

import subprocess

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk

from ..config import save_config
from ..constants import SYNC_DIR_DEFAULT
from ..i18n import t

# Modes the engine implements today. `stream` (on-demand access) needs a
# `rclone mount` backend that does not exist: offering it meant promising a
# full download under another name.
MODES = ("mirror", "selective")


class SyncSettingsPage(Gtk.Box):
    def __init__(self, config, sync_mgr, on_toast=None):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.config = config
        self.mgr = sync_mgr
        self._on_toast = on_toast

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        clamp = Adw.Clamp(hexpand=True, margin_top=20, margin_bottom=20,
                          margin_start=24, margin_end=24)
        scroll.set_child(clamp)

        col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
        clamp.set_child(col)

        title = Gtk.Label(xalign=0)
        title.set_markup(f'<span size="xx-large" weight="bold">{t("sync.title")}</span>')
        col.append(title)

        g1 = Adw.PreferencesGroup(title=t("sync.location"))
        col.append(g1)

        self._folder_row = Adw.ActionRow(
            title=t("sync.sync_folder"),
            subtitle=config.get("sync_dir", SYNC_DIR_DEFAULT),
            icon_name="folder-symbolic")
        b = Gtk.Button(label=t("sync.change"), css_classes=["flat"])
        b.connect("clicked", lambda _: self._pick_folder())
        self._folder_row.add_suffix(b)
        g1.add(self._folder_row)

        g2 = Adw.PreferencesGroup(title=t("sync.mode"))
        col.append(g2)

        self._mode = Adw.ComboRow(
            title=t("sync.file_mode"),
            subtitle=t("sync.file_mode_subtitle"),
            model=Gtk.StringList.new([
                t("sync.mode_mirror"),
                t("sync.mode_selective"),
            ]),
        )
        guardado = config.get("sync_mode", "mirror")
        # A settings.json with "stream" (a valid value before) must not break the
        # page: it falls back to the mode the engine is actually going to run.
        self._mode.set_selected(MODES.index(guardado) if guardado in MODES else 0)
        self._mode.connect("notify::selected", self._on_mode)
        g2.add(self._mode)

        g3 = Adw.PreferencesGroup(title=t("sync.scheduling"))
        col.append(g3)

        self._interval = Adw.ComboRow(
            title=t("sync.interval"),
            subtitle=t("sync.interval_subtitle"),
            model=Gtk.StringList.new([
                t("sync.interval_manual"),
                t("sync.interval_5min"),
                t("sync.interval_15min"),
                t("sync.interval_30min"),
                t("sync.interval_1hr"),
            ]),
        )
        iv = ["manual", "5min", "15min", "30min", "1hr"]
        self._interval.set_selected(iv.index(config.get("sync_interval", "manual")))
        self._interval.connect("notify::selected", self._on_interval)
        g3.add(self._interval)

        self._pause_sw = Adw.SwitchRow(
            title=t("sync.pause_sync"),
            subtitle=t("sync.pause_subtitle"),
            active=config.get("paused", False),
        )
        self._pause_sw.connect("notify::active", self._on_pause)
        g3.add(self._pause_sw)

        self._watch_sw = Adw.SwitchRow(
            title=t("sync.watch_changes"),
            subtitle=t("sync.watch_subtitle"),
            active=config.get("watch_changes", False),
        )
        self._watch_sw.connect("notify::active", self._on_watch)
        g3.add(self._watch_sw)

        g4 = Adw.PreferencesGroup(title=t("sync.bandwidth"))
        col.append(g4)

        bw = Adw.ActionRow(title=t("sync.global_limit"), subtitle=t("sync.no_limit"))
        self._bw = Gtk.Entry(placeholder_text=t("sync.placeholder_bw"),
                             text=config.get("bandwidth_limit", ""),
                             hexpand=True, valign=Gtk.Align.CENTER)
        self._bw.connect("changed",
                         lambda e: self._set("bandwidth_limit", e.get_text()))
        bw.add_suffix(self._bw)
        g4.add(bw)

        g5 = Adw.PreferencesGroup(title=t("sync.advanced"))
        col.append(g5)

        sw1 = Adw.SwitchRow(title=t("sync.skip_hidden"), active=config.get("skip_hidden", True))
        sw1.connect("notify::active", lambda r, _: self._set("skip_hidden", r.get_active()))
        g5.add(sw1)

        sw2 = Adw.SwitchRow(title=t("sync.store_metadata"),
                            active=config.get("store_metadata", True))
        sw2.connect("notify::active",
                    lambda r, _: self._set("store_metadata", r.get_active()))
        g5.add(sw2)

        th = Adw.ActionRow(title=t("sync.transfer_threads"))
        sp = Gtk.SpinButton.new_with_range(1, 16, 1)
        sp.set_value(config.get("num_threads", 4))
        sp.connect("value-changed",
                   lambda s: self._set("num_threads", int(s.get_value())))
        th.add_suffix(sp)
        g5.add(th)

        g6 = Adw.PreferencesGroup(title=t("sync.shortcuts"))
        col.append(g6)

        o1 = Adw.ActionRow(title=t("sync.open_local"), icon_name="folder-open-symbolic")
        b1 = Gtk.Button(label=t("sync.open"), css_classes=["flat"])
        b1.connect("clicked", lambda _: subprocess.Popen(
            ["xdg-open", self.config.get("sync_dir", SYNC_DIR_DEFAULT)]))
        o1.add_suffix(b1)
        g6.add(o1)

        o2 = Adw.ActionRow(title=t("sync.open_browser"), icon_name="globe-symbolic")
        b2 = Gtk.Button(label=t("sync.open"), css_classes=["flat"])
        b2.connect("clicked", lambda _: subprocess.Popen(
            ["xdg-open", "https://drive.google.com"]))
        o2.add_suffix(b2)
        g6.add(o2)

    def _pick_folder(self):
        d = Gtk.FileDialog(title=t("sync.select_folder"))
        d.select_folder(self.get_root(), None, self._on_picked)

    def _on_picked(self, d, result):
        try:
            f = d.select_folder_finish(result)
            if f:
                p = f.get_path()
                self.config["sync_dir"] = p
                save_config(self.config)
                self._folder_row.set_subtitle(p)
                self.mgr.config = self.config
        except GLib.Error:
            pass

    def _on_mode(self, row, _):
        self.config["sync_mode"] = MODES[row.get_selected()]
        save_config(self.config)

    def _on_interval(self, row, _):
        iv = ["manual", "5min", "15min", "30min", "1hr"]
        self.config["sync_interval"] = iv[row.get_selected()]
        save_config(self.config)
        self.mgr.start_periodic()

    def _on_pause(self, row, _):
        if row.get_active():
            self.mgr.pause()
        else:
            self.mgr.resume()

    def _on_watch(self, row, _):
        self.config["watch_changes"] = row.get_active()
        save_config(self.config)
        # Push the config to the daemon so it (re)starts the watcher.
        if hasattr(self.mgr, "_push_interval"):
            self.mgr._push_interval()

    def _set(self, key, val, toast=True):
        old = self.config.get(key)
        self.config[key] = val
        save_config(self.config)
        if toast and old != val and hasattr(self, "_on_toast") and self._on_toast:
            self._on_toast(t("sync.config_saved"))
