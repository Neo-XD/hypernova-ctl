use hypernova_ctl::protocol::{
    BatteryStatus, Lod, MouseConfig, PollingRate, RgbColor, SensorMode,
};
use hypernova_ctl::transport::ConnectionKind;

#[derive(Debug, Clone, Default)]
pub struct DeviceSnapshot {
    pub connection: Option<ConnectionKind>,
    pub online: bool,
    pub firmware: Option<u16>,
    pub battery: Option<BatteryStatus>,
    pub config: Option<MouseConfig>,
    pub permission_denied: bool,
}

#[derive(Debug, Clone)]
pub enum GuiCommand {
    Refresh,
    SetPollingRate(PollingRate),
    SetActiveDpi(u8),
    SetMaxDpiStages(u8),
    SetDpiStage { stage: u8, x: u16, y: u16 },
    SetDpiColor { stage: u8, color: RgbColor },
    SetLod(Lod),
    SetDebounce(u8),
    SetSensorMode(SensorMode),
    SetMotionSync(bool),
    SetRipple(bool),
    SetAngleSnapping(bool),
    SetSleep(Option<u16>),
    FactoryReset,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Snapshot(DeviceSnapshot),
    Toast(String),
}

impl DeviceSnapshot {
    pub fn header_subtitle(&self) -> String {
        let Some(conn) = self.connection else {
            return if self.permission_denied {
                "Permission denied on hidraw".to_string()
            } else {
                "Mouse disconnected".to_string()
            };
        };
        let conn_label = match conn {
            ConnectionKind::Wired => "Wired USB",
            ConnectionKind::Wireless4K => "4K Wireless",
        };
        let state = match (self.battery, self.online) {
            (Some(b), true) if b.charging => format!("{}% Charging", b.percent),
            (Some(b), true) => format!("{}% ({} mV)", b.percent, b.voltage_mv),
            (Some(b), false) => format!("{}% (Asleep)", b.percent),
            (None, false) => "Asleep".to_string(),
            (None, true) => "Online".to_string(),
        };
        match self.firmware {
            Some(fw) => format!("{conn_label} · {state} · FW 0x{fw:04X}"),
            None => format!("{conn_label} · {state}"),
        }
    }

    pub fn banner_message(&self) -> Option<&'static str> {
        if self.permission_denied {
            return Some("Permission denied on /dev/hidraw. Install 70-cosmic-byte-hypernova.rules and replug.");
        }
        if self.connection.is_none() {
            return Some("Mouse not detected. Plug in the mouse or 4K wireless dongle.");
        }
        if !self.online {
            return Some("Mouse is asleep. Move the mouse to wake it before applying changes.");
        }
        None
    }

    pub fn supported_rates(&self) -> &'static [PollingRate] {
        match self.connection {
            Some(kind) => kind.supported_polling_rates(),
            None => ConnectionKind::Wireless4K.supported_polling_rates(),
        }
    }

    pub fn active_rate_index(&self) -> u32 {
        let Some(ref cfg) = self.config else {
            return 0;
        };
        self.supported_rates()
            .iter()
            .position(|&r| r == cfg.polling_rate)
            .unwrap_or(0) as u32
    }
}

pub fn snap_dpi(value: u16) -> u16 {
    let clamped = value.clamp(50, 26_000);
    ((clamped + 25) / 50) * 50
}

pub fn snap_sleep_seconds(value: u16) -> u16 {
    let clamped = value.clamp(30, 2400);
    ((clamped + 5) / 10) * 10
}

pub fn rgb_to_f32(color: RgbColor) -> (f32, f32, f32) {
    (
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
    )
}

pub fn f32_to_rgb(r: f32, g: f32, b: f32) -> RgbColor {
    RgbColor {
        r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_helpers_clamp_and_align_to_hardware_steps() {
        assert_eq!(snap_dpi(10), 50);
        assert_eq!(snap_dpi(820), 800);
        assert_eq!(snap_dpi(830), 850);
        assert_eq!(snap_dpi(30_000), 26_000);

        assert_eq!(snap_sleep_seconds(5), 30);
        assert_eq!(snap_sleep_seconds(304), 300);
        assert_eq!(snap_sleep_seconds(306), 310);
        assert_eq!(snap_sleep_seconds(5000), 2400);
    }

    #[test]
    fn rgb_float_conversion_roundtrips_cleanly() {
        let color = RgbColor {
            r: 0,
            g: 255,
            b: 128,
        };
        let (r, g, b) = rgb_to_f32(color);
        assert_eq!(f32_to_rgb(r, g, b), color);
    }

    #[test]
    fn snapshot_banner_and_subtitle_reflect_device_state() {
        let disconnected = DeviceSnapshot::default();
        assert_eq!(disconnected.header_subtitle(), "Mouse disconnected");
        assert!(disconnected.banner_message().is_some());

        let online = DeviceSnapshot {
            connection: Some(ConnectionKind::Wireless4K),
            online: true,
            firmware: Some(0x0109),
            battery: Some(BatteryStatus {
                percent: 90,
                charging: false,
                voltage_mv: 4120,
            }),
            config: None,
            permission_denied: false,
        };
        assert_eq!(
            online.header_subtitle(),
            "4K Wireless · 90% (4120 mV) · FW 0x0109"
        );
        assert!(online.banner_message().is_none());
    }
}
