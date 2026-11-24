use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::model::{snap_sleep_seconds, DeviceSnapshot, GuiCommand};

pub struct PowerPage {
    pub widget: adw::PreferencesPage,
    sleep_group: adw::PreferencesGroup,
    sleep_switch: gtk4::Switch,
    sleep_spin: gtk4::SpinButton,
    reset_group: adw::PreferencesGroup,
    tray_row: adw::ActionRow,
    tray_switch: gtk4::Switch,
}

impl PowerPage {
    pub fn new(cmd_tx: Sender<GuiCommand>, updating: Rc<Cell<bool>>) -> Self {
        let page = adw::PreferencesPage::new();

        let sleep_group = adw::PreferencesGroup::builder()
            .title("Wireless Sleep")
            .build();

        let sleep_switch = gtk4::Switch::builder()
            .valign(gtk4::Align::Center)
            .build();
        let sleep_toggle_row = adw::ActionRow::builder()
            .title("Custom Sleep Timer")
            .subtitle("Override firmware default inactivity timeout")
            .build();
        sleep_toggle_row.add_suffix(&sleep_switch);
        sleep_toggle_row.set_activatable_widget(Some(&sleep_switch));

        let sleep_spin = gtk4::SpinButton::with_range(30.0, 2400.0, 10.0);
        sleep_spin.set_snap_to_ticks(true);
        sleep_spin.set_valign(gtk4::Align::Center);
        let sleep_secs_row = adw::ActionRow::builder()
            .title("Sleep Timeout (seconds)")
            .subtitle("30 to 2400 seconds in 10-second steps")
            .build();
        sleep_secs_row.add_suffix(&sleep_spin);
        sleep_secs_row.set_activatable_widget(Some(&sleep_spin));

        let tx_sw = cmd_tx.clone();
        let upd_sw = updating.clone();
        let spin_for_sw = sleep_spin.clone();
        sleep_switch.connect_state_set(move |_, active| {
            spin_for_sw.set_sensitive(active);
            if !upd_sw.get() {
                let value = if active {
                    Some(snap_sleep_seconds(spin_for_sw.value() as u16))
                } else {
                    None
                };
                let _ = tx_sw.send(GuiCommand::SetSleep(value));
            }
            glib::Propagation::Proceed
        });

        let tx_sp = cmd_tx.clone();
        let upd_sp = updating.clone();
        let sw_for_sp = sleep_switch.clone();
        sleep_spin.connect_value_changed(move |spin| {
            if upd_sp.get() || !sw_for_sp.is_active() {
                return;
            }
            let secs = snap_sleep_seconds(spin.value() as u16);
            let _ = tx_sp.send(GuiCommand::SetSleep(Some(secs)));
        });

        sleep_group.add(&sleep_toggle_row);
        sleep_group.add(&sleep_secs_row);
        page.add(&sleep_group);

        let tray_group = adw::PreferencesGroup::builder()
            .title("Desktop Integration")
            .build();
        let tray_switch = gtk4::Switch::builder()
            .valign(gtk4::Align::Center)
            .build();
        let tray_row = adw::ActionRow::builder()
            .title("System Tray Indicator")
            .subtitle("Show battery percentage and quick menu in the KDE panel")
            .build();
        tray_row.add_suffix(&tray_switch);
        tray_row.set_activatable_widget(Some(&tray_switch));

        let upd_tray = updating.clone();
        let tray_row_ref = tray_row.clone();
        tray_switch.connect_state_set(move |_, active| {
            if upd_tray.get() {
                return glib::Propagation::Proceed;
            }
            let res = if active {
                hypernova_ctl::tray::enable_service()
            } else {
                hypernova_ctl::tray::disable_service()
            };
            if let Err(err) = res {
                tray_row_ref.set_subtitle(&err);
            } else {
                tray_row_ref.set_subtitle(if active {
                    "System tray service enabled and running"
                } else {
                    "System tray service disabled"
                });
            }
            glib::Propagation::Proceed
        });

        tray_group.add(&tray_row);
        page.add(&tray_group);

        let reset_group = adw::PreferencesGroup::builder()
            .title("Maintenance")
            .build();

        let confirm_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        confirm_bar.set_valign(gtk4::Align::Center);
        let confirm_btn = gtk4::Button::builder()
            .label("Confirm Reset")
            .css_classes(vec!["destructive-action".to_string()])
            .build();
        let cancel_btn = gtk4::Button::builder().label("Cancel").build();
        confirm_bar.append(&cancel_btn);
        confirm_bar.append(&confirm_btn);

        let confirm_revealer = gtk4::Revealer::builder()
            .child(&confirm_bar)
            .reveal_child(false)
            .build();

        let reset_btn = gtk4::Button::builder()
            .label("Factory Reset")
            .valign(gtk4::Align::Center)
            .css_classes(vec!["destructive-action".to_string()])
            .build();

        let rev_show = confirm_revealer.clone();
        let reset_btn_hide = reset_btn.clone();
        reset_btn.connect_clicked(move |_| {
            reset_btn_hide.set_visible(false);
            rev_show.set_reveal_child(true);
        });

        let rev_cancel = confirm_revealer.clone();
        let reset_btn_cancel = reset_btn.clone();
        cancel_btn.connect_clicked(move |_| {
            rev_cancel.set_reveal_child(false);
            reset_btn_cancel.set_visible(true);
        });

        let rev_ok = confirm_revealer.clone();
        let reset_btn_ok = reset_btn.clone();
        confirm_btn.connect_clicked(move |_| {
            rev_ok.set_reveal_child(false);
            reset_btn_ok.set_visible(true);
            let _ = cmd_tx.send(GuiCommand::FactoryReset);
        });

        let reset_row = adw::ActionRow::builder()
            .title("Restore Factory Defaults")
            .subtitle("Resets DPI stages, polling rate, and sensor settings on the mouse")
            .build();
        reset_row.add_suffix(&reset_btn);
        reset_row.add_suffix(&confirm_revealer);
        reset_group.add(&reset_row);
        page.add(&reset_group);

        Self {
            widget: page,
            sleep_group,
            sleep_switch,
            sleep_spin,
            reset_group,
            tray_row,
            tray_switch,
        }
    }

    pub fn sync(&self, snapshot: &DeviceSnapshot) {
        let interactive = snapshot.online && snapshot.config.is_some();
        self.sleep_group.set_sensitive(interactive);
        self.reset_group.set_sensitive(interactive);

        if let Ok(status) = hypernova_ctl::tray::service_status() {
            let active = status.lines().any(|l| l.starts_with("Active:") && l.ends_with("active"));
            self.tray_switch.set_active(active);
            self.tray_row.set_subtitle(if active {
                "System tray service active"
            } else {
                "System tray service inactive"
            });
        }

        let Some(ref cfg) = snapshot.config else {
            return;
        };
        self.sleep_switch.set_active(cfg.custom_sleep_enabled);
        self.sleep_spin.set_sensitive(cfg.custom_sleep_enabled);
        self.sleep_spin
            .set_value(f64::from(snap_sleep_seconds(cfg.sleep_seconds)));
    }
}
