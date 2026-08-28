// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    env,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{gio, glib, prelude::*};

#[derive(Clone)]
struct FileEntry {
    path: PathBuf,
    file_type: gio::FileType,
}

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

    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    columns.add_css_class("columns");
    columns.set_vexpand(true);
    append_directory_column(&columns, home_directory(), 0);

    let browser = gtk::ScrolledWindow::builder()
        .child(&columns)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content = gtk::Paned::new(gtk::Orientation::Horizontal);
    content.set_wide_handle(false);
    content.set_shrink_start_child(false);
    content.set_resize_start_child(false);
    content.set_position(220);
    content.set_vexpand(true);
    content.set_start_child(Some(&build_sidebar(&columns)));
    content.set_end_child(Some(&browser));
    root.append(&content);

    window.set_child(Some(&root));
    window.present();
}

fn build_sidebar(columns: &gtk::Box) -> gtk::Widget {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.add_css_class("sidebar");
    sidebar.set_size_request(180, -1);

    let heading = gtk::Label::new(Some("PLACES"));
    heading.set_xalign(0.0);
    heading.add_css_class("caption");
    sidebar.append(&heading);

    let home = home_directory();
    for (icon, name, path) in [
        ("user-home-symbolic", "Home", home.clone()),
        (
            "folder-documents-symbolic",
            "Documents",
            home.join("Documents"),
        ),
        (
            "folder-download-symbolic",
            "Downloads",
            home.join("Downloads"),
        ),
        (
            "folder-pictures-symbolic",
            "Pictures",
            home.join("Pictures"),
        ),
        ("folder-videos-symbolic", "Videos", home.join("Videos")),
    ] {
        let row = gtk::Button::builder()
            .icon_name(icon)
            .label(name)
            .halign(gtk::Align::Fill)
            .build();
        row.set_has_frame(false);

        let columns = columns.clone();
        row.connect_clicked(move |_| reset_columns(&columns, path.clone()));
        sidebar.append(&row);
    }

    sidebar.upcast()
}

fn append_directory_column(columns: &gtk::Box, path: PathBuf, depth: usize) {
    let column = build_directory_column(columns, path, depth);
    let revealer = gtk::Revealer::builder()
        .child(&column)
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(180)
        .reveal_child(false)
        .build();

    columns.append(&revealer);
    glib::idle_add_local_once(move || revealer.set_reveal_child(true));
}

fn build_directory_column(columns: &gtk::Box, path: PathBuf, depth: usize) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.add_css_class("directory-column");
    column.set_size_request(300, -1);
    column.set_vexpand(true);

    let name = display_directory_name(&path);
    let heading = gtk::Label::new(Some(&name));
    heading.set_xalign(0.0);
    heading.set_tooltip_text(Some(&path.to_string_lossy()));
    heading.add_css_class("column-header");
    column.append(&heading);

    let entries = Rc::new(RefCell::new(Vec::<FileEntry>::new()));
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
    list.set_single_click_activate(true);
    list.set_vexpand(true);

    let columns_for_activate = columns.clone();
    let entries_for_activate = entries.clone();
    list.connect_activate(move |_, position| {
        let Some(entry) = entries_for_activate
            .borrow()
            .get(position as usize)
            .cloned()
        else {
            return;
        };

        if entry.file_type == gio::FileType::Directory {
            remove_columns_after(&columns_for_activate, depth);
            append_directory_column(&columns_for_activate, entry.path, depth + 1);
        } else {
            open_file(&entry.path);
        }
    });

    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    column.append(&scroll);

    enumerate_directory(path, model, entries);
    column.upcast()
}

fn enumerate_directory(
    path: PathBuf,
    model: gtk::StringList,
    entries: Rc<RefCell<Vec<FileEntry>>>,
) {
    glib::MainContext::default().spawn_local(async move {
        let directory = gio::File::for_path(&path);
        let result = directory
            .enumerate_children_future(
                "standard::display-name,standard::name,standard::type,standard::is-hidden",
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
                    let mut batch: Vec<_> = files
                        .into_iter()
                        .filter(|info| !info.is_hidden())
                        .map(|info| {
                            let file_type = info.file_type();
                            let label = if file_type == gio::FileType::Directory {
                                format!("▸  {}", info.display_name())
                            } else {
                                format!("   {}", info.display_name())
                            };
                            let entry = FileEntry {
                                path: path.join(info.name()),
                                file_type,
                            };
                            (label, entry)
                        })
                        .collect();
                    batch.sort_unstable_by_key(|(label, _)| label.to_lowercase());

                    let mut current_entries = entries.borrow_mut();
                    for (label, entry) in batch {
                        current_entries.push(entry);
                        model.append(&label);
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

fn reset_columns(columns: &gtk::Box, path: PathBuf) {
    remove_all_columns(columns);
    append_directory_column(columns, path, 0);
}

fn remove_columns_after(columns: &gtk::Box, depth: usize) {
    let mut position = 0;
    let mut child = columns.first_child();

    while let Some(current) = child {
        child = current.next_sibling();
        if position > depth {
            columns.remove(&current);
        }
        position += 1;
    }
}

fn remove_all_columns(columns: &gtk::Box) {
    while let Some(child) = columns.first_child() {
        columns.remove(&child);
    }
}

fn open_file(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    if let Err(error) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        eprintln!("Unable to open {}: {error}", path.display());
    }
}

fn display_directory_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
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
