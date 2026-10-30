"""Utility functions: formatting, logging, rclone, notifications, autostart."""

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
    """Writes to debug.log. If also_activity=True, also to activity.log."""
    if not config.get("logging_enabled", True):
        return
    min_level = LOG_LEVELS.get(config.get("log_level", "info"), 1)
    if LOG_LEVELS.get(level, 1) > min_level:
        # Even though debug.log filters by level, activity.log always records
        # significant events (also_activity) regardless of log_level.
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
    """Appends an event to activity.log (flat format with the | separator)."""
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
    """True if the rclone remote (default RCLONE_REMOTE) is configured."""
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
    """Returns the email of the account connected to the remote, or None.

    Google Drive does not expose the email as a config field, so we infer it
    by querying the drive itself via `rclone backend`.
    """
    name = RCLONE_REMOTE.rstrip(":")
    # 1) Try reading the config 'user'/'account' field (other backends).
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
    # 2) For Google Drive: get it from the backend API.
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
    """True if the remote exists and has a valid token (not just configured)."""
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
    """Returns a dict with graphical environment variables for subprocess.

    When the app/tray runs as a user systemd service, the process environment
    usually has DISPLAY but is missing WAYLAND_DISPLAY/BROWSER, which stops
    rclone (or xdg-open) from opening the browser for OAuth. Here we force
    the known variables of the active graphical session.
    """
    env = dict(os.environ)
    # Ensure XDG_RUNTIME_DIR and DISPLAY (fallback to standard values).
    env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    env.setdefault("DISPLAY", ":0")
    # Wayland: the default socket lives in XDG_RUNTIME_DIR/wayland-0.
    if "WAYLAND_DISPLAY" not in env:
        env["WAYLAND_DISPLAY"] = "wayland-0"
    # Ensure the GNOME/Ubuntu data paths so xdg-open finds the .desktop
    # of the default browser.
    if "XDG_DATA_DIRS" not in env:
        env["XDG_DATA_DIRS"] = "/usr/share/ubuntu:/usr/share/gnome:/usr/local/share/:/usr/share/"
    # Full PATH (systemd sometimes trims PATH and xdg-open finds no browser).
    env["PATH"] = (
        "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin:"
        + env.get("PATH", ""))
    return env


def _open_browser_for_oauth(stream_reader, stop_event):
    """Thread that reads rclone's output and opens the browser when it sees the
    local callback URL (http://127.0.0.1:53682/auth?...).

    rclone prints the URL on stderr; some environments fail to open it on
    their own (systemd services without BROWSER/WAYLAND_DISPLAY). This thread
    opens it explicitly with xdg-open using the graphical environment.
    """
    import re
    url_re = re.compile(r"http://127\.0\.0\.1:53682/auth\?state=\S+")
    buf = ""
    while not stop_event.is_set():
        ch = stream_reader.read(1)
        if not ch:
            break
        buf += ch
        # Process complete lines.
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
                return  # Opening it once is enough.


def rclone_sign_in(on_line=None):
    """Create the 'gdrive' remote (if absent) and launch OAuth in the browser.

    Robust flow:
      1. If the remote does not exist, create it with `rclone config create`
         (non-interactive: it only sets type=drive and scope=drive, no OAuth).
      2. Run `rclone config reconnect gdrive:` feeding answers through stdin:
         'y' (use the local browser) and 'n' (it is not a Team Drive).
      3. A separate thread watches rclone's output and opens the OAuth URL
         explicitly with xdg-open, in the graphical environment (fallback
         for systemd services without WAYLAND_DISPLAY/BROWSER).
      4. Verify with rclone_is_authorized().

    The optional `on_line(line)` receives each line to show UI progress.
    Returns True if the remote ended up authorized.
    """
    import threading
    name = RCLONE_REMOTE.rstrip(":")

    # 1) Create the remote entry if it does not exist (no OAuth).
    if not rclone_remote_exists():
        try:
            subprocess.run(
                ["rclone", "config", "create", name, "drive", "scope=drive",
                 "--non-interactive"],
                capture_output=True, text=True, timeout=15,
            )
        except Exception:
            pass

    # 2) reconnect + thread that opens the browser explicitly.
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
    """Deletes the rclone remote (config delete)."""
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
    """Project root directory (where launch.sh lives).

    helpers.py sits at <root>/gdrive/, so we go up one level.
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
    """Sends a desktop notification via GApplication.

    `app` must be the GDriveApp instance. If it is unavailable, no-op.
    """
    try:
        notif = Gio.Notification.new(title)
        notif.set_body(body)
        notif.set_priority(priority)
        app.send_notification(None, notif)
    except Exception:
        pass
