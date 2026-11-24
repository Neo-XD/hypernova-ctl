#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n1)"
STAGE="$ROOT_DIR/packaging/deb/stage"
OUT_DIR="$ROOT_DIR/dist"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

rm -rf "$STAGE"
mkdir -p "$STAGE/DEBIAN" "$OUT_DIR"

cd "$ROOT_DIR"
cargo build --workspace --release --locked

install -Dm755 "target/release/hypernova-ctl" "$STAGE/usr/bin/hypernova-ctl"
install -Dm755 "target/release/hypernova-gui" "$STAGE/usr/bin/hypernova-gui"
install -Dm644 "70-cosmic-byte-hypernova.rules" "$STAGE/usr/lib/udev/rules.d/70-cosmic-byte-hypernova.rules"
install -Dm644 "packaging/systemd/hypernova-tray.service" "$STAGE/usr/lib/systemd/user/hypernova-tray.service"
install -Dm644 "packaging/desktop/hypernova-gui.desktop" "$STAGE/usr/share/applications/hypernova-gui.desktop"
install -Dm644 "packaging/desktop/hypernova-gui.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/hypernova-gui.svg"
install -Dm644 "README.md" "$STAGE/usr/share/doc/hypernova-ctl/README.md"
install -Dm644 "LICENSE" "$STAGE/usr/share/doc/hypernova-ctl/LICENSE"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: hypernova-ctl
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: Neo-XD <amritanshu.praveen@gmail.com>
Depends: libudev1, libgtk-4-1, libadwaita-1-0
Homepage: https://github.com/Neo-XD/hypernova-ctl
Description: Linux CLI, system tray, and GTK4 GUI for the Cosmic Byte Hypernova gaming mouse
 Configure DPI stages, polling rate (up to 8000 Hz wired / 4000 Hz wireless),
 PAW3395 sensor settings, and wireless sleep timers on Linux without root.
EOF

cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || true
fi
if command -v systemctl >/dev/null 2>&1; then
    systemctl --global enable hypernova-tray.service >/dev/null 2>&1 || true
fi
EOF
chmod 755 "$STAGE/DEBIAN/postinst"

cat > "$STAGE/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1; then
    systemctl --global disable hypernova-tray.service >/dev/null 2>&1 || true
fi
EOF
chmod 755 "$STAGE/DEBIAN/prerm"

OUT_FILE="$OUT_DIR/hypernova-ctl_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$OUT_FILE"
rm -rf "$STAGE"
echo "Built Debian package at $OUT_FILE"
