#!/bin/bash
# Build arexibo DEB package
# Usage: ./deb/build-deb.sh <version> [release]
set -euo pipefail

VERSION="${1:?Usage: $0 <version> [release]}"

# Parse version-release (e.g. 0.3.1-2 → version=0.3.1, release=2)
# $2 from shared workflow overrides parsed release
BASE_VERSION="${VERSION%%-*}"
if [[ -n "${2:-}" ]]; then
  RELEASE="$2"
elif [[ "$VERSION" == *-* ]]; then
  RELEASE="${VERSION#*-}"
else
  RELEASE="1"
fi

# Detect architecture
ARCH=$(dpkg --print-architecture)

echo "Building arexibo ${BASE_VERSION}-${RELEASE} for ${ARCH}"

# Build Rust binary
export CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo build --release

# Create DEB package structure
PKG_DIR="deb-pkg/arexibo"
rm -rf deb-pkg
mkdir -p "${PKG_DIR}/DEBIAN"
mkdir -p "${PKG_DIR}/usr/bin"
mkdir -p "${PKG_DIR}/usr/share/doc/arexibo"
mkdir -p "${PKG_DIR}/usr/share/applications"
mkdir -p "${PKG_DIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${PKG_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${PKG_DIR}/usr/lib/systemd/user"

# Install files
install -m755 target/release/arexibo "${PKG_DIR}/usr/bin/arexibo"
install -m644 arexibo.service "${PKG_DIR}/usr/lib/systemd/user/arexibo.service"
install -m644 LICENSE "${PKG_DIR}/usr/share/doc/arexibo/"
install -m644 README.md "${PKG_DIR}/usr/share/doc/arexibo/"
install -m644 CHANGELOG.md "${PKG_DIR}/usr/share/doc/arexibo/"
install -m644 arexibo.desktop "${PKG_DIR}/usr/share/applications/"
install -m644 assets/arexibo-256.png "${PKG_DIR}/usr/share/icons/hicolor/256x256/apps/arexibo.png"
install -m644 assets/logo.svg "${PKG_DIR}/usr/share/icons/hicolor/scalable/apps/arexibo.svg"

# Create control file
cat > "${PKG_DIR}/DEBIAN/control" << EOF
Package: arexibo
Version: ${BASE_VERSION}-${RELEASE}
Section: misc
Priority: optional
Architecture: ${ARCH}
Depends: libqt6webenginecore6, libqt6webenginewidgets6, libdbus-1-3, libzmq5
Maintainer: Pau Aliagas <pau@linuxnow.com>
Description: Rust-based digital signage player for Xibo CMS
 Arexibo is a Rust-based digital signage player compatible with Xibo CMS.
 It provides a lightweight alternative to the official Xibo player,
 designed for kiosk and digital signage deployments on Linux.
Homepage: https://github.com/birkenfeld/arexibo
EOF

# Build DEB
mkdir -p dist
dpkg-deb --build "${PKG_DIR}" "dist/arexibo_${BASE_VERSION}-${RELEASE}_${ARCH}.deb"

echo "Built DEBs:"
ls -lh dist/*.deb

# Clean up
rm -rf deb-pkg
