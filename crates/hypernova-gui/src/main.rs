mod model;
mod ui;
mod worker;

use libadwaita as adw;
use libadwaita::prelude::*;

const APP_ID: &str = "io.github.neo_xd.HypernovaGui";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(ui::build_window);
    app.run();
}
