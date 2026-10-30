#!/usr/bin/env python3
"""GDrive - Professional Desktop Client (entry-point).

Phase 3: the GTK4 UI delegates the engine to the native daemon (Rust) over
D-Bus. This entry-point starts the window and, if the daemon is not running,
launches it automatically so the app works without manual steps.
"""

import os
import subprocess
import sys
from pathlib import Path

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Gio", "2.0")
from gi.repository import Adw, Gio, GLib

from gdrive.config import load_config
from gdrive.constants import APP_ID, DAEMON_ID
from gdrive.i18n import set_language, t
from gdrive.daemon_client import DaemonClient
from gdrive.theme import load_theme
from gdrive.window import DriveWindow


def _daemon_binary() -> Path | None:
    """Locates the native daemon binary (built in native/gdrive-core)."""
    candidates = [
        Path(os.environ.get("GDRIVE_DAEMON_BIN", "")) if os.environ.get("GDRIVE_DAEMON_BIN") else None,
        Path(__file__).resolve().parent / "native" / "gdrive-core" / "target" / "release" / "gdrive-daemon",
        Path(__file__).resolve().parent / "native" / "gdrive-core" / "target" / "debug" / "gdrive-daemon",
        Path.home() / ".local" / "bin" / "gdrive-daemon",
        Path("/usr/local/bin/gdrive-daemon"),
        Path("/usr/bin/gdrive-daemon"),
    ]
    for c in candidates:
        if c and c.exists():
            return c
    return None


def _daemon_running() -> bool:
    """Checks if the name com.gdrive.Client has an owner on the session bus."""
    try:
        import subprocess
        r = subprocess.run(
            ["busctl", "--user", "status", DAEMON_ID],
            capture_output=True, text=True, timeout=3,
        )
        return r.returncode == 0
    except Exception:
        # Fallback: try gapplication
        return False


def _ensure_daemon() -> bool:
    """Starts the native daemon if not running. Returns whether it is available."""
    if _daemon_running():
        return True
    binary = _daemon_binary()
    if not binary:
        return False
    try:
        subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
        return True
    except Exception:
        return False


def _ui_already_running() -> bool:
    """Checks whether another UI instance is already registered on D-Bus.

    The UI's `application_id` (`com.gdrive.ClientUI`) is unique on the session
    bus: if an owner already exists, this process is a second instance and
    must not open a duplicate window.
    """
    try:
        conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        reply = conn.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            GLib.Variant("(s)", (APP_ID,)),
            GLib.VariantType.new("(b)"),
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        )
        return bool(reply.unpack()[0])
    except Exception:
        return False


def _activate_existing_instance() -> bool:
    """Activates the window of the instance that is already running, over D-Bus."""
    for method in (
        ["gapplication", "activate", APP_ID],
        ["dbus-send", "--session", "--type=method_call",
         f"--dest={APP_ID}", "/com/gdrive/ClientUI",
         "org.freedesktop.Application.Activate"],
    ):
        try:
            subprocess.run(method, capture_output=True, timeout=3)
        except Exception:
            continue
    return True


class GDriveApp(Adw.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.config = load_config()
        set_language(self.config.get("language", "es"))
        self.daemon_client = DaemonClient(self.config, app=self)
        self.win = None
        self._first_activate = True
        self.create_action("quit", self._quit, ["<Control>q"])
        self.create_action("sync-now", self._sync_now, ["<Control>s"])
        self.create_action("toggle-pause", self._toggle_pause, ["<Control>r"])
        self.create_action("go-settings", self._go_settings, ["<Control>comma"])
        self.create_action("refresh-account", self._refresh_account, ["F5"])

    def do_startup(self):
        Adw.Application.do_startup(self)
        load_theme()
        # Make sure the native engine is alive before opening the window.
        _ensure_daemon()

    def do_activate(self):
        # Automatic startup must not open the window: the app stays in the
        # tray and the window appears when the user asks for it (second
        # `activate`, launcher icon or "Settings" from the menu).
        if self._first_activate and self.config.get("start_hidden", False):
            self._first_activate = False
            # With no windows of its own a `GApplication` quits as soon as
            # `activate` ends (that is exactly what killed the single-instance
            # notice). The hold is what keeps the tray session alive.
            self.hold()
            self.daemon_client.refresh_status()
            return
        self._first_activate = False
        self.show_window()
        self.daemon_client.refresh_status()

    def show_window(self):
        """Creates the window if it does not exist and brings it to the front."""
        if not self.win:
            self.win = DriveWindow(self, self.config, self.daemon_client)
        self.win.present()

    def create_action(self, name, callback, accels=None):
        action = Gio.SimpleAction.new(name, None)
        action.connect("activate", callback)
        self.add_action(action)
        if accels:
            self.set_accels_for_action(f"app.{name}", accels)

    def _quit(self, *a):
        self.daemon_client.close()
        self.quit()

    def _sync_now(self, *a):
        if self.win:
            self.win._status_pg._do_sync()

    def _toggle_pause(self, *a):
        if self.win:
            self.win._status_pg._do_pause()

    def _go_settings(self, *a):
        # Asking for Settings asks for the window: create it on hidden startup.
        self.show_window()
        self.win._switch("settings")

    def _refresh_account(self, *a):
        if self.win:
            self.win._account_pg.refresh()


def main():
    # Singleton: if a UI instance is already alive, do not open a second one.
    # The existing window is activated and an informational alert is shown.
    if _ui_already_running():
        _activate_existing_instance()
        _show_already_running_alert()
        return 0

    app = GDriveApp()
    return app.run(None)


def _show_already_running_alert():
    """Shows an alert (GTK4 dialog + desktop notification).

    Runs in a short-lived process with no parent window. We use a desktop
    notification (via `org.freedesktop.Notifications`, without registering any
    conflicting app) and a standalone modal dialog.
    """
    _send_desktop_notification(
        t("app.already_running_notif"), t("app.already_running_body")
    )
    try:
        _dialog_loop()
    except Exception:
        pass


def _send_desktop_notification(title, body):
    """Sends a desktop notification via `org.freedesktop.Notifications`."""
    try:
        conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        params = GLib.Variant(
            "(susssasa{sv}i)",
            (
                "gdrive",                # app_name
                0,                       # replaces_id
                "google-drive",          # icon
                title,
                body,
                [],                      # actions
                {},                      # hints
                5000,                    # timeout ms
            ),
        )
        conn.call_sync(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "Notify",
            params,
            GLib.VariantType.new("(u)"),
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        )
    except Exception:
        pass


def _dialog_loop():
    """Shows a standalone `Adw.MessageDialog` and enters its own main loop."""
    app = Adw.Application(application_id="com.gdrive.ClientUI.alert")

    def on_activate(application):
        dlg = Adw.MessageDialog(
            transient_for=None,
            modal=True,
            heading=t("app.already_running_title"),
            body=t("app.already_running_body"),
        )
        dlg.add_response("ok", t("widget.confirm"))
        dlg.set_response_appearance("ok", Adw.ResponseAppearance.SUGGESTED)
        dlg.set_default_response("ok")
        dlg.set_close_response("ok")
        dlg.connect("response", lambda d, r: application.quit())
        # Without this the application has NOT ONE window registered (an
        # `Adw.MessageDialog` with `transient_for=None` is not part of the app),
        # so `run()` returns as soon as `activate` ends: the notice appeared and
        # closed by itself, too fast to read. Measured: with `add_window` the
        # dialog is still alive after 2 s; without it the process dies at once.
        application.add_window(dlg)
        dlg.present()

    app.connect("activate", on_activate)
    app.run(None)


if __name__ == "__main__":
    main()