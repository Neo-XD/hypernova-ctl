use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Icon, MenuItem, ToolTip, Tray};

use crate::device::HypernovaDevice;
use crate::protocol::{Lod, PollingRate, SensorMode};
use crate::transport::{HidApiTransport, TransportError};
use crate::tray_model::{render_tray_icon, PollOutcome, TrayState, ICON_SIZE};

const SERVICE_NAME: &str = "hypernova-tray.service";

enum TrayAction {
    SetPollingRate(PollingRate),
    SetActiveDpi(u8),
    SetLod(Lod),
    SetSensorMode(SensorMode),
    SetMotionSync(bool),
    SetRipple(bool),
    SetAngleSnapping(bool),
    Refresh,
    Quit,
}

struct HypernovaTray {
    state: TrayState,
    tx: Sender<TrayAction>,
}

impl HypernovaTray {
    fn send(&self, action: TrayAction) {
        let _ = self.tx.send(action);
    }

    fn polling_rate_menu(&self) -> MenuItem<Self> {
        let rates = self.state.supported_rates();
        let options = rates
            .iter()
            .map(|rate| RadioItem {
                label: rate.to_string(),
                enabled: self.state.connection.is_some(),
                ..Default::default()
            })
            .collect();
        SubMenu {
            label: "Polling Rate".into(),
            enabled: self.state.connection.is_some(),
            submenu: vec![RadioGroup {
                selected: self.state.active_rate_index(),
                select: Box::new(|this: &mut Self, idx| {
                    if let Some(&rate) = this.state.supported_rates().get(idx) {
                        this.send(TrayAction::SetPollingRate(rate));
                    }
                }),
                options,
            }
            .into()],
            ..Default::default()
        }
        .into()
    }

    fn dpi_stage_menu(&self) -> MenuItem<Self> {
        let stages = self.state.enabled_dpi_stages();
        let options = stages
            .iter()
            .map(|(num, stage)| RadioItem {
                label: format!("Stage {num} ({} DPI)", stage.x),
                ..Default::default()
            })
            .collect();
        SubMenu {
            label: "DPI Stage".into(),
            enabled: !stages.is_empty(),
            submenu: vec![RadioGroup {
                selected: self.state.active_dpi_index(),
                select: Box::new(|this: &mut Self, idx| {
                    this.send(TrayAction::SetActiveDpi((idx + 1) as u8));
                }),
                options,
            }
            .into()],
            ..Default::default()
        }
        .into()
    }

    fn sensor_menu(&self) -> MenuItem<Self> {
        let Some(ref cfg) = self.state.config else {
            return SubMenu {
                label: "Sensor".into(),
                enabled: false,
                submenu: Vec::new(),
                ..Default::default()
            }
            .into();
        };

        let lod_selected = usize::from(cfg.lod == Lod::Mm2);
        let mode_selected = usize::from(cfg.sensor_mode == SensorMode::HighPerformance);
        let mode_editable = !cfg.polling_rate.manages_sensor_mode_internally();
        let motion_sync = cfg.motion_sync;
        let ripple_control = cfg.ripple_control;
        let angle_snapping = cfg.angle_snapping;

        SubMenu {
            label: "Sensor".into(),
            submenu: vec![
                SubMenu {
                    label: "LOD".into(),
                    submenu: vec![RadioGroup {
                        selected: lod_selected,
                        select: Box::new(|this: &mut Self, idx| {
                            let lod = if idx == 0 { Lod::Mm1 } else { Lod::Mm2 };
                            this.send(TrayAction::SetLod(lod));
                        }),
                        options: vec![
                            RadioItem {
                                label: "1 mm".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "2 mm".into(),
                                ..Default::default()
                            },
                        ],
                    }
                    .into()],
                    ..Default::default()
                }
                .into(),
                SubMenu {
                    label: if mode_editable {
                        "Sensor Mode".into()
                    } else {
                        "Sensor Mode (Auto at 2K+)".into()
                    },
                    enabled: mode_editable,
                    submenu: vec![RadioGroup {
                        selected: mode_selected,
                        select: Box::new(|this: &mut Self, idx| {
                            let mode = if idx == 0 {
                                SensorMode::LowPower
                            } else {
                                SensorMode::HighPerformance
                            };
                            this.send(TrayAction::SetSensorMode(mode));
                        }),
                        options: vec![
                            RadioItem {
                                label: "Low Power".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "High Performance".into(),
                                ..Default::default()
                            },
                        ],
                    }
                    .into()],
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                CheckmarkItem {
                    label: "Motion Sync".into(),
                    checked: motion_sync,
                    activate: Box::new(move |this: &mut Self| {
                        this.send(TrayAction::SetMotionSync(!motion_sync));
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Ripple Control".into(),
                    checked: ripple_control,
                    activate: Box::new(move |this: &mut Self| {
                        this.send(TrayAction::SetRipple(!ripple_control));
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Angle Snapping".into(),
                    checked: angle_snapping,
                    activate: Box::new(move |this: &mut Self| {
                        this.send(TrayAction::SetAngleSnapping(!angle_snapping));
                    }),
                    ..Default::default()
                }
                .into(),
            ],
            ..Default::default()
        }
        .into()
    }
}

impl Tray for HypernovaTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn title(&self) -> String {
        self.state.tooltip_title()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![Icon {
            width: ICON_SIZE,
            height: ICON_SIZE,
            data: render_tray_icon(self.state.icon_badge()),
        }]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title: self.state.tooltip_title(),
            description: self.state.tooltip_description(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![StandardItem {
            label: self.state.status_summary(),
            enabled: false,
            ..Default::default()
        }
        .into()];

        if let Some(notice) = &self.state.last_notice {
            items.push(
                StandardItem {
                    label: notice.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }

        items.extend([
            MenuItem::Separator,
            self.polling_rate_menu(),
            self.dpi_stage_menu(),
            self.sensor_menu(),
            MenuItem::Separator,
            StandardItem {
                label: "Refresh".into(),
                icon_name: "view-refresh".into(),
                activate: Box::new(|this: &mut Self| this.send(TrayAction::Refresh)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|this: &mut Self| this.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]);

        items
    }
}

pub fn run_tray(poll_interval: Duration) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let tray = HypernovaTray {
        state: TrayState::default(),
        tx,
    };
    let handle = tray
        .assume_sni_available(true)
        .spawn()
        .map_err(|e| format!("failed to start system tray: {e}"))?;

    let mut device: Option<HypernovaDevice<HidApiTransport>> = None;
    let initial = poll_device(&mut device);
    handle.update(|t| t.state.apply_poll(initial));

    loop {
        if handle.is_closed() {
            break;
        }
        match rx.recv_timeout(poll_interval) {
            Ok(TrayAction::Quit) | Err(RecvTimeoutError::Disconnected) => {
                handle.shutdown().wait();
                break;
            }
            Ok(TrayAction::Refresh) | Err(RecvTimeoutError::Timeout) => {
                let outcome = poll_device(&mut device);
                handle.update(|t| t.state.apply_poll(outcome));
            }
            Ok(action) => {
                let notice = apply_action(&mut device, action).err();
                let outcome = poll_device(&mut device);
                handle.update(|t| {
                    t.state.apply_poll(outcome);
                    if let Some(msg) = notice {
                        t.state.set_notice(msg);
                    }
                });
            }
        }
    }
    Ok(())
}

fn ensure_device(
    slot: &mut Option<HypernovaDevice<HidApiTransport>>,
) -> Result<&HypernovaDevice<HidApiTransport>, TransportError> {
    if slot.is_none() {
        let transport = HidApiTransport::open()?;
        *slot = Some(HypernovaDevice::new(transport));
    }
    Ok(slot.as_ref().expect("device initialized"))
}

fn poll_device(slot: &mut Option<HypernovaDevice<HidApiTransport>>) -> PollOutcome {
    let dev = match ensure_device(slot) {
        Ok(dev) => dev,
        Err(err) => {
            *slot = None;
            let msg = err.to_string();
            if msg.contains("Permission denied") {
                eprintln!("hypernova-ctl tray: {msg}");
                return PollOutcome::PermissionDenied;
            }
            return PollOutcome::Disconnected;
        }
    };
    let connection = dev.connection_kind();
    match dev.is_online() {
        Ok(false) => PollOutcome::Asleep { connection },
        Ok(true) => match (dev.battery(), dev.read_config()) {
            (Ok(battery), Ok(config)) => PollOutcome::Online {
                connection,
                battery,
                config,
            },
            _ => {
                *slot = None;
                PollOutcome::Disconnected
            }
        },
        Err(_) => {
            *slot = None;
            PollOutcome::Disconnected
        }
    }
}

fn apply_action(
    slot: &mut Option<HypernovaDevice<HidApiTransport>>,
    action: TrayAction,
) -> Result<(), String> {
    let dev = ensure_device(slot).map_err(|e| e.to_string())?;
    let online = match dev.is_online() {
        Ok(online) => online,
        Err(e) => {
            *slot = None;
            return Err(e.to_string());
        }
    };
    if !online {
        return Err("Mouse is asleep. Move mouse to wake.".to_string());
    }

    let result = match action {
        TrayAction::SetPollingRate(rate) => dev.set_polling_rate(rate),
        TrayAction::SetActiveDpi(stage) => dev.set_active_dpi_stage(stage),
        TrayAction::SetLod(lod) => dev.set_lod(lod),
        TrayAction::SetSensorMode(mode) => dev.set_sensor_mode(mode),
        TrayAction::SetMotionSync(on) => dev.set_motion_sync(on),
        TrayAction::SetRipple(on) => dev.set_ripple_control(on),
        TrayAction::SetAngleSnapping(on) => dev.set_angle_snapping(on),
        TrayAction::Refresh | TrayAction::Quit => Ok(()),
    };
    if let Err(e) = result {
        if matches!(e, TransportError::Hid(_)) {
            *slot = None;
        }
        return Err(e.to_string());
    }
    Ok(())
}

pub fn enable_service() -> Result<(), String> {
    let _ = run_systemctl(&["unmask", SERVICE_NAME]);
    run_systemctl(&["enable", "--now", SERVICE_NAME])?;
    Ok(())
}

pub fn disable_service() -> Result<(), String> {
    run_systemctl(&["mask", "--now", SERVICE_NAME])?;
    Ok(())
}

pub fn service_status() -> Result<String, String> {
    let active = systemctl_output(&["is-active", SERVICE_NAME]);
    let enabled = systemctl_output(&["is-enabled", SERVICE_NAME]);
    Ok(format!(
        "Tray Service:   {SERVICE_NAME}\nActive:         {active}\nStartup:        {enabled}"
    ))
}

fn run_systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run systemctl --user: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!("systemctl --user {} failed", args.join(" "))
    } else {
        stderr
    })
}

fn systemctl_output(args: &[&str]) -> String {
    match Command::new("systemctl").arg("--user").args(args).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                stdout
            } else {
                String::from_utf8_lossy(&out.stderr).trim().to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}
