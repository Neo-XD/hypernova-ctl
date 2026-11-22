use hidapi::{HidApi, HidDevice};
use std::fmt;
use thiserror::Error;

use crate::protocol::{
    validate_response, PollingRate, ProtocolError, HID_INTERFACE_NUMBER, PACKET_LEN, PID_WIRED,
    PID_WIRELESS_4K, VENDOR_ID, WIRED_POLLING_RATES, WIRELESS_4K_POLLING_RATES,
};

const READ_TIMEOUT_MS: i32 = 150;
const MAX_RETRIES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    Wired,
    Wireless4K,
}

impl ConnectionKind {
    pub fn supported_polling_rates(self) -> &'static [PollingRate] {
        match self {
            Self::Wired => &WIRED_POLLING_RATES,
            Self::Wireless4K => &WIRELESS_4K_POLLING_RATES,
        }
    }

    pub fn max_polling_rate(self) -> PollingRate {
        match self {
            Self::Wired => PollingRate::Hz1000,
            Self::Wireless4K => PollingRate::Hz4000,
        }
    }
}

impl fmt::Display for ConnectionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wired => write!(f, "Wired USB (0x{:04X}:0x{:04X})", VENDOR_ID, PID_WIRED),
            Self::Wireless4K => write!(
                f,
                "4K Wireless Dongle (0x{:04X}:0x{:04X})",
                VENDOR_ID, PID_WIRELESS_4K
            ),
        }
    }
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("no Cosmic Byte Hypernova mouse or 4K dongle found (VID 0x3554, PID 0xF5FA/0xF5FB)")]
    DeviceNotFound,
    #[error("mouse is offline or asleep (move mouse to wake)")]
    DeviceOffline,
    #[error("HID communication failed: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("device did not respond after {0} attempts")]
    Timeout(usize),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

pub trait HidTransport {
    fn connection_kind(&self) -> ConnectionKind;
    fn exchange(&self, req: &[u8; PACKET_LEN]) -> Result<[u8; PACKET_LEN], TransportError>;
}

pub struct HidApiTransport {
    device: HidDevice,
    kind: ConnectionKind,
}

impl HidApiTransport {
    pub fn open() -> Result<Self, TransportError> {
        let api = HidApi::new()?;
        let candidate = api
            .device_list()
            .filter(|info| info.vendor_id() == VENDOR_ID)
            .filter(|info| matches!(info.product_id(), PID_WIRED | PID_WIRELESS_4K))
            .filter(|info| info.interface_number() == HID_INTERFACE_NUMBER)
            // Prefer vendor usage page (>= 0xFF00) when multiple collections are exposed per interface.
            .max_by_key(|info| u8::from(info.usage_page() >= 0xFF00))
            .ok_or(TransportError::DeviceNotFound)?;

        let kind = if candidate.product_id() == PID_WIRED {
            ConnectionKind::Wired
        } else {
            ConnectionKind::Wireless4K
        };
        let device = candidate.open_device(&api)?;
        Ok(Self { device, kind })
    }
}

impl HidTransport for HidApiTransport {
    fn connection_kind(&self) -> ConnectionKind {
        self.kind
    }

    fn exchange(&self, req: &[u8; PACKET_LEN]) -> Result<[u8; PACKET_LEN], TransportError> {
        let expected_cmd = req[1];
        for _ in 0..MAX_RETRIES {
            self.device.write(req)?;
            let mut buf = [0u8; PACKET_LEN];
            let n = self.device.read_timeout(&mut buf, READ_TIMEOUT_MS)?;
            if n == PACKET_LEN && validate_response(expected_cmd, &buf).is_ok() {
                return Ok(buf);
            }
        }
        Err(TransportError::Timeout(MAX_RETRIES))
    }
}
