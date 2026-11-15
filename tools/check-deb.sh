#!/bin/bash
#
# check-deb.sh — verifies that the built .deb contains a runnable application.
#
# The package used to ship the launcher and the documentation but no
# application code, no daemon binary and an `Exec` still pointing at
# RUTA_DEL_PROYECTO (issue #2). A .deb that installs cleanly and launches
# nothing is worse than no .deb at all, so this script asserts content, not
# merely that `dpkg-deb --build` exited 0.
#
# Usage: tools/check-deb.sh [path/to/package.deb]
#   Without an argument it builds the package first.
#
set -uo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_NAME="google-drive-ubuntu"
cd "$REPO_DIR" || exit 1

red()   { printf '\033[0;31m[✗]\033[0m %s\n' "$1"; }
green() { printf '\033[0;32m[✓]\033[0m %s\n' "$1"; }

DEB="${1:-}"
if [ -z "$DEB" ]; then
    if ! ./build-deb.sh >/dev/null 2>&1; then
        red "build-deb.sh failed; run it directly to see why"
        exit 1
    fi
    VERSION="$(grep '^Version:' DEBIAN/control | cut -d' ' -f2)"
    DEB="build/$PACKAGE_NAME-$VERSION.deb"
fi

if [ ! -f "$DEB" ]; then
    red "package not found: $DEB"
    exit 1
fi

ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT
dpkg-deb -x "$DEB" "$ROOT/fs" || { red "dpkg-deb -x failed on $DEB"; exit 1; }
dpkg-deb -e "$DEB" "$ROOT/meta" || { red "dpkg-deb -e failed on $DEB"; exit 1; }

failures=0
fail() { red "$1"; failures=$((failures + 1)); }
pass() { green "$1"; }

echo "Checking $DEB"
echo

# ── 1. Application code ─────────────────────────────────────────────────────
echo "Application payload:"
for path in \
    usr/share/gdrive/gdrive-app.py \
    usr/share/gdrive/gdrive-tray.py \
    usr/share/gdrive/launch.sh \
    usr/share/gdrive/theme/theme.css \
    usr/share/gdrive/gdrive/__init__.py \
    usr/share/gdrive/gdrive/config.py \
    usr/share/gdrive/gdrive/i18n.py \
    usr/share/gdrive/gdrive/helpers.py \
    usr/share/gdrive/gdrive/window.py \
    usr/share/gdrive/gdrive/constants.py \
    usr/share/gdrive/gdrive/daemon_client.py \
    usr/share/gdrive/gdrive/widgets.py \
    usr/share/gdrive/gdrive/theme.py
do
    [ -f "$ROOT/fs/$path" ] || fail "missing /$path"
done
for path in usr/share/gdrive/gdrive/pages; do
    [ -d "$ROOT/fs/$path" ] || fail "missing /$path"
done
[ -f "$ROOT/fs/usr/share/gdrive/gdrive/pages/status.py" ] \
    || fail "missing /usr/share/gdrive/gdrive/pages/status.py"
[ "$failures" -eq 0 ] && pass "application package staged"

# ── 2. Binaries and entry points ────────────────────────────────────────────
echo
echo "Entry points:"
[ -x "$ROOT/fs/usr/bin/gdrive" ] || fail "/usr/bin/gdrive missing or not executable"
[ -x "$ROOT/fs/usr/bin/gdrive-daemon" ] || fail "/usr/bin/gdrive-daemon missing or not executable"
if [ -x "$ROOT/fs/usr/bin/gdrive-daemon" ]; then
    if file "$ROOT/fs/usr/bin/gdrive-daemon" | grep -q 'ELF'; then
        pass "daemon is a compiled ELF binary"
    else
        fail "/usr/bin/gdrive-daemon is not an ELF binary"
    fi
fi
if [ -x "$ROOT/fs/usr/bin/gdrive" ]; then
    # The entry point starts the shared launcher, and the launcher starts the
    # window. Asserting the chain (not just the first file) keeps one startup
    # path shared with the clone install instead of duplicating it in a wrapper.
    launcher="$(grep -o '/usr/share/gdrive/[a-z-]*\.sh' "$ROOT/fs/usr/bin/gdrive" | head -1)"
    if [ -n "$launcher" ] && grep -q 'gdrive-app\.py' "$ROOT/fs$launcher" 2>/dev/null; then
        pass "/usr/bin/gdrive reaches the window through $launcher"
    elif grep -q 'gdrive-app\.py' "$ROOT/fs/usr/bin/gdrive"; then
        pass "/usr/bin/gdrive starts the window directly"
    else
        fail "/usr/bin/gdrive does not reach gdrive-app.py"
    fi
fi

# ── 3. Nautilus extension, icons, desktop entries, docs ─────────────────────
echo
echo "Integration and documentation:"
for path in \
    usr/share/nautilus-python/extensions/goa_drive.py \
    usr/share/applications/gdrive-settings.desktop \
    usr/share/applications/google-drive.desktop \
    usr/share/applications/google-docs.desktop \
    usr/share/applications/google-sheets.desktop \
    usr/share/applications/google-slides.desktop \
    usr/share/icons/hicolor/scalable/places/google-drive.svg \
    usr/share/icons/hicolor/scalable/status/gdrive-status-symbolic.svg \
    usr/share/doc/google-drive-ubuntu/README.md \
    usr/share/doc/google-drive-ubuntu/copyright
do
    [ -f "$ROOT/fs/$path" ] || fail "missing /$path"
done
if [ -d "$ROOT/fs/usr/share/icons/hicolor/scalable/actions" ]; then
    count=$(find "$ROOT/fs/usr/share/icons/hicolor/scalable/actions" -name '*.svg' | wc -l)
    [ "$count" -gt 0 ] || fail "no Lucide symbolic icons staged"
fi

# ── 4. Placeholders and retired dependencies ────────────────────────────────
echo
echo "Package hygiene:"
if grep -rn 'RUTA_DEL_PROYECTO' "$ROOT/fs" >/dev/null 2>&1; then
    fail "RUTA_DEL_PROYECTO reached the package (desktop Exec not substituted)"
    grep -rn 'RUTA_DEL_PROYECTO' "$ROOT/fs" | head -3 | sed 's/^/    /'
else
    pass "no RUTA_DEL_PROYECTO placeholder"
fi

if grep -rIn '@[A-Z_]\+@' "$ROOT/fs" >/dev/null 2>&1; then
    fail "unresolved @PLACEHOLDER@ token reached the package"
    grep -rIn '@[A-Z_]\+@' "$ROOT/fs" | head -3 | sed 's/^/    /'
else
    pass "no unresolved @PLACEHOLDER@ token"
fi

for retired in gnome-online-accounts gir1.2-goa-1.0; do
    if grep -q "$retired" "$ROOT/meta/control"; then
        fail "DEBIAN/control still depends on $retired (GOA dropped Drive file support)"
    fi
done

if grep -rq '__pycache__' <(find "$ROOT/fs" -name '__pycache__'); then
    fail "__pycache__ staged into the package"
else
    pass "no __pycache__ staged"
fi

if [ -d "$ROOT/fs/usr/local/bin" ]; then
    fail "package still installs into /usr/local/bin"
fi

# ── 5. Architecture ─────────────────────────────────────────────────────────
echo
echo "Metadata:"
pkg_arch="$(grep '^Architecture:' "$ROOT/meta/control" | awk '{print $2}')"
host_arch="$(dpkg --print-architecture)"
if [ "$pkg_arch" = "$host_arch" ]; then
    pass "Architecture: $pkg_arch (matches the host)"
elif [ "$pkg_arch" = "all" ]; then
    fail "Architecture: all, but the package ships a compiled daemon"
else
    fail "Architecture: $pkg_arch does not match the host ($host_arch)"
fi

# The desktop entry must launch something the package installed.
exec_line="$(grep '^Exec=' "$ROOT/fs/usr/share/applications/gdrive-settings.desktop" | cut -d= -f2-)"
exec_path="$(printf '%s' "$exec_line" | awk '{print $1}')"
if [ -n "$exec_path" ] && [ -e "$ROOT/fs$exec_path" ]; then
    pass "desktop Exec resolves inside the package: $exec_line"
else
    fail "desktop Exec does not resolve inside the package: '$exec_line'"
fi

if [ -f "$ROOT/meta/postinst" ] && [ -f "$ROOT/meta/postrm" ]; then
    pass "maintainer scripts present"
else
    fail "DEBIAN/postinst or DEBIAN/postrm missing"
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "All checks passed: $DEB"
    exit 0
fi
echo "$failures check(s) failed"
exit 1
