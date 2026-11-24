#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n1)"
TOPDIR="$ROOT_DIR/packaging/rpm/build"
OUT_DIR="$ROOT_DIR/dist"
ARCH="$(uname -m)"

rm -rf "$TOPDIR"
mkdir -p "$TOPDIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS} "$OUT_DIR"

cd "$ROOT_DIR"
cargo build --workspace --release --locked

SPEC="$TOPDIR/SPECS/hypernova-ctl.spec"
cat > "$SPEC" <<EOF
Name:           hypernova-ctl
Version:        $VERSION
Release:        1
Summary:        Linux CLI, system tray, and GTK4 GUI for the Cosmic Byte Hypernova gaming mouse
License:        MIT
URL:            https://github.com/Neo-XD/hypernova-ctl
Requires:       systemd-libs, gtk4, libadwaita

%define _build_id_links none

%description
Configure DPI stages, USB polling rate (up to 8000 Hz wired / 4000 Hz wireless),
PAW3395 sensor settings, and wireless sleep timers on Linux without root.

%install
install -Dm755 "$ROOT_DIR/target/release/hypernova-ctl" %{buildroot}/usr/bin/hypernova-ctl
install -Dm755 "$ROOT_DIR/target/release/hypernova-gui" %{buildroot}/usr/bin/hypernova-gui
install -Dm644 "$ROOT_DIR/70-cosmic-byte-hypernova.rules" %{buildroot}/usr/lib/udev/rules.d/70-cosmic-byte-hypernova.rules
install -Dm644 "$ROOT_DIR/packaging/systemd/hypernova-tray.service" %{buildroot}/usr/lib/systemd/user/hypernova-tray.service
install -Dm644 "$ROOT_DIR/packaging/desktop/hypernova-gui.desktop" %{buildroot}/usr/share/applications/hypernova-gui.desktop
install -Dm644 "$ROOT_DIR/packaging/desktop/hypernova-gui.svg" %{buildroot}/usr/share/icons/hicolor/scalable/apps/hypernova-gui.svg
install -Dm644 "$ROOT_DIR/README.md" %{buildroot}/usr/share/doc/hypernova-ctl/README.md
install -Dm644 "$ROOT_DIR/LICENSE" %{buildroot}/usr/share/licenses/hypernova-ctl/LICENSE

%post
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || true
fi
if command -v systemctl >/dev/null 2>&1; then
    systemctl --global enable hypernova-tray.service >/dev/null 2>&1 || true
fi

%preun
if [ \$1 -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl --global disable hypernova-tray.service >/dev/null 2>&1 || true
fi

%files
/usr/bin/hypernova-ctl
/usr/bin/hypernova-gui
/usr/lib/udev/rules.d/70-cosmic-byte-hypernova.rules
/usr/lib/systemd/user/hypernova-tray.service
/usr/share/applications/hypernova-gui.desktop
/usr/share/icons/hicolor/scalable/apps/hypernova-gui.svg
/usr/share/doc/hypernova-ctl/README.md
/usr/share/licenses/hypernova-ctl/LICENSE
EOF

rpmbuild --define "_topdir $TOPDIR" --target "$ARCH" -bb "$SPEC"
cp "$TOPDIR/RPMS/$ARCH/"*.rpm "$OUT_DIR/"
rm -rf "$TOPDIR"
echo "Built RPM package in $OUT_DIR"
