use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use hypernova_ctl::protocol::{Lod, PollingRate, SensorMode};

use crate::model::{DeviceSnapshot, GuiCommand};

pub struct SensorPage {
    pub widget: adw::PreferencesPage,
    rate_combo: adw::ComboRow,
    rates: Rc<RefCell<Vec<PollingRate>>>,
    lod_combo: adw::ComboRow,
    debounce_spin: gtk4::SpinButton,
    sensor_mode_combo: adw::ComboRow,
    motion_sync_switch: gtk4::Switch,
    ripple_switch: gtk4::Switch,
    angle_switch: gtk4::Switch,
}

impl SensorPage {
    pub fn new(cmd_tx: Sender<GuiCommand>, updating: Rc<Cell<bool>>) -> Self {
        let page = adw::PreferencesPage::new();
        let rates = Rc::new(RefCell::new(Vec::<PollingRate>::new()));

        let perf_group = adw::PreferencesGroup::builder()
            .title("Polling & Response")
            .build();

        let rate_combo = adw::ComboRow::builder()
            .title("USB Polling Rate")
            .subtitle("Wired supports up to 1000 Hz; 4K dongle supports up to 4000 Hz")
            .build();

        let tx_rate = cmd_tx.clone();
        let upd_rate = updating.clone();
        let rates_ref = rates.clone();
        rate_combo.connect_selected_notify(move |combo| {
            if upd_rate.get() {
                return;
            }
            if let Some(&rate) = rates_ref.borrow().get(combo.selected() as usize) {
                let _ = tx_rate.send(GuiCommand::SetPollingRate(rate));
            }
        });

        let debounce_spin = gtk4::SpinButton::with_range(0.0, 30.0, 1.0);
        debounce_spin.set_valign(gtk4::Align::Center);
        let debounce_row = adw::ActionRow::builder()
            .title("Button Debounce (ms)")
            .subtitle("0 to 30 ms")
            .build();
        debounce_row.add_suffix(&debounce_spin);
        debounce_row.set_activatable_widget(Some(&debounce_spin));

        let tx_deb = cmd_tx.clone();
        let upd_deb = updating.clone();
        debounce_spin.connect_value_changed(move |spin| {
            if upd_deb.get() {
                return;
            }
            let _ = tx_deb.send(GuiCommand::SetDebounce(spin.value() as u8));
        });

        perf_group.add(&rate_combo);
        perf_group.add(&debounce_row);
        page.add(&perf_group);

        let sensor_group = adw::PreferencesGroup::builder()
            .title("PAW3395 Sensor")
            .build();

        let lod_combo = adw::ComboRow::builder()
            .title("Lift-Off Distance")
            .model(&gtk4::StringList::new(&["1 mm", "2 mm"]))
            .build();
        let tx_lod = cmd_tx.clone();
        let upd_lod = updating.clone();
        lod_combo.connect_selected_notify(move |combo| {
            if upd_lod.get() {
                return;
            }
            let lod = if combo.selected() == 0 {
                Lod::Mm1
            } else {
                Lod::Mm2
            };
            let _ = tx_lod.send(GuiCommand::SetLod(lod));
        });

        let sensor_mode_combo = adw::ComboRow::builder()
            .title("Sensor Power Mode")
            .model(&gtk4::StringList::new(&["Low Power", "High Performance"]))
            .build();
        let tx_mode = cmd_tx.clone();
        let upd_mode = updating.clone();
        sensor_mode_combo.connect_selected_notify(move |combo| {
            if upd_mode.get() {
                return;
            }
            let mode = if combo.selected() == 0 {
                SensorMode::LowPower
            } else {
                SensorMode::HighPerformance
            };
            let _ = tx_mode.send(GuiCommand::SetSensorMode(mode));
        });

        let (motion_row, motion_sync_switch) = make_switch_row(
            "Motion Sync",
            "Align sensor polls with USB frames",
            cmd_tx.clone(),
            updating.clone(),
            GuiCommand::SetMotionSync,
        );
        let (ripple_row, ripple_switch) = make_switch_row(
            "Ripple Control",
            "Smooth sensor jitter at high DPI",
            cmd_tx.clone(),
            updating.clone(),
            GuiCommand::SetRipple,
        );
        let (angle_row, angle_switch) = make_switch_row(
            "Angle Snapping",
            "Linear movement correction",
            cmd_tx,
            updating,
            GuiCommand::SetAngleSnapping,
        );

        sensor_group.add(&lod_combo);
        sensor_group.add(&sensor_mode_combo);
        sensor_group.add(&motion_row);
        sensor_group.add(&ripple_row);
        sensor_group.add(&angle_row);
        page.add(&sensor_group);

        Self {
            widget: page,
            rate_combo,
            rates,
            lod_combo,
            debounce_spin,
            sensor_mode_combo,
            motion_sync_switch,
            ripple_switch,
            angle_switch,
        }
    }

    pub fn sync(&self, snapshot: &DeviceSnapshot) {
        let interactive = snapshot.online && snapshot.config.is_some();
        self.widget.set_sensitive(interactive);

        let supported = snapshot.supported_rates();
        *self.rates.borrow_mut() = supported.to_vec();
        let rate_labels: Vec<String> = supported.iter().map(ToString::to_string).collect();
        let refs: Vec<&str> = rate_labels.iter().map(String::as_str).collect();
        self.rate_combo
            .set_model(Some(&gtk4::StringList::new(&refs)));

        let Some(ref cfg) = snapshot.config else {
            return;
        };

        self.rate_combo.set_selected(snapshot.active_rate_index());
        self.debounce_spin.set_value(f64::from(cfg.debounce_ms));
        self.lod_combo.set_selected(u32::from(cfg.lod == Lod::Mm2));

        let fw_managed = cfg.polling_rate.manages_sensor_mode_internally();
        self.sensor_mode_combo.set_sensitive(!fw_managed);
        self.sensor_mode_combo.set_subtitle(if fw_managed {
            "Firmware-managed at 2000 Hz and 4000 Hz"
        } else {
            "Low Power saves battery; High Performance lowers tracking latency"
        });
        self.sensor_mode_combo
            .set_selected(u32::from(cfg.sensor_mode == SensorMode::HighPerformance));

        self.motion_sync_switch.set_active(cfg.motion_sync);
        self.ripple_switch.set_active(cfg.ripple_control);
        self.angle_switch.set_active(cfg.angle_snapping);
    }
}

fn make_switch_row<F>(
    title: &str,
    subtitle: &str,
    cmd_tx: Sender<GuiCommand>,
    updating: Rc<Cell<bool>>,
    build_cmd: F,
) -> (adw::ActionRow, gtk4::Switch)
where
    F: Fn(bool) -> GuiCommand + 'static,
{
    let switch = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .build();
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_suffix(&switch);
    row.set_activatable_widget(Some(&switch));

    switch.connect_state_set(move |_, active| {
        if !updating.get() {
            let _ = cmd_tx.send(build_cmd(active));
        }
        glib::Propagation::Proceed
    });
    (row, switch)
}
