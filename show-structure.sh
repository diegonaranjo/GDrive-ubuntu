#!/bin/bash
#
# Show structure of GDrive package
#

echo "=========================================="
echo "  GDrive Package Structure"
echo "=========================================="
echo

echo "Files created:"
echo ""

find . -type f -not -path "./.git/*" -not -path "./build/*" | sort | while read -r file; do
    echo "  $file"
done

echo
echo "=========================================="
echo "  Quick Start"
echo "=========================================="
echo

echo "Option 1: Direct installation"
echo "  sudo ./install.sh"
echo

echo "Option 2: Build .deb package"
echo "  make package"
echo "  sudo dpkg -i build/google-drive-ubuntu-$(grep '^Version:' DEBIAN/control | cut -d' ' -f2).deb"
echo

echo "Option 3: Docker test"
echo "  make test-install"
echo

echo "=========================================="
echo "  Verification"
echo "=========================================="
echo

echo "After installation, verify with:"
echo "  ./verify.sh"
echo

echo "=========================================="
