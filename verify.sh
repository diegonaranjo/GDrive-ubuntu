#!/bin/bash
#
# Verification script for GDrive installation
#

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=========================================="
echo "  Verifying GDrive Installation"
echo "=========================================="
echo

check() {
    local name="$1"
    local command="$2"
    
    if eval "$command" &>/dev/null; then
        echo -e "${GREEN}[✓]${NC} $name"
        return 0
    else
        echo -e "${RED}[✗]${NC} $name"
        return 1
    fi
}

issues=0

echo "Checking dependencies..."
check "gnome-online-accounts" "dpkg -l gnome-online-accounts" || ((issues++))
check "nautilus" "dpkg -l nautilus" || ((issues++))
check "python3-nautilus" "dpkg -l python3-nautilus" || ((issues++))
check "rclone" "command -v rclone" || ((issues++))

echo
echo "Checking installed files..."
check "Nautilus extension" "test -f /usr/share/nautilus-python/extensions/goa_drive.py" || ((issues++))
check "Google Drive shortcut" "test -f /usr/share/applications/google-drive.desktop" || ((issues++))
check "Google Docs shortcut" "test -f /usr/share/applications/google-docs.desktop" || ((issues++))
check "Google Sheets shortcut" "test -f /usr/share/applications/google-sheets.desktop" || ((issues++))
check "Google Slides shortcut" "test -f /usr/share/applications/google-slides.desktop" || ((issues++))
check "Sync script" "test -x /usr/local/bin/gdrive-sync" || ((issues++))
check "Uninstaller" "test -x /usr/local/bin/google-drive-uninstall" || ((issues++))

REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(eval echo "~$REAL_USER")

echo
echo "Checking autostart..."
check "Autostart desktop file" "test -f $REAL_HOME/.config/autostart/google-drive-sync.desktop" || ((issues++))

echo
echo "Checking rclone configuration..."
if [ -f "$REAL_HOME/.config/rclone/rclone.conf" ] && grep -q "\[gdrive\]" "$REAL_HOME/.config/rclone/rclone.conf" 2>/dev/null; then
    echo -e "${GREEN}[✓]${NC} rclone gdrive configuration found"
else
    echo -e "${YELLOW}[!]${NC} rclone gdrive configuration not found (run: rclone config)"
    ((issues++))
fi

echo
echo "=========================================="
if [ $issues -eq 0 ]; then
    echo -e "${GREEN}All checks passed!${NC}"
    echo
    echo "Next steps:"
    echo "  1. Open Settings → Online Accounts"
    echo "  2. Add your Google account"
    echo "  3. Enable 'Files' option"
    echo "  4. Configure rclone: rclone config"
else
    echo -e "${YELLOW}Found $issues issue(s)${NC}"
    echo
    echo "Run 'sudo ./install.sh' to fix missing components"
fi
echo "=========================================="
