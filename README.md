# hypernova-ctl

Linux CLI and Rust library for configuring the Cosmic Byte Hypernova gaming mouse over USB (wired `0x3554:0xF5FA` and 4K wireless dongle `0x3554:0xF5FB`).

See [docs/PROTOCOL.md](docs/PROTOCOL.md) for the reverse-engineered HID protocol specification.

## Install on Linux

Install `libudev` development headers before building from source: `systemd-libs` on Arch/CachyOS, `libudev-dev` on Debian/Ubuntu, or `systemd-devel` on Fedora.

### Arch / CachyOS (`makepkg`)

```sh
cd packaging/arch
makepkg -si
```

This installs `/usr/bin/hypernova-ctl` and `/usr/lib/udev/rules.d/99-cosmic-byte-hypernova.rules`. Pacman reloads udev rules automatically. Unplug and replug the mouse or 4K dongle after installing.

### Any Linux Distro (`make`)

```sh
make
sudo make install
```

This builds the stripped release binary, installs `/usr/local/bin/hypernova-ctl` and `/etc/udev/rules.d/99-cosmic-byte-hypernova.rules`, and reloads udev. Override `PREFIX` or `DESTDIR` if needed. Run `sudo make uninstall` to remove both files. Unplug and replug the mouse or 4K dongle after installing.

### Cargo User Install

```sh
cargo install --path .
sudo cp 99-cosmic-byte-hypernova.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Unplug and replug the mouse or 4K dongle after applying the udev rule.

## Usage

```sh
# Read connection state, firmware version, battery, and all sensor settings
hypernova-ctl status

# Wait up to 5s by default if the wireless mouse is asleep (use --wait 0 to fail immediately)
hypernova-ctl --wait 10 rate 4000

# Read or adjust polling rate (wired: 125..1000 Hz, 4K dongle: 125..4000 Hz)
hypernova-ctl rate
hypernova-ctl rate 4000
hypernova-ctl rate up
hypernova-ctl rate down
hypernova-ctl rate cycle
hypernova-ctl rate max

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
