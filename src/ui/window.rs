// SPDX-License-Identifier: GPL-3.0-or-later

use std::{env, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};

use crate::{
    adapters::LocalFileSource,
    app::Browser,
    model::{Location, SortKey},
};

use super::browser::{BrowserView, PeekBehavior};

pub fn present(application: &gtk::Application) {
    present_location(application, None);
}

pub fn present_location(application: &gtk::Application, location: Option<PathBuf>) {
    crate::assets::register_icon_theme();
    load_styles();

    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("Strata")
        .default_width(1200)
        .default_height(760)
        .build();

    let browser = BrowserView::new(Rc::new(LocalFileSource), PeekBehavior::default());
    let controller = browser.browser();

    let title = gtk::Label::new(Some("Strata"));
    title.add_css_class("title");
    let title_area = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    title_area.append(&title);
    title_area.append(&browser.location_widget());
    let header = gtk::HeaderBar::builder().title_widget(&title_area).build();
    let search_button = gtk::Button::builder()
        .icon_name(crate::assets::icons::SEARCH)
        .tooltip_text("Search")
        .build();
    header.pack_end(&search_button);

    let sort = gtk::DropDown::from_strings(&["Name", "Type", "Size", "Modified"]);
    sort.set_tooltip_text(Some("Sort directory"));
    let weak_controller = Rc::downgrade(&controller);
    sort.connect_selected_notify(move |sort| {
        let sort_key = match sort.selected() {
            1 => SortKey::Type,
            2 => SortKey::Size,
            3 => SortKey::Modified,
            _ => SortKey::Name,
        };
        if let Some(controller) = weak_controller.upgrade() {
            controller.set_sort_key(sort_key);
        }
    });
    header.pack_end(&sort);

    let direction = navigation_button("view-sort-ascending-symbolic", "Reverse sort direction");
    let weak_controller = Rc::downgrade(&controller);
    direction.connect_clicked(move |_| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.toggle_sort_direction();
        }
    });
    header.pack_end(&direction);

    let folders_first = gtk::ToggleButton::builder()
        .label("Folders first")
        .active(true)
        .tooltip_text("Keep folders before files")
        .build();
    let weak_controller = Rc::downgrade(&controller);
    folders_first.connect_toggled(move |button| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.set_folders_first(button.is_active());
        }
    });
    header.pack_end(&folders_first);

    let back = navigation_button("go-previous-symbolic", "Back");
    let weak_controller = Rc::downgrade(&controller);
    back.connect_clicked(move |_| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.back();
        }
    });
    header.pack_start(&back);

    let forward = navigation_button("go-next-symbolic", "Forward");
    let weak_controller = Rc::downgrade(&controller);
    forward.connect_clicked(move |_| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.forward();
        }
    });
    header.pack_start(&forward);

    let parent = navigation_button("go-up-symbolic", "Parent directory");
    let weak_controller = Rc::downgrade(&controller);
    parent.connect_clicked(move |_| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.parent();
        }
    });
    header.pack_start(&parent);

    let home = navigation_button(crate::assets::icons::HOME, "Home");
    let weak_controller = Rc::downgrade(&controller);
    home.connect_clicked(move |_| {
        if let Some(controller) = weak_controller.upgrade() {
            controller.navigate(Location::local(home_directory()));
        }
    });
    header.pack_start(&home);
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
    install_keyboard_navigation(&window, &browser);
    browser.navigate(location.unwrap_or_else(home_directory));

    let browser_controller = browser.browser();
    window.connect_destroy(move |_| browser_controller.clear_observer());
    window.present();
    crate::metrics::mark_window_presented();
}

fn install_keyboard_navigation(window: &gtk::ApplicationWindow, view: &BrowserView) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let view = view.clone();
    let weak_browser = Rc::downgrade(&view.browser());
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(browser) = weak_browser.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let alt = modifiers.contains(gtk::gdk::ModifierType::ALT_MASK);
        let control = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        if control && key == gtk::gdk::Key::l {
            view.begin_location_edit();
            return glib::Propagation::Stop;
        }
        if view.location_has_focus() {
            if key == gtk::gdk::Key::Escape {
                view.cancel_location_edit();
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }
        if control && key == gtk::gdk::Key::h {
            browser.toggle_hidden();
            return glib::Propagation::Stop;
        }

        match (key, alt) {
            (gtk::gdk::Key::Left, true) => browser.back(),
            (gtk::gdk::Key::Right, true) => browser.forward(),
            (gtk::gdk::Key::Home, true) => {
                browser.navigate(Location::local(home_directory()));
            }
            (gtk::gdk::Key::j | gtk::gdk::Key::Down, false) => browser.move_selection(1),
            (gtk::gdk::Key::k | gtk::gdk::Key::Up, false) => browser.move_selection(-1),
            (gtk::gdk::Key::h | gtk::gdk::Key::Left, false) => browser.focus_parent(),
            (
                gtk::gdk::Key::l
                | gtk::gdk::Key::Right
                | gtk::gdk::Key::Return
                | gtk::gdk::Key::KP_Enter,
                false,
            ) => browser.activate_focused(),
            (gtk::gdk::Key::Escape, false) => browser.escape(),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
}

fn navigation_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build()
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
