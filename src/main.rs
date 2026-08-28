// SPDX-License-Identifier: GPL-3.0-or-later

mod window;

use gtk::{gio, prelude::*};

const APPLICATION_ID: &str = "io.github.lgse.Strata";

fn main() -> gtk::glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_activate(window::present);
    application.run()
}
