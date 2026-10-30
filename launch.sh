#!/bin/bash
# GDrive - Launcher
# Starts the native daemon (Rust) and the main window (GTK4).
# The tray icon is handled EXCLUSIVELY by the systemd user service
# `gdrive-tray.service` to avoid duplicates at startup.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Native daemon (Rust). Tray and app detect it and respect a running one.
DAEMON_BIN="$SCRIPT_DIR/native/gdrive-core/target/release/gdrive-daemon"
if [ ! -x "$DAEMON_BIN" ]; then
    # Fallback: debug or installed location.
    DAEMON_BIN="$SCRIPT_DIR/native/gdrive-core/target/debug/gdrive-daemon"
fi
# The "already running" test is ownership of the D-Bus name, not `pgrep
# -f`: the pgrep pattern matched the command line of the shell calling
# this script, so launch.sh "saw" processes that did not exist and skipped
# startup (measured: `busctl --user status` gives 0 with owner, 1 without).
if [ -x "$DAEMON_BIN" ] && ! busctl --user status com.gdrive.Client > /dev/null 2>&1; then
    "$DAEMON_BIN" >/dev/null 2>&1 &
fi

# 2. Main window (only if there is no instance already running).
if ! busctl --user status com.gdrive.ClientUI > /dev/null 2>&1; then
    python3 "$SCRIPT_DIR/gdrive-app.py" &
else
    # Reuse the existing window instead of opening a second one: activate
    # the registered instance over D-Bus (com.gdrive.ClientUI).
    gapplication activate com.gdrive.ClientUI >/dev/null 2>&1 ||
        dbus-send --session --type=method_call \
            --dest=com.gdrive.ClientUI \
            /com/gdrive/ClientUI \
            org.freedesktop.Application.Activate >/dev/null 2>&1 || true
fi