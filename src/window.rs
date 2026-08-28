// SPDX-License-Identifier: GPL-3.0-or-later

use std::{env, path::PathBuf};

use gtk::{gio, glib, prelude::*};

pub fn present(application: &gtk::Application) {
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
        .icon_name("system-search-symbolic")
        .tooltip_text("Search")
        .build();
    header.pack_end(&search_button);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);

    let content = gtk::Paned::new(gtk::Orientation::Horizontal);
    content.set_wide_handle(false);
    content.set_shrink_start_child(false);
    content.set_resize_start_child(false);
    content.set_position(220);
    content.set_vexpand(true);

    content.set_start_child(Some(&build_sidebar()));
    content.set_end_child(Some(&build_directory_column(home_directory())));
    root.append(&content);

    window.set_child(Some(&root));
    window.present();
}

fn build_sidebar() -> gtk::Widget {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.add_css_class("sidebar");
    sidebar.set_size_request(180, -1);

    let heading = gtk::Label::new(Some("PLACES"));
    heading.set_xalign(0.0);
    heading.add_css_class("caption");
    sidebar.append(&heading);

    for (icon, name) in [
        ("user-home-symbolic", "Home"),
        ("folder-documents-symbolic", "Documents"),
        ("folder-download-symbolic", "Downloads"),
        ("folder-pictures-symbolic", "Pictures"),
        ("folder-videos-symbolic", "Videos"),
    ] {
        let row = gtk::Button::builder()
            .icon_name(icon)
            .label(name)
            .halign(gtk::Align::Fill)
            .build();
        row.set_has_frame(false);
        sidebar.append(&row);
    }

    sidebar.upcast()
}

fn build_directory_column(path: PathBuf) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Home");
    let heading = gtk::Label::new(Some(name));
    heading.set_xalign(0.0);
    heading.add_css_class("column-header");
    column.append(&heading);

    let model = gtk::StringList::new(&[]);
    let selection = gtk::SingleSelection::new(Some(model.clone()));
    selection.set_autoselect(false);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item
            .downcast_ref::<gtk::ListItem>()
            .expect("factory item must be a ListItem");
        let label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item
            .downcast_ref::<gtk::ListItem>()
            .expect("factory item must be a ListItem");
        let value = item
            .item()
            .and_downcast::<gtk::StringObject>()
            .expect("model item must be a StringObject");
        let label = item
            .child()
            .and_downcast::<gtk::Label>()
            .expect("list child must be a Label");
        label.set_label(&value.string());
    });

    let list = gtk::ListView::new(Some(selection), Some(factory));
    list.add_css_class("file-list");
    list.set_vexpand(true);

    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    column.append(&scroll);

    enumerate_directory(path, model);
    column.upcast()
}

fn enumerate_directory(path: PathBuf, model: gtk::StringList) {
    glib::MainContext::default().spawn_local(async move {
        let directory = gio::File::for_path(path);
        let result = directory
            .enumerate_children_future(
                "standard::display-name,standard::type,standard::is-hidden",
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await;

        let Ok(enumerator) = result else {
            model.append("Unable to read this directory");
            return;
        };

        loop {
            match enumerator
                .next_files_future(128, glib::Priority::DEFAULT)
                .await
            {
                Ok(files) if files.is_empty() => break,
                Ok(files) => {
                    let mut entries: Vec<_> = files
                        .into_iter()
                        .filter(|info| !info.is_hidden())
                        .map(|info| {
                            let prefix = if info.file_type() == gio::FileType::Directory {
                                "▸  "
                            } else {
                                "   "
                            };
                            format!("{prefix}{}", info.display_name())
                        })
                        .collect();
                    entries.sort_unstable_by_key(|name| name.to_lowercase());
                    for entry in entries {
                        model.append(&entry);
                    }
                }
                Err(error) => {
                    model.append(&format!("Unable to continue: {error}"));
                    break;
                }
            }
        }
    });
}

fn home_directory() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn load_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
