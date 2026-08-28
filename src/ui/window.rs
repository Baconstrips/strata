// SPDX-License-Identifier: GPL-3.0-or-later

use std::{env, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};

use crate::{adapters::LocalFileSource, app::Browser, model::Location};

use super::browser::BrowserView;

pub fn present(application: &gtk::Application) {
    crate::assets::register_icon_theme();
    load_styles();

    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("Strata")
        .default_width(1200)
        .default_height(760)
        .build();

    let title = gtk::Label::new(Some("Strata"));
    title.add_css_class("title");
    let header = gtk::HeaderBar::builder().title_widget(&title).build();
    let search_button = gtk::Button::builder()
        .icon_name(crate::assets::icons::SEARCH)
        .tooltip_text("Search")
        .build();
    header.pack_end(&search_button);

    let browser = BrowserView::new(Rc::new(LocalFileSource));
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);

    let content = gtk::Paned::new(gtk::Orientation::Horizontal);
    content.set_wide_handle(false);
    content.set_shrink_start_child(false);
    content.set_resize_start_child(false);
    content.set_position(220);
    content.set_vexpand(true);
    content.set_start_child(Some(&build_sidebar(&browser.browser())));
    content.set_end_child(Some(&browser.widget()));
    root.append(&content);

    window.set_child(Some(&root));
    browser.navigate(home_directory());

    let browser_controller = browser.browser();
    window.connect_destroy(move |_| browser_controller.clear_observer());
    window.present();
}

fn build_sidebar(browser: &Rc<Browser>) -> gtk::Widget {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.add_css_class("sidebar");
    sidebar.set_size_request(180, -1);

    let heading = gtk::Label::new(Some("PLACES"));
    heading.set_xalign(0.0);
    heading.add_css_class("caption");
    sidebar.append(&heading);

    let home = home_directory();
    let mut places = vec![(crate::assets::icons::HOME, "Home", home)];
    for (icon, name, directory) in [
        (
            crate::assets::icons::DOCUMENTS,
            "Documents",
            glib::UserDirectory::Documents,
        ),
        (
            crate::assets::icons::DOWNLOADS,
            "Downloads",
            glib::UserDirectory::Downloads,
        ),
        (
            crate::assets::icons::PICTURES,
            "Pictures",
            glib::UserDirectory::Pictures,
        ),
        (
            crate::assets::icons::VIDEOS,
            "Videos",
            glib::UserDirectory::Videos,
        ),
    ] {
        if let Some(path) = glib::user_special_dir(directory) {
            places.push((icon, name, path));
        }
    }

    for (icon, name, path) in places {
        let row = gtk::Button::builder()
            .icon_name(icon)
            .label(name)
            .halign(gtk::Align::Fill)
            .build();
        row.set_has_frame(false);

        let weak_browser = Rc::downgrade(browser);
        row.connect_clicked(move |_| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.navigate(Location::local(path.clone()));
            }
        });
        sidebar.append(&row);
    }

    sidebar.upcast()
}

fn home_directory() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn load_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../style.css"));

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
