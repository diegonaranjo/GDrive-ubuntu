#!/bin/bash
#
# build-deb.sh — builds the installable .deb package.
#
# The package ships the application itself: the Python window and tray, the
# native daemon, the Nautilus extension, the icons and the desktop entries.
# Before this it shipped the launcher, the extension and the docs, but no app
# code, no daemon and an `Exec` still pointing at RUTA_DEL_PROYECTO, so it
# installed cleanly and launched nothing (issue #2).
#
# System-wide, not per-user: the package cannot own ~/.config/systemd/user.
# `setup.sh` stays the only path that generates the user units.
#
set -euo pipefail

PACKAGE_NAME="google-drive-ubuntu"
# DEBIAN/control is the single source of truth for the version.
VERSION="$(grep '^Version:' DEBIAN/control | cut -d' ' -f2)"
# The daemon is a compiled binary, so the package is architecture-specific.
# `DEBIAN/control` carries `any` as a placeholder and this script resolves it.
ARCH="$(dpkg --print-architecture)"

BUILD_DIR="build"
DEB_DIR="$BUILD_DIR/$PACKAGE_NAME"
APP_DIR="$DEB_DIR/usr/share/gdrive"
DAEMON_BIN="native/gdrive-core/target/release/gdrive-daemon"

echo "Building $PACKAGE_NAME v$VERSION ($ARCH)..."

# ── 1. The native daemon ────────────────────────────────────────────────────
if [ ! -x "$DAEMON_BIN" ]; then
    echo "Building the native daemon (cargo build --release)..."
    ( cd native/gdrive-core && cargo build --release --locked )
fi
if [ ! -x "$DAEMON_BIN" ]; then
    echo "error: $DAEMON_BIN not found after the build" >&2
    exit 1
fi

# ── 2. Staging tree ─────────────────────────────────────────────────────────
rm -rf "$BUILD_DIR"
mkdir -p "$DEB_DIR/DEBIAN" \
         "$APP_DIR" \
         "$DEB_DIR/usr/bin" \
         "$DEB_DIR/usr/share/applications" \
         "$DEB_DIR/usr/share/nautilus-python/extensions" \
         "$DEB_DIR/usr/share/doc/$PACKAGE_NAME" \
         "$DEB_DIR/usr/share/icons/hicolor/scalable/places" \
         "$DEB_DIR/usr/share/icons/hicolor/scalable/status" \
         "$DEB_DIR/usr/share/icons/hicolor/scalable/actions"

# ── 3. Application payload ──────────────────────────────────────────────────
cp -a gdrive "$APP_DIR/"
find "$APP_DIR/gdrive" -name '__pycache__' -type d -prune -exec rm -rf {} +
cp gdrive-app.py gdrive-tray.py launch.sh "$APP_DIR/"
cp -a theme "$APP_DIR/"
cp -a icons "$APP_DIR/"

# ── 4. Entry points ─────────────────────────────────────────────────────────
cp "$DAEMON_BIN" "$DEB_DIR/usr/bin/gdrive-daemon"
chmod 0755 "$DEB_DIR/usr/bin/gdrive-daemon"

cat > "$DEB_DIR/usr/bin/gdrive" <<'EOF'
#!/bin/sh
# Starts the Google Drive window. The engine is started by the launcher, which
# reuses an already running daemon instead of spawning a second one.
exec /bin/bash /usr/share/gdrive/launch.sh "$@"
EOF
chmod 0755 "$DEB_DIR/usr/bin/gdrive"

# ── 5. Icons, desktop entries, Nautilus extension ───────────────────────────
cp icons/google-drive.svg \
   "$DEB_DIR/usr/share/icons/hicolor/scalable/places/google-drive.svg"
cp icons/gdrive-*.svg \
   "$DEB_DIR/usr/share/icons/hicolor/scalable/status/" 2>/dev/null || true
if [ -d icons/lucide ]; then
    find icons/lucide -name '*.svg' \
        -exec cp -f {} "$DEB_DIR/usr/share/icons/hicolor/scalable/actions/" \;
fi

# `gdrive-settings.desktop` carries the `@GDRIVE_LAUNCHER@` placeholder: the
# launcher does not exist at a fixed path until the package is installed, and
# a desktop entry with an unresolved path is a menu item that does nothing.
# The build-time comments are stripped: they describe this script, not the
# installed application.
for f in desktop-files/*.desktop; do
    sed -e 's|@GDRIVE_LAUNCHER@|/usr/bin/gdrive|g' -e '/^#/d' "$f" \
        > "$DEB_DIR/usr/share/applications/$(basename "$f")"
done

cp nautilus-extension/goa_drive.py \
   "$DEB_DIR/usr/share/nautilus-python/extensions/goa_drive.py"

# ── 6. Documentation ────────────────────────────────────────────────────────
cp README.md "$DEB_DIR/usr/share/doc/$PACKAGE_NAME/README.md"
cp LICENSE "$DEB_DIR/usr/share/doc/$PACKAGE_NAME/copyright"
gzip -9 -n -c CHANGELOG.md > "$DEB_DIR/usr/share/doc/$PACKAGE_NAME/changelog.gz"

# ── 7. Control files ────────────────────────────────────────────────────────
sed "s|^Architecture:.*|Architecture: $ARCH|" DEBIAN/control \
    > "$DEB_DIR/DEBIAN/control"
cp DEBIAN/postinst DEBIAN/postrm "$DEB_DIR/DEBIAN/"
chmod 0755 "$DEB_DIR/DEBIAN/postinst" "$DEB_DIR/DEBIAN/postrm"

# ── 8. Build ────────────────────────────────────────────────────────────────
# --root-owner-group so the files are owned by root even when the build runs
# as a normal user, which is how every package must look once installed.
dpkg-deb --root-owner-group --build "$DEB_DIR" "$BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"

echo ""
echo "Package built: $BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"
echo "Verify it with: tools/check-deb.sh"
echo ""
echo "To install:"
echo "  sudo dpkg -i $BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"
echo "  sudo apt-get install -f  # Fix any missing dependencies"
echo ""
echo "The systemd user services are not created by the package. For the tray"
echo "and the supervised engine, run ./setup.sh from your clone as your user."
