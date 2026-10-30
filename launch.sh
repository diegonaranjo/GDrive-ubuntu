#!/bin/bash
# GDrive - Launcher
# Inicia el daemon nativo (Rust) y la ventana principal (GTK4).
# El icono de bandeja lo gestiona EXCLUSIVAMENTE el systemd user service
# `gdrive-tray.service` para evitar duplicados en el arranque.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Daemon nativo (Rust). El tray y la app lo detectan y respetan si ya corre.
DAEMON_BIN="$SCRIPT_DIR/native/gdrive-core/target/release/gdrive-daemon"
if [ ! -x "$DAEMON_BIN" ]; then
    # Fallback: debug o ubicación instalada.
    DAEMON_BIN="$SCRIPT_DIR/native/gdrive-core/target/debug/gdrive-daemon"
fi
# La prueba de "ya está corriendo" es la posesión del nombre D-Bus, no `pgrep
# -f`: el patrón de pgrep comparaba contra la línea de comando del shell que
# invoca este script, así que launch.sh "veía" procesos que no existían y se
# saltaba el arranque (medido: `busctl --user status` da 0 con dueño y 1 sin él).
if [ -x "$DAEMON_BIN" ] && ! busctl --user status com.gdrive.Client > /dev/null 2>&1; then
    "$DAEMON_BIN" >/dev/null 2>&1 &
fi

# 2. Ventana principal (solo si no hay una instancia ya corriendo).
if ! busctl --user status com.gdrive.ClientUI > /dev/null 2>&1; then
    python3 "$SCRIPT_DIR/gdrive-app.py" &
else
    # Reutilizar la ventana existente en lugar de abrir una segunda: activar
    # la instancia registrada vía D-Bus (com.gdrive.ClientUI).
    gapplication activate com.gdrive.ClientUI >/dev/null 2>&1 ||
        dbus-send --session --type=method_call \
            --dest=com.gdrive.ClientUI \
            /com/gdrive/ClientUI \
            org.freedesktop.Application.Activate >/dev/null 2>&1 || true
fi