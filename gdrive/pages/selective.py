"""Folders page (selective sync)."""

import threading

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk

from ..config import save_config
from ..constants import RCLONE_REMOTE
from ..helpers import rclone_lsd
from ..i18n import t


class SelectiveSyncPage(Gtk.Box):
    def __init__(self, config):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.config = config
        self._selected = set(config.get("selected_folders", []))

        header = Gtk.Box(margin_top=20, margin_bottom=10, margin_start=24,
                         margin_end=24, spacing=8)
        self.append(header)

        tt = Gtk.Label(xalign=0, hexpand=True)
        tt.set_markup(f'<span size="xx-large" weight="bold">{t("selective.title")}</span>')
        header.append(tt)

        rb = Gtk.Button(label=t("selective.refresh"), css_classes=["flat"])
        rb.connect("clicked", lambda _: self.refresh())
        header.append(rb)

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        self._list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE,
                                css_classes=["boxed-list"])
        scroll.set_child(self._list)

        GLib.idle_add(self.refresh)

    def refresh(self):
        threading.Thread(target=self._load, daemon=True).start()

    def _load(self):
        dirs = rclone_lsd(RCLONE_REMOTE)
        GLib.idle_add(self._apply, dirs)

    def _apply(self, dirs):
        while c := self._list.get_first_child():
            self._list.remove(c)
        if not dirs:
            self._list.append(Adw.ActionRow(title=t("selective.no_folders")))
            return
        for d in dirs:
            row = Adw.ActionRow(title=d, icon_name="folder-symbolic")
            sw = Gtk.Switch(active=(d in self._selected), valign=Gtk.Align.CENTER)
            sw.connect("notify::active",
                       lambda s, _, n=d: self._toggle(n, s.get_active()))
            row.add_suffix(sw)
            self._list.append(row)

    def _toggle(self, folder, on):
        if on:
            self._selected.add(folder)
        else:
            self._selected.discard(folder)
        self.config["selected_folders"] = list(self._selected)
        save_config(self.config)
