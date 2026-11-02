#!/bin/bash
#
# setup.sh — User installation for GDrive
#
# Run after cloning the repository. NO sudo. It installs:
#   - Icons (google-drive + Lucide) into ~/.local/share/icons
#   - Nautilus extension into ~/.local/share/nautilus-python/extensions
#   - User systemd service gdrive-tray.service
#   - .desktop launcher for the applications menu (with this clone's real path)
#
# Usage:
#   git clone <repo> ~/GDrive && cd ~/GDrive && ./setup.sh
#
set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'
log()  { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }

# ── 1. Check dependencies ───────────────────────────────────────────────────
check_dep() {
    if ! command -v "$1" >/dev/null 2>&1 && ! python3 -c "import gi; gi.require_version('$2','$3')" 2>/dev/null; then
        warn "Missing dependency: $1 (gi $2 $3). Install it before continuing."
    fi
}

log "Comprobando dependencias..."
python3 -c "import gi; gi.require_version('Gtk','4.0'); gi.require_version('Adw','1')" 2>/dev/null \
    || { warn "Falta GTK4/libadwaita. Instala: sudo apt install gtk4 libadwaita-1 gir1.2-gtk-4.0 gir1.2-adw-1"; }
command -v rclone >/dev/null 2>&1 \
    || warn "Falta rclone. Instala desde https://rclone.org/install/"
python3 -c "import gi; gi.require_version('AyatanaAppIndicator3','0.1')" 2>/dev/null \
    || warn "Falta AppIndicator3 (tray). Instala: sudo apt install gir1.2-ayatanaappindicator3-0.1"

# ── 2. Icons ────────────────────────────────────────────────────────────────
ICONS_DIR="$HOME/.local/share/icons/hicolor/scalable"
log "Installing icons in $ICONS_DIR ..."
mkdir -p "$ICONS_DIR"/{actions,places,status,mimetypes,categories}

# Main icon in places (for the .desktop and the tray)
cp -f "$PROJECT_DIR/icons/google-drive.svg" "$ICONS_DIR/places/google-drive.svg"

# GDrive status icons
cp -f "$PROJECT_DIR"/icons/gdrive-*.svg "$ICONS_DIR/status/" 2>/dev/null || true

# Lucide symbolic icons
if [ -d "$PROJECT_DIR/icons/lucide" ]; then
    find "$PROJECT_DIR/icons/lucide" -name '*.svg' -exec cp -f {} "$ICONS_DIR/actions/" \; 2>/dev/null || true
fi
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
log "Icons installed"

# ── 2b. Native daemon (Rust) ────────────────────────────────────────────────
log "Installing daemon (gdrive-daemon)..."
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
    log "Daemon installed at $BIN_DIR/gdrive-daemon"
else
    warn "gdrive-daemon binary not found. Build it with:"
    warn "  cd native/gdrive-core && cargo build --release"
    warn "  ./setup.sh   (then run it again)"
fi

# ── 3. Nautilus extension ───────────────────────────────────────────────────
NAUT_EXT_DIR="$HOME/.local/share/nautilus-python/extensions"
if [ -d "$PROJECT_DIR/nautilus-extension" ]; then
    log "Installing Nautilus extension in $NAUT_EXT_DIR ..."
    mkdir -p "$NAUT_EXT_DIR"
    cp -f "$PROJECT_DIR/nautilus-extension/goa_drive.py" "$NAUT_EXT_DIR/"
fi

# ── 4. User systemd services (daemon + tray) ────────────────────────────────
SYSTEMD_DIR="$HOME/.config/systemd/user"
log "Installing systemd user services in $SYSTEMD_DIR ..."
mkdir -p "$SYSTEMD_DIR"

# The sync engine is supervised so a crash restarts it on its own. launch.sh and
# the UI's _ensure_daemon() look for this unit and start it instead of spawning
# an unsupervised copy of themselves.
DAEMON_EXEC="$BIN_DIR/gdrive-daemon"
[ -x "$DAEMON_EXEC" ] || DAEMON_EXEC="$DAEMON_SRC"
if [ -n "$DAEMON_EXEC" ] && [ -x "$DAEMON_EXEC" ]; then
    cat > "$SYSTEMD_DIR/gdrive-daemon.service" <<EOF
[Unit]
Description=GDrive native daemon (rclone sync engine)
After=graphical-session.target
PartOf=graphical-session.target

[Service]
ExecStart=$DAEMON_EXEC
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical-session.target
EOF
    log "Installed gdrive-daemon.service (ExecStart=$DAEMON_EXEC)"
else
    warn "No daemon binary found: gdrive-daemon.service was not installed."
fi

cat > "$SYSTEMD_DIR/gdrive-tray.service" <<EOF
[Unit]
Description=GDrive Tray Icon
After=graphical-session.target
PartOf=graphical-session.target

[Service]
ExecStart=/usr/bin/python3 $PROJECT_DIR/gdrive-tray.py
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical-session.target
EOF
systemctl --user daemon-reload
log "Enable them with: systemctl --user enable --now gdrive-daemon.service gdrive-tray.service"

# ── 5. .desktop launcher ────────────────────────────────────────────────────
APPS_DIR="$HOME/.local/share/applications"
log "Installing launcher in $APPS_DIR ..."
mkdir -p "$APPS_DIR"
cat > "$APPS_DIR/gdrive.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Drive
Comment=Google Drive desktop client
Comment[es]=Cliente de escritorio para Google Drive
Exec=bash $PROJECT_DIR/launch.sh
Icon=google-drive
Terminal=false
Categories=Utility;FileTools;Network;
StartupNotify=true
Keywords=Google;Drive;Cloud;Sync;
EOF
update-desktop-database "$APPS_DIR" 2>/dev/null || true

# ── 6. Configure rclone (interactive) ───────────────────────────────────────
if command -v rclone >/dev/null 2>&1; then
    if ! rclone listremotes 2>/dev/null | grep -qx "gdrive:"; then
        warn "rclone remote 'gdrive:' was not found."
        warn "To configure it run:  rclone config"
        warn "(Create a remote named 'gdrive', type 'drive', and authorize your account.)"
    else
        log "Remote 'gdrive:' already configured."
    fi
fi

echo
log "Setup complete."
echo
echo "  Default sync folder:  \$HOME/GoogleDrive"
echo "  Configuration:        ~/.config/gdrive/settings.json"
echo
echo "  Start the app now:    bash $PROJECT_DIR/launch.sh"
echo "  Start on login:       systemctl --user enable --now gdrive-daemon.service gdrive-tray.service"
echo
