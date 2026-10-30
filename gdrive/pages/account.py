"""Account page."""

import subprocess
import threading

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk

from ..helpers import rclone_about, rclone_user_email, format_size
from ..i18n import t


class AccountPage(Gtk.Box):
    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        clamp = Adw.Clamp(hexpand=True, margin_top=20, margin_bottom=20,
                          margin_start=24, margin_end=24)
        scroll.set_child(clamp)

        col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
        clamp.set_child(col)

        title = Gtk.Label(xalign=0)
        title.set_markup(f'<span size="xx-large" weight="bold">{t("account.title")}</span>')
        col.append(title)

        self._email = Gtk.Label(xalign=0)
        self._email.set_markup(f'<span size="large" weight="bold">{t("account.not_signed_in")}</span>')
        self._email.add_css_class("dim-label")
        col.append(self._email)

        grp = Adw.PreferencesGroup(title=t("account.storage"))
        col.append(grp)

        self._bar = Gtk.ProgressBar(show_text=True, height_request=32)
        self._bar.add_css_class("osd")
        self._bar.add_css_class("storage-bar")
        grp.add(self._bar)

        self._used = Gtk.Label(xalign=0)
        grp.add(self._used)

        self._detail = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self._detail.add_css_class("dim-label")
        grp.add(self._detail)

        g2 = Adw.PreferencesGroup(title=t("account.actions"))
        col.append(g2)

        r1 = Adw.ActionRow(title=t("account.open_drive"), icon_name="globe-symbolic")
        b1 = Gtk.Button(label=t("account.open"), css_classes=["flat"])
        b1.connect("clicked", lambda _: subprocess.Popen(["xdg-open", "https://drive.google.com"]))
        r1.add_suffix(b1)
        g2.add(r1)

        r2 = Adw.ActionRow(title=t("account.manage_account"), icon_name="user-symbolic")
        b2 = Gtk.Button(label=t("account.open"), css_classes=["flat"])
        b2.connect("clicked", lambda _: subprocess.Popen(
            ["xdg-open", "https://myaccount.google.com"]))
        r2.add_suffix(b2)
        g2.add(r2)

        btn = Gtk.Button(label=t("account.refresh_info"), css_classes=["flat", "pill"])
        btn.set_halign(Gtk.Align.START)
        btn.connect("clicked", lambda _: self.refresh())
        g2.add(btn)

        GLib.idle_add(self.refresh)

    def refresh(self):
        threading.Thread(target=self._load, daemon=True).start()

    def _load(self):
        about = rclone_about()
        email = rclone_user_email()
        GLib.idle_add(self._apply, about, email)

    def _apply(self, about, email=None):
        if email:
            self._email.set_markup(
                f'<span size="large" weight="bold">{email}</span>')
        elif about:
            self._email.set_markup(
                f'<span size="large" weight="bold">{t("account.google_drive_account")}</span>')
        if not about:
            self._bar.set_fraction(0)
            self._bar.set_text("—")
            self._used.set_text(t("account.no_storage_info"))
            return
        used = total = 0
        for tt in about.get("totals", []):
            used += tt.get("used", 0)
            total += tt.get("total", 0)
        if total > 0:
            f = used / total
            self._bar.set_fraction(f)
            self._bar.set_text(f"{f * 100:.1f}%")
            self._used.set_markup(
                f'<span size="large">{t("account.storage_used", used=format_size(used), total=format_size(total))}</span>')
        else:
            self._bar.set_fraction(0)
            self._bar.set_text("∞")
            self._used.set_text(t("account.no_limit"))
