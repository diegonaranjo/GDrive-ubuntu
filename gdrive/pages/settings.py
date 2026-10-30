"""General settings page."""

import subprocess

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk

from ..config import save_config, CONFIG_FILE, DEFAULTS
from ..constants import APP_VERSION
from ..helpers import toggle_autostart, LOG_DIR
from ..i18n import t, set_language, available_languages


class SettingsPage(Gtk.Box):
    def __init__(self, config, on_toast=None, on_config_reset=None):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.config = config
        self._on_toast = on_toast
        self._on_config_reset = on_config_reset

        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        self.append(scroll)

        clamp = Adw.Clamp(hexpand=True, margin_top=20, margin_bottom=20,
                          margin_start=24, margin_end=24)
        scroll.set_child(clamp)

        col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
        clamp.set_child(col)

        title = Gtk.Label(xalign=0)
        title.set_markup(f'<span size="xx-large" weight="bold">{t("settings.title")}</span>')
        col.append(title)

        # ── General ──
        g1 = Adw.PreferencesGroup(title=t("settings.general"))
        col.append(g1)

        sw_notif = Adw.SwitchRow(title=t("settings.notifications"),
                                 active=config.get("notifications_enabled", True))
        sw_notif.connect("notify::active",
                         lambda r, _: self._set("notifications_enabled", r.get_active()))
        g1.add(sw_notif)

        sw_startup = Adw.SwitchRow(title=t("settings.startup"),
                                   active=config.get("launch_on_startup", True))
        sw_startup.connect("notify::active", self._on_startup)
        g1.add(sw_startup)

        sw_hidden = Adw.SwitchRow(title=t("settings.start_hidden"),
                                  active=config.get("start_hidden", True))
        sw_hidden.connect("notify::active",
                          lambda r, _: self._set("start_hidden", r.get_active()))
        g1.add(sw_hidden)

        sw_nautilus = Adw.SwitchRow(title=t("settings.nautilus"),
                                    active=config.get("show_in_file_manager", True))
        sw_nautilus.connect("notify::active",
                            lambda r, _: self._set("show_in_file_manager", r.get_active()))
        g1.add(sw_nautilus)

        # ── Language ──
        g_lang = Adw.PreferencesGroup(title=t("settings.language"))
        col.append(g_lang)

        langs = available_languages()
        lang_names = [name for _, name in langs]
        lang_codes = [code for code, _ in langs]

        lang_row = Adw.ComboRow(
            title=t("settings.language"),
            subtitle=t("settings.language_subtitle"),
            model=Gtk.StringList.new(lang_names),
        )
        current_lang = config.get("language", "es")
        lang_row.set_selected(lang_codes.index(current_lang) if current_lang in lang_codes else 0)
        lang_row.connect("notify::selected", self._on_language)
        g_lang.add(lang_row)

        # ── Network ──
        g2 = Adw.PreferencesGroup(title=t("settings.network"))
        col.append(g2)

        sw_proxy = Adw.SwitchRow(title=t("settings.use_proxy"),
                                 active=config.get("proxy_enabled", False))
        sw_proxy.connect("notify::active",
                         lambda r, _: self._set("proxy_enabled", r.get_active()))
        g2.add(sw_proxy)

        proxy = Adw.ActionRow(title=t("settings.proxy_url"))
        entry = Gtk.Entry(text=config.get("proxy_url", ""),
                          placeholder_text="socks5://127.0.0.1:1080",
                          hexpand=True, valign=Gtk.Align.CENTER)
        entry.connect("changed", lambda e: self._set("proxy_url", e.get_text()))
        proxy.add_suffix(entry)
        g2.add(proxy)

        # ── Cache ──
        g3 = Adw.PreferencesGroup(title=t("settings.cache"))
        col.append(g3)

        clear = Gtk.Button(label=t("settings.clear_cache"), css_classes=["flat", "pill"])
        clear.set_halign(Gtk.Align.START)
        clear.connect("clicked", lambda _: self._clear_cache())
        g3.add(clear)

        # ── Security ──
        g4 = Adw.PreferencesGroup(title=t("settings.security"))
        col.append(g4)

        sw_enc = Adw.SwitchRow(title=t("settings.encryption"),
                               active=config.get("encryption_enabled", False))
        sw_enc.connect("notify::active",
                       lambda r, _: self._set("encryption_enabled", r.get_active()))
        g4.add(sw_enc)

        # ── Logs ──
        g5 = Adw.PreferencesGroup(title=t("settings.logs"))
        col.append(g5)

        sw_log = Adw.SwitchRow(title=t("settings.enable_logs"),
                               subtitle=t("settings.enable_logs_subtitle"),
                               active=config.get("logging_enabled", True))
        sw_log.connect("notify::active",
                       lambda r, _: self._set("logging_enabled", r.get_active()))
        g5.add(sw_log)

        log_lvl = Adw.ComboRow(
            title=t("settings.log_level"),
            subtitle=t("settings.log_level_subtitle"),
            model=Gtk.StringList.new([
                t("settings.log_level_error"), t("settings.log_level_info"),
                t("settings.log_level_debug"), t("settings.log_level_trace"),
            ]),
        )
        levels = ["error", "info", "debug", "trace"]
        log_lvl.set_selected(levels.index(config.get("log_level", "info")))
        log_lvl.connect("notify::selected",
                        lambda r, _: self._set("log_level", levels[r.get_selected()]))
        g5.add(log_lvl)

        sw_rot = Adw.SwitchRow(title=t("settings.log_rotate"),
                               subtitle=t("settings.log_rotate_subtitle"),
                               active=config.get("log_rotate", True))
        sw_rot.connect("notify::active",
                       lambda r, _: self._set("log_rotate", r.get_active()))
        g5.add(sw_rot)

        sz = Adw.ActionRow(title=t("settings.log_max_size"))
        szsp = Gtk.SpinButton.new_with_range(1, 100, 1)
        szsp.set_value(config.get("log_max_size_mb", 10))
        szsp.connect("value-changed",
                     lambda s: self._set("log_max_size_mb", int(s.get_value())))
        sz.add_suffix(szsp)
        g5.add(sz)

        cnt = Adw.ActionRow(title=t("settings.log_max_files"))
        cntsp = Gtk.SpinButton.new_with_range(1, 50, 1)
        cntsp.set_value(config.get("log_max_files", 5))
        cntsp.connect("value-changed",
                      lambda s: self._set("log_max_files", int(s.get_value())))
        cnt.add_suffix(cntsp)
        g5.add(cnt)

        # ── Log files ──
        g6 = Adw.PreferencesGroup(title=t("settings.log_files"))
        col.append(g6)

        for name, label_key in [("activity.log", "settings.log_activity"),
                                ("rclone.log", "settings.log_rclone"),
                                ("debug.log", "settings.log_debug")]:
            row = Adw.ActionRow(title=t(label_key), subtitle=str(LOG_DIR / name),
                                icon_name="file-text-symbolic")
            btn = Gtk.Button(label=t("settings.open"), css_classes=["flat"])
            path = str(LOG_DIR / name)
            btn.connect("clicked", lambda _, p=path: subprocess.Popen(["xdg-open", p]))
            row.add_suffix(btn)
            g6.add(row)

        folder_row = Adw.ActionRow(title=t("settings.log_folder"),
                                   subtitle=str(LOG_DIR),
                                   icon_name="folder-symbolic")
        fbtn = Gtk.Button(label=t("settings.open_folder"), css_classes=["flat"])
        fbtn.connect("clicked", lambda _: subprocess.Popen(["xdg-open", str(LOG_DIR)]))
        folder_row.add_suffix(fbtn)
        g6.add(folder_row)

        # ── Log actions ──
        g7 = Adw.PreferencesGroup(title=t("settings.log_actions"))
        col.append(g7)

        exp = Gtk.Button(label=t("settings.export_logs"), css_classes=["flat", "pill"])
        exp.set_halign(Gtk.Align.START)
        exp.connect("clicked", lambda _: self._export())
        g7.add(exp)

        clr = Gtk.Button(label=t("settings.delete_all_logs"),
                         css_classes=["destructive-action", "pill"])
        clr.set_halign(Gtk.Align.START)
        clr.connect("clicked", lambda _: self._confirm_clear_logs())
        g7.add(clr)

        # ── About ──
        g8 = Adw.PreferencesGroup(title=t("settings.about"))
        col.append(g8)

        g8.add(Adw.ActionRow(title=t("settings.version"), subtitle=APP_VERSION))

        help_r = Adw.ActionRow(title=t("settings.docs"), icon_name="help-circle-symbolic")
        hb = Gtk.Button(label=t("settings.open"), css_classes=["flat"])
        hb.connect("clicked", lambda _: subprocess.Popen(
            ["xdg-open", "https://github.com/diegonaranjo/GDrive"]))
        help_r.add_suffix(hb)
        g8.add(help_r)

        rst = Gtk.Button(label=t("settings.reset_config"),
                         css_classes=["destructive-action", "pill"])
        rst.set_halign(Gtk.Align.START)
        rst.connect("clicked", lambda _: self._confirm_reset_config())
        g8.add(rst)

    def _set(self, key, val):
        old = self.config.get(key)
        self.config[key] = val
        save_config(self.config)
        if old != val:
            self._toast(t("settings.config_saved"))

    def _on_startup(self, row, _):
        self.config["launch_on_startup"] = row.get_active()
        save_config(self.config)
        toggle_autostart(row.get_active())

    def _on_language(self, row, _):
        langs = available_languages()
        lang_codes = [code for code, _ in langs]
        new_lang = lang_codes[row.get_selected()]
        old_lang = self.config.get("language", "es")
        if new_lang != old_lang:
            self.config["language"] = new_lang
            save_config(self.config)
            set_language(new_lang)
            if self._on_config_reset:
                self._on_config_reset()

    def _toast(self, msg):
        if self._on_toast:
            self._on_toast(msg)

    def _clear_cache(self):
        subprocess.run(["rclone", "cleancache", "gdrive:"], capture_output=True)
        self._toast(t("settings.cache_cleared"))

    def _export(self):
        d = Gtk.FileDialog(title=t("settings.export_logs_title"))
        d.save(self.get_root(), None, self._on_export)

    def _on_export(self, d, result):
        try:
            f = d.save_finish(result)
            if f:
                import zipfile
                with zipfile.ZipFile(f.get_path(), "w", zipfile.ZIP_DEFLATED) as zf:
                    for name in ["activity.log", "rclone.log", "debug.log"]:
                        p = LOG_DIR / name
                        if p.exists():
                            zf.write(p, name)
                self._toast(t("settings.logs_exported"))
        except GLib.Error:
            pass

    def _clear_logs(self):
        for name in ["activity.log", "rclone.log", "debug.log"]:
            (LOG_DIR / name).unlink(missing_ok=True)
        self._toast(t("settings.logs_deleted"))

    def _confirm_clear_logs(self):
        from ..widgets import ConfirmDialog
        dlg = ConfirmDialog(self.get_root(),
                            t("settings.confirm_delete_logs"),
                            t("settings.confirm_delete_logs_body"),
                            confirm_label=t("settings.confirm_delete"))
        dlg.connect("response", self._on_clear_response)
        dlg.present()

    def _on_clear_response(self, dlg, response):
        if response == "confirm":
            self._clear_logs()

    def request_clear_logs(self):
        self._clear_logs()

    def _reset_config(self):
        CONFIG_FILE.unlink(missing_ok=True)
        self.config.clear()
        self.config.update(DEFAULTS)
        save_config(self.config)
        self._toast(t("settings.config_reset"))
        if self._on_config_reset:
            self._on_config_reset()

    def _confirm_reset_config(self):
        from ..widgets import ConfirmDialog
        dlg = ConfirmDialog(self.get_root(),
                            t("settings.confirm_reset"),
                            t("settings.confirm_reset_body"),
                            confirm_label=t("settings.confirm_reset_btn"))
        dlg.connect("response", self._on_reset_response)
        dlg.present()

    def _on_reset_response(self, dlg, response):
        if response == "confirm":
            self._reset_config()
