# hypernova-ctl

Linux CLI and Rust library for configuring the Cosmic Byte Hypernova gaming mouse over USB (wired `0x3554:0xF5FA` and 4K wireless dongle `0x3554:0xF5FB`).

See [docs/PROTOCOL.md](docs/PROTOCOL.md) for the reverse-engineered HID protocol specification.

## Build on Linux

Install `libudev-dev` (Debian/Ubuntu) or `systemd-devel` (Fedora) for `hidapi` `linux-native` (`hidraw`), then build:

```sh
cargo build --release
```

The binary is written to `target/release/hypernova-ctl`.

## udev Rule (Non-Root Access)

Copy `99-cosmic-byte-hypernova.rules` into `/etc/udev/rules.d/` and reload udev:

```sh
sudo cp 99-cosmic-byte-hypernova.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Unplug and replug the mouse or 4K dongle after applying the rule.

## Usage

```sh
# Read connection state, firmware version, battery, and all sensor settings
hypernova-ctl status

# Set polling rate (125, 250, 500, 1000, 2000, 4000 Hz)
hypernova-ctl rate 4000

# Configure DPI stage 2 to 1600 DPI with green LED indicator and select it
hypernova-ctl dpi set 2 1600 --color 00FF00
hypernova-ctl dpi active 2
hypernova-ctl dpi stages 4

# Sensor and power settings
hypernova-ctl lod 1mm
hypernova-ctl debounce 4
hypernova-ctl sensor-mode hp
hypernova-ctl motion-sync on
hypernova-ctl ripple off
hypernova-ctl angle-snapping off
hypernova-ctl sleep 300
hypernova-ctl reset
```
