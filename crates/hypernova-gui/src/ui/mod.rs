mod about_page;
mod dpi_page;
mod power_page;
mod sensor_page;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::model::{GuiCommand, WorkerEvent};
use crate::worker::DeviceWorker;

pub fn build_window(app: &adw::Application) {
    let worker = DeviceWorker::spawn();
    let cmd_tx = worker.cmd_tx.clone();
    let updating = Rc::new(Cell::new(false));

    let view_stack = adw::ViewStack::new();
    let dpi_page = dpi_page::DpiPage::new(cmd_tx.clone(), updating.clone());
    let sensor_page = sensor_page::SensorPage::new(cmd_tx.clone(), updating.clone());
    let power_page = power_page::PowerPage::new(cmd_tx.clone(), updating.clone());
    let about_page = about_page::AboutPage::new();

    view_stack
        .add_titled(&dpi_page.widget, Some("dpi"), "DPI")
        .set_icon_name(Some("input-mouse-symbolic"));
    view_stack
        .add_titled(&sensor_page.widget, Some("sensor"), "Sensor")
        .set_icon_name(Some("preferences-system-symbolic"));
    view_stack
        .add_titled(&power_page.widget, Some("power"), "Power")
        .set_icon_name(Some("battery-good-symbolic"));
    view_stack
        .add_titled(&about_page.widget, Some("about"), "About")
        .set_icon_name(Some("help-about-symbolic"));

    let switcher_title = adw::ViewSwitcherTitle::builder()
        .stack(&view_stack)
        .title("Hypernova")
        .build();

    let refresh_btn = gtk4::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh device state")
        .build();
    let refresh_tx = cmd_tx.clone();
    refresh_btn.connect_clicked(move |_| {
        let _ = refresh_tx.send(GuiCommand::Refresh);
    });

    let header = adw::HeaderBar::builder()
        .title_widget(&switcher_title)
        .build();
    header.pack_end(&refresh_btn);

    let status_bar = gtk4::Label::builder()
        .label("Connecting to Cosmic Byte Hypernova...")
        .halign(gtk4::Align::Center)
        .margin_top(6)
        .margin_bottom(6)
        .css_classes(vec!["dim-label".to_string(), "caption".to_string()])
        .build();

    let banner_label = gtk4::Label::builder()
        .wrap(true)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    let banner_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    banner_box.set_halign(gtk4::Align::Center);
    banner_box.append(&banner_label);
    let banner_revealer = gtk4::Revealer::builder()
        .child(&banner_box)
        .reveal_child(false)
        .build();

    let switcher_bar = adw::ViewSwitcherBar::builder().stack(&view_stack).build();
    switcher_title
        .bind_property("title-visible", &switcher_bar, "reveal")
        .sync_create()
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&status_bar);
    content.append(&banner_revealer);
    content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    content.append(&view_stack);
    content.append(&switcher_bar);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Hypernova")
        .default_width(560)
        .default_height(660)
        .content(&toast_overlay)
        .build();

    let rx = worker.event_rx;
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(event) = rx.try_recv() {
            match event {
                WorkerEvent::Snapshot(snapshot) => {
                    updating.set(true);
                    status_bar.set_label(&snapshot.header_subtitle());
                    if let Some(msg) = snapshot.banner_message() {
                        banner_label.set_label(msg);
                        banner_revealer.set_reveal_child(true);
                    } else {
                        banner_revealer.set_reveal_child(false);
                    }
                    dpi_page.sync(&snapshot);
                    sensor_page.sync(&snapshot);
                    power_page.sync(&snapshot);
                    updating.set(false);
                }
                WorkerEvent::Toast(msg) => {
                    toast_overlay.add_toast(adw::Toast::new(&msg));
                }
            }
        }
        glib::ControlFlow::Continue
    });

    window.present();
}
