use libadwaita as adw;
use libadwaita::prelude::*;

const REPO_URL: &str = "https://github.com/Neo-XD/hypernova-ctl";

pub struct AboutPage {
    pub widget: adw::PreferencesPage,
}

impl AboutPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::new();

        let project_group = adw::PreferencesGroup::builder()
            .title("Hypernova Control")
            .description("Open-source Linux CLI, system tray indicator, and GTK4 desktop GUI for configuring the Cosmic Byte Hypernova gaming mouse.")
            .build();

        project_group.add(&info_row("Version", env!("CARGO_PKG_VERSION")));
        project_group.add(&info_row("Author", "Neo-XD"));
        project_group.add(&info_row("License", "MIT"));

        let repo_row = adw::ActionRow::builder()
            .title("Source Repository")
            .subtitle(REPO_URL)
            .build();
        let open_btn = gtk4::LinkButton::builder()
            .label("Open GitHub")
            .uri(REPO_URL)
            .valign(gtk4::Align::Center)
            .build();
        repo_row.add_suffix(&open_btn);
        project_group.add(&repo_row);
        page.add(&project_group);

        let hw_group = adw::PreferencesGroup::builder()
            .title("Supported Hardware")
            .description("Reverse-engineered USB HID protocol over Linux hidraw without detaching the kernel input driver.")
            .build();

        hw_group.add(&info_row("Wired USB", "Compx VID 0x3554, PID 0xF5FA (125 to 8000 Hz)"));
        hw_group.add(&info_row(
            "4K Wireless Dongle",
            "Compx VID 0x3554, PID 0xF5FB (125 to 4000 Hz)",
        ));
        hw_group.add(&info_row(
            "Sensor & MCUs",
            "PixArt PAW3395 · Nordic nRF52833 · WCH CH32V305",
        ));
        page.add(&hw_group);

        Self { widget: page }
    }
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .build()
}
