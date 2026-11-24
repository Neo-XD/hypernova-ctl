use std::thread;
use std::time::Duration;

use crate::protocol::{
    build_read_flash, build_simple_cmd, build_write_flash_field, encode_paw3395_dpi, flash_addr,
    parse_battery, parse_flash_config, parse_online, parse_polling_rate_field, parse_version,
    BatteryStatus, Lod, MouseConfig, PollingRate, ProtocolError, RgbColor, SensorMode,
    CMD_CLEAR_SETTING, CMD_GET_BATTERY, CMD_GET_ONLINE, CMD_GET_VERSION, FLASH_CONFIG_LEN,
    MAX_DPI_STAGES, MAX_FLASH_CHUNK,
};
use crate::transport::{ConnectionKind, HidTransport, TransportError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingRateStep {
    Up,
    Down,
    Cycle,
}

pub struct HypernovaDevice<T: HidTransport> {
    transport: T,
}

impl<T: HidTransport> HypernovaDevice<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn connection_kind(&self) -> ConnectionKind {
        self.transport.connection_kind()
    }

    pub fn is_online(&self) -> Result<bool, TransportError> {
        let resp = self.transport.exchange(&build_simple_cmd(CMD_GET_ONLINE))?;
        Ok(parse_online(&resp)?)
    }

    pub fn wait_until_online<F>(
        &self,
        timeout: Duration,
        poll_interval: Duration,
        on_offline: F,
    ) -> Result<bool, TransportError>
    where
        F: FnMut(),
    {
        let max_attempts = if timeout.is_zero() || poll_interval.is_zero() {
            0
        } else {
            let attempts = timeout.as_millis().div_ceil(poll_interval.as_millis());
            usize::try_from(attempts).unwrap_or(usize::MAX)
        };
        self.wait_until_online_with(max_attempts, on_offline, || thread::sleep(poll_interval))
    }

    pub fn wait_until_online_with<F, S>(
        &self,
        max_attempts: usize,
        mut on_offline: F,
        mut wait_step: S,
    ) -> Result<bool, TransportError>
    where
        F: FnMut(),
        S: FnMut(),
    {
        if self.is_online()? {
            return Ok(false);
        }
        if max_attempts == 0 {
            return Err(TransportError::DeviceOffline);
        }
        on_offline();
        for _ in 0..max_attempts {
            wait_step();
            if self.is_online()? {
                return Ok(true);
            }
        }
        Err(TransportError::DeviceOffline)
    }

    pub fn battery(&self) -> Result<BatteryStatus, TransportError> {
        let resp = self
            .transport
            .exchange(&build_simple_cmd(CMD_GET_BATTERY))?;
        Ok(parse_battery(&resp)?)
    }

    pub fn firmware_version(&self) -> Result<u16, TransportError> {
        let resp = self
            .transport
            .exchange(&build_simple_cmd(CMD_GET_VERSION))?;
        Ok(parse_version(&resp)?)
    }

    pub fn read_config(&self) -> Result<MouseConfig, TransportError> {
        let mut flash = [0u8; FLASH_CONFIG_LEN];
        let mut addr: usize = 0;
        while addr < FLASH_CONFIG_LEN {
            let chunk = (FLASH_CONFIG_LEN - addr).min(MAX_FLASH_CHUNK);
            let req = build_read_flash(addr as u16, chunk as u8)?;
            let resp = self.transport.exchange(&req)?;
            flash[addr..addr + chunk].copy_from_slice(&resp[6..6 + chunk]);
            addr += chunk;
        }
        Ok(parse_flash_config(&flash))
    }

    pub fn read_polling_rate(&self) -> Result<PollingRate, TransportError> {
        let req = build_read_flash(flash_addr::REPORT_RATE, 2)?;
        let resp = self.transport.exchange(&req)?;
        Ok(parse_polling_rate_field([resp[6], resp[7]]))
    }

    pub fn set_polling_rate(&self, rate: PollingRate) -> Result<(), TransportError> {
        let supported = self.connection_kind().supported_polling_rates();
        if !supported.contains(&rate) {
            return Err(ProtocolError::UnsupportedPollingRateForConnection {
                rate_hz: rate.hz(),
                max_hz: self.connection_kind().max_polling_rate().hz(),
            }
            .into());
        }
        self.ensure_online()?;
        self.write_u8(flash_addr::REPORT_RATE, rate as u8)
    }

    pub fn step_polling_rate(
        &self,
        step: PollingRateStep,
    ) -> Result<(PollingRate, PollingRate), TransportError> {
        self.ensure_online()?;
        let current = self.read_polling_rate()?;
        let supported = self.connection_kind().supported_polling_rates();
        let next = match step {
            PollingRateStep::Up => current.step_up(supported),
            PollingRateStep::Down => current.step_down(supported),
            PollingRateStep::Cycle => current.cycle(supported),
        };
        if next != current {
            self.write_u8(flash_addr::REPORT_RATE, next as u8)?;
        }
        Ok((current, next))
    }

    pub fn set_max_dpi_stages(&self, count: u8) -> Result<(), TransportError> {
        if !(1..=MAX_DPI_STAGES as u8).contains(&count) {
            return Err(ProtocolError::InvalidStage(count).into());
        }
        self.write_u8(flash_addr::MAX_DPI, count)
    }

    pub fn set_active_dpi_stage(&self, stage_one_based: u8) -> Result<(), TransportError> {
        let idx = validate_stage(stage_one_based)?;
        self.write_u8(flash_addr::CURRENT_DPI, idx)
    }

    pub fn set_dpi_stage(
        &self,
        stage_one_based: u8,
        x_dpi: u16,
        y_dpi: u16,
    ) -> Result<(), TransportError> {
        let idx = validate_stage(stage_one_based)?;
        let encoded = encode_paw3395_dpi(x_dpi, y_dpi)?;
        let addr = flash_addr::DPI_TABLE_BASE + u16::from(idx) * 4;
        self.write_field(addr, &encoded)
    }

    pub fn set_dpi_color(&self, stage_one_based: u8, color: RgbColor) -> Result<(), TransportError> {
        let idx = validate_stage(stage_one_based)?;
        let addr = flash_addr::DPI_COLOR_BASE + u16::from(idx) * 4;
        self.write_field(addr, &[color.r, color.g, color.b])
    }

    pub fn set_lod(&self, lod: Lod) -> Result<(), TransportError> {
        self.write_u8(flash_addr::SILENCE_HEIGHT, lod as u8)
    }

    pub fn set_debounce_ms(&self, ms: u8) -> Result<(), TransportError> {
        if ms > 30 {
            return Err(ProtocolError::InvalidDebounce(ms).into());
        }
        self.write_u8(flash_addr::DEBOUNCE_TIME, ms)
    }

    pub fn set_motion_sync(&self, enabled: bool) -> Result<(), TransportError> {
        self.write_u8(flash_addr::MOTION_SYNC, u8::from(enabled))
    }

    pub fn set_angle_snapping(&self, enabled: bool) -> Result<(), TransportError> {
        self.write_u8(flash_addr::ANGLE_SNAPPING, u8::from(enabled))
    }

    pub fn set_ripple_control(&self, enabled: bool) -> Result<(), TransportError> {
        self.write_u8(flash_addr::RIPPLE_CONTROL, u8::from(enabled))
    }

    pub fn set_sensor_mode(&self, mode: SensorMode) -> Result<(), TransportError> {
        self.write_u8(flash_addr::SENSOR_MODE, mode as u8)
    }

    pub fn set_sleep_time(&self, seconds: Option<u16>) -> Result<(), TransportError> {
        match seconds {
            None => self.write_u8(flash_addr::CUSTOM_SLEEP_ENABLE, 0),
            Some(secs) => {
                if !(30..=2400).contains(&secs) || !secs.is_multiple_of(10) {
                    return Err(ProtocolError::InvalidSleepSeconds(secs).into());
                }
                self.write_u8(flash_addr::CUSTOM_SLEEP_ENABLE, 1)?;
                self.write_u8(flash_addr::SENSOR_SLEEP_TIME, (secs / 10) as u8)
            }
        }
    }

    pub fn factory_reset(&self) -> Result<(), TransportError> {
        self.transport
            .exchange(&build_simple_cmd(CMD_CLEAR_SETTING))?;
        Ok(())
    }

    fn ensure_online(&self) -> Result<(), TransportError> {
        if !self.is_online()? {
            return Err(TransportError::DeviceOffline);
        }
        Ok(())
    }

    fn write_u8(&self, addr: u16, value: u8) -> Result<(), TransportError> {
        self.write_field(addr, &[value])
    }

    fn write_field(&self, addr: u16, value: &[u8]) -> Result<(), TransportError> {
        let req = build_write_flash_field(addr, value)?;
        self.transport.exchange(&req)?;
        Ok(())
    }
}

fn validate_stage(stage_one_based: u8) -> Result<u8, ProtocolError> {
    if !(1..=MAX_DPI_STAGES as u8).contains(&stage_one_based) {
        return Err(ProtocolError::InvalidStage(stage_one_based));
    }
    Ok(stage_one_based - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{checksum, CMD_READ_FLASH, CMD_WRITE_FLASH, PACKET_LEN, REPORT_ID};
    use std::cell::{Cell, RefCell};

    struct FakeTransport {
        kind: ConnectionKind,
        offline_polls: Cell<usize>,
        flash: RefCell<[u8; FLASH_CONFIG_LEN]>,
        sent: RefCell<Vec<[u8; PACKET_LEN]>>,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self::with_offline_polls(ConnectionKind::Wireless4K, 0)
        }

        fn wired() -> Self {
            Self::with_offline_polls(ConnectionKind::Wired, 0)
        }

        fn offline() -> Self {
            Self::with_offline_polls(ConnectionKind::Wireless4K, usize::MAX)
        }

        fn waking_after(offline_polls: usize) -> Self {
            Self::with_offline_polls(ConnectionKind::Wireless4K, offline_polls)
        }

        fn with_offline_polls(kind: ConnectionKind, offline_polls: usize) -> Self {
            Self {
                kind,
                offline_polls: Cell::new(offline_polls),
                flash: RefCell::new([0xFFu8; FLASH_CONFIG_LEN]),
                sent: RefCell::new(Vec::new()),
            }
        }
    }

    impl HidTransport for FakeTransport {
        fn connection_kind(&self) -> ConnectionKind {
            self.kind
        }

        fn exchange(&self, req: &[u8; PACKET_LEN]) -> Result<[u8; PACKET_LEN], TransportError> {
            self.sent.borrow_mut().push(*req);
            let mut resp = [0u8; PACKET_LEN];
            resp[0] = REPORT_ID;
            resp[1] = req[1];
            match req[1] {
                CMD_GET_ONLINE => {
                    let remaining = self.offline_polls.get();
                    if remaining > 0 {
                        self.offline_polls.set(remaining.saturating_sub(1));
                        resp[6] = 0;
                    } else {
                        resp[6] = 1;
                    }
                }
                CMD_WRITE_FLASH => {
                    let addr = usize::from(u16::from_be_bytes([req[3], req[4]]));
                    let len = usize::from(req[5]);
                    self.flash.borrow_mut()[addr..addr + len].copy_from_slice(&req[6..6 + len]);
                }
                CMD_READ_FLASH => {
                    let addr = usize::from(u16::from_be_bytes([req[3], req[4]]));
                    let len = usize::from(req[5]);
                    resp[3] = req[3];
                    resp[4] = req[4];
                    resp[5] = req[5];
                    resp[6..6 + len].copy_from_slice(&self.flash.borrow()[addr..addr + len]);
                }
                _ => {}
            }
            resp[16] = checksum(&resp[..16]);
            Ok(resp)
        }
    }

    #[test]
    fn writes_and_reads_back_full_sensor_configuration() {
        let dev = HypernovaDevice::new(FakeTransport::new());

        dev.set_polling_rate(PollingRate::Hz4000).unwrap();
        dev.set_max_dpi_stages(4).unwrap();
        dev.set_active_dpi_stage(2).unwrap();
        dev.set_dpi_stage(2, 1600, 1600).unwrap();
        dev.set_dpi_color(2, RgbColor { r: 0, g: 255, b: 128 })
            .unwrap();
        dev.set_lod(Lod::Mm2).unwrap();
        dev.set_debounce_ms(4).unwrap();
        dev.set_motion_sync(true).unwrap();
        dev.set_angle_snapping(false).unwrap();
        dev.set_ripple_control(true).unwrap();
        dev.set_sensor_mode(SensorMode::HighPerformance).unwrap();
        dev.set_sleep_time(Some(300)).unwrap();

        let cfg = dev.read_config().unwrap();
        assert_eq!(cfg.polling_rate, PollingRate::Hz4000);
        assert_eq!(cfg.max_dpi_stages, 4);
        assert_eq!(cfg.active_dpi_stage, 1);
        assert_eq!(cfg.dpi_stages[1].x, 1600);
        assert_eq!(cfg.dpi_stages[1].y, 1600);
        assert_eq!(cfg.dpi_stages[1].color, RgbColor { r: 0, g: 255, b: 128 });
        assert_eq!(cfg.lod, Lod::Mm2);
        assert_eq!(cfg.debounce_ms, 4);
        assert!(cfg.motion_sync);
        assert!(!cfg.angle_snapping);
        assert!(cfg.ripple_control);
        assert_eq!(cfg.sensor_mode, SensorMode::HighPerformance);
        assert!(cfg.custom_sleep_enabled);
        assert_eq!(cfg.sleep_seconds, 300);
    }

    #[test]
    fn rejects_invalid_stage_debounce_and_sleep_inputs() {
        let dev = HypernovaDevice::new(FakeTransport::new());
        assert!(dev.set_active_dpi_stage(0).is_err());
        assert!(dev.set_active_dpi_stage(7).is_err());
        assert!(dev.set_debounce_ms(31).is_err());
        assert!(dev.set_sleep_time(Some(20)).is_err());
        assert!(dev.set_sleep_time(Some(35)).is_err());
        assert!(dev.set_sleep_time(Some(2410)).is_err());
    }

    #[test]
    fn wireless_4k_rejects_8k_while_wired_accepts_8k() {
        let wireless = HypernovaDevice::new(FakeTransport::new());
        let err_8k = wireless.set_polling_rate(PollingRate::Hz8000).unwrap_err();
        assert!(matches!(
            err_8k,
            TransportError::Protocol(ProtocolError::UnsupportedPollingRateForConnection {
                rate_hz: 8000,
                max_hz: 4000
            })
        ));
        assert!(wireless.transport.sent.borrow().is_empty());

        let wired = HypernovaDevice::new(FakeTransport::wired());
        wired.set_polling_rate(PollingRate::Hz8000).unwrap();
        assert_eq!(wired.read_polling_rate().unwrap(), PollingRate::Hz8000);
    }

    #[test]
    fn polling_rate_write_and_step_fail_when_mouse_is_offline() {
        let dev = HypernovaDevice::new(FakeTransport::offline());
        assert!(matches!(
            dev.set_polling_rate(PollingRate::Hz1000),
            Err(TransportError::DeviceOffline)
        ));
        assert!(matches!(
            dev.step_polling_rate(PollingRateStep::Up),
            Err(TransportError::DeviceOffline)
        ));
        assert!(dev
            .transport
            .sent
            .borrow()
            .iter()
            .all(|pkt| pkt[1] == CMD_GET_ONLINE));
    }

    #[test]
    fn read_polling_rate_reads_only_two_bytes_at_zero() {
        let dev = HypernovaDevice::new(FakeTransport::new());
        dev.set_polling_rate(PollingRate::Hz2000).unwrap();
        dev.transport.sent.borrow_mut().clear();

        assert_eq!(dev.read_polling_rate().unwrap(), PollingRate::Hz2000);
        let sent = dev.transport.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0][1], CMD_READ_FLASH);
        assert_eq!(sent[0][3..5], [0x00, 0x00]);
        assert_eq!(sent[0][5], 2);
    }

    #[test]
    fn step_polling_rate_clamps_skips_redundant_writes_and_cycles() {
        let wired = HypernovaDevice::new(FakeTransport::wired());
        wired.set_polling_rate(PollingRate::Hz8000).unwrap();
        wired.transport.sent.borrow_mut().clear();

        assert_eq!(
            wired.step_polling_rate(PollingRateStep::Up).unwrap(),
            (PollingRate::Hz8000, PollingRate::Hz8000)
        );
        assert!(wired
            .transport
            .sent
            .borrow()
            .iter()
            .all(|pkt| pkt[1] != CMD_WRITE_FLASH));

        assert_eq!(
            wired.step_polling_rate(PollingRateStep::Cycle).unwrap(),
            (PollingRate::Hz8000, PollingRate::Hz125)
        );
        assert_eq!(wired.read_polling_rate().unwrap(), PollingRate::Hz125);

        assert_eq!(
            wired.step_polling_rate(PollingRateStep::Down).unwrap(),
            (PollingRate::Hz125, PollingRate::Hz125)
        );

        let wireless = HypernovaDevice::new(FakeTransport::new());
        wireless.set_polling_rate(PollingRate::Hz1000).unwrap();
        assert_eq!(
            wireless.step_polling_rate(PollingRateStep::Up).unwrap(),
            (PollingRate::Hz1000, PollingRate::Hz2000)
        );
        assert_eq!(
            wireless.step_polling_rate(PollingRateStep::Up).unwrap(),
            (PollingRate::Hz2000, PollingRate::Hz4000)
        );
        assert_eq!(
            wireless.step_polling_rate(PollingRateStep::Cycle).unwrap(),
            (PollingRate::Hz4000, PollingRate::Hz125)
        );
    }

    #[test]
    fn wait_until_online_skips_prompt_when_awake_and_polls_when_asleep() {
        let awake = HypernovaDevice::new(FakeTransport::new());
        let mut prompted = 0;
        let mut sleeps = 0;
        let waited = awake
            .wait_until_online_with(5, || prompted += 1, || sleeps += 1)
            .unwrap();
        assert!(!waited);
        assert_eq!(prompted, 0);
        assert_eq!(sleeps, 0);

        let waking = HypernovaDevice::new(FakeTransport::waking_after(2));
        let mut prompted = 0;
        let mut sleeps = 0;
        let waited = waking
            .wait_until_online_with(5, || prompted += 1, || sleeps += 1)
            .unwrap();
        assert!(waited);
        assert_eq!(prompted, 1);
        assert_eq!(sleeps, 2);

        let sleeping = HypernovaDevice::new(FakeTransport::offline());
        let mut prompted = 0;
        let mut sleeps = 0;
        let err = sleeping
            .wait_until_online_with(3, || prompted += 1, || sleeps += 1)
            .unwrap_err();
        assert!(matches!(err, TransportError::DeviceOffline));
        assert_eq!(prompted, 1);
        assert_eq!(sleeps, 3);

        let no_wait = HypernovaDevice::new(FakeTransport::offline());
        let mut prompted = 0;
        let err = no_wait
            .wait_until_online_with(0, || prompted += 1, || {})
            .unwrap_err();
        assert!(matches!(err, TransportError::DeviceOffline));
        assert_eq!(prompted, 0);
    }
}
