#!/usr/bin/env python3
"""GDrive - Professional Desktop Client (entry-point).

Fase 3: la UI GTK4 delega el motor en el daemon nativo (Rust) vía D-Bus. Este
entry-point arranca la ventana y, si el daemon no está corriendo, lo lanza
automáticamente para que la app funcione sin pasos manuales.
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
    """Localiza el binario del daemon nativo (compilado en native/gdrive-core)."""
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
    """Consulta si el nombre com.gdrive.Client tiene owner en el bus de sesión."""
    try:
        import subprocess
        r = subprocess.run(
            ["busctl", "--user", "status", DAEMON_ID],
            capture_output=True, text=True, timeout=3,
        )
        return r.returncode == 0
    except Exception:
        # Fallback: intentar gapplication
        return False


def _ensure_daemon() -> bool:
    """Lanza el daemon nativo si no está corriendo. Devuelve si quedó disponible."""
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
    """Comprueba si ya hay otra instancia de la UI registrada en D-Bus.

    El `application_id` de la UI (`com.gdrive.ClientUI`) es único en el bus de
    sesión: si ya existe un propietario, este proceso es una segunda instancia
    y no debe abrir una ventana duplicada.
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
    """Activa la ventana de la instancia ya en ejecución vía D-Bus."""
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
        # Garantizar que el motor nativo esté vivo antes de abrir la ventana.
        _ensure_daemon()

    def do_activate(self):
        # El arranque automatico no debe abrir la ventana: la app queda en la
        # bandeja y la ventana aparece cuando el usuario la pide (segundo
        # `activate`, icono del launcher o "Configuracion" del menu).
        if self._first_activate and self.config.get("start_hidden", False):
            self._first_activate = False
            # Sin ventanas propias una `GApplication` se cierra apenas termina
            # `activate` (es exactamente lo que mataba al aviso de instancia
            # unica). El hold es lo que mantiene viva la sesion en bandeja.
            self.hold()
            self.daemon_client.refresh_status()
            return
        self._first_activate = False
        self.show_window()
        self.daemon_client.refresh_status()

    def show_window(self):
        """Crea la ventana si no existe y la trae al frente."""
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
        # Pedir Ajustes es pedir la ventana: crearla si el arranque fue oculto.
        self.show_window()
        self.win._switch("settings")

    def _refresh_account(self, *a):
        if self.win:
            self.win._account_pg.refresh()


def main():
    # Singleton: si ya hay una instancia de la UI viva, no abrir una segunda.
    # Se activa la ventana existente y se muestra una alerta informativa.
    if _ui_already_running():
        _activate_existing_instance()
        _show_already_running_alert()
        return 0

    app = GDriveApp()
    return app.run(None)


def _show_already_running_alert():
    """Muestra una alerta (diálogo GTK4 + notificación de escritorio).

    Se ejecuta en un proceso efímero sin ventana padre. Usamos una notificación
    de escritorio (vía `org.freedesktop.Notifications`, sin registrar ninguna
    app conflictiva) y un diálogo modal autónomo.
    """
    _send_desktop_notification(
        t("app.already_running_notif"), t("app.already_running_body")
    )
    try:
        _dialog_loop()
    except Exception:
        pass


def _send_desktop_notification(title, body):
    """Envía una notificación de escritorio vía `org.freedesktop.Notifications`."""
    try:
        conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        params = GLib.Variant(
            "(susssasa{sv}i)",
            (
                "gdrive",                # app_name
                0,                       # replaces_id
                "google-drive",          # icono
                title,
                body,
                [],                      # acciones
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
    """Muestra un `Adw.MessageDialog` autónomo y entra en su propio main loop."""
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
        # Sin esto la aplicación no tiene NINGUNA ventana registrada (un
        # `Adw.MessageDialog` con `transient_for=None` no pertenece a la app),
        # así que `run()` retorna apenas termina `activate`: el aviso aparecía
        # y se cerraba solo, sin dejar leerlo. Medido: con `add_window` el
        # diálogo sigue vivo a los 2 s; sin él, el proceso muere al instante.
        application.add_window(dlg)
        dlg.present()

    app.connect("activate", on_activate)
    app.run(None)


if __name__ == "__main__":
    main()