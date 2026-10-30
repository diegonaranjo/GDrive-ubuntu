"""Activity page."""

import time

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk

from ..config import CONFIG_DIR
from ..i18n import t


class ActivityPage(Gtk.Box):
    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)

        header = Gtk.Box(margin_top=20, margin_bottom=10, margin_start=24,
                         margin_end=24, spacing=8)
        self.append(header)

        tt = Gtk.Label(xalign=0, hexpand=True)
        tt.set_markup(f'<span size="xx-large" weight="bold">{t("activity.title")}</span>')
        header.append(tt)

        clr = Gtk.Button(label=t("activity.clear"), css_classes=["flat"])
        clr.connect("clicked", lambda _: self._clear())
        header.append(clr)

        self._search = Gtk.SearchBar()
        self._search_entry = Gtk.SearchEntry()
        self._search.connect_entry(self._search_entry)
        self._search_entry.set_placeholder_text(t("activity.search_placeholder"))
        self._search.set_child(self._search_entry)
        self.append(self._search)
        self._search.set_search_mode(False)

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        self._list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE,
                                css_classes=["boxed-list"])
        self._list.set_filter_func(self._filter_func)
        self._search_entry.connect("search-changed",
                                   lambda _: self._list.invalidate_filter())
        scroll.set_child(self._list)
        self._load()

    def _filter_func(self, row):
        text = self._search_entry.get_text().strip().lower()
        if not text:
            return True
        title = row.get_title() or ""
        sub = row.get_subtitle() or ""
        return text in title.lower() or text in sub.lower()

    def _load(self):
        log = CONFIG_DIR / "activity.log"
        if not log.exists():
            self._list.append(Adw.ActionRow(title=t("activity.no_activity")))
            return
        for line in reversed(log.read_text().strip().splitlines()[-100:]):
            parts = line.split(" | ", 2)
            if len(parts) >= 2:
                self._list.append(
                    Adw.ActionRow(title=parts[1], subtitle=parts[0]))

    def _clear(self):
        (CONFIG_DIR / "activity.log").unlink(missing_ok=True)
        while c := self._list.get_first_child():
            self._list.remove(c)
        self._list.append(Adw.ActionRow(title=t("activity.no_activity")))

    def log(self, action, detail=""):
        now = time.strftime("%d/%m/%Y %H:%M:%S")
        logf = CONFIG_DIR / "activity.log"
        with open(logf, "a") as f:
            f.write(f"{now} | {action} | {detail}\n")
