#!/usr/bin/env python3
"""Google Drive Tray Icon - GTK3 daemon for system tray."""

import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gio", "2.0")
gi.require_version("AyatanaAppIndicator3", "0.1")
from gi.repository import Gtk, GLib, Gio
from gi.repository import AyatanaAppIndicator3 as AppIndicator3

_PROJECT_ROOT = Path(__file__).resolve().parent
ICON_PATH = str(_PROJECT_ROOT / "icons")
APP_SCRIPT = str(_PROJECT_ROOT / "gdrive-app.py")
CONFIG_DIR = Path.home() / ".config" / "gdrive"
CONFIG_FILE = CONFIG_DIR / "settings.json"
SYNC_DIR_DEFAULT = str(Path.home() / "GoogleDrive")

DAEMON_ID = "com.gdrive.Client"
UI_ID = "com.gdrive.ClientUI"


def _daemon_binary():
    """Localiza el binario del daemon nativo (Rust)."""
    candidates = [
        _PROJECT_ROOT / "native" / "gdrive-core" / "target" / "release" / "gdrive-daemon",
        _PROJECT_ROOT / "native" / "gdrive-core" / "target" / "debug" / "gdrive-daemon",
        Path.home() / ".local" / "bin" / "gdrive-daemon",
        Path("/usr/local/bin/gdrive-daemon"),
        Path("/usr/bin/gdrive-daemon"),
    ]
    for c in candidates:
        if c.exists():
            return str(c)
    return None


def _ensure_daemon():
    """Lanza el daemon nativo si no está corriendo. Devuelve True si quedó vivo."""
    if _daemon_running():
        return True
    binary = _daemon_binary()
    if not binary:
        log_msg("Daemon binario no encontrado (compila antes)")
        return False
    log_msg(f"Lanzando daemon nativo: {binary}")
    try:
        subprocess.Popen([binary], stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
        return True
    except Exception as e:
        log_msg(f"Error lanzando daemon: {e}")
        return False

_anim_state = {
    "frame": 1,
    "tick_count": 0,
    "last_sync_state": False,
}


def _daemon_running():
    """True si el daemon nativo (dueño de com.gdrive.Client) está registrado."""
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        res = bus.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            GLib.Variant("(s)", (DAEMON_ID,)),
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None
        )
        return res.unpack()[0]
    except Exception:
        return False


# Alias de compatibilidad con el código previo.
def _is_app_running():
    return _daemon_running()


def _load_i18n():
    if _PROJECT_ROOT.as_posix() not in sys.path:
        sys.path.insert(0, _PROJECT_ROOT.as_posix())
    from gdrive.i18n import t, set_language
    from gdrive.config import load_config
    cfg = load_config()
    set_language(cfg.get("language", "es"))
    return t


def load_config():
    if CONFIG_FILE.exists():
        try:
            return json.loads(CONFIG_FILE.read_text())
        except Exception:
            pass
    return {}


def log_msg(msg):
    now = time.strftime("%Y-%m-%d %H:%M:%S")
    logf = CONFIG_DIR / "debug.log"
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(logf, "a") as f:
        f.write(f"[{now}] [INFO ] [Tray] {msg}\n")


def _icon_item(label, icon_name, callback):
    item = Gtk.MenuItem()
    box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
    img = Gtk.Image.new_from_icon_name(icon_name, Gtk.IconSize.MENU)
    box.pack_start(img, False, False, 0)
    lbl = Gtk.Label(label=label)
    box.pack_start(lbl, False, False, 0)
    item.add(box)
    item.connect("activate", callback)
    return item


def _daemon_status():
    """Consulta el estado real al daemon nativo vía D-Bus.

    Devuelve el valor de `status` (`idle` | `syncing` | `error` | …) o `None`
    si el daemon no responde. Sustituye a la antigua detección por lock file,
    que dependía de un prefijo bisync hardcodeado.
    """
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        res = bus.call_sync(
            DAEMON_ID,
            "/com/gdrive/Client",
            "com.gdrive.Daemon1",
            "GetStatus",
            None,
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
        )
        status = dict(res.unpack()[0])
        return status.get("status")
    except Exception:
        return None


def _is_syncing():
    """True si el daemon nativo indica que un sync está en curso."""
    return _daemon_status() == "syncing"


def open_settings(*_):
    log_msg("Opening settings")
    # Pedirle a la instancia YA registrada que cambie a la página de Ajustes.
    # Antes se lanzaba un segundo `gdrive-app.py`: ese proceso choca contra el
    # control de instancia única, así que solo activaba la ventana existente,
    # mostraba el aviso "ya se está ejecutando" y se cerraba — la Configuración
    # nunca abría y el usuario veía una ventana aparecer y desaparecer.
    for cmd in (["gapplication", "action", UI_ID, "go-settings"],
                ["gapplication", "activate", UI_ID]):
        try:
            if subprocess.run(cmd, capture_output=True, timeout=5).returncode == 0:
                return
        except Exception:
            continue
    # Sin instancia de UI viva: arrancarla (ella sola mostrará la ventana).
    subprocess.Popen([sys.executable, APP_SCRIPT])


def sync_now(*_):
    global _anim_state
    t = _load_i18n()
    log_msg("Sync now requested from tray")
    if not _ensure_daemon():
        log_msg("No se pudo asegurar el daemon")
        return

    _anim_state["last_sync_state"] = True
    _anim_state["frame"] = 1
    _anim_state["tick_count"] = 12

    try:
        # Llamada directa al daemon nativo (más robusta que gapplication action).
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync(
            DAEMON_ID,
            "/com/gdrive/Client",
            "com.gdrive.Daemon1",
            "SyncNow",
            None,
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
        )
        subprocess.Popen(["notify-send", "Google Drive", t("tray.notify_sync_started")])
    except Exception as e:
        log_msg(f"Error sending sync-now action: {e}")


def pause_sync(indicator, *_):
    t = _load_i18n()
    config = load_config()
    paused = not config.get("paused", False)
    method = "Pause" if paused else "Resume"
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync(
            DAEMON_ID,
            "/com/gdrive/Client",
            "com.gdrive.Daemon1",
            method,
            None,
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
        )
        config["paused"] = paused
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        with open(CONFIG_FILE, "w") as f:
            json.dump(config, f, indent=2)
    except Exception as e:
        log_msg(f"Error toggling pause via daemon: {e}")
        # Fallback: togglear el archivo directamente (si el daemon no está).
        config["paused"] = paused
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        with open(CONFIG_FILE, "w") as f:
            json.dump(config, f, indent=2)
    state = t("tray.sync_paused") if config["paused"] else t("tray.sync_resumed")
    log_msg(state)
    update_indicator_state(indicator, config)


def open_folder(*_):
    config = load_config()
    path = config.get("sync_dir", SYNC_DIR_DEFAULT)
    log_msg(f"Opening folder: {path}")
    subprocess.Popen(["xdg-open", path])


def open_web(*_):
    log_msg("Opening Google Drive in browser")
    subprocess.Popen(["xdg-open", "https://drive.google.com"])


def open_manage(*_):
    log_msg("Opening Google account management")
    subprocess.Popen(["xdg-open", "https://myaccount.google.com"])


def quit_app(*_):
    log_msg("Quitting tray and application")
    try:
        subprocess.run(["gapplication", "action", DAEMON_ID, "quit"])
    except Exception as e:
        log_msg(f"Error quitting gdrive-daemon: {e}")
    Gtk.main_quit()


def _import_signin():
    if _PROJECT_ROOT.as_posix() not in sys.path:
        sys.path.insert(0, _PROJECT_ROOT.as_posix())
    from gdrive.helpers import (rclone_is_authorized, rclone_sign_in,
                                rclone_sign_out, rclone_user_email)
    return rclone_is_authorized, rclone_sign_in, rclone_sign_out, rclone_user_email


def sign_in_flow(state, *_):
    import threading
    t = _load_i18n()
    log_msg("Starting Google sign-in")
    try:
        subprocess.Popen(["notify-send", "Google Drive", t("tray.notify_opening_browser")])
    except Exception:
        pass

    def worker():
        try:
            _, do_sign_in, _, _ = _import_signin()
            ok = do_sign_in()
        except Exception as e:
            ok = False
            log_msg(f"Sign-in error: {e}")
        t_fn = _load_i18n()
        msg = t_fn("tray.notify_signin_ok") if ok else t_fn("tray.notify_signin_fail")
        try:
            subprocess.Popen(["notify-send", "Google Drive", msg])
        except Exception:
            pass
        log_msg(msg)
        GLib.idle_add(lambda: refresh_session_state(state))

    threading.Thread(target=worker, daemon=True).start()


def sign_out_flow(state, *_):
    import threading
    t = _load_i18n()
    log_msg("Signing out from Google")

    def worker():
        try:
            _, _, do_sign_out, _ = _import_signin()
            ok = do_sign_out()
        except Exception as e:
            ok = False
            log_msg(f"Sign-out error: {e}")
        t_fn = _load_i18n()
        msg = t_fn("tray.notify_signout_ok") if ok else t_fn("tray.notify_signout_fail")
        try:
            subprocess.Popen(["notify-send", "Google Drive", msg])
        except Exception:
            pass
        log_msg(msg)
        GLib.idle_add(lambda: refresh_session_state(state))

    threading.Thread(target=worker, daemon=True).start()


def update_indicator_state(indicator, config=None):
    if config is None:
        config = load_config()
    t = _load_i18n()

    if _is_syncing():
        indicator.set_icon_full("gdrive-syncing-frame-1", "Google Drive — Syncing…")
    elif config.get("paused"):
        indicator.set_icon_full("gdrive-paused-symbolic",
                                f"Google Drive — {t('tray.paused')}")
    else:
        indicator.set_icon_full("google-drive",
                                f"Google Drive — {t('tray.idle')}")


def _icon_animate(indicator):
    global _anim_state
    _anim_state["tick_count"] += 1

    is_sync = False
    if _anim_state["last_sync_state"]:
        is_sync = _is_syncing()
    else:
        if _anim_state["tick_count"] >= 12:
            _anim_state["tick_count"] = 0
            is_sync = _is_syncing()
        else:
            is_sync = False

    _anim_state["last_sync_state"] = is_sync

    if is_sync:
        frame = _anim_state["frame"]
        indicator.set_icon_full(f"gdrive-syncing-frame-{frame}", "Google Drive — Syncing…")
        _anim_state["frame"] = (frame % 8) + 1
    else:
        if _anim_state["tick_count"] == 0:
            update_indicator_state(indicator)

    return True


def refresh_session_state(state):
    signed_in = False
    email = None
    try:
        is_auth, _, _, get_email = _import_signin()
        signed_in = is_auth()
        if signed_in:
            email = get_email()
    except Exception:
        pass

    t = _load_i18n()
    if signed_in:
        new_lbl = f"👤 {email}" if email else f"👤 {t('tray.session_started')}"
        state["sess_item"].set_label(new_lbl)
        state["sess_item"].show()
        state["signout_item"].show()
        state["signin_item"].hide()
    else:
        state["sess_item"].hide()
        state["signout_item"].hide()
        state["signin_item"].show()


def build_menu(indicator):
    t = _load_i18n()
    config = load_config()

    menu = Gtk.Menu()

    state_key = "paused" if config.get("paused") else "idle"
    state_lbl = "● " + t(f"tray.{state_key}")
    state_item = Gtk.MenuItem(label=state_lbl)
    state_item.set_sensitive(False)
    menu.append(state_item)

    sess_item = Gtk.MenuItem(label=f"👤 {t('tray.session_started')}")
    sess_item.set_sensitive(False)
    menu.append(sess_item)

    signin_item = _icon_item(t("tray.sign_in"), "changes-prevent-symbolic",
                             lambda *a: sign_in_flow(state, *a))
    menu.append(signin_item)

    signout_item = _icon_item(t("tray.sign_out"), "system-log-out-symbolic",
                              lambda *a: sign_out_flow(state, *a))
    menu.append(signout_item)

    menu.append(Gtk.SeparatorMenuItem())

    menu.append(_icon_item(t("tray.settings"), "preferences-system-symbolic",
                           open_settings))
    menu.append(_icon_item(t("tray.sync_now"), "view-refresh-symbolic",
                           sync_now))
    paused = config.get("paused", False)
    pause_label = t("tray.resume") if paused else t("tray.pause")
    pause_icon = ("media-playback-start-symbolic" if paused
                  else "media-playback-pause-symbolic")
    menu.append(_icon_item(pause_label, pause_icon,
                           lambda *a: pause_sync(indicator, *a)))

    menu.append(Gtk.SeparatorMenuItem())

    menu.append(_icon_item(t("tray.open_folder"), "folder-open-symbolic",
                           open_folder))
    menu.append(_icon_item(t("tray.open_browser"), "applications-internet-symbolic",
                           open_web))

    menu.append(Gtk.SeparatorMenuItem())

    menu.append(_icon_item(t("tray.manage_account"), "user-info-symbolic", open_manage))

    menu.append(Gtk.SeparatorMenuItem())

    menu.append(_icon_item(t("tray.quit"), "application-exit-symbolic", quit_app))

    menu.show_all()
    indicator.set_menu(menu)

    state = {
        "sess_item": sess_item,
        "signin_item": signin_item,
        "signout_item": signout_item,
    }
    refresh_session_state(state)
    return state


def main():
    signal.signal(signal.SIGINT, signal.SIG_DFL)

    indicator = AppIndicator3.Indicator.new(
        "google-drive",
        "google-drive",
        AppIndicator3.IndicatorCategory.APPLICATION_STATUS,
    )
    indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
    if os.path.isdir(ICON_PATH):
        indicator.set_icon_theme_path(ICON_PATH)
    update_indicator_state(indicator)

    build_menu(indicator)

    GLib.timeout_add(250, _icon_animate, indicator)

    try:
        if not _daemon_running():
            log_msg("Starting gdrive-daemon in background...")
            if _ensure_daemon():
                log_msg("Daemon nativo iniciado")
            else:
                log_msg("No se pudo iniciar el daemon nativo (compila con cargo build --release)")
    except Exception as e:
        log_msg(f"Failed to start gdrive-daemon: {e}")

    Gtk.main()


if __name__ == "__main__":
    main()
