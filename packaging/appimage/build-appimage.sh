#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APPDIR="$ROOT_DIR/packaging/appimage/AppDir"
TOOLS_DIR="$ROOT_DIR/packaging/appimage/tools"
OUT_DIR="$ROOT_DIR/dist"
ARCH="$(uname -m)"

mkdir -p "$TOOLS_DIR" "$OUT_DIR"
rm -rf "$APPDIR"

cd "$ROOT_DIR"
cargo build -p hypernova-gui --release --locked

LINUXDEPLOY="$(command -v linuxdeploy || echo "$TOOLS_DIR/linuxdeploy-$ARCH.AppImage")"
if [ ! -x "$LINUXDEPLOY" ]; then
    echo "Downloading linuxdeploy for $ARCH..."
    curl -fsSL -o "$LINUXDEPLOY" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage"
    chmod +x "$LINUXDEPLOY"
fi

APPIMAGE_EXTRACT_AND_RUN=1 \
OUTPUT="$OUT_DIR/hypernova-gui-$ARCH.AppImage" \
"$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --executable "$ROOT_DIR/target/release/hypernova-gui" \
    --desktop-file "$ROOT_DIR/packaging/desktop/hypernova-gui.desktop" \
    --icon-file "$ROOT_DIR/packaging/desktop/hypernova-gui.svg" \
    --output appimage

echo "Built AppImage at $OUT_DIR/hypernova-gui-$ARCH.AppImage"
