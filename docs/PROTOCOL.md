# Cosmic Byte Hypernova USB HID Protocol

Reverse-engineered from the vendor Windows driver artifacts in `Cosmic Byte Hypernova Gaming Mouse/Sys64/`:
- `HIDUsb.dll` (transport, packet framing, checksum, flash parser and writer)
- `CosmicByteHypernovaGamingMouse.pdb` (`MouseConfig` struct field layout)
- `functionset.obj`, `sensor.obj`, `sensorrate.obj`, `sensorlod.obj`, `debouncetime.obj`, `motionsync.obj`, `rippecontrol.obj`, `straightline.obj` (value encodings)
- `usersetting/Config.ini` and `Sys32/driver_sensor.h` (USB IDs, sensor capabilities)

## Hardware Identification

| Property | Value | Source |
|---|---|---|
| Vendor ID (VID) | `0x3554` (Compx) | `Config.ini` (`VID=MzU1NA==`) |
| Wired USB PID | `0xF5FA` | `Config.ini` (`USB_PID=RjVGQQo=`) |
| 4K Wireless Dongle PID | `0xF5FB` | `Config.ini` (`Wireless4_PID=RjVGQg==`) |
| USB Interface | `1` (`MI_01`, `col05` on Windows) | `Config.ini` (`InterfaceId=1`, `DeviceId=5`) |
| Sensor | PixArt PAW3395 | `Config.ini` (`Sensor=MzM5NQ==`) |
| Mouse MCU | Nordic nRF52833 | `Config.ini` (`MM=TlJGNTI4MzMgCgo=`) |
| 4K Dongle MCU | WCH CH32V305 | `Config.ini` (`D4M=Q0gzMlYzMDU=`) |

On Linux, `hidapi` with the `hidraw` backend opens the `/dev/hidraw*` node for USB interface `1` without detaching the kernel `usbhid` driver.

## Packet Framing

All host-to-device and device-to-host packets are 17-byte HID reports (Output Report to send, Input Report to receive) with Report ID `0x08`:

| Byte(s) | Meaning |
|---|---|
| `0` | Report ID (`0x08`) |
| `1` | Command ID |
| `2` | Reserved (`0x00`) |
| `3..=4` | Flash address (big-endian `u16`, for flash read/write; `0x0000` otherwise) |
| `5` | Payload length (0..=10) |
| `6..=15` | Payload bytes (zero-padded) |
| `16` | Packet checksum |

### Checksum (`HIDUsb.dll` `0x18000EC10`)

Both packet checksums and flash-field checksums use the same 8-bit function:

```text
checksum(bytes) = (0x55 - sum(bytes)) mod 256
```

- For a 17-byte HID packet, `pkt[16] = checksum(&pkt[0..16])` (including Report ID `0x08` at `pkt[0]`).
- A valid response satisfies `resp[1] == req[1]` and `resp[16] == checksum(&resp[0..16])`.

## Commands

| Cmd | Name | Request (`pkt[3..16]`) | Response (`resp[6..16]`) |
|---|---|---|---|
| `0x02` | Set PC Driver Status | `pkt[5]=1`, `pkt[6]=0\|1` | Echo |
| `0x03` | Get Device Online | All zeros | `resp[6] > 0` indicates mouse is awake/connected |
| `0x04` | Get Battery Level | All zeros | `resp[6]` = percent (`0..=100`), `resp[7]` = charging (`0\|1`), `resp[8..=9]` = voltage in mV (big-endian `u16`) |
| `0x07` | Write Flash | `pkt[3..=4]` = addr (BE), `pkt[5]` = len (`1..=10`), `pkt[6..6+len]` = data | Echo |
| `0x08` | Read Flash | `pkt[3..=4]` = addr (BE), `pkt[5]` = len (`1..=10`) | `resp[6..6+len]` = flash bytes |
| `0x09` | Factory Reset | All zeros | Echo |
| `0x0E` | Read Active Profile | All zeros | `resp[6]` = profile index |
| `0x0F` | Set Active Profile | `pkt[5]=1`, `pkt[6]=profile` | Echo |
| `0x12` | Get Firmware Version | All zeros | `resp[6..=7]` = version (big-endian `u16`) |

## Flash Memory Map (`0x0000..0x00BF`)

`UsbServer_ReadAllFlashData` reads 192 bytes (`0x0000..0x00BF`) in 10-byte chunks using command `0x08`. Each field in flash is stored as `(value_bytes..., checksum(value_bytes))`. If a field's stored checksum does not match, `MouseConfigParser` falls back to its built-in default.

| Flash Addr | Layout | Field (`MouseConfig`) | Encoding / Default |
|---|---|---|---|
| `0x00..=0x01` | 1B + 1B csum | `reportRate` | `0x08`=125 Hz, `0x04`=250 Hz, `0x02`=500 Hz, `0x01`=1000 Hz, `0x10`=2000 Hz, `0x20`=4000 Hz, `0x40`=8000 Hz (default `0x01`) |
| `0x02..=0x03` | 1B + 1B csum | `maxDPI` | Active DPI stage count `1..=6` (default `6`) |
| `0x04..=0x05` | 1B + 1B csum | `currentDPI` | Active DPI stage index `0..maxDPI` (default `2`) |
| `0x06..=0x07` | 1B + 1B csum | `xSpindown` | Default `0` |
| `0x08..=0x09` | 1B + 1B csum | `ySpindown` | Default `0` |
| `0x0A..=0x0B` | 1B + 1B csum | `silenceHeight` (LOD) | `1` = 1 mm, `2` = 2 mm |
| `0x0C + 4*i` (`i=0..7`) | 3B + 1B csum | `dpi[i]` | PAW3395 50-DPI step encoding (see below) |
| `0x2C + 4*i` (`i=0..7`) | 3B + 1B csum | `dpiColor[i]` | `R, G, B` bytes |
| `0xA9..=0xAA` | 1B + 1B csum | `keyDebounceTime` | Debounce in ms (`0..=30`, default `8`) |
| `0xAB..=0xAC` | 1B + 1B csum | `motionSyncEnable` | `0` = off, `1` = on (default `1`) |
| `0xAD..=0xAE` | 1B + 1B csum | `allLedOffTime` | 10-second ticks (default `6` = 60 s) |
| `0xAF..=0xB0` | 1B + 1B csum | `linearCorrectionEnable` | Angle snapping: `0` = off, `1` = on (default `0`) |
| `0xB1..=0xB2` | 1B + 1B csum | `rippleControlEnable` | `0` = off, `1` = on (default `0`) |
| `0xB3..=0xB4` | 1B + 1B csum | `moveOffLedEnable` | `0` = off, `1` = on (default `0`) |
| `0xB5..=0xB6` | 1B + 1B csum | `sensorCustomSleepTimeEnable` | `0` = off, `1` = on (default `0`) |
| `0xB7..=0xB8` | 1B + 1B csum | `sensorSleepTime` | 10-second ticks (`3`=30 s, `6`=1 min, `30`=5 min .. `240`=40 min; default `6`) |
| `0xB9..=0xBA` | 1B + 1B csum | `sensorPowerSavingModeEnable` | Sensor mode: `0` = LP (low power), `1` = HP (high performance) |

### PAW3395 DPI Encoding (`Sensor::WriteDPI` in `sensor.obj`)

Valid range: `50..=26000` DPI in increments of `50`:

```text
x_raw = (x_dpi / 50) - 1   // 0..=519 (10-bit)
y_raw = (y_dpi / 50) - 1   // 0..=519 (10-bit)

b0 = x_raw & 0xFF
b1 = y_raw & 0xFF
b2 = (((x_raw >> 8) & 0x03) << 2) | (((y_raw >> 8) & 0x03) << 6)
b3 = checksum([b0, b1, b2])
```

Decoding back:

```text
x_dpi = ((b0 | (((b2 >> 2) & 0x03) << 8)) + 1) * 50
y_dpi = ((b1 | (((b2 >> 6) & 0x03) << 8)) + 1) * 50
```
