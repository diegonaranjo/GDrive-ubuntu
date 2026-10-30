"""Funciones utilitarias: formato, logging, rclone, notificaciones, autostart."""

import json
import os
import re
import subprocess
import time
from pathlib import Path

import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio

from .config import LOG_DIR
from .constants import RCLONE_REMOTE

LOG_LEVELS = {"error": 0, "info": 1, "debug": 2, "trace": 3}


def format_size(b):
    if b <= 0:
        return "0 B"
    for u in ["B", "KB", "MB", "GB", "TB"]:
        if abs(b) < 1024:
            return f"{b:.1f} {u}"
        b /= 1024
    return f"{b:.1f} PB"


def log_write(config, level, component, message, also_activity=False):
    """Escribe en debug.log. Si also_activity=True, también en activity.log."""
    if not config.get("logging_enabled", True):
        return
    min_level = LOG_LEVELS.get(config.get("log_level", "info"), 1)
    if LOG_LEVELS.get(level, 1) > min_level:
        # Aunque el debug.log filtre por nivel, activity.log siempre registra
        # eventos significativos (also_activity) sin importar log_level.
        if not also_activity:
            return
    now = time.strftime("%Y-%m-%d %H:%M:%S")
    line = f"[{now}] [{level.upper():5s}] [{component}] {message}\n"
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    logf = LOG_DIR / "debug.log"
    if config.get("log_rotate", True) and logf.exists():
        max_mb = config.get("log_max_size_mb", 10)
        if logf.stat().st_size > max_mb * 1024 * 1024:
            for i in range(config.get("log_max_files", 5) - 1, 0, -1):
                src = LOG_DIR / f"debug.{i}.log"
                dst = LOG_DIR / f"debug.{i + 1}.log"
                if src.exists():
                    if i + 1 >= config.get("log_max_files", 5):
                        src.unlink()
                    else:
                        src.rename(dst)
            if logf.exists():
                logf.rename(LOG_DIR / "debug.1.log")
    with open(logf, "a") as f:
        f.write(line)

    if also_activity:
        write_activity(component, message)


def write_activity(component, message):
    """Anexa un evento al activity.log (formato plano con separador |)."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    now = time.strftime("%d/%m/%Y %H:%M:%S")
    with open(LOG_DIR / "activity.log", "a") as f:
        f.write(f"{now} | {component} | {message}\n")


def rclone_about():
    try:
        r = subprocess.run(
            ["rclone", "about", RCLONE_REMOTE, "--json"],
            capture_output=True, text=True, timeout=10,
        )
        if r.returncode == 0 and r.stdout.strip():
            return json.loads(r.stdout)
    except Exception:
        pass
    return None


def rclone_remote_exists(remote=None):
    """True si el remote de rclone (default RCLONE_REMOTE) está configurado."""
    remote = remote or RCLONE_REMOTE
    name = remote.rstrip(":")
    try:
        r = subprocess.run(
            ["rclone", "listremotes"],
            capture_output=True, text=True, timeout=5,
        )
        if r.returncode == 0:
            return any(line.strip().rstrip(":") == name
                       for line in r.stdout.splitlines())
    except Exception:
        pass
    return False


def rclone_user_email():
    """Obtiene el email de la cuenta conectada al remote, o None.

    Google Drive no expone el email como campo del config, así que lo
    inferimos consultando el propio drive vía `rclone backend`.
    """
    name = RCLONE_REMOTE.rstrip(":")
    # 1) Intentar leer el campo 'user'/'account' del config (otros backends).
    try:
        r = subprocess.run(
            ["rclone", "config", "show", RCLONE_REMOTE],
            capture_output=True, text=True, timeout=5,
        )
        if r.returncode == 0:
            for line in r.stdout.splitlines():
                line = line.strip()
                for key in ("user", "account", "email"):
                    if line.startswith(f"{key} ="):
                        val = line.split("=", 1)[1].strip()
                        if val:
                            return val
    except Exception:
        pass
    # 2) Para Google Drive: obtenerlo desde la API del backend.
    try:
        r = subprocess.run(
            ["rclone", "backend", "user", RCLONE_REMOTE],
            capture_output=True, text=True, timeout=10,
        )
        if r.returncode == 0 and r.stdout.strip():
            import json as _json
            data = _json.loads(r.stdout)
            return data.get("email") or data.get("userEmailAddress")
    except Exception:
        pass
    return None


def rclone_is_authorized():
    """True si el remote existe y tiene token válido (no solo configurado)."""
    if not rclone_remote_exists():
        log_write({}, "debug", "Auth", "Remote does not exist")
        return False
    try:
        r = subprocess.run(
            ["rclone", "about", RCLONE_REMOTE, "--json"],
            capture_output=True, text=True, timeout=30,
        )
        ok = r.returncode == 0 and bool(r.stdout.strip())
        if not ok:
            log_write({}, "debug", "Auth",
                      f"rclone about failed: rc={r.returncode} "
                      f"stdout={r.stdout[:200]} stderr={r.stderr[:200]}")
        else:
            log_write({}, "debug", "Auth", "rclone about OK")
        return ok
    except Exception as e:
        log_write({}, "debug", "Auth", f"rclone about exception: {e}")
        return False


def _graphical_env():
    """Devuelve un dict con variables de entorno gráfico para subprocess.

    Cuando la app/tray corre como servicio systemd de usuario, el entorno
    del proceso suele tener DISPLAY pero faltar WAYLAND_DISPLAY/BROWSER, lo
    que impide que rclone (o xdg-open) abran el navegador para OAuth.
    Aquí forzamos las variables conocidas de la sesión gráfica activa.
    """
    env = dict(os.environ)
    # Garantizar XDG_RUNTIME_DIR y DISPLAY (fallback a valores estándar).
    env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    env.setdefault("DISPLAY", ":0")
    # Wayland: el socket por defecto vive en XDG_RUNTIME_DIR/wayland-0.
    if "WAYLAND_DISPLAY" not in env:
        env["WAYLAND_DISPLAY"] = "wayland-0"
    # Asegurar rutas de datos GNOME/Ubuntu para que xdg-open encuentre el .desktop
    # del navegador por defecto.
    if "XDG_DATA_DIRS" not in env:
        env["XDG_DATA_DIRS"] = "/usr/share/ubuntu:/usr/share/gnome:/usr/local/share/:/usr/share/"
    # PATH completo (systemd a veces recorta PATH y xdg-open no halla browsers).
    env["PATH"] = (
        "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin:"
        + env.get("PATH", ""))
    return env


def _open_browser_for_oauth(stream_reader, stop_event):
    """Hilo que lee la salida de rclone y abre el navegador cuando ve la URL
    de callback local (http://127.0.0.1:53682/auth?...).

    rclone imprime la URL en stderr; algunos entornos no logran abrirla solos
    (servicios systemd sin BROWSER/WAYLAND_DISPLAY). Este hilo la abre
    explícitamente con xdg-open usando el entorno gráfico.
    """
    import re
    url_re = re.compile(r"http://127\.0\.0\.1:53682/auth\?state=\S+")
    buf = ""
    while not stop_event.is_set():
        ch = stream_reader.read(1)
        if not ch:
            break
        buf += ch
        # Procesar líneas completas.
        if ch == "\n":
            line = buf.rstrip()
            buf = ""
            m = url_re.search(line)
            if m:
                url = m.group(0)
                try:
                    subprocess.Popen(["xdg-open", url],
                                     env=_graphical_env(),
                                     stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL)
                except Exception:
                    pass
                return  # Bastante con abrirla una vez.


def rclone_sign_in(on_line=None):
    """Crea el remote 'gdrive' (si falta) y lanza OAuth en el navegador.

    Flujo robusto:
      1. Si el remote no existe, lo crea con `rclone config create` (no
         interactivo: solo define type=drive y scope=drive, sin OAuth).
      2. Ejecuta `rclone config reconnect gdrive:` alimentando respuestas por
         stdin: 'y' (usar browser local) y 'n' (no es Team Drive).
      3. Un hilo aparte vigila la salida de rclone y abre explícitamente el
         navegador con la URL de OAuth vía xdg-open usando el entorno gráfico
         (fallback para servicios systemd sin WAYLAND_DISPLAY/BROWSER).
      4. Verifica con rclone_is_authorized().

    `on_line(line)` opcional recibe cada línea para mostrar progreso en la UI.
    Devuelve True si el remote quedó autorizado.
    """
    import threading
    name = RCLONE_REMOTE.rstrip(":")

    # 1) Crear la entrada del remote si no existe (sin OAuth).
    if not rclone_remote_exists():
        try:
            subprocess.run(
                ["rclone", "config", "create", name, "drive", "scope=drive",
                 "--non-interactive"],
                capture_output=True, text=True, timeout=15,
            )
        except Exception:
            pass

    # 2) reconnect + hilo que abre el navegador explícitamente.
    try:
        proc = subprocess.Popen(
            ["rclone", "config", "reconnect", RCLONE_REMOTE],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, bufsize=1,
            env=_graphical_env(),
        )
        stop_event = threading.Event()
        reader_thread = threading.Thread(
            target=_open_browser_for_oauth,
            args=(proc.stdout, stop_event),
            daemon=True,
        )
        reader_thread.start()
        answers = "y\nn\n"
        try:
            out, _ = proc.communicate(input=answers, timeout=180)
            if on_line and out:
                for line in out.splitlines():
                    on_line(line.rstrip())
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()
            stop_event.set()
            return False
        stop_event.set()
    except Exception as e:
        if on_line:
            on_line(f"exception: {e}")
        return False

    return rclone_is_authorized()


def rclone_sign_out():
    """Elimina el remote de rclone (config delete)."""
    name = RCLONE_REMOTE.rstrip(":")
    try:
        subprocess.run(
            ["rclone", "config", "delete", name],
            capture_output=True, timeout=10,
        )
        return True
    except Exception:
        return False
    return None


def rclone_lsd(remote):
    try:
        r = subprocess.run(
            ["rclone", "lsd", remote],
            capture_output=True, text=True, timeout=30,
        )
        dirs = []
        if r.returncode == 0 and r.stdout.strip():
            for line in r.stdout.strip().split("\n"):
                parts = line.strip().split()
                if parts:
                    dirs.append(parts[-1])
        return dirs
    except Exception:
        pass
    return []


def _project_root():
    """Directorio raíz del proyecto (donde está launch.sh).

    helpers.py está en <root>/gdrive/, subimos un nivel.
    """
    return Path(__file__).resolve().parent.parent


def toggle_autostart(enable):
    d = Path.home() / ".config" / "autostart"
    f = d / "gdrive.desktop"
    if enable:
        launch = _project_root() / "launch.sh"
        d.mkdir(parents=True, exist_ok=True)
        f.write_text(
            "[Desktop Entry]\nType=Application\nName=Google Drive\n"
            f"Exec=bash {launch}\n"
            "Icon=google-drive\nTerminal=false\nHidden=false\n"
            "X-GNOME-Autostart-enabled=true\n"
        )
    else:
        f.unlink(missing_ok=True)


def send_notification(app, title, body, priority=Gio.NotificationPriority.NORMAL):
    """Envía una notificación de escritorio vía GApplication.

    `app` debe ser la instancia de GDriveApp. Si no está disponible, no-op.
    """
    try:
        notif = Gio.Notification.new(title)
        notif.set_body(body)
        notif.set_priority(priority)
        app.send_notification(None, notif)
    except Exception:
        pass
