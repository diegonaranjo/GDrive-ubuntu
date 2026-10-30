"""D-Bus client for the native daemon (Rust), used by the GTK4 UI.

Phase 3: replaces the Python `SyncManager` as the engine source. The UI uses
this `DaemonClient` (same GObject signal interface as `SyncManager`, to avoid
touching `window.py`/`status.py`) and talks to `gdrive-daemon` over D-Bus.

If the daemon is not available, `available` is `False` and the methods are safe
no-ops; the UI may then opt for a fallback.
"""

import gi
gi.require_version("Gio", "2.0")
gi.require_version("GObject", "2.0")
from gi.repository import Gio, GLib, GObject

from .constants import DAEMON_ID, DAEMON_PATH, DAEMON_INTERFACE
from .i18n import t

DAEMON_BUS = Gio.BusType.SESSION


class DaemonClient(GObject.Object):
    """GObject → D-Bus adapter for the native daemon.

    Emits the same signals as SyncManager:
      - status-changed(str)
      - finished(bool, str)
      - progress(str, int, int)
      - speed-changed(str)
    """

    __gsignals__ = {
        "status-changed": (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        "finished": (GObject.SignalFlags.RUN_FIRST, None, (bool, str)),
        "progress": (GObject.SignalFlags.RUN_FIRST, None, (str, int, int)),
        "speed-changed": (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        "available-changed": (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
    }

    def __init__(self, config, app=None):
        super().__init__()
        self.config = config
        self.app = app
        self._status = "idle"
        self._available = False
        self._conn = None
        self._subs = {}
        self._checking = False

        ALIASES = {
            "idle": "status.idle",
            "syncing": "status.syncing",
            "paused": "status.paused",
            "error": "status.error",
        }
        GObject.type_register(DaemonClient)
        # Connect to the bus and check the daemon's availability.
        Gio.bus_get(DAEMON_BUS, None, self._on_bus_ready, None)

    # ── Properties ───────────────────────────────────────────────────────────
    @property
    def status(self):
        return self._status

    @status.setter
    def status(self, v):
        self._status = v
        GLib.idle_add(self.emit, "status-changed", v)

    @property
    def available(self):
        return self._available

    # ── D-Bus connection ─────────────────────────────────────────────────────
    def _on_bus_ready(self, obj, result, _userdata):
        try:
            self._conn = Gio.bus_get_finish(result)
        except Exception:
            return
        self._check_available()

    def _check_available(self):
        if self._checking:
            return
        self._checking = True
        # Query the name: if it has an owner, the daemon is alive.
        self._conn.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            GLib.Variant("(s)", (DAEMON_ID,)),
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
            self._on_has_owner,
            None,
        )

    def _on_has_owner(self, obj, result, _userdata):
        self._checking = False
        try:
            reply = self._conn.call_finish(result)
            has = reply.unpack()[0] if reply else False
        except Exception:
            has = False
        self._set_available(bool(has))

    def _set_available(self, available):
        if available == self._available:
            if available:
                self._subscribe()  # ensure subscription
            return
        self._available = available
        if available:
            self._subscribe()
            self.refresh_status()
        else:
            self.status = "idle"
        self.emit("available-changed", available)

    def _subscribe(self):
        """Subscribes to the daemon signals: StatusChanged, Progress,
        SpeedChanged and SyncFinished."""
        if self._conn is None:
            return
        subs = {
            "StatusChanged": self._on_status_signal,
            "Progress": self._on_progress_signal,
            "SpeedChanged": self._on_speed_signal,
            "SyncFinished": self._on_finished_signal,
        }
        for name, cb in subs.items():
            if self._subs.get(name):
                continue
            try:
                self._subs[name] = self._conn.signal_subscribe(
                    DAEMON_ID,
                    DAEMON_INTERFACE,
                    name,
                    DAEMON_PATH,
                    None,
                    Gio.DBusSignalFlags.NONE,
                    cb,
                    None,
                )
            except Exception:
                self._subs[name] = None

    def _on_status_signal(self, conn, sender, path, iface, signal, params, userdata):
        if params is None:
            return
        status, paused = params.unpack()
        self._apply_status(status, paused)

    def _on_progress_signal(self, conn, sender, path, iface, signal, params, userdata):
        try:
            text, transferred, total = params.unpack()
            transferred = int(transferred)
            total = int(total)
            GLib.idle_add(self.emit, "progress", text, transferred, total)
        except Exception:
            pass

    def _on_speed_signal(self, conn, sender, path, iface, signal, params, userdata):
        try:
            speed = params.unpack()[0]
            GLib.idle_add(self.emit, "speed-changed", speed)
        except Exception:
            pass

    def _on_finished_signal(self, conn, sender, path, iface, signal, params, userdata):
        try:
            ok, message = params.unpack()
            GLib.idle_add(self.emit, "finished", bool(ok), message)
            GLib.idle_add(self.emit, "speed-changed", "—")
        except Exception:
            pass

    def _apply_status(self, status, paused):
        self._status = status
        self.emit("status-changed", status)
        if status in ("idle", "paused", "error"):
            self.emit("speed-changed", "—")

    # ── Control methods (mirror of SyncManager) ─────────────────────────────
    def start_periodic(self):
        """Syncs `sync_interval`/`paused` with the daemon. The native daemon
        runs the periodic scheduling; here we only push the config."""
        self._push_interval()

    def stop_periodic(self):
        """No-op: the daemon re-evaluates `sync_interval` continuously."""
        pass

    def _push_interval(self):
        if not self._conn or not self._available:
            return
        # Push the keys that govern the engine to the native daemon. The daemon
        # re-evaluates the config continuously (scheduler/watcher), but we move
        # the changes forward so they take effect without waiting for a tick.
        changes = GLib.Variant(
            "a{ss}",
            {
                "sync_interval": self.config.get("sync_interval", "manual"),
                "sync_mode": self.config.get("sync_mode", "mirror"),
                "watch_changes": "true" if self.config.get("watch_changes", False) else "false",
            },
        )
        try:
            self._conn.call(
                DAEMON_ID,
                DAEMON_PATH,
                DAEMON_INTERFACE,
                "UpdateConfig",
                changes,
                None,
                Gio.DBusCallFlags.NONE,
                -1,
                None,
                self._on_simple_reply,
                "UpdateConfig",
            )
        except Exception as e:
            print(f"[daemon] UpdateConfig error: {e}")

    def start_sync(self):
        self._call_simple("SyncNow")

    def pause(self):
        self._call_simple("Pause")

    def resume(self):
        self._call_simple("Resume")

    def refresh_status(self):
        self._call_get_status()

    def _call_simple(self, method):
        if not self._conn or not self._available:
            return
        try:
            self._conn.call(
                DAEMON_ID,
                DAEMON_PATH,
                DAEMON_INTERFACE,
                method,
                None,
                None,
                Gio.DBusCallFlags.NONE,
                -1,
                None,
                self._on_simple_reply,
                method,
            )
        except Exception as e:
            print(f"[daemon] {method} error: {e}")

    def _on_simple_reply(self, obj, result, method):
        try:
            self._conn.call_finish(result)
        except Exception as e:
            print(f"[daemon] {method} reply error: {e}")

    def _call_get_status(self):
        if not self._conn or not self._available:
            return
        try:
            self._conn.call(
                DAEMON_ID,
                DAEMON_PATH,
                DAEMON_INTERFACE,
                "GetStatus",
                None,
                None,
                Gio.DBusCallFlags.NONE,
                -1,
                None,
                self._on_get_status,
                None,
            )
        except Exception:
            pass

    def _on_get_status(self, obj, result, _userdata):
        try:
            reply = self._conn.call_finish(result)
            d = reply.unpack()[0]
            status = d.get("status", "idle")
            paused = d.get("paused", "false") == "true"
            self._apply_status(status, paused)
        except Exception:
            pass

    # ── Cleanup ──────────────────────────────────────────────────────────────
    def close(self):
        if self._conn:
            for sid in self._subs.values():
                if sid:
                    try:
                        self._conn.signal_unsubscribe(sid)
                    except Exception:
                        pass
        self._subs = {}
        self._conn = None

    def _daemon_running(self):
        return self._available