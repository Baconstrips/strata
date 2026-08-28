// SPDX-License-Identifier: GPL-3.0-or-later

mod adapters;
mod app;
mod assets;
mod model;
mod services;
mod ui;

use gtk::{gio, prelude::*};

const APPLICATION_ID: &str = "io.github.lgse.Strata";

fn main() -> gtk::glib::ExitCode {
    if let Err(error) = tracing_subscriber::fmt::try_init() {
        eprintln!("Unable to initialize logging: {error}");
    }

    if let Err(error) = assets::prepare() {
        eprintln!("Unable to prepare bundled assets: {error}");
    }

    let application = gtk::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_activate(ui::present);
    application.run()
}
