// SPDX-License-Identifier: GPL-3.0-or-later

use std::{cell::RefCell, path::Path, rc::Rc};

use gtk::{gio, glib, prelude::*};

use crate::{
    app::{Browser, BrowserEvent},
    model::{FileEntry, Location},
    services::FileSource,
};

#[derive(Clone)]
struct ColumnView {
    model: gtk::StringList,
    entries: Rc<RefCell<Vec<FileEntry>>>,
    spinner: gtk::Spinner,
}

struct ViewState {
    columns_widget: gtk::Box,
    scroller: gtk::ScrolledWindow,
    columns: RefCell<Vec<ColumnView>>,
    browser: Rc<Browser>,
}

pub struct BrowserView {
    state: Rc<ViewState>,
}

impl BrowserView {
    pub fn new(source: Rc<dyn FileSource>) -> Self {
        let columns_widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        columns_widget.add_css_class("columns");
        columns_widget.set_vexpand(true);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&columns_widget)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        let browser = Browser::new(source);
        let state = Rc::new(ViewState {
            columns_widget,
            scroller,
            columns: RefCell::new(Vec::new()),
            browser,
        });

        // The observer owns the view state while its window is alive. The window clears
        // the observer on destruction to break this deliberate lifecycle cycle.
        let observer_state = state.clone();
        state
            .browser
            .observe(move |event| observer_state.handle(event));

        Self { state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.state.scroller.clone().upcast()
    }

    pub fn navigate(&self, path: impl AsRef<Path>) {
        self.state
            .browser
            .navigate(Location::local(path.as_ref().to_path_buf()));
    }

    pub fn browser(&self) -> Rc<Browser> {
        self.state.browser.clone()
    }
}

impl ViewState {
    fn handle(self: &Rc<Self>, event: BrowserEvent) {
        match event {
            BrowserEvent::Reset => self.truncate(0),
            BrowserEvent::ColumnsTruncated { len } => self.truncate(len),
            BrowserEvent::ColumnAdded { depth, location } => self.append_column(depth, &location),
            BrowserEvent::EntriesAdded { depth, entries } => {
                if let Some(column) = self.columns.borrow().get(depth).cloned() {
                    let mut stored = column.entries.borrow_mut();
                    for entry in entries {
                        let prefix = if entry.is_directory() { "▸  " } else { "   " };
                        column
                            .model
                            .append(&format!("{prefix}{}", entry.display_name));
                        stored.push(entry);
                    }
                }
            }
            BrowserEvent::LoadFinished { depth } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.spinner.stop();
                    column.spinner.set_visible(false);
                    if column.entries.borrow().is_empty() {
                        column.model.append("This directory is empty");
                    }
                }
            }
            BrowserEvent::LoadFailed { depth, message } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.spinner.stop();
                    column.spinner.set_visible(false);
                    column.model.append(&format!("Unable to read: {message}"));
                }
            }
        }
    }

    fn append_column(self: &Rc<Self>, depth: usize, location: &Location) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.add_css_class("directory-column");
        column.set_size_request(300, -1);
        column.set_vexpand(true);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("column-header");
        let heading = gtk::Label::new(Some(&location.display_name()));
        heading.set_xalign(0.0);
        heading.set_hexpand(true);
        heading.set_tooltip_text(Some(&location.path().to_string_lossy()));
        let spinner = gtk::Spinner::new();
        spinner.start();
        header.append(&heading);
        header.append(&spinner);
        column.append(&header);

        let entries = Rc::new(RefCell::new(Vec::<FileEntry>::new()));
        let model = gtk::StringList::new(&[]);
        let selection = gtk::SingleSelection::new(Some(model.clone()));
        selection.set_autoselect(false);

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(value) = item.item().and_downcast::<gtk::StringObject>() else {
                return;
            };
            let Some(label) = item.child().and_downcast::<gtk::Label>() else {
                return;
            };
            label.set_label(&value.string());
        });

        let list = gtk::ListView::new(Some(selection), Some(factory));
        list.add_css_class("file-list");
        list.set_single_click_activate(true);
        list.set_vexpand(true);

        let weak_browser = Rc::downgrade(&self.browser);
        let entries_for_activate = entries.clone();
        list.connect_activate(move |_, position| {
            let entry = entries_for_activate
                .borrow()
                .get(position as usize)
                .cloned();
            let Some(entry) = entry else {
                return;
            };
            let Some(browser) = weak_browser.upgrade() else {
                return;
            };
            browser.select(depth, position as usize);
            if entry.is_directory() {
                browser.descend(depth, entry.location);
            } else {
                open_file(entry.location.path());
            }
        });

        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        column.append(&scroll);

        let revealer = gtk::Revealer::builder()
            .child(&column)
            .transition_type(gtk::RevealerTransitionType::SlideRight)
            .transition_duration(180)
            .reveal_child(false)
            .build();
        self.columns_widget.append(&revealer);
        self.columns.borrow_mut().push(ColumnView {
            model,
            entries,
            spinner,
        });

        let adjustment = self.scroller.hadjustment();
        let completed_adjustment = adjustment.clone();
        revealer.connect_child_revealed_notify(move |revealer| {
            if revealer.is_child_revealed() {
                scroll_to_end(&completed_adjustment);
            }
        });
        glib::idle_add_local_once(move || {
            revealer.set_reveal_child(true);
            scroll_to_end(&adjustment);
        });
    }

    fn truncate(&self, len: usize) {
        while self.columns.borrow().len() > len {
            self.columns.borrow_mut().pop();
            if let Some(child) = self.columns_widget.last_child() {
                self.columns_widget.remove(&child);
            }
        }
    }
}

fn scroll_to_end(adjustment: &gtk::Adjustment) {
    let end = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    adjustment.set_value(end);
}

fn open_file(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    if let Err(error) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        tracing::warn!(path = %path.display(), error = %error, "unable to open file");
    }
}
