use std::fmt;
use thiserror::Error;

pub const VENDOR_ID: u16 = 0x3554;
pub const PID_WIRED: u16 = 0xF5FA;
pub const PID_WIRELESS_4K: u16 = 0xF5FB;
pub const HID_INTERFACE_NUMBER: i32 = 1;

pub const REPORT_ID: u8 = 0x08;
pub const PACKET_LEN: usize = 17;
pub const MAX_FLASH_CHUNK: usize = 10;
pub const FLASH_CONFIG_LEN: usize = 0xC0;
pub const MAX_DPI_STAGES: usize = 6;
pub const TOTAL_DPI_SLOTS: usize = 8;

pub const CMD_SET_PC_DRIVER_STATUS: u8 = 0x02;
pub const CMD_GET_ONLINE: u8 = 0x03;
pub const CMD_GET_BATTERY: u8 = 0x04;
pub const CMD_WRITE_FLASH: u8 = 0x07;
pub const CMD_READ_FLASH: u8 = 0x08;
pub const CMD_CLEAR_SETTING: u8 = 0x09;
pub const CMD_READ_CONFIG: u8 = 0x0E;
pub const CMD_SET_CONFIG: u8 = 0x0F;
pub const CMD_GET_VERSION: u8 = 0x12;

pub mod flash_addr {
    pub const REPORT_RATE: u16 = 0x00;
    pub const MAX_DPI: u16 = 0x02;
    pub const CURRENT_DPI: u16 = 0x04;
    pub const SILENCE_HEIGHT: u16 = 0x0A;
    pub const DPI_TABLE_BASE: u16 = 0x0C;
    pub const DPI_COLOR_BASE: u16 = 0x2C;
    pub const DEBOUNCE_TIME: u16 = 0xA9;
    pub const MOTION_SYNC: u16 = 0xAB;
    pub const LED_OFF_TIME: u16 = 0xAD;
    pub const ANGLE_SNAPPING: u16 = 0xAF;
    pub const RIPPLE_CONTROL: u16 = 0xB1;
    pub const MOVE_OFF_LED: u16 = 0xB3;
    pub const CUSTOM_SLEEP_ENABLE: u16 = 0xB5;
    pub const SENSOR_SLEEP_TIME: u16 = 0xB7;
    pub const SENSOR_MODE: u16 = 0xB9;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("invalid response checksum: expected 0x{expected:02X}, got 0x{actual:02X}")]
    BadChecksum { expected: u8, actual: u8 },
    #[error("unexpected response command: expected 0x{expected:02X}, got 0x{actual:02X}")]
    UnexpectedCommand { expected: u8, actual: u8 },
    #[error("invalid polling rate {0} Hz (supported: 125, 250, 500, 1000, 2000, 4000, 8000)")]
    InvalidPollingRate(u16),
    #[error("polling rate {rate_hz} Hz is not supported on this connection (max {max_hz} Hz)")]
    UnsupportedPollingRateForConnection { rate_hz: u16, max_hz: u16 },
    #[error("invalid DPI {0} (must be 50..=26000 in steps of 50)")]
    InvalidDpi(u16),
    #[error("invalid DPI stage {0} (must be 1..={max})", max = MAX_DPI_STAGES)]
    InvalidStage(u8),
    #[error("invalid debounce time {0} ms (must be 0..=30)")]
    InvalidDebounce(u8),
    #[error("invalid sleep time {0} s (must be 30..=2400 in steps of 10)")]
    InvalidSleepSeconds(u16),
    #[error("flash chunk length {0} exceeds maximum {MAX_FLASH_CHUNK}")]
    ChunkTooLarge(usize),
}

// Compx 8-bit checksum used for both 17-byte HID packets and flash fields.
pub fn checksum(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    0x55u8.wrapping_sub(sum)
}

pub fn build_packet(cmd: u8, addr: u16, payload: &[u8]) -> Result<[u8; PACKET_LEN], ProtocolError> {
    if payload.len() > MAX_FLASH_CHUNK {
        return Err(ProtocolError::ChunkTooLarge(payload.len()));
    }
    let mut pkt = [0u8; PACKET_LEN];
    pkt[0] = REPORT_ID;
    pkt[1] = cmd;
    pkt[3] = (addr >> 8) as u8;
    pkt[4] = (addr & 0xFF) as u8;
    pkt[5] = payload.len() as u8;
    pkt[6..6 + payload.len()].copy_from_slice(payload);
    pkt[16] = checksum(&pkt[..16]);
    Ok(pkt)
}

pub fn build_simple_cmd(cmd: u8) -> [u8; PACKET_LEN] {
    let mut pkt = [0u8; PACKET_LEN];
    pkt[0] = REPORT_ID;
    pkt[1] = cmd;
    pkt[16] = checksum(&pkt[..16]);
    pkt
}

pub fn build_read_flash(addr: u16, len: u8) -> Result<[u8; PACKET_LEN], ProtocolError> {
    if usize::from(len) > MAX_FLASH_CHUNK {
        return Err(ProtocolError::ChunkTooLarge(usize::from(len)));
    }
    let mut pkt = [0u8; PACKET_LEN];
    pkt[0] = REPORT_ID;
    pkt[1] = CMD_READ_FLASH;
    pkt[3] = (addr >> 8) as u8;
    pkt[4] = (addr & 0xFF) as u8;
    pkt[5] = len;
    pkt[16] = checksum(&pkt[..16]);
    Ok(pkt)
}

pub fn build_write_flash_field(addr: u16, value: &[u8]) -> Result<[u8; PACKET_LEN], ProtocolError> {
    if value.len() + 1 > MAX_FLASH_CHUNK {
        return Err(ProtocolError::ChunkTooLarge(value.len() + 1));
    }
    let mut field = [0u8; MAX_FLASH_CHUNK];
    field[..value.len()].copy_from_slice(value);
    field[value.len()] = checksum(value);
    build_packet(CMD_WRITE_FLASH, addr, &field[..=value.len()])
}

pub fn validate_response(expected_cmd: u8, resp: &[u8; PACKET_LEN]) -> Result<(), ProtocolError> {
    let expected = checksum(&resp[..16]);
    if resp[16] != expected {
        return Err(ProtocolError::BadChecksum {
            expected,
            actual: resp[16],
        });
    }
    if resp[1] != expected_cmd {
        return Err(ProtocolError::UnexpectedCommand {
            expected: expected_cmd,
            actual: resp[1],
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingRate {
    Hz125 = 0x08,
    Hz250 = 0x04,
    Hz500 = 0x02,
    Hz1000 = 0x01,
    Hz2000 = 0x10,
    Hz4000 = 0x20,
    Hz8000 = 0x40,
}

pub const WIRED_POLLING_RATES: [PollingRate; 7] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
    PollingRate::Hz2000,
    PollingRate::Hz4000,
    PollingRate::Hz8000,
];

pub const WIRELESS_4K_POLLING_RATES: [PollingRate; 6] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
    PollingRate::Hz2000,
    PollingRate::Hz4000,
];

impl PollingRate {
    pub fn from_hz(hz: u16) -> Result<Self, ProtocolError> {
        match hz {
            125 => Ok(Self::Hz125),
            250 => Ok(Self::Hz250),
            500 => Ok(Self::Hz500),
            1000 => Ok(Self::Hz1000),
            2000 => Ok(Self::Hz2000),
            4000 => Ok(Self::Hz4000),
            8000 => Ok(Self::Hz8000),
            other => Err(ProtocolError::InvalidPollingRate(other)),
        }
    }

    pub fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            0x08 => Some(Self::Hz125),
            0x04 => Some(Self::Hz250),
            0x02 => Some(Self::Hz500),
            0x01 => Some(Self::Hz1000),
            0x10 => Some(Self::Hz2000),
            0x20 => Some(Self::Hz4000),
            0x40 => Some(Self::Hz8000),
            _ => None,
        }
    }

    pub fn hz(self) -> u16 {
        match self {
            Self::Hz125 => 125,
            Self::Hz250 => 250,
            Self::Hz500 => 500,
            Self::Hz1000 => 1000,
            Self::Hz2000 => 2000,
            Self::Hz4000 => 4000,
            Self::Hz8000 => 8000,
        }
    }

    pub fn step_up(self, supported: &[Self]) -> Self {
        if let Some(idx) = supported.iter().position(|&r| r == self) {
            supported[(idx + 1).min(supported.len() - 1)]
        } else {
            supported
                .iter()
                .copied()
                .find(|r| r.hz() > self.hz())
                .or_else(|| supported.last().copied())
                .unwrap_or(self)
        }
    }

    pub fn step_down(self, supported: &[Self]) -> Self {
        if let Some(idx) = supported.iter().position(|&r| r == self) {
            supported[idx.saturating_sub(1)]
        } else {
            supported
                .iter()
                .rev()
                .copied()
                .find(|r| r.hz() < self.hz())
                .or_else(|| supported.first().copied())
                .unwrap_or(self)
        }
    }

    pub fn cycle(self, supported: &[Self]) -> Self {
        if let Some(idx) = supported.iter().position(|&r| r == self) {
            supported[(idx + 1) % supported.len()]
        } else {
            supported
                .iter()
                .copied()
                .find(|r| r.hz() > self.hz())
                .or_else(|| supported.first().copied())
                .unwrap_or(self)
        }
    }

    pub fn manages_sensor_mode_internally(self) -> bool {
        matches!(self, Self::Hz2000 | Self::Hz4000 | Self::Hz8000)
    }
}

pub fn parse_polling_rate_field(bytes: [u8; 2]) -> PollingRate {
    if bytes[1] == checksum(&bytes[..1]) {
        PollingRate::from_wire(bytes[0]).unwrap_or(PollingRate::Hz1000)
    } else {
        PollingRate::Hz1000
    }
}

impl fmt::Display for PollingRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.hz())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    Mm1 = 1,
    Mm2 = 2,
}

impl Lod {
    pub fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Mm1),
            2 => Some(Self::Mm2),
            _ => None,
        }
    }
}

impl fmt::Display for Lod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mm1 => write!(f, "1 mm"),
            Self::Mm2 => write!(f, "2 mm"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorMode {
    LowPower = 0,
    HighPerformance = 1,
}

impl SensorMode {
    pub fn from_wire(byte: u8) -> Self {
        if byte == 0 {
            Self::LowPower
        } else {
            Self::HighPerformance
        }
    }
}

impl fmt::Display for SensorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowPower => write!(f, "LP (Low Power)"),
            Self::HighPerformance => write!(f, "HP (High Performance)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl fmt::Display for RgbColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiStage {
    pub x: u16,
    pub y: u16,
    pub color: RgbColor,
}

pub fn validate_dpi(dpi: u16) -> Result<u16, ProtocolError> {
    if !(50..=26000).contains(&dpi) || !dpi.is_multiple_of(50) {
        return Err(ProtocolError::InvalidDpi(dpi));
    }
    Ok(dpi)
}

pub fn encode_paw3395_dpi(x_dpi: u16, y_dpi: u16) -> Result<[u8; 3], ProtocolError> {
    let x_raw = (validate_dpi(x_dpi)? / 50) - 1;
    let y_raw = (validate_dpi(y_dpi)? / 50) - 1;
    let b0 = (x_raw & 0xFF) as u8;
    let b1 = (y_raw & 0xFF) as u8;
    let x_hi = ((x_raw >> 8) & 0x03) as u8;
    let y_hi = ((y_raw >> 8) & 0x03) as u8;
    let b2 = (x_hi << 2) | (y_hi << 6);
    Ok([b0, b1, b2])
}

pub fn decode_paw3395_dpi(bytes: [u8; 3]) -> (u16, u16) {
    let x_hi = u16::from((bytes[2] >> 2) & 0x03);
    let y_hi = u16::from((bytes[2] >> 6) & 0x03);
    let x_raw = u16::from(bytes[0]) | (x_hi << 8);
    let y_raw = u16::from(bytes[1]) | (y_hi << 8);
    ((x_raw + 1) * 50, (y_raw + 1) * 50)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    pub percent: u8,
    pub charging: bool,
    pub voltage_mv: u16,
}

pub fn parse_battery(resp: &[u8; PACKET_LEN]) -> Result<BatteryStatus, ProtocolError> {
    validate_response(CMD_GET_BATTERY, resp)?;
    Ok(BatteryStatus {
        percent: resp[6],
        charging: resp[7] != 0,
        voltage_mv: u16::from_be_bytes([resp[8], resp[9]]),
    })
}

pub fn parse_online(resp: &[u8; PACKET_LEN]) -> Result<bool, ProtocolError> {
    validate_response(CMD_GET_ONLINE, resp)?;
    Ok(resp[6] > 0)
}

pub fn parse_version(resp: &[u8; PACKET_LEN]) -> Result<u16, ProtocolError> {
    validate_response(CMD_GET_VERSION, resp)?;
    Ok(u16::from_be_bytes([resp[6], resp[7]]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseConfig {
    pub polling_rate: PollingRate,
    pub max_dpi_stages: u8,
    pub active_dpi_stage: u8,
    pub lod: Lod,
    pub debounce_ms: u8,
    pub motion_sync: bool,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    pub custom_sleep_enabled: bool,
    pub sleep_seconds: u16,
    pub sensor_mode: SensorMode,
    pub dpi_stages: [DpiStage; TOTAL_DPI_SLOTS],
}

fn read_u8_field(flash: &[u8; FLASH_CONFIG_LEN], addr: u16, default: u8) -> u8 {
    let idx = usize::from(addr);
    if flash[idx + 1] == checksum(&flash[idx..idx + 1]) {
        flash[idx]
    } else {
        default
    }
}

fn read_triple_field(flash: &[u8; FLASH_CONFIG_LEN], addr: u16, default: [u8; 3]) -> [u8; 3] {
    let idx = usize::from(addr);
    if flash[idx + 3] == checksum(&flash[idx..idx + 3]) {
        [flash[idx], flash[idx + 1], flash[idx + 2]]
    } else {
        default
    }
}

pub fn parse_flash_config(flash: &[u8; FLASH_CONFIG_LEN]) -> MouseConfig {
    let polling_rate = parse_polling_rate_field([
        flash[usize::from(flash_addr::REPORT_RATE)],
        flash[usize::from(flash_addr::REPORT_RATE) + 1],
    ]);
    let max_dpi_stages = read_u8_field(flash, flash_addr::MAX_DPI, MAX_DPI_STAGES as u8);
    let raw_active = read_u8_field(flash, flash_addr::CURRENT_DPI, 2);
    let active_dpi_stage = if raw_active < max_dpi_stages {
        raw_active
    } else {
        0
    };
    let lod = Lod::from_wire(read_u8_field(flash, flash_addr::SILENCE_HEIGHT, 1))
        .unwrap_or(Lod::Mm1);

    let mut dpi_stages = [DpiStage {
        x: 800,
        y: 800,
        color: RgbColor {
            r: 255,
            g: 255,
            b: 255,
        },
    }; TOTAL_DPI_SLOTS];

    for (i, slot) in dpi_stages.iter_mut().enumerate() {
        let dpi_bytes = read_triple_field(
            flash,
            flash_addr::DPI_TABLE_BASE + (i as u16) * 4,
            [0x0F, 0x0F, 0x00],
        );
        let (x, y) = decode_paw3395_dpi(dpi_bytes);
        let rgb = read_triple_field(
            flash,
            flash_addr::DPI_COLOR_BASE + (i as u16) * 4,
            [0xFF, 0xFF, 0xFF],
        );
        *slot = DpiStage {
            x,
            y,
            color: RgbColor {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            },
        };
    }

    MouseConfig {
        polling_rate,
        max_dpi_stages,
        active_dpi_stage,
        lod,
        debounce_ms: read_u8_field(flash, flash_addr::DEBOUNCE_TIME, 8),
        motion_sync: read_u8_field(flash, flash_addr::MOTION_SYNC, 1) > 0,
        angle_snapping: read_u8_field(flash, flash_addr::ANGLE_SNAPPING, 0) > 0,
        ripple_control: read_u8_field(flash, flash_addr::RIPPLE_CONTROL, 0) > 0,
        custom_sleep_enabled: read_u8_field(flash, flash_addr::CUSTOM_SLEEP_ENABLE, 0) > 0,
        sleep_seconds: u16::from(read_u8_field(flash, flash_addr::SENSOR_SLEEP_TIME, 6)) * 10,
        sensor_mode: SensorMode::from_wire(read_u8_field(flash, flash_addr::SENSOR_MODE, 0)),
        dpi_stages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_compx_formula() {
        let pkt = build_simple_cmd(CMD_GET_BATTERY);
        assert_eq!(pkt[0], 0x08);
        assert_eq!(pkt[1], 0x04);
        // 0x55 - (0x08 + 0x04) = 0x49
        assert_eq!(pkt[16], 0x49);
    }

    #[test]
    fn dpi_encode_decode_roundtrip_across_paw3395_range() {
        for dpi in [50u16, 400, 800, 1600, 3200, 12800, 12850, 25600, 26000] {
            let encoded = encode_paw3395_dpi(dpi, dpi).unwrap();
            assert_eq!(decode_paw3395_dpi(encoded), (dpi, dpi));
        }
        let asym = encode_paw3395_dpi(1600, 26000).unwrap();
        assert_eq!(decode_paw3395_dpi(asym), (1600, 26000));
    }

    #[test]
    fn dpi_rejects_out_of_range_and_non_50_steps() {
        assert!(encode_paw3395_dpi(0, 800).is_err());
        assert!(encode_paw3395_dpi(75, 800).is_err());
        assert!(encode_paw3395_dpi(26050, 800).is_err());
    }

    #[test]
    fn write_flash_field_includes_field_and_packet_checksums() {
        let pkt = build_write_flash_field(flash_addr::REPORT_RATE, &[PollingRate::Hz4000 as u8])
            .unwrap();
        assert_eq!(pkt[0], 0x08);
        assert_eq!(pkt[1], CMD_WRITE_FLASH);
        assert_eq!(pkt[3..5], [0x00, 0x00]);
        assert_eq!(pkt[5], 2);
        assert_eq!(pkt[6], 0x20);
        assert_eq!(pkt[7], checksum(&[0x20]));
        assert_eq!(pkt[16], checksum(&pkt[..16]));
    }

    #[test]
    fn parse_battery_extracts_percent_charging_and_millivolts() {
        let mut resp = [0u8; PACKET_LEN];
        resp[0] = REPORT_ID;
        resp[1] = CMD_GET_BATTERY;
        resp[6] = 85;
        resp[7] = 1;
        resp[8] = 0x0F;
        resp[9] = 0xA0; // 4000 mV
        resp[16] = checksum(&resp[..16]);

        let status = parse_battery(&resp).unwrap();
        assert_eq!(
            status,
            BatteryStatus {
                percent: 85,
                charging: true,
                voltage_mv: 4000
            }
        );
    }

    #[test]
    fn parse_flash_config_decodes_valid_fields_and_falls_back_on_bad_checksum() {
        let mut flash = [0xFFu8; FLASH_CONFIG_LEN];
        let write_u8 = |buf: &mut [u8; FLASH_CONFIG_LEN], addr: u16, val: u8| {
            let idx = usize::from(addr);
            buf[idx] = val;
            buf[idx + 1] = checksum(&[val]);
        };
        write_u8(&mut flash, flash_addr::REPORT_RATE, PollingRate::Hz2000 as u8);
        write_u8(&mut flash, flash_addr::MAX_DPI, 4);
        write_u8(&mut flash, flash_addr::CURRENT_DPI, 1);
        write_u8(&mut flash, flash_addr::SILENCE_HEIGHT, Lod::Mm2 as u8);
        write_u8(&mut flash, flash_addr::DEBOUNCE_TIME, 4);
        write_u8(&mut flash, flash_addr::MOTION_SYNC, 0);
        write_u8(&mut flash, flash_addr::ANGLE_SNAPPING, 1);
        write_u8(&mut flash, flash_addr::RIPPLE_CONTROL, 1);
        write_u8(&mut flash, flash_addr::CUSTOM_SLEEP_ENABLE, 1);
        write_u8(&mut flash, flash_addr::SENSOR_SLEEP_TIME, 30);
        write_u8(&mut flash, flash_addr::SENSOR_MODE, SensorMode::HighPerformance as u8);

        let dpi1 = encode_paw3395_dpi(1600, 1600).unwrap();
        flash[0x10..0x13].copy_from_slice(&dpi1);
        flash[0x13] = checksum(&dpi1);

        let cfg = parse_flash_config(&flash);
        assert_eq!(cfg.polling_rate, PollingRate::Hz2000);
        assert_eq!(cfg.max_dpi_stages, 4);
        assert_eq!(cfg.active_dpi_stage, 1);
        assert_eq!(cfg.lod, Lod::Mm2);
        assert_eq!(cfg.debounce_ms, 4);
        assert!(!cfg.motion_sync);
        assert!(cfg.angle_snapping);
        assert!(cfg.ripple_control);
        assert!(cfg.custom_sleep_enabled);
        assert_eq!(cfg.sleep_seconds, 300);
        assert_eq!(cfg.sensor_mode, SensorMode::HighPerformance);
        assert_eq!(cfg.dpi_stages[1].x, 1600);
        assert_eq!(cfg.dpi_stages[1].y, 1600);
    }

    #[test]
    fn polling_rate_step_clamps_and_cycles_within_supported_slice() {
        assert_eq!(PollingRate::Hz125.step_down(&WIRED_POLLING_RATES), PollingRate::Hz125);
        assert_eq!(PollingRate::Hz500.step_down(&WIRED_POLLING_RATES), PollingRate::Hz250);
        assert_eq!(PollingRate::Hz500.step_up(&WIRED_POLLING_RATES), PollingRate::Hz1000);
        assert_eq!(PollingRate::Hz4000.step_up(&WIRED_POLLING_RATES), PollingRate::Hz8000);
        assert_eq!(PollingRate::Hz8000.step_up(&WIRED_POLLING_RATES), PollingRate::Hz8000);
        assert_eq!(PollingRate::Hz8000.cycle(&WIRED_POLLING_RATES), PollingRate::Hz125);
        assert_eq!(PollingRate::Hz1000.step_up(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz2000);
        assert_eq!(PollingRate::Hz4000.step_up(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz4000);
        assert_eq!(PollingRate::Hz4000.cycle(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz125);
        // When flash holds a wired-only 8K rate while on the 4K dongle, stepping falls back into range.
        assert_eq!(PollingRate::Hz8000.step_up(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz4000);
        assert_eq!(PollingRate::Hz8000.step_down(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz4000);
        assert_eq!(PollingRate::Hz8000.cycle(&WIRELESS_4K_POLLING_RATES), PollingRate::Hz125);
    }

    #[test]
    fn polling_rate_identifies_firmware_managed_sensor_mode() {
        assert!(!PollingRate::Hz125.manages_sensor_mode_internally());
        assert!(!PollingRate::Hz1000.manages_sensor_mode_internally());
        assert!(PollingRate::Hz2000.manages_sensor_mode_internally());
        assert!(PollingRate::Hz4000.manages_sensor_mode_internally());
        assert!(PollingRate::Hz8000.manages_sensor_mode_internally());
    }

    #[test]
    fn parse_polling_rate_field_validates_checksum_and_falls_back() {
        let valid = [PollingRate::Hz4000 as u8, checksum(&[PollingRate::Hz4000 as u8])];
        assert_eq!(parse_polling_rate_field(valid), PollingRate::Hz4000);
        assert_eq!(parse_polling_rate_field([PollingRate::Hz4000 as u8, 0x00]), PollingRate::Hz1000);
    }
}
