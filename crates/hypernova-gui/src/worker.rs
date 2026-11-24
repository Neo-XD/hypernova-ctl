use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use hypernova_ctl::device::HypernovaDevice;
use hypernova_ctl::transport::{HidApiTransport, TransportError};

use crate::model::{DeviceSnapshot, GuiCommand, WorkerEvent};

const POLL_INTERVAL: Duration = Duration::from_secs(15);

pub struct DeviceWorker {
    pub cmd_tx: Sender<GuiCommand>,
    pub event_rx: Receiver<WorkerEvent>,
}

impl DeviceWorker {
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        thread::spawn(move || worker_loop(cmd_rx, event_tx));
        Self { cmd_tx, event_rx }
    }
}

fn worker_loop(cmd_rx: Receiver<GuiCommand>, event_tx: Sender<WorkerEvent>) {
    let mut device: Option<HypernovaDevice<HidApiTransport>> = None;
    let mut snapshot = DeviceSnapshot::default();

    poll_into(&mut device, &mut snapshot);
    let _ = event_tx.send(WorkerEvent::Snapshot(snapshot.clone()));

    loop {
        match cmd_rx.recv_timeout(POLL_INTERVAL) {
            Ok(GuiCommand::Refresh) | Err(RecvTimeoutError::Timeout) => {
                poll_into(&mut device, &mut snapshot);
                let _ = event_tx.send(WorkerEvent::Snapshot(snapshot.clone()));
            }
            Ok(cmd) => {
                match apply_command(&mut device, cmd) {
                    Ok(Some(msg)) => {
                        let _ = event_tx.send(WorkerEvent::Toast(msg));
                    }
                    Ok(None) => {}
                    Err(err) => {
                        let _ = event_tx.send(WorkerEvent::Toast(err));
                    }
                }
                poll_into(&mut device, &mut snapshot);
                let _ = event_tx.send(WorkerEvent::Snapshot(snapshot.clone()));
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
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

fn poll_into(slot: &mut Option<HypernovaDevice<HidApiTransport>>, snapshot: &mut DeviceSnapshot) {
    let dev = match ensure_device(slot) {
        Ok(dev) => dev,
        Err(err) => {
            *slot = None;
            let permission_denied = err.to_string().contains("Permission denied");
            *snapshot = DeviceSnapshot {
                permission_denied,
                ..DeviceSnapshot::default()
            };
            return;
        }
    };

    snapshot.permission_denied = false;
    snapshot.connection = Some(dev.connection_kind());

    match dev.is_online() {
        Ok(false) => {
            snapshot.online = false;
        }
        Ok(true) => {
            snapshot.online = true;
            if let Ok(fw) = dev.firmware_version() {
                snapshot.firmware = Some(fw);
            }
            match (dev.battery(), dev.read_config()) {
                (Ok(battery), Ok(config)) => {
                    snapshot.battery = Some(battery);
                    snapshot.config = Some(config);
                }
                _ => {
                    *slot = None;
                    snapshot.online = false;
                }
            }
        }
        Err(_) => {
            *slot = None;
            *snapshot = DeviceSnapshot::default();
        }
    }
}

fn apply_command(
    slot: &mut Option<HypernovaDevice<HidApiTransport>>,
    cmd: GuiCommand,
) -> Result<Option<String>, String> {
    let dev = ensure_device(slot).map_err(|e| e.to_string())?;
    let online = match dev.is_online() {
        Ok(online) => online,
        Err(err) => {
            *slot = None;
            return Err(err.to_string());
        }
    };
    if !online {
        return Err("Mouse is asleep. Move the mouse to wake it.".to_string());
    }

    let (result, toast) = match cmd {
        GuiCommand::Refresh => (Ok(()), None),
        GuiCommand::SetPollingRate(rate) => (
            dev.set_polling_rate(rate),
            Some(format!("Polling rate set to {rate}")),
        ),
        GuiCommand::SetActiveDpi(stage) => (
            dev.set_active_dpi_stage(stage),
            Some(format!("Active DPI stage set to {stage}")),
        ),
        GuiCommand::SetMaxDpiStages(count) => (
            dev.set_max_dpi_stages(count),
            Some(format!("Enabled DPI stages set to {count}")),
        ),
        GuiCommand::SetDpiStage { stage, x, y } => (
            dev.set_dpi_stage(stage, x, y),
            Some(format!("Stage {stage} set to {x}x{y} DPI")),
        ),
        GuiCommand::SetDpiColor { stage, color } => (
            dev.set_dpi_color(stage, color),
            Some(format!("Stage {stage} color updated")),
        ),
        GuiCommand::SetLod(lod) => (dev.set_lod(lod), Some(format!("LOD set to {lod}"))),
        GuiCommand::SetDebounce(ms) => (
            dev.set_debounce_ms(ms),
            Some(format!("Debounce set to {ms} ms")),
        ),
        GuiCommand::SetSensorMode(mode) => (
            dev.set_sensor_mode(mode),
            Some(format!("Sensor mode set to {mode}")),
        ),
        GuiCommand::SetMotionSync(on) => (dev.set_motion_sync(on), None),
        GuiCommand::SetRipple(on) => (dev.set_ripple_control(on), None),
        GuiCommand::SetAngleSnapping(on) => (dev.set_angle_snapping(on), None),
        GuiCommand::SetSleep(secs) => (
            dev.set_sleep_time(secs),
            Some(match secs {
                Some(s) => format!("Sleep timer set to {s} s"),
                None => "Custom sleep timer disabled".to_string(),
            }),
        ),
        GuiCommand::FactoryReset => (
            dev.factory_reset(),
            Some("Factory defaults restored".to_string()),
        ),
    };

    if let Err(err) = result {
        if matches!(err, TransportError::Hid(_)) {
            *slot = None;
        }
        return Err(err.to_string());
    }
    Ok(toast)
}
