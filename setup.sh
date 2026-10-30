#!/bin/bash
#
# setup.sh — Instalación de usuario para GDrive
#
# Ejecutar tras clonar el repositorio. SIN sudo. Instala:
#   - Iconos (google-drive + Lucide) en ~/.local/share/icons
#   - Extensión Nautilus en ~/.local/share/nautilus-python/extensions
#   - Servicio systemd de usuario gdrive-tray.service
#   - Lanzador .desktop para el menú de aplicaciones (con la ruta real de este clone)
#
# Uso:
#   git clone <repo> ~/GDrive && cd ~/GDrive && ./setup.sh
#
set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'
log()  { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }

# ── 1. Comprobar dependencias ───────────────────────────────────────────────
check_dep() {
    if ! command -v "$1" >/dev/null 2>&1 && ! python3 -c "import gi; gi.require_version('$2','$3')" 2>/dev/null; then
        warn "Falta dependencia: $1 (gi $2 $3). Instálala antes de continuar."
    fi
}

log "Comprobando dependencias..."
python3 -c "import gi; gi.require_version('Gtk','4.0'); gi.require_version('Adw','1')" 2>/dev/null \
    || { warn "Falta GTK4/libadwaita. Instala: sudo apt install gtk4 libadwaita-1 gir1.2-gtk-4.0 gir1.2-adw-1"; }
command -v rclone >/dev/null 2>&1 \
    || warn "Falta rclone. Instala desde https://rclone.org/install/"
python3 -c "import gi; gi.require_version('AyatanaAppIndicator3','0.1')" 2>/dev/null \
    || warn "Falta AppIndicator3 (tray). Instala: sudo apt install gir1.2-ayatanaappindicator3-0.1"

# ── 2. Iconos ───────────────────────────────────────────────────────────────
ICONS_DIR="$HOME/.local/share/icons/hicolor/scalable"
log "Instalando iconos en $ICONS_DIR ..."
mkdir -p "$ICONS_DIR"/{actions,places,status,mimetypes,categories}

# Icono principal en places (para .desktop y bandeja)
cp -f "$PROJECT_DIR/icons/google-drive.svg" "$ICONS_DIR/places/google-drive.svg"

# Iconos de estado de GDrive
cp -f "$PROJECT_DIR"/icons/gdrive-*.svg "$ICONS_DIR/status/" 2>/dev/null || true

# Iconos Lucide symbolic
if [ -d "$PROJECT_DIR/icons/lucide" ]; then
    find "$PROJECT_DIR/icons/lucide" -name '*.svg' -exec cp -f {} "$ICONS_DIR/actions/" \; 2>/dev/null || true
fi
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
log "Iconos instalados"

# ── 2b. Daemon nativo (Rust) ────────────────────────────────────────────────
log "Instalando daemon nativo (gdrive-daemon)..."
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
DAEMON_SRC=""
for cand in \
    "$PROJECT_DIR/native/gdrive-core/target/release/gdrive-daemon" \
    "$PROJECT_DIR/native/gdrive-core/target/debug/gdrive-daemon"
do
    [ -x "$cand" ] && DAEMON_SRC="$cand" && break
done
if [ -n "$DAEMON_SRC" ]; then
    cp -f "$DAEMON_SRC" "$BIN_DIR/gdrive-daemon"
    chmod +x "$BIN_DIR/gdrive-daemon"
    log "Daemon instalado en $BIN_DIR/gdrive-daemon"
else
    warn "Binario gdrive-daemon no encontrado. Compílalo con:"
    warn "  cd native/gdrive-core && cargo build --release"
    warn "  ./setup.sh   (re-ejecuta después)"
fi

# ── 3. Extensión Nautilus ───────────────────────────────────────────────────
NAUT_EXT_DIR="$HOME/.local/share/nautilus-python/extensions"
if [ -d "$PROJECT_DIR/nautilus-extension" ]; then
    log "Instalando extensión Nautilus en $NAUT_EXT_DIR ..."
    mkdir -p "$NAUT_EXT_DIR"
    cp -f "$PROJECT_DIR/nautilus-extension/goa_drive.py" "$NAUT_EXT_DIR/"
fi

# ── 4. Servicio systemd de usuario (tray) ───────────────────────────────────
SYSTEMD_DIR="$HOME/.config/systemd/user"
log "Instalando servicio systemd en $SYSTEMD_DIR ..."
mkdir -p "$SYSTEMD_DIR"
cat > "$SYSTEMD_DIR/gdrive-tray.service" <<EOF
[Unit]
Description=GDrive Tray Icon
After=graphical-session.target

[Service]
ExecStart=/usr/bin/python3 $PROJECT_DIR/gdrive-tray.py
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
log "Servicio instalado. Habilítalo con: systemctl --user enable --now gdrive-tray.service"

# ── 5. Lanzador .desktop ────────────────────────────────────────────────────
APPS_DIR="$HOME/.local/share/applications"
log "Instalando lanzador en $APPS_DIR ..."
mkdir -p "$APPS_DIR"
cat > "$APPS_DIR/gdrive.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Drive
Comment=Cliente de escritorio para Google Drive
Exec=bash $PROJECT_DIR/launch.sh
Icon=google-drive
Terminal=false
Categories=Utility;FileTools;Network;
StartupNotify=true
Keywords=Google;Drive;Cloud;Sync;
EOF
update-desktop-database "$APPS_DIR" 2>/dev/null || true

# ── 6. Configurar rclone (interactivo) ──────────────────────────────────────
if command -v rclone >/dev/null 2>&1; then
    if ! rclone listremotes 2>/dev/null | grep -qx "gdrive:"; then
        warn "No se encontró el remote 'gdrive:' en rclone."
        warn "Para configurarlo ejecuta:  rclone config"
        warn "(Crea un remote llamado 'gdrive', tipo 'drive', y autoriza tu cuenta.)"
    else
        log "Remote 'gdrive:' ya configurado."
    fi
fi

echo
log "Setup completado."
echo
echo "  Carpeta de sync por defecto: \$HOME/GoogleDrive"
echo "  Configuración:               ~/.config/gdrive/settings.json"
echo
echo "  Lanzar la app ahora:         bash $PROJECT_DIR/launch.sh"
echo "  Iniciar el tray al login:    systemctl --user enable --now gdrive-tray.service"
echo
