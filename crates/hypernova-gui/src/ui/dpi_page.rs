use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::model::{f32_to_rgb, rgb_to_f32, snap_dpi, DeviceSnapshot, GuiCommand};

struct StageRow {
    row: adw::ActionRow,
    x_spin: gtk4::SpinButton,
    y_spin: gtk4::SpinButton,
    color_btn: gtk4::ColorButton,
}

pub struct DpiPage {
    pub widget: adw::PreferencesPage,
    stages_spin: gtk4::SpinButton,
    active_combo: adw::ComboRow,
    stage_rows: Vec<StageRow>,
}

impl DpiPage {
    pub fn new(cmd_tx: Sender<GuiCommand>, updating: Rc<Cell<bool>>) -> Self {
        let page = adw::PreferencesPage::new();

        let overview = adw::PreferencesGroup::builder()
            .title("DPI Profile")
            .description("PAW3395 supports 50 to 26,000 DPI in 50-DPI steps.")
            .build();

        let stages_spin = gtk4::SpinButton::with_range(1.0, 6.0, 1.0);
        stages_spin.set_valign(gtk4::Align::Center);
        let stages_row = adw::ActionRow::builder()
            .title("Enabled DPI Stages")
            .subtitle("Number of stages cycled by the DPI button (1 to 6)")
            .build();
        stages_row.add_suffix(&stages_spin);
        stages_row.set_activatable_widget(Some(&stages_spin));

        let tx_stages = cmd_tx.clone();
        let upd_stages = updating.clone();
        stages_spin.connect_value_changed(move |spin| {
            if upd_stages.get() {
                return;
            }
            let _ = tx_stages.send(GuiCommand::SetMaxDpiStages(spin.value() as u8));
        });

        let active_combo = adw::ComboRow::builder()
            .title("Active Stage")
            .model(&gtk4::StringList::new(&[
                "Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5", "Stage 6",
            ]))
            .build();

        let tx_active = cmd_tx.clone();
        let upd_active = updating.clone();
        active_combo.connect_selected_notify(move |combo| {
            if upd_active.get() {
                return;
            }
            let stage = (combo.selected() + 1) as u8;
            let _ = tx_active.send(GuiCommand::SetActiveDpi(stage));
        });

        overview.add(&stages_row);
        overview.add(&active_combo);
        page.add(&overview);

        let stages_group = adw::PreferencesGroup::builder()
            .title("Stages (X DPI, Y DPI, LED Color)")
            .build();

        let mut stage_rows = Vec::with_capacity(6);
        for idx in 0..6u8 {
            let stage_num = idx + 1;
            let row = adw::ActionRow::builder()
                .title(format!("Stage {stage_num}"))
                .build();

            let x_spin = make_dpi_spin();
            x_spin.set_tooltip_text(Some("X-axis DPI"));
            let y_spin = make_dpi_spin();
            y_spin.set_tooltip_text(Some("Y-axis DPI"));
            let color_btn = gtk4::ColorButton::builder()
                .valign(gtk4::Align::Center)
                .tooltip_text("Stage indicator LED color")
                .build();

            let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            controls.set_valign(gtk4::Align::Center);
            controls.append(&x_spin);
            controls.append(&y_spin);
            controls.append(&color_btn);
            row.add_suffix(&controls);

            let tx_x = cmd_tx.clone();
            let upd_x = updating.clone();
            let y_ref = y_spin.clone();
            x_spin.connect_value_changed(move |spin| {
                if upd_x.get() {
                    return;
                }
                let x = snap_dpi(spin.value() as u16);
                let y = snap_dpi(y_ref.value() as u16);
                let _ = tx_x.send(GuiCommand::SetDpiStage {
                    stage: stage_num,
                    x,
                    y,
                });
            });

            let tx_y = cmd_tx.clone();
            let upd_y = updating.clone();
            let x_ref = x_spin.clone();
            y_spin.connect_value_changed(move |spin| {
                if upd_y.get() {
                    return;
                }
                let x = snap_dpi(x_ref.value() as u16);
                let y = snap_dpi(spin.value() as u16);
                let _ = tx_y.send(GuiCommand::SetDpiStage {
                    stage: stage_num,
                    x,
                    y,
                });
            });

            let tx_c = cmd_tx.clone();
            let upd_c = updating.clone();
            color_btn.connect_color_set(move |btn| {
                if upd_c.get() {
                    return;
                }
                let rgba = btn.rgba();
                let color = f32_to_rgb(rgba.red(), rgba.green(), rgba.blue());
                let _ = tx_c.send(GuiCommand::SetDpiColor {
                    stage: stage_num,
                    color,
                });
            });

            stages_group.add(&row);
            stage_rows.push(StageRow {
                row,
                x_spin,
                y_spin,
                color_btn,
            });
        }
        page.add(&stages_group);

        Self {
            widget: page,
            stages_spin,
            active_combo,
            stage_rows,
        }
    }

    pub fn sync(&self, snapshot: &DeviceSnapshot) {
        let interactive = snapshot.online && snapshot.config.is_some();
        self.widget.set_sensitive(interactive);

        let Some(ref cfg) = snapshot.config else {
            return;
        };

        let enabled = cfg.max_dpi_stages.clamp(1, 6);
        self.stages_spin.set_value(f64::from(enabled));

        let labels: Vec<String> = (0..usize::from(enabled))
            .map(|i| format!("Stage {} ({} DPI)", i + 1, cfg.dpi_stages[i].x))
            .collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        self.active_combo
            .set_model(Some(&gtk4::StringList::new(&refs)));
        self.active_combo
            .set_selected(u32::from(cfg.active_dpi_stage).min(u32::from(enabled - 1)));

        for (i, stage_ui) in self.stage_rows.iter().enumerate() {
            let visible = i < usize::from(enabled);
            stage_ui.row.set_visible(visible);
            let st = cfg.dpi_stages[i];
            stage_ui.x_spin.set_value(f64::from(st.x));
            stage_ui.y_spin.set_value(f64::from(st.y));
            let (r, g, b) = rgb_to_f32(st.color);
            stage_ui.color_btn.set_rgba(&gdk::RGBA::new(r, g, b, 1.0));
        }
    }
}

fn make_dpi_spin() -> gtk4::SpinButton {
    let spin = gtk4::SpinButton::with_range(50.0, 26_000.0, 50.0);
    spin.set_snap_to_ticks(true);
    spin.set_width_chars(6);
    spin.set_valign(gtk4::Align::Center);
    spin
}
