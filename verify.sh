#!/bin/bash
#
# verify.sh — verifies a GDrive installation.
#
# There are two supported install paths and this script checks whichever is
# present:
#   1. the packaged .deb  → /usr/share/gdrive, /usr/bin/gdrive, /usr/bin/gdrive-daemon
#   2. `./setup.sh`       → ~/.local/bin/gdrive-daemon, ~/.local/share/...
#
# The old root installer (`install.sh`, /usr/local/bin/gdrive-sync, the
# ~/.config/autostart entry) is retired: its checks are informational and do
# not decide the result.
#
set -uo pipefail

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[1;33m'
NC=$'\033[0m'
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================="
echo "  Verifying GDrive Installation"
echo "=========================================="
echo

issues=0
check() {
    local name="$1"
    local command="$2"
    if eval "$command" &>/dev/null; then
        printf '%s[✓]%s %s\n' "$GREEN" "$NC" "$name"
        return 0
    fi
    printf '%s[✗]%s %s\n' "$RED" "$NC" "$name"
    issues=$((issues + 1))
    return 1
}
info() {
    local name="$1"
    local command="$2"
    if eval "$command" &>/dev/null; then
        printf '%s[✓]%s %s\n' "$GREEN" "$NC" "$name"
    else
        printf '%s[!]%s %s (not present)\n' "$YELLOW" "$NC" "$name"
    fi
}

# ── Dependencies ────────────────────────────────────────────────────────────
echo "Dependencies:"
check "python3" "command -v python3"
check "GTK4 (gir1.2-gtk-4.0)" "python3 -c \"import gi; gi.require_version('Gtk','4.0')\""
check "libadwaita (gir1.2-adw-1)" "python3 -c \"import gi; gi.require_version('Adw','1')\""
check "AppIndicator3 (tray)" "python3 -c \"import gi; gi.require_version('AyatanaAppIndicator3','0.1')\""
check "nautilus" "command -v nautilus"
check "python3-nautilus" "python3 -c \"import gi; gi.require_version('Nautilus','4.1')\""
check "rclone" "command -v rclone"
check "busctl" "command -v busctl"

# ── Install paths ───────────────────────────────────────────────────────────
echo
echo "Installation:"
packaged=0
user_install=0
[ -d /usr/share/gdrive ] && packaged=1
[ -f /usr/bin/gdrive ] && packaged=1
[ -x "$HOME/.local/bin/gdrive-daemon" ] && user_install=1
[ -f "$HOME/.local/share/nautilus-python/extensions/goa_drive.py" ] && user_install=1

if [ "$packaged" -eq 1 ]; then
    echo "  packaged (.deb):"
    check "  /usr/share/gdrive/gdrive-app.py" "test -f /usr/share/gdrive/gdrive-app.py"
    check "  /usr/share/gdrive/gdrive/i18n.py" "test -f /usr/share/gdrive/gdrive/i18n.py"
    check "  /usr/share/gdrive/theme/theme.css" "test -f /usr/share/gdrive/theme/theme.css"
    check "  /usr/bin/gdrive" "test -x /usr/bin/gdrive"
    check "  /usr/bin/gdrive-daemon" "test -x /usr/bin/gdrive-daemon"
    check "  Nautilus extension" "test -f /usr/share/nautilus-python/extensions/goa_drive.py"
    check "  Google Drive menu entry" "test -f /usr/share/applications/gdrive-settings.desktop"
fi

if [ "$user_install" -eq 1 ]; then
    echo "  user install (setup.sh):"
    check "  ~/.local/bin/gdrive-daemon" "test -x $HOME/.local/bin/gdrive-daemon"
    check "  Nautilus extension" "test -f $HOME/.local/share/nautilus-python/extensions/goa_drive.py"
    check "  icons" "test -f $HOME/.local/share/icons/hicolor/scalable/places/google-drive.svg"
fi

if [ "$packaged" -eq 0 ] && [ "$user_install" -eq 0 ]; then
    clone=0
    [ -x "$REPO_DIR/native/gdrive-core/target/release/gdrive-daemon" ] && clone=1
    if [ "$clone" -eq 1 ]; then
        echo "  clone build (this repository):"
        check "  target/release/gdrive-daemon" \
            "test -x $REPO_DIR/native/gdrive-core/target/release/gdrive-daemon"
        echo "  (install the .deb or run ./setup.sh for the packaged/user layout)"
    else
        echo "  no installation found"
        issues=$((issues + 1))
    fi
fi

# ── systemd user units ──────────────────────────────────────────────────────
echo
echo "User services:"
check "gdrive-daemon.service enabled" "systemctl --user is-enabled gdrive-daemon.service"
check "gdrive-tray.service enabled" "systemctl --user is-enabled gdrive-tray.service"

# A unit that is not part of graphical-session.target survives logout and dies
# with the display under it, restarting in a loop (AGENTS.md documents it).
for unit in gdrive-daemon.service gdrive-tray.service; do
    if systemctl --user cat "$unit" >/dev/null 2>&1; then
        partof="$(systemctl --user show "$unit" -p PartOf --value)"
        if [ "$partof" = "graphical-session.target" ]; then
            printf '%s[✓]%s %s is PartOf=graphical-session.target\n' "$GREEN" "$NC" "$unit"
        else
            printf '%s[✗]%s %s PartOf=%s (expected graphical-session.target)\n' \
                "$RED" "$NC" "$unit" "${partof:-<empty>}"
            issues=$((issues + 1))
        fi
    fi
done

# ── Runtime ─────────────────────────────────────────────────────────────────
echo
echo "Runtime:"
check "daemon owns com.gdrive.Client" "busctl --user status com.gdrive.Client"
if [ -f "$HOME/.config/rclone/rclone.conf" ] && grep -q '^\[gdrive\]' "$HOME/.config/rclone/rclone.conf"; then
    printf '%s[✓]%s rclone remote gdrive configured\n' "$GREEN" "$NC"
else
    printf '%s[✗]%s rclone remote gdrive not configured (run: rclone config)\n' "$RED" "$NC"
    issues=$((issues + 1))
fi

# ── Retired root installer (informational) ──────────────────────────────────
echo
echo "Legacy root installer (informational, retired):"
info "  /usr/local/bin/gdrive-sync" "test -e /usr/local/bin/gdrive-sync"
info "  autostart entry" "test -e $HOME/.config/autostart/google-drive-sync.desktop"

# ── Result ──────────────────────────────────────────────────────────────────
echo
echo "=========================================="
if [ "$issues" -eq 0 ]; then
    printf '%sAll checks passed!%s\n' "$GREEN" "$NC"
    echo
    echo "Next steps if the account is not configured yet:"
    echo "  1. rclone config          (create the 'gdrive' remote)"
    echo "  2. ./setup.sh             (tray + supervised engine, as your user)"
    echo "  3. gdrive                 (or the Google Drive menu entry)"
else
    printf '%sFound %s issue(s)%s\n' "$YELLOW" "$issues" "$NC"
    echo
    echo "To fix a packaged install: sudo apt install -f && sudo dpkg -i build/google-drive-ubuntu-*.deb"
    echo "To fix a user install:     ./setup.sh"
fi
echo "=========================================="

[ "$issues" -eq 0 ]
