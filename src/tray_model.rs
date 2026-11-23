use crate::protocol::{BatteryStatus, DpiStage, MouseConfig, PollingRate};
use crate::transport::ConnectionKind;

pub const ICON_SIZE: i32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    Online {
        connection: ConnectionKind,
        battery: BatteryStatus,
        config: MouseConfig,
    },
    Asleep {
        connection: ConnectionKind,
    },
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconBadge {
    Percent {
        value: u8,
        charging: bool,
        asleep: bool,
    },
    Unknown {
        asleep: bool,
    },
    Disconnected,
}

#[derive(Debug, Clone, Default)]
pub struct TrayState {
    pub connection: Option<ConnectionKind>,
    pub online: bool,
    pub battery: Option<BatteryStatus>,
    pub config: Option<MouseConfig>,
    pub last_notice: Option<String>,
}

impl TrayState {
    pub fn apply_poll(&mut self, outcome: PollOutcome) {
        match outcome {
            PollOutcome::Online {
                connection,
                battery,
                config,
            } => {
                self.connection = Some(connection);
                self.online = true;
                self.battery = Some(battery);
                self.config = Some(config);
                self.last_notice = None;
            }
            PollOutcome::Asleep { connection } => {
                self.connection = Some(connection);
                self.online = false;
            }
            PollOutcome::Disconnected => {
                self.connection = None;
                self.online = false;
                self.battery = None;
                self.config = None;
            }
        }
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.last_notice = Some(message.into());
    }

    pub fn supported_rates(&self) -> &'static [PollingRate] {
        match self.connection {
            Some(kind) => kind.supported_polling_rates(),
            None => ConnectionKind::Wireless4K.supported_polling_rates(),
        }
    }

    pub fn active_rate_index(&self) -> usize {
        let Some(ref cfg) = self.config else {
            return 0;
        };
        self.supported_rates()
            .iter()
            .position(|&rate| rate == cfg.polling_rate)
            .unwrap_or(0)
    }

    pub fn enabled_dpi_stages(&self) -> Vec<(u8, DpiStage)> {
        let Some(ref cfg) = self.config else {
            return Vec::new();
        };
        let count = usize::from(cfg.max_dpi_stages.clamp(1, 6));
        (0..count)
            .map(|i| ((i + 1) as u8, cfg.dpi_stages[i]))
            .collect()
    }

    pub fn active_dpi_index(&self) -> usize {
        let Some(ref cfg) = self.config else {
            return 0;
        };
        let count = usize::from(cfg.max_dpi_stages.clamp(1, 6));
        usize::from(cfg.active_dpi_stage).min(count.saturating_sub(1))
    }

    pub fn icon_badge(&self) -> IconBadge {
        if self.connection.is_none() {
            return IconBadge::Disconnected;
        }
        match self.battery {
            Some(batt) => IconBadge::Percent {
                value: batt.percent.min(100),
                charging: batt.charging,
                asleep: !self.online,
            },
            None => IconBadge::Unknown {
                asleep: !self.online,
            },
        }
    }

    pub fn status_summary(&self) -> String {
        let Some(conn) = self.connection else {
            return "Hypernova: Disconnected".to_string();
        };
        let label = short_connection_label(conn);
        match (self.battery, self.online) {
            (Some(batt), true) if batt.charging => {
                format!("{}% (Charging) · {label}", batt.percent)
            }
            (Some(batt), true) => format!("{}% · {label}", batt.percent),
            (Some(batt), false) => format!("{}% (Asleep) · {label}", batt.percent),
            (None, false) => format!("Asleep · {label}"),
            (None, true) => format!("Online · {label}"),
        }
    }

    pub fn tooltip_title(&self) -> String {
        match (self.battery, self.connection, self.online) {
            (Some(batt), Some(_), true) if batt.charging => {
                format!("Hypernova: {}% (Charging)", batt.percent)
            }
            (Some(batt), Some(_), true) => format!("Hypernova: {}%", batt.percent),
            (Some(batt), Some(_), false) => format!("Hypernova: {}% (Asleep)", batt.percent),
            (None, Some(_), false) => "Hypernova: Asleep".to_string(),
            (None, Some(_), true) => "Hypernova: Online".to_string(),
            (_, None, _) => "Hypernova: Disconnected".to_string(),
        }
    }

    pub fn tooltip_description(&self) -> String {
        let Some(conn) = self.connection else {
            return "Plug in the mouse or 4K dongle".to_string();
        };
        let mut lines = vec![format!("Connection: {conn}")];
        if let Some(batt) = self.battery {
            let suffix = if !self.online {
                ", asleep"
            } else if batt.charging {
                ", charging"
            } else {
                ""
            };
            lines.push(format!(
                "Battery: {}% ({} mV{suffix})",
                batt.percent, batt.voltage_mv
            ));
        } else if !self.online {
            lines.push("Status: Asleep (move mouse to wake)".to_string());
        }
        if let Some(ref cfg) = self.config {
            let idx = self.active_dpi_index();
            let stage = cfg.dpi_stages[idx];
            lines.push(format!(
                "DPI: {} (Stage {}/{})",
                stage.x,
                idx + 1,
                cfg.max_dpi_stages
            ));
            lines.push(format!("Polling Rate: {}", cfg.polling_rate));
        }
        if let Some(notice) = &self.last_notice {
            lines.push(notice.clone());
        }
        lines.join("\n")
    }
}

fn short_connection_label(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Wired => "Wired",
        ConnectionKind::Wireless4K => "4K Wireless",
    }
}

/// Renders a 32x32 ARGB32 (network byte order) tray icon for the given badge state.
pub fn render_tray_icon(badge: IconBadge) -> Vec<u8> {
    let size = ICON_SIZE as usize;
    let mut pixels = vec![0u8; size * size * 4];

    let (bg, border, fg, fill_pct) = match badge {
        IconBadge::Percent {
            value,
            charging,
            asleep,
        } => {
            let accent = if asleep {
                [0xFF, 0x9C, 0xA3, 0xAF]
            } else if charging {
                [0xFF, 0x38, 0xBD, 0xF8]
            } else if value <= 20 {
                [0xFF, 0xF8, 0x71, 0x71]
            } else if value <= 45 {
                [0xFF, 0xFB, 0xBF, 0x24]
            } else {
                [0xFF, 0x4A, 0xDE, 0x80]
            };
            let bg = if asleep {
                [0xD8, 0x1F, 0x24, 0x2D]
            } else {
                [0xF2, 0x15, 0x1A, 0x22]
            };
            (bg, accent, accent, Some(value))
        }
        IconBadge::Unknown { asleep } => {
            let fg = if asleep {
                [0xFF, 0x9C, 0xA3, 0xAF]
            } else {
                [0xFF, 0xE5, 0xE7, 0xEB]
            };
            ([0xD8, 0x1F, 0x24, 0x2D], fg, fg, None)
        }
        IconBadge::Disconnected => (
            [0xC8, 0x1B, 0x1E, 0x24],
            [0xFF, 0x6B, 0x72, 0x80],
            [0xFF, 0x6B, 0x72, 0x80],
            None,
        ),
    };

    draw_rounded_box(&mut pixels, size, bg, border);
    if let Some(pct) = fill_pct {
        draw_battery_bar(&mut pixels, size, pct, fg);
    }

    let label = match badge {
        IconBadge::Percent { value, .. } => value.to_string(),
        IconBadge::Unknown { .. } => "??".to_string(),
        IconBadge::Disconnected => "--".to_string(),
    };
    draw_centered_text(&mut pixels, size, &label, fg);
    pixels
}

fn set_px(pixels: &mut [u8], size: usize, x: usize, y: usize, argb: [u8; 4]) {
    if x < size && y < size {
        let idx = (y * size + x) * 4;
        pixels[idx..idx + 4].copy_from_slice(&argb);
    }
}

fn draw_rounded_box(pixels: &mut [u8], size: usize, bg: [u8; 4], border: [u8; 4]) {
    let min = 1usize;
    let max = size - 2;
    for y in min..=max {
        for x in min..=max {
            let corner = (x == min || x == max) && (y == min || y == max);
            if corner {
                continue;
            }
            let is_edge = x == min || x == max || y == min || y == max;
            set_px(pixels, size, x, y, if is_edge { border } else { bg });
        }
    }
}

fn draw_battery_bar(pixels: &mut [u8], size: usize, pct: u8, color: [u8; 4]) {
    let bar_x0 = 4usize;
    let bar_w = size - 8;
    let filled = (usize::from(pct.min(100)) * bar_w).div_ceil(100).min(bar_w);
    let track = [0xFF, 0x2E, 0x35, 0x42];
    for y in (size - 6)..=(size - 4) {
        for dx in 0..bar_w {
            let px_color = if dx < filled { color } else { track };
            set_px(pixels, size, bar_x0 + dx, y, px_color);
        }
    }
}

fn draw_centered_text(pixels: &mut [u8], size: usize, text: &str, color: [u8; 4]) {
    let scale = if text.len() >= 3 { 1usize } else { 2usize };
    let glyph_w = 5 * scale;
    let glyph_h = 7 * scale;
    let spacing = scale;
    let total_w = text.len() * glyph_w + text.len().saturating_sub(1) * spacing;
    let start_x = size.saturating_sub(total_w) / 2;
    let start_y = (size - 8).saturating_sub(glyph_h) / 2 + 2;

    for (idx, ch) in text.chars().enumerate() {
        let ox = start_x + idx * (glyph_w + spacing);
        let rows = glyph_5x7(ch);
        for (ry, row) in rows.iter().enumerate() {
            for rx in 0..5usize {
                if (row >> (4 - rx)) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            set_px(pixels, size, ox + rx * scale + sx, start_y + ry * scale + sy, color);
                        }
                    }
                }
            }
        }
    }
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        _ => [0x00; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Lod, RgbColor, SensorMode};

    fn sample_config() -> MouseConfig {
        let mut cfg = MouseConfig {
            polling_rate: PollingRate::Hz1000,
            max_dpi_stages: 4,
            active_dpi_stage: 1,
            lod: Lod::Mm1,
            dpi_stages: [DpiStage {
                x: 800,
                y: 800,
                color: RgbColor { r: 255, g: 0, b: 0 },
            }; 8],
            debounce_ms: 4,
            motion_sync: true,
            ripple_control: false,
            angle_snapping: false,
            sensor_mode: SensorMode::HighPerformance,
            custom_sleep_enabled: false,
            sleep_seconds: 60,
        };
        cfg.dpi_stages[1].x = 1600;
        cfg.dpi_stages[1].y = 1600;
        cfg
    }

    #[test]
    fn sleeping_retains_last_battery_and_config_while_disconnect_clears_state() {
        let mut state = TrayState::default();
        assert_eq!(state.icon_badge(), IconBadge::Disconnected);

        state.apply_poll(PollOutcome::Online {
            connection: ConnectionKind::Wireless4K,
            battery: BatteryStatus {
                percent: 82,
                charging: false,
                voltage_mv: 4050,
            },
            config: sample_config(),
        });
        assert_eq!(
            state.icon_badge(),
            IconBadge::Percent {
                value: 82,
                charging: false,
                asleep: false,
            }
        );
        assert_eq!(state.status_summary(), "82% · 4K Wireless");
        assert!(state.tooltip_description().contains("DPI: 1600 (Stage 2/4)"));

        state.apply_poll(PollOutcome::Asleep {
            connection: ConnectionKind::Wireless4K,
        });
        assert_eq!(
            state.icon_badge(),
            IconBadge::Percent {
                value: 82,
                charging: false,
                asleep: true,
            }
        );
        assert_eq!(state.status_summary(), "82% (Asleep) · 4K Wireless");

        state.apply_poll(PollOutcome::Disconnected);
        assert_eq!(state.icon_badge(), IconBadge::Disconnected);
        assert!(state.battery.is_none());
        assert!(state.config.is_none());
    }

    #[test]
    fn wired_connection_limits_supported_rates_and_tracks_active_index() {
        let mut state = TrayState::default();
        state.apply_poll(PollOutcome::Online {
            connection: ConnectionKind::Wired,
            battery: BatteryStatus {
                percent: 100,
                charging: true,
                voltage_mv: 4200,
            },
            config: sample_config(),
        });
        assert_eq!(
            state.supported_rates(),
            ConnectionKind::Wired.supported_polling_rates()
        );
        assert_eq!(state.active_rate_index(), 3);
        assert_eq!(state.enabled_dpi_stages().len(), 4);
        assert_eq!(state.active_dpi_index(), 1);
    }

    #[test]
    fn render_tray_icon_produces_valid_argb32_buffer_and_distinct_state_colors() {
        let full = render_tray_icon(IconBadge::Percent {
            value: 100,
            charging: false,
            asleep: false,
        });
        let low = render_tray_icon(IconBadge::Percent {
            value: 15,
            charging: false,
            asleep: false,
        });
        let disconnected = render_tray_icon(IconBadge::Disconnected);

        let expected_len = (ICON_SIZE as usize) * (ICON_SIZE as usize) * 4;
        assert_eq!(full.len(), expected_len);
        assert_eq!(low.len(), expected_len);
        assert_eq!(disconnected.len(), expected_len);
        assert_ne!(full, low);
        assert_ne!(low, disconnected);
    }
}
