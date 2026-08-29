// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    env,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gio, glib, prelude::*};

use crate::{
    adapters::LocalFileSource,
    app::Browser,
    model::{Location, SortKey},
};

use super::{
    browser::{BrowserView, PeekBehavior},
    motion::{animations_enabled, emphasized_deceleration},
};

const SIDEBAR_WIDTH: i32 = 208;
const SIDEBAR_TRANSITION: Duration = Duration::from_millis(300);

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

    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let sidebar_toggle = gtk::ToggleButton::builder()
        .icon_name(crate::assets::icons::PANEL_LEFT)
        .active(true)
        .tooltip_text("Toggle sidebar")
        .build();
    sidebar_toggle.add_css_class("sidebar-toggle");
    header.pack_start(&sidebar_toggle);
    header.pack_start(&browser.location_widget());
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

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);

    let content = gtk::Paned::new(gtk::Orientation::Horizontal);
    content.set_wide_handle(false);
    content.set_shrink_start_child(true);
    content.set_resize_start_child(false);
    content.set_position(SIDEBAR_WIDTH);
    content.set_vexpand(true);
    let sidebar = build_sidebar(browser.browser());
    content.set_start_child(Some(&sidebar.widget));
    content.set_end_child(Some(&browser.widget()));
    let animation_generation = Rc::new(Cell::new(0));
    let animated_content = content.clone();
    let animated_sidebar = sidebar.widget.clone();
    sidebar_toggle.connect_toggled(move |toggle| {
        animate_sidebar(
            &animated_content,
            &animated_sidebar,
            &animation_generation,
            toggle.is_active(),
        );
    });
    root.append(&content);

    window.set_child(Some(&root));
    install_keyboard_navigation(&window, &browser, &sidebar_toggle);
    browser.navigate(location.unwrap_or_else(home_directory));

    let browser_controller = browser.browser();
    window.connect_destroy(move |_| {
        browser_controller.clear_observer();
        sidebar.disconnect();
    });
    window.present();
    crate::metrics::mark_window_presented();
}

fn animate_sidebar(
    paned: &gtk::Paned,
    sidebar: &gtk::Widget,
    generation: &Rc<Cell<u64>>,
    expanded: bool,
) {
    let animation_id = generation.get().saturating_add(1);
    generation.set(animation_id);
    let target = if expanded { SIDEBAR_WIDTH } else { 0 };
    let start = paned.position();
    if expanded {
        sidebar.set_visible(true);
    }

    if !animations_enabled() || start == target {
        paned.set_position(target);
        sidebar.set_visible(expanded);
        return;
    }

    let started = Instant::now();
    let paned = paned.clone();
    let sidebar = sidebar.clone();
    let generation = generation.clone();
    let _tick = paned.clone().add_tick_callback(move |_, _| {
        if generation.get() != animation_id {
            return glib::ControlFlow::Break;
        }

        let progress =
            (started.elapsed().as_secs_f64() / SIDEBAR_TRANSITION.as_secs_f64()).clamp(0.0, 1.0);
        let eased = emphasized_deceleration(progress);
        let position = f64::from(start) + f64::from(target - start) * eased;
        paned.set_position(position.round() as i32);

        if progress >= 1.0 {
            paned.set_position(target);
            if !expanded {
                sidebar.set_visible(false);
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn install_keyboard_navigation(
    window: &gtk::ApplicationWindow,
    view: &BrowserView,
    sidebar_toggle: &gtk::ToggleButton,
) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let view = view.clone();
    let sidebar_toggle = sidebar_toggle.clone();
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
        if control && key == gtk::gdk::Key::b {
            sidebar_toggle.set_active(!sidebar_toggle.is_active());
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
            (gtk::gdk::Key::Up, true) => browser.parent(),
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

struct SidebarState {
    widget: gtk::Box,
    browser: Rc<Browser>,
    volume_monitor: gio::VolumeMonitor,
}

struct SidebarView {
    widget: gtk::Widget,
    state: Rc<SidebarState>,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
}

impl SidebarView {
    fn disconnect(&self) {
        for handler in self.handlers.take() {
            self.state.volume_monitor.disconnect(handler);
        }
    }
}

impl SidebarState {
    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.widget.first_child() {
            self.widget.remove(&child);
        }

        self.append_place(
            crate::assets::icons::HOME,
            "Home",
            Location::local(home_directory()),
        );
        self.append_place(
            crate::assets::icons::TRASH,
            "Trash",
            Location::uri("trash:///"),
        );
        self.append_separator();

        for (icon, name, directory) in [
            (
                crate::assets::icons::FOLDER,
                "Desktop",
                glib::UserDirectory::Desktop,
            ),
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
                self.append_place(icon, name, Location::local(path));
            }
        }

        let volumes = self.volume_monitor.volumes();
        let mounts: Vec<_> = self
            .volume_monitor
            .mounts()
            .into_iter()
            .filter(|mount| !mount.is_shadowed() && mount.volume().is_none())
            .map(|mount| {
                let root = mount.root();
                let location = root
                    .path()
                    .map(Location::local)
                    .unwrap_or_else(|| Location::uri(root.uri()));
                (mount.name().to_string(), location)
            })
            .collect();
        if !volumes.is_empty() || !mounts.is_empty() {
            self.append_separator();
            for volume in volumes {
                self.append_volume(volume);
            }
            for (name, location) in mounts {
                self.append_place(crate::assets::icons::HARD_DRIVE, &name, location);
            }
        }
    }

    fn append_separator(&self) {
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.add_css_class("sidebar-separator");
        self.widget.append(&separator);
    }

    fn append_volume(&self, volume: gio::Volume) {
        let name = volume.name().to_string();
        let row = sidebar_button(crate::assets::icons::HARD_DRIVE, &name);
        row.set_tooltip_text(Some(&name));
        let weak_browser = Rc::downgrade(&self.browser);
        row.connect_clicked(move |button| {
            let Some(browser) = weak_browser.upgrade() else {
                return;
            };
            if let Some(mount) = volume.get_mount() {
                navigate_to_gio_file(&browser, &mount.root());
                return;
            }

            let window = button.root().and_downcast::<gtk::Window>();
            let operation = gtk::MountOperation::new(window.as_ref());
            let volume = volume.clone();
            glib::MainContext::default().spawn_local(async move {
                match volume
                    .mount_future(gio::MountMountFlags::NONE, Some(&operation))
                    .await
                {
                    Ok(()) => {
                        if let Some(mount) = volume.get_mount() {
                            navigate_to_gio_file(&browser, &mount.root());
                        }
                    }
                    Err(error) => {
                        let dialog = gtk::AlertDialog::builder()
                            .modal(true)
                            .message("Unable to mount volume")
                            .detail(error.to_string())
                            .build();
                        dialog.show(window.as_ref());
                    }
                }
            });
        });
        self.widget.append(&row);
    }

    fn append_place(&self, icon: &str, name: &str, location: Location) {
        let row = sidebar_button(icon, name);
        row.set_tooltip_text(Some(&location.display_path()));
        let weak_browser = Rc::downgrade(&self.browser);
        row.connect_clicked(move |_| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.navigate(location.clone());
            }
        });
        self.widget.append(&row);
    }
}

fn sidebar_button(icon: &str, name: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(18);
    let label = gtk::Label::new(Some(name));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&image);
    content.append(&label);

    let row = gtk::Button::builder()
        .child(&content)
        .halign(gtk::Align::Fill)
        .build();
    row.add_css_class("sidebar-row");
    row.set_has_frame(false);
    row
}

fn navigate_to_gio_file(browser: &Rc<Browser>, file: &gio::File) {
    let location = file
        .path()
        .map(Location::local)
        .unwrap_or_else(|| Location::uri(file.uri()));
    browser.navigate(location);
}

fn build_sidebar(browser: Rc<Browser>) -> SidebarView {
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 2);
    widget.add_css_class("sidebar");
    let scroller = gtk::ScrolledWindow::builder()
        .child(&widget)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .width_request(SIDEBAR_WIDTH)
        .build();
    scroller.add_css_class("sidebar-scroll");
    let volume_monitor = gio::VolumeMonitor::get();
    let state = Rc::new(SidebarState {
        widget,
        browser,
        volume_monitor,
    });

    let mut handlers = Vec::new();
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_mount_added(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_mount_removed(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_mount_changed(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_volume_added(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_volume_removed(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    let weak = Rc::downgrade(&state);
    handlers.push(state.volume_monitor.connect_volume_changed(move |_, _| {
        if let Some(state) = weak.upgrade() {
            state.rebuild();
        }
    }));
    state.rebuild();

    SidebarView {
        widget: scroller.upcast(),
        state,
        handlers: RefCell::new(handlers),
    }
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
