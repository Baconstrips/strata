// SPDX-License-Identifier: GPL-3.0-or-later

use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

use gtk::{gio, glib, prelude::*};

use crate::{
    app::{Browser, BrowserEvent},
    model::{FileEntry, Location},
    services::FileSource,
};

#[derive(Clone)]
struct ColumnView {
    model: gtk::StringList,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    entries: Rc<RefCell<Vec<FileEntry>>>,
    spinner: gtk::Spinner,
}

struct PeekView {
    revealer: gtk::Revealer,
    location: Location,
    model: gtk::StringList,
    entries: Rc<RefCell<Vec<FileEntry>>>,
    spinner: gtk::Spinner,
}

#[derive(Clone, Copy)]
pub struct PeekBehavior {
    pub open_delay: Duration,
    pub close_delay: Duration,
    pub fade_duration: Duration,
    pub item_limit: usize,
}

impl Default for PeekBehavior {
    fn default() -> Self {
        Self {
            open_delay: Duration::from_millis(180),
            close_delay: Duration::from_millis(80),
            fade_duration: Duration::from_millis(100),
            item_limit: 8,
        }
    }
}

struct ViewState {
    overlay: gtk::Overlay,
    columns_widget: gtk::Box,
    scroller: gtk::ScrolledWindow,
    columns: RefCell<Vec<ColumnView>>,
    peek: RefCell<Option<PeekView>>,
    pending_peek: RefCell<Option<glib::SourceId>>,
    pending_close: RefCell<Option<glib::SourceId>>,
    peek_anchor: RefCell<Option<gtk::Widget>>,
    peek_behavior: PeekBehavior,
    browser: Rc<Browser>,
}

pub struct BrowserView {
    state: Rc<ViewState>,
}

impl BrowserView {
    pub fn new(source: Rc<dyn FileSource>, peek_behavior: PeekBehavior) -> Self {
        let columns_widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        columns_widget.add_css_class("columns");
        columns_widget.set_halign(gtk::Align::Start);
        columns_widget.set_vexpand(true);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&columns_widget)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scroller));

        let browser = Browser::new(source);
        let state = Rc::new(ViewState {
            overlay,
            columns_widget,
            scroller,
            columns: RefCell::new(Vec::new()),
            peek: RefCell::new(None),
            pending_peek: RefCell::new(None),
            pending_close: RefCell::new(None),
            peek_anchor: RefCell::new(None),
            peek_behavior,
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
        self.state.overlay.clone().upcast()
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
            BrowserEvent::PeekStarted { location } => self.append_peek(&location),
            BrowserEvent::PeekEntriesAdded { entries } => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    append_entries(
                        &peek.model,
                        &peek.entries,
                        entries,
                        Some(self.peek_behavior.item_limit),
                    );
                }
            }
            BrowserEvent::PeekFinished => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    peek.spinner.stop();
                    peek.spinner.set_visible(false);
                    if peek.entries.borrow().is_empty() {
                        peek.model.append("This directory is empty");
                    }
                }
            }
            BrowserEvent::PeekFailed { message } => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    peek.spinner.stop();
                    peek.spinner.set_visible(false);
                    peek.model.append(&format!("Unable to read: {message}"));
                }
            }
            BrowserEvent::PeekClosed => self.close_peek_visual(),
            BrowserEvent::FocusChanged { depth, position } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    if let Some(position) = position {
                        column.selection.set_selected(position as u32);
                        column
                            .list
                            .scroll_to(position as u32, gtk::ListScrollFlags::FOCUS, None);
                    }
                    column.list.grab_focus();
                }
            }
            BrowserEvent::OpenRequested { location } => open_file(location.path()),
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
        let weak_state = Rc::downgrade(self);
        let hover_entries = entries.clone();
        factory.connect_setup(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let label = gtk::Label::builder()
                .halign(gtk::Align::Fill)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let motion = gtk::EventControllerMotion::new();
            let list_item = item.clone();
            let entries = hover_entries.clone();
            let anchor: gtk::Widget = label.clone().upcast();
            let weak_state_for_enter = weak_state.clone();
            motion.connect_enter(move |_, _, _| {
                let entry = entries.borrow().get(list_item.position() as usize).cloned();
                if let (Some(state), Some(entry)) = (weak_state_for_enter.upgrade(), entry) {
                    if entry.is_directory() {
                        state.schedule_peek(depth, entry.location, anchor.clone());
                    } else {
                        cancel_source(&state.pending_peek);
                        state.browser.close_peek();
                    }
                }
            });
            let weak_state_for_leave = weak_state.clone();
            motion.connect_leave(move |_| {
                if let Some(state) = weak_state_for_leave.upgrade() {
                    state.schedule_close_peek();
                }
            });
            label.add_controller(motion);
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

        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
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
            selection,
            list,
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

    fn schedule_peek(
        self: &Rc<Self>,
        origin_depth: usize,
        location: Location,
        anchor: gtk::Widget,
    ) {
        cancel_source(&self.pending_peek);
        cancel_source(&self.pending_close);
        if self
            .peek
            .borrow()
            .as_ref()
            .is_some_and(|peek| peek.location == location)
        {
            return;
        }
        self.peek_anchor.replace(Some(anchor));

        let weak_state = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(self.peek_behavior.open_delay, move || {
            if let Some(state) = weak_state.upgrade() {
                state.pending_peek.take();
                state.browser.begin_peek(origin_depth, location);
            }
        });
        self.pending_peek.replace(Some(source));
    }

    fn schedule_close_peek(self: &Rc<Self>) {
        cancel_source(&self.pending_peek);
        cancel_source(&self.pending_close);

        let weak_state = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(self.peek_behavior.close_delay, move || {
            if let Some(state) = weak_state.upgrade() {
                state.pending_close.take();
                state.browser.close_peek();
            }
        });
        self.pending_close.replace(Some(source));
    }

    fn append_peek(self: &Rc<Self>, location: &Location) {
        let anchor = self.peek_anchor.take();
        self.close_peek_visual();
        let Some(anchor) = anchor else {
            self.browser.close_peek();
            return;
        };

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_size_request(256, -1);
        content.set_overflow(gtk::Overflow::Hidden);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("column-header");
        let heading = gtk::Label::new(Some(&location.display_name()));
        heading.set_xalign(0.0);
        heading.set_hexpand(true);
        let spinner = gtk::Spinner::new();
        spinner.start();
        header.append(&heading);
        header.append(&spinner);
        content.append(&header);

        let entries = Rc::new(RefCell::new(Vec::<FileEntry>::new()));
        let model = gtk::StringList::new(&[]);
        let selection = gtk::NoSelection::new(Some(model.clone()));
        let factory = basic_label_factory();
        let list = gtk::ListView::new(Some(selection), Some(factory));
        list.add_css_class("file-list");
        let weak_browser = Rc::downgrade(&self.browser);
        list.connect_activate(move |_, _| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.commit_peek();
            }
        });
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(240)
            .propagate_natural_height(true)
            .build();
        content.append(&scroll);

        let motion = gtk::EventControllerMotion::new();
        let weak_state = Rc::downgrade(self);
        motion.connect_enter(move |_, _, _| {
            if let Some(state) = weak_state.upgrade() {
                cancel_source(&state.pending_close);
            }
        });
        let weak_state = Rc::downgrade(self);
        motion.connect_leave(move |_| {
            if let Some(state) = weak_state.upgrade() {
                state.schedule_close_peek();
            }
        });
        content.add_controller(motion);

        let click = gtk::GestureClick::new();
        let weak_browser = Rc::downgrade(&self.browser);
        click.connect_released(move |_, _, _, _| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.commit_peek();
            }
        });
        content.add_controller(click);

        let Some(bounds) = anchor.compute_bounds(&self.overlay) else {
            self.browser.close_peek();
            return;
        };
        content.add_css_class("peek-popover");
        let right = bounds.x() + bounds.width() + 4.0;
        let left = (bounds.x() - 260.0).max(0.0);
        let x = if right + 256.0 <= self.overlay.width() as f32 {
            right
        } else {
            left
        };
        let transition_duration = self
            .peek_behavior
            .fade_duration
            .as_millis()
            .min(u128::from(u32::MAX)) as u32;
        let revealer = gtk::Revealer::builder()
            .child(&content)
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(transition_duration)
            .reveal_child(false)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(x.round() as i32)
            .margin_top(bounds.y().round().max(0.0) as i32)
            .build();
        self.overlay.add_overlay(&revealer);
        self.peek.replace(Some(PeekView {
            revealer: revealer.clone(),
            location: location.clone(),
            model,
            entries,
            spinner,
        }));
        glib::idle_add_local_once(move || revealer.set_reveal_child(true));
    }

    fn close_peek_visual(&self) {
        cancel_source(&self.pending_peek);
        cancel_source(&self.pending_close);
        if let Some(peek) = self.peek.take() {
            peek.revealer.set_can_target(false);
            peek.revealer.set_reveal_child(false);
            let overlay = self.overlay.clone();
            let revealer = peek.revealer;
            let delay = Duration::from_millis(u64::from(revealer.transition_duration()));
            glib::timeout_add_local_once(delay, move || overlay.remove_overlay(&revealer));
        }
    }

    fn truncate(&self, len: usize) {
        self.close_peek_visual();
        while self.columns.borrow().len() > len {
            self.columns.borrow_mut().pop();
            if let Some(child) = self.columns_widget.last_child() {
                self.columns_widget.remove(&child);
            }
        }
    }
}

fn basic_label_factory() -> gtk::SignalListItemFactory {
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
    factory
}

fn append_entries(
    model: &gtk::StringList,
    stored: &Rc<RefCell<Vec<FileEntry>>>,
    entries: Vec<FileEntry>,
    limit: Option<usize>,
) {
    let mut stored = stored.borrow_mut();
    let remaining = limit
        .map(|limit| limit.max(1).saturating_sub(stored.len()))
        .unwrap_or(entries.len());
    for entry in entries.into_iter().take(remaining) {
        let prefix = if entry.is_directory() { "▸  " } else { "   " };
        model.append(&format!("{prefix}{}", entry.display_name));
        stored.push(entry);
    }
}

fn cancel_source(source: &RefCell<Option<glib::SourceId>>) {
    if let Some(source) = source.take() {
        source.remove();
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
