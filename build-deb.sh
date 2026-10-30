#!/bin/bash
#
# Build script for GDrive .deb package
#

set -e

PACKAGE_NAME="google-drive-ubuntu"
# DEBIAN/control is the single source of truth for the version.
VERSION="$(grep '^Version:' DEBIAN/control | cut -d' ' -f2)"
BUILD_DIR="build"
DEB_DIR="$BUILD_DIR/$PACKAGE_NAME"

echo "Building $PACKAGE_NAME v$VERSION..."

# Clean previous build
rm -rf "$BUILD_DIR"
mkdir -p "$DEB_DIR/DEBIAN"
mkdir -p "$DEB_DIR/usr/local/bin"
mkdir -p "$DEB_DIR/usr/share/applications"
mkdir -p "$DEB_DIR/usr/share/nautilus-python/extensions"
mkdir -p "$DEB_DIR/usr/share/doc/$PACKAGE_NAME"

# Copy control file
cp DEBIAN/control "$DEB_DIR/DEBIAN/"

# Copy install script
cp install.sh "$DEB_DIR/usr/local/bin/google-drive-install"
chmod +x "$DEB_DIR/usr/local/bin/google-drive-install"

# Copy Nautilus extension
cp nautilus-extension/goa_drive.py "$DEB_DIR/usr/share/nautilus-python/extensions/"

# Copy desktop files
cp desktop-files/*.desktop "$DEB_DIR/usr/share/applications/"

# Copy documentation
cp README.md "$DEB_DIR/usr/share/doc/$PACKAGE_NAME/README.md"
cp LICENSE "$DEB_DIR/usr/share/doc/$PACKAGE_NAME/" 2>/dev/null || true

# Build .deb package
dpkg-deb --build "$DEB_DIR" "$BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"

echo ""
echo "Package built successfully: $BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"
echo ""
echo "To install:"
echo "  sudo dpkg -i $BUILD_DIR/$PACKAGE_NAME-$VERSION.deb"
echo "  sudo apt-get install -f  # Fix any missing dependencies"
