#!/bin/bash
#
# Google Drive Client for Ubuntu
# Equivalent to GoogleDrive_arm64.pkg for macOS
# Compatible with Ubuntu 22.04+ with GNOME desktop
#

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[✗]${NC} $1"; exit 1; }

check_root() {
    if [ "$EUID" -ne 0 ]; then
        error "Run as root: sudo ./install.sh"
    fi
}

check_ubuntu() {
    if ! grep -qi "ubuntu" /etc/os-release 2>/dev/null; then
        warn "This script is designed for Ubuntu. It may not work on other distributions."
    fi
}

check_gnome() {
    if [ "$XDG_CURRENT_DESKTOP" != *"GNOME"* ] && [ "$XDG_CURRENT_DESKTOP" != *"ubuntu:GNOME"* ]; then
        warn "Detected a desktop other than GNOME. Native integration may not work."
    fi
}

update_system() {
    log "Updating package list..."
    apt-get update -qq
}

install_dependencies() {
    log "Installing dependencies..."
    apt-get install -y -qq \
        gnome-online-accounts \
        nautilus \
        python3-nautilus \
        gir1.2-goa-1.0 \
        gir1.2-nautilus-4.1 \
        dbus-x11 \
        xdg-utils \
        libglib2.0-bin \
        gnome-terminal \
        2>/dev/null
}

install_rclone() {
    log "Installing rclone for manual synchronization..."
    if ! command -v rclone &> /dev/null; then
        curl -fsSL https://rclone.org/install.sh | bash 2>/dev/null
        log "rclone installed successfully"
    else
        warn "rclone is already installed"
    fi
}

configure_gnome_online_accounts() {
    log "Configuring GNOME Online Accounts..."
    
    GOA_CONFIG_DIR="/etc/xdg/autostart"
    GOA_DESKTOP="$GOA_CONFIG_DIR/gnome-online-accounts-service.desktop"
    
    # Ensure GOA service starts with GNOME
    if [ ! -f "$GOA_DESKTOP" ]; then
        cat > "$GOA_DESKTOP" << 'EOF'
[Desktop Entry]
Type=Application
Name=GNOME Online Accounts
Exec=/usr/libexec/gnome-online-accounts-service
OnlyShowIn=GNOME;
X-GNOME-AutostartPhase=Initialization
X-GNOME-AutoRestart=true
EOF
    fi
    
    log "GNOME Online Accounts configured"
}

create_nautilus_extension() {
    log "Creating the Nautilus extension for Google Drive..."
    
    EXTENSION_DIR="/usr/share/nautilus-python/extensions"
    mkdir -p "$EXTENSION_DIR"
    
    cat > "$EXTENSION_DIR/goa_drive.py" << 'PYEOF'
import os
import subprocess
from gi.repository import Nautilus, GObject
from typing import List


class GoogleDriveExtension(GObject.GObject, Nautilus.MenuProvider):
    def _open_drive(self, *args):
        subprocess.Popen(["xdg-open", "drive:///?scheme=googledrive"])
    
    def _sync_drive(self, *args):
        subprocess.Popen(["gnome-terminal", "--", "rclone", "bisync", 
                         "gdrive:/", os.path.expanduser("~/GoogleDrive"), 
                         "--resync"])
    
    def get_file_items(self, files: List[Nautilus.FileInfo]) -> List[Nautilus.MenuItem]:
        return []
    
    def get_background_items(self, current_folder: Nautilus.FileInfo) -> List[Nautilus.MenuItem]:
        submenu = Nautilus.Menu()
        
        item_open = Nautilus.MenuItem(
            name="GoogleDriveExtension::open_drive",
            label="Abrir Google Drive",
        )
        item_open.connect("activate", self._open_drive)
        submenu.append_item(item_open)
        
        item_sync = Nautilus.MenuItem(
            name="GoogleDriveExtension::sync_drive",
            label="Sincronizar con rclone",
        )
        item_sync.connect("activate", self._sync_drive)
        submenu.append_item(item_sync)
        
        top_menuitem = Nautilus.MenuItem(
            name="GoogleDriveExtension::parent_menu",
            label="Google Drive",
        )
        top_menuitem.set_submenu(submenu)
        
        return [top_menuitem]
PYEOF
    
    log "Nautilus extension created"
}

create_shortcuts() {
    log "Creating shortcuts..."
    
    SHORTCUTS_DIR="/usr/share/applications"
    
    # Google Drive shortcut
    cat > "$SHORTCUTS_DIR/google-drive.desktop" << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Drive
Comment=Open Google Drive
Comment[es]=Acceder a Google Drive
Exec=xdg-open "drive:///?scheme=googledrive"
Icon=google-drive
Terminal=false
Categories=Utility;FileTools;
StartupNotify=true
EOF
    
    # Google Docs shortcut
    cat > "$SHORTCUTS_DIR/google-docs.desktop" << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Docs
Comment=Create Google documents
Comment[es]=Crear documentos de Google
Exec=xdg-open https://docs.google.com/document/create
Icon=google-docs
Terminal=false
Categories=Office;WordProcessor;
StartupNotify=true
EOF
    
    # Google Sheets shortcut
    cat > "$SHORTCUTS_DIR/google-sheets.desktop" << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Sheets
Comment=Create Google spreadsheets
Comment[es]=Crear hojas de cálculo de Google
Exec=xdg-open https://docs.google.com/spreadsheets/create
Icon=google-sheets
Terminal=false
Categories=Office;Spreadsheet;
StartupNotify=true
EOF
    
    # Google Slides shortcut
    cat > "$SHORTCUTS_DIR/google-slides.desktop" << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Google Slides
Comment=Create Google presentations
Comment[es]=Crear presentaciones de Google
Exec=xdg-open https://docs.google.com/presentation/create
Icon=google-slides
Terminal=false
Categories=Office;Presentation;
StartupNotify=true
EOF
    
    log "Shortcuts created"
}

create_autostart() {
    log "Configuring autostart..."
    
    REAL_USER="${SUDO_USER:-$USER}"
    REAL_HOME=$(eval echo "~$REAL_USER")
    
    AUTOSTART_DIR="$REAL_HOME/.config/autostart"
    mkdir -p "$AUTOSTART_DIR"
    
    cat > "$AUTOSTART_DIR/google-drive-sync.desktop" << EOF
[Desktop Entry]
Type=Application
Name=Google Drive Sync
Comment=Automatic Google Drive synchronization
Comment[es]=Sincronización automática de Google Drive
Exec=/usr/local/bin/gdrive-sync
Hidden=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-phase=Applications
EOF
    
    chown -R "$REAL_USER":"$REAL_USER" "$AUTOSTART_DIR/google-drive-sync.desktop"
    
    # Create sync script
    cat > "/usr/local/bin/gdrive-sync" << 'SYNEOF'
#!/bin/bash
# Google Drive background sync using rclone
# Checks for changes every 5 minutes

REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(eval echo "~$REAL_USER")
SYNC_DIR="$REAL_HOME/GoogleDrive"
LOG_FILE="/var/log/gdrive-sync.log"

if [ ! -d "$SYNC_DIR" ]; then
    mkdir -p "$SYNC_DIR"
    chown "$REAL_USER":"$REAL_USER" "$SYNC_DIR"
fi

while true; do
    if command -v rclone &>/dev/null; then
        rclone sync gdrive:/ "$SYNC_DIR" \
            --exclude ".DS_Store" \
            --exclude "._*" \
            --log-file="$LOG_FILE" \
            --log-level INFO 2>/dev/null
    fi
    sleep 300
done
SYNEOF
    
    chmod +x "/usr/local/bin/gdrive-sync"
    
    log "Autostart configured"
}

setup_rclone_config() {
    log "Configuring rclone for Google Drive..."
    
    REAL_USER="${SUDO_USER:-$USER}"
    REAL_HOME=$(eval echo "~$REAL_USER")
    
    RCLONE_CONFIG="$REAL_HOME/.config/rclone/rclone.conf"
    mkdir -p "$(dirname "$RCLONE_CONFIG")"
    
    if [ ! -f "$RCLONE_CONFIG" ] || ! grep -q "\[gdrive\]" "$RCLONE_CONFIG" 2>/dev/null; then
        cat >> "$RCLONE_CONFIG" << 'EOF'

[gdrive]
type = drive
scope = full
token = {"access_token":"","token_type":"","refresh_token":"","expiry":""}
trashed_only = false
EOF
        chown -R "$REAL_USER":"$REAL_USER" "$REAL_HOME/.config/rclone"
        warn "Configure rclone with: rclone config (select 'gdrive')"
    else
        warn "rclone configuration for gdrive already exists"
    fi
}

create_uninstaller() {
    log "Creating uninstaller..."
    
    cat > "/usr/local/bin/google-drive-uninstall" << 'UNINSTALLEOF'
#!/bin/bash
#
# Google Drive Uninstaller for Ubuntu
#

set -e

REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(eval echo "~$REAL_USER")

echo "Uninstalling Google Drive for Ubuntu..."

# Stop sync service
pkill -f gdrive-sync 2>/dev/null || true

# Remove autostart
rm -f "$REAL_HOME/.config/autostart/google-drive-sync.desktop"

# Remove Nautilus extension
rm -f /usr/share/nautilus-python/extensions/goa_drive.py

# Remove shortcuts
rm -f /usr/share/applications/google-drive.desktop
rm -f /usr/share/applications/google-docs.desktop
rm -f /usr/share/applications/google-sheets.desktop
rm -f /usr/share/applications/google-slides.desktop

# Remove sync script
rm -f /usr/local/bin/gdrive-sync
rm -f /usr/local/bin/google-drive-uninstall

# Remove log
rm -f /var/log/gdrive-sync.log

echo "Uninstallation complete"
echo "The files in ~/GoogleDrive have not been deleted"
echo "The rclone configuration is kept in ~/.config/rclone/"
UNINSTALLEOF
    
    chmod +x "/usr/local/bin/google-drive-uninstall"
    
    log "Uninstaller created at /usr/local/bin/google-drive-uninstall"
}

main() {
    echo "=========================================="
    echo "  Google Drive Client for Ubuntu"
    echo "  Version 1.1.0"
    echo "=========================================="
    echo
    
    check_root
    check_ubuntu
    check_gnome
    
    update_system
    install_dependencies
    install_rclone
    
    configure_gnome_online_accounts
    create_nautilus_extension
    create_shortcuts
    create_autostart
    setup_rclone_config
    create_uninstaller
    
    echo
    echo "=========================================="
    echo "  Installation complete"
    echo "=========================================="
    echo
    echo "Next steps:"
    echo "  1. Open 'Online Accounts' from System Settings"
    echo "  2. Add your Google account"
    echo "  3. Enable the 'Files' option"
    echo "  4. Google Drive will appear in Nautilus (Files)"
    echo
    echo "For automatic synchronization:"
    echo "  Configure rclone: rclone config"
    echo "  Select 'gdrive' and follow the instructions"
    echo
    echo "Shortcuts created:"
    echo "  - Google Drive"
    echo "  - Google Docs"
    echo "  - Google Sheets"
    echo "  - Google Slides"
    echo
    echo "To uninstall:"
    echo "  sudo google-drive-uninstall"
    echo
}

main "$@"
