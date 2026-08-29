// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gio, glib, prelude::*};

use crate::{
    app::{Browser, BrowserEvent},
    model::{FileEntry, Location},
    services::FileSource,
};

use super::motion::{animations_enabled, emphasized_deceleration};

const COLUMN_WIDTH: i32 = 300;
const COLUMN_OFFSET: i32 = 24;
const COLUMN_TRANSITION: Duration = Duration::from_millis(220);

#[derive(Clone)]
struct LoadPresentation {
    stack: gtk::Stack,
    skeleton: gtk::Box,
    feedback: gtk::Box,
    message: gtk::Label,
    retry: Option<gtk::Button>,
}

#[derive(Clone)]
struct ColumnView {
    shell: gtk::Box,
    animation_generation: Rc<Cell<u64>>,
    presentation: LoadPresentation,
    model: gtk::StringList,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    entry_count: Rc<Cell<usize>>,
    spinner: gtk::Spinner,
}

struct PeekView {
    revealer: gtk::Revealer,
    location: Location,
    presentation: LoadPresentation,
    model: gtk::StringList,
    entry_count: Rc<Cell<usize>>,
    spinner: gtk::Spinner,
}

impl LoadPresentation {
    fn new(content: &impl IsA<gtk::Widget>, retry: Option<gtk::Button>) -> Self {
        let skeleton = gtk::Box::new(gtk::Orientation::Vertical, 9);
        skeleton.add_css_class("loading-skeleton");
        for width in [168, 124, 192, 148, 176, 112] {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            row.add_css_class("skeleton-row");
            row.set_size_request(width, 10);
            row.set_halign(gtk::Align::Start);
            skeleton.append(&row);
        }

        let feedback = gtk::Box::new(gtk::Orientation::Vertical, 8);
        feedback.add_css_class("directory-feedback");
        feedback.set_halign(gtk::Align::Center);
        feedback.set_valign(gtk::Align::Center);
        let message = gtk::Label::new(None);
        message.add_css_class("status-message");
        message.set_justify(gtk::Justification::Center);
        message.set_wrap(true);
        feedback.append(&message);
        if let Some(button) = retry.as_ref() {
            button.set_halign(gtk::Align::Center);
            feedback.append(button);
        }

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(100)
            .hexpand(true)
            .vexpand(true)
            .build();
        stack.add_named(content, Some("content"));
        stack.add_named(&skeleton, Some("loading"));
        stack.add_named(&feedback, Some("feedback"));
        stack.set_visible_child_name("loading");

        Self {
            stack,
            skeleton,
            feedback,
            message,
            retry,
        }
    }

    fn show_loading(&self) {
        self.skeleton.set_visible(true);
        self.feedback.set_visible(true);
        if let Some(retry) = self.retry.as_ref() {
            retry.set_visible(false);
        }
        self.stack.set_visible_child_name("loading");
    }

    fn show_content(&self) {
        self.stack.set_visible_child_name("content");
    }

    fn show_empty(&self) {
        self.message.set_text("This directory is empty");
        self.message.remove_css_class("error");
        if let Some(retry) = self.retry.as_ref() {
            retry.set_visible(false);
        }
        self.stack.set_visible_child_name("feedback");
    }

    fn show_error(&self, message: &str) {
        self.message.set_text(message);
        self.message.add_css_class("error");
        if let Some(retry) = self.retry.as_ref() {
            retry.set_visible(true);
        }
        self.stack.set_visible_child_name("feedback");
    }
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
    location_stack: gtk::Stack,
    breadcrumbs: gtk::Box,
    location_entry: gtk::Entry,
    location_error: gtk::Label,
    columns_widget: gtk::Box,
    scroller: gtk::ScrolledWindow,
    columns: RefCell<Vec<ColumnView>>,
    horizontal_scroll_generation: Rc<Cell<u64>>,
    peek: RefCell<Option<PeekView>>,
    pending_peek: RefCell<Option<glib::SourceId>>,
    pending_close: RefCell<Option<glib::SourceId>>,
    peek_anchor: RefCell<Option<gtk::Widget>>,
    peek_behavior: PeekBehavior,
    browser: Rc<Browser>,
}

#[derive(Clone)]
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

        let location_entry = gtk::Entry::builder()
            .hexpand(true)
            .width_chars(48)
            .placeholder_text("Enter an absolute path")
            .tooltip_text("Location (Ctrl+L)")
            .build();
        location_entry.add_css_class("location-entry");
        let location_error = gtk::Label::new(None);
        location_error.add_css_class("location-error");
        location_error.set_visible(false);
        location_error.set_xalign(0.0);
        let entry_control = gtk::Box::new(gtk::Orientation::Vertical, 0);
        entry_control.append(&location_entry);
        entry_control.append(&location_error);

        let breadcrumbs = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        breadcrumbs.add_css_class("breadcrumbs");
        let breadcrumb_scroller = gtk::ScrolledWindow::builder()
            .child(&breadcrumbs)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .build();
        let location_stack = gtk::Stack::builder()
            .hhomogeneous(false)
            .vhomogeneous(false)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(100)
            .build();
        location_stack.add_named(&breadcrumb_scroller, Some("breadcrumbs"));
        location_stack.add_named(&entry_control, Some("entry"));
        location_stack.set_visible_child_name("breadcrumbs");
        location_stack.add_css_class("location-control");
        location_stack.set_hexpand(true);

        let browser = Browser::new(source);
        let state = Rc::new(ViewState {
            overlay,
            location_stack,
            breadcrumbs,
            location_entry,
            location_error,
            columns_widget,
            scroller,
            columns: RefCell::new(Vec::new()),
            horizontal_scroll_generation: Rc::new(Cell::new(0)),
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

        let weak_state = Rc::downgrade(&state);
        state.location_entry.connect_activate(move |_| {
            if let Some(state) = weak_state.upgrade() {
                state.submit_location();
            }
        });

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

    pub fn location_widget(&self) -> gtk::Widget {
        self.state.location_stack.clone().upcast()
    }

    pub fn begin_location_edit(&self) {
        self.state.clear_location_error();
        self.state.location_stack.set_visible_child_name("entry");
        self.state.location_entry.grab_focus();
        self.state.location_entry.select_region(0, -1);
    }

    pub fn location_has_focus(&self) -> bool {
        self.state.location_entry.has_focus()
    }

    pub fn cancel_location_edit(&self) {
        self.state.restore_location_text();
        self.state.clear_location_error();
        self.state
            .location_stack
            .set_visible_child_name("breadcrumbs");
        self.state.browser.focus_active();
    }
}

impl ViewState {
    fn submit_location(self: &Rc<Self>) {
        let input = self.location_entry.text();
        match self.browser.navigate_input(input.as_str()) {
            Ok(()) => self.clear_location_error(),
            Err(error) => {
                self.location_entry.add_css_class("error");
                self.location_error.set_text(&error.to_string());
                self.location_error.set_visible(true);
                self.location_entry.grab_focus();
            }
        }
    }

    fn restore_location_text(&self) {
        if let Some(location) = self.browser.active_location() {
            self.location_entry.set_text(&location.display_path());
        }
    }

    fn sync_active_location(self: &Rc<Self>) {
        if let Some(location) = self.browser.active_location() {
            self.set_location(&location);
        }
    }

    fn set_location(self: &Rc<Self>, location: &Location) {
        self.location_entry.set_text(&location.display_path());
        while let Some(child) = self.breadcrumbs.first_child() {
            self.breadcrumbs.remove(&child);
        }

        let home = Location::local(glib::home_dir());
        let mut locations = location.breadcrumbs();
        if let Some(home_index) = locations.iter().position(|crumb| crumb == &home) {
            locations.drain(..home_index);
        }
        let last = locations.len().saturating_sub(1);
        for (index, crumb) in locations.into_iter().enumerate() {
            if index > 0 {
                let separator = gtk::Label::new(Some("/"));
                separator.add_css_class("breadcrumb-separator");
                self.breadcrumbs.append(&separator);
            }

            let label = if crumb == home {
                "~".to_owned()
            } else {
                crumb.display_name()
            };
            let button = gtk::Button::with_label(&label);
            button.add_css_class("breadcrumb");
            button.set_has_frame(false);
            button.set_tooltip_text(Some(&crumb.display_path()));
            if index == last {
                button.add_css_class("current");
                button.set_sensitive(false);
            } else {
                let weak = Rc::downgrade(self);
                button.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.browser.navigate(crumb.clone());
                    }
                });
            }
            self.breadcrumbs.append(&button);
        }
        self.location_stack.set_visible_child_name("breadcrumbs");
    }

    fn clear_location_error(&self) {
        self.location_entry.remove_css_class("error");
        self.location_error.set_visible(false);
        self.location_error.set_text("");
    }

    fn handle(self: &Rc<Self>, event: BrowserEvent) {
        match event {
            BrowserEvent::Reset => {
                self.truncate(0);
                self.clear_location_error();
            }
            BrowserEvent::ColumnsTruncated { len } => {
                self.truncate(len);
                self.sync_active_location();
            }
            BrowserEvent::ColumnAdded { depth, location } => {
                self.set_location(&location);
                self.clear_location_error();
                self.append_column(depth, &location);
            }
            BrowserEvent::EntriesInserted { depth, insertions } => {
                let render_started = Instant::now();
                let entry_count = insertions
                    .iter()
                    .map(|insertion| insertion.entries.len())
                    .sum();
                if let Some(column) = self.columns.borrow().get(depth).cloned() {
                    if entry_count > 0 {
                        column.presentation.show_content();
                    }
                    for insertion in insertions {
                        let labels: Vec<_> = insertion
                            .entries
                            .iter()
                            .map(|entry| format!("{}{}", entry_prefix(entry), entry.display_name))
                            .collect();
                        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                        column.model.splice(insertion.position as u32, 0, &labels);
                    }
                    column
                        .entry_count
                        .set(column.entry_count.get() + entry_count);
                    crate::metrics::mark_batch_rendered(entry_count, render_started);
                }
            }
            BrowserEvent::EntriesReplaced { depth, entries } => {
                if let Some(column) = self.columns.borrow().get(depth).cloned() {
                    if !entries.is_empty() {
                        column.presentation.show_content();
                    }
                    let labels: Vec<_> = entries
                        .iter()
                        .map(|entry| format!("{}{}", entry_prefix(entry), entry.display_name))
                        .collect();
                    let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                    column.model.splice(0, column.model.n_items(), &labels);
                    column.entry_count.set(entries.len());
                }
            }
            BrowserEvent::EntriesSpliced {
                depth,
                splices,
                selected,
            } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    let mut count = column.entry_count.get();
                    for splice in splices {
                        let labels: Vec<_> = splice
                            .entries
                            .iter()
                            .map(|entry| format!("{}{}", entry_prefix(entry), entry.display_name))
                            .collect();
                        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                        column
                            .model
                            .splice(splice.position as u32, splice.removed as u32, &labels);
                        count = count
                            .saturating_sub(splice.removed)
                            .saturating_add(splice.entries.len());
                    }
                    column.entry_count.set(count);
                    column.selection.set_selected(
                        selected
                            .map(|position| position as u32)
                            .unwrap_or(gtk::INVALID_LIST_POSITION),
                    );
                    if count == 0 {
                        column.presentation.show_empty();
                    } else {
                        column.presentation.show_content();
                    }
                }
            }
            BrowserEvent::ColumnReloaded { depth } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.model.splice(0, column.model.n_items(), &[]);
                    column.entry_count.set(0);
                    column.spinner.set_visible(true);
                    column.spinner.start();
                    column.presentation.show_loading();
                }
            }
            BrowserEvent::LoadFinished { depth } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.spinner.stop();
                    column.spinner.set_visible(false);
                    if column.entry_count.get() == 0 {
                        column.presentation.show_empty();
                    } else {
                        column.presentation.show_content();
                    }
                }
            }
            BrowserEvent::LoadFailed { depth, message } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.spinner.stop();
                    column.spinner.set_visible(false);
                    column
                        .presentation
                        .show_error(&format!("Unable to read this directory\n{message}"));
                }
            }
            BrowserEvent::PeekStarted { location } => self.append_peek(&location),
            BrowserEvent::PeekEntriesAdded { entries } => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    if !entries.is_empty() {
                        peek.presentation.show_content();
                    }
                    append_entries(
                        &peek.model,
                        &peek.entry_count,
                        entries,
                        Some(self.peek_behavior.item_limit),
                    );
                }
            }
            BrowserEvent::PeekFinished => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    peek.spinner.stop();
                    peek.spinner.set_visible(false);
                    if peek.entry_count.get() == 0 {
                        peek.presentation.show_empty();
                    } else {
                        peek.presentation.show_content();
                    }
                }
            }
            BrowserEvent::PeekFailed { message } => {
                if let Some(peek) = self.peek.borrow().as_ref() {
                    peek.spinner.stop();
                    peek.spinner.set_visible(false);
                    peek.presentation
                        .show_error(&format!("Unable to read this directory\n{message}"));
                }
            }
            BrowserEvent::PeekClosed => self.close_peek_visual(),
            BrowserEvent::SelectionChanged { depth, position } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    column.selection.set_selected(position as u32);
                    column
                        .list
                        .scroll_to(position as u32, gtk::ListScrollFlags::NONE, None);
                }
            }
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
            BrowserEvent::OpenRequested { location } => {
                open_location(&location, &self.overlay);
            }
            BrowserEvent::NavigationRejected { message } => {
                show_error_dialog(&self.overlay, "Unable to open directory", &message);
            }
        }
    }

    fn append_column(self: &Rc<Self>, depth: usize, location: &Location) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.add_css_class("directory-column");
        column.set_hexpand(true);
        column.set_vexpand(true);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("column-header");
        let heading = gtk::Label::new(Some(&location.display_name()));
        heading.set_xalign(0.0);
        heading.set_hexpand(true);
        heading.set_tooltip_text(Some(&location.display_path()));
        let spinner = gtk::Spinner::new();
        spinner.start();
        header.append(&heading);
        header.append(&spinner);
        column.append(&header);

        let entry_count = Rc::new(Cell::new(0));
        let model = gtk::StringList::new(&[]);
        let selection = gtk::SingleSelection::new(Some(model.clone()));
        selection.set_autoselect(false);

        let factory = gtk::SignalListItemFactory::new();
        let weak_state = Rc::downgrade(self);
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
            let anchor: gtk::Widget = label.clone().upcast();
            let weak_state_for_enter = weak_state.clone();
            motion.connect_enter(move |_, _, _| {
                if let Some(state) = weak_state_for_enter.upgrade() {
                    let entry = state.browser.entry_at(depth, list_item.position() as usize);
                    if let Some(entry) = entry {
                        if entry.is_directory() {
                            state.schedule_peek(depth, entry.location, anchor.clone());
                        } else {
                            cancel_source(&state.pending_peek);
                            state.browser.close_peek();
                        }
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
        list.connect_activate(move |_, position| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.activate(depth, position as usize);
            }
        });

        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let retry = gtk::Button::with_label("Retry");
        retry.add_css_class("retry-button");
        let weak_browser = Rc::downgrade(&self.browser);
        retry.connect_clicked(move |_| {
            if let Some(browser) = weak_browser.upgrade() {
                browser.retry_column(depth);
            }
        });
        let presentation = LoadPresentation::new(&scroll, Some(retry));
        column.append(&presentation.stack);

        let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        shell.set_size_request(COLUMN_WIDTH, -1);
        shell.set_vexpand(true);
        shell.set_overflow(gtk::Overflow::Hidden);
        shell.append(&column);
        let animation_generation = Rc::new(Cell::new(0));
        let previous = depth
            .checked_sub(1)
            .and_then(|previous| self.columns.borrow().get(previous).cloned())
            .map(|column| column.shell);
        self.columns_widget
            .insert_child_after(&shell, previous.as_ref());
        self.columns.borrow_mut().push(ColumnView {
            shell: shell.clone(),
            animation_generation: animation_generation.clone(),
            presentation,
            model,
            selection,
            list,
            entry_count,
            spinner,
        });

        animate_column_entry(&shell, &column, &animation_generation);
        self.reveal_column(shell);
    }

    fn reveal_column(self: &Rc<Self>, shell: gtk::Box) {
        let animation_id = self.horizontal_scroll_generation.get().saturating_add(1);
        self.horizontal_scroll_generation.set(animation_id);
        let weak = Rc::downgrade(self);
        let measured_shell = shell;
        let _tick = self.scroller.add_tick_callback(move |_, _| {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if state.horizontal_scroll_generation.get() != animation_id
                || measured_shell.parent().is_none()
            {
                return glib::ControlFlow::Break;
            }
            let adjustment = state.scroller.hadjustment();
            if measured_shell.width() <= 0 || adjustment.page_size() <= 0.0 {
                return glib::ControlFlow::Continue;
            }
            let Some(bounds) = measured_shell.compute_bounds(&state.columns_widget) else {
                return glib::ControlFlow::Continue;
            };
            let target = horizontal_reveal_target(
                adjustment.value(),
                adjustment.page_size(),
                adjustment.lower(),
                adjustment.upper(),
                f64::from(bounds.x()),
                f64::from(bounds.x() + bounds.width()),
            );
            animate_horizontal_scroll(
                &state.scroller,
                &adjustment,
                target,
                &state.horizontal_scroll_generation,
                animation_id,
            );
            glib::ControlFlow::Break
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

        let entry_count = Rc::new(Cell::new(0));
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
        let presentation = LoadPresentation::new(&scroll, None);
        presentation.stack.set_size_request(-1, 120);
        content.append(&presentation.stack);

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
            presentation,
            model,
            entry_count,
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

    fn truncate(self: &Rc<Self>, len: usize) {
        self.close_peek_visual();
        self.horizontal_scroll_generation
            .set(self.horizontal_scroll_generation.get().saturating_add(1));
        while self.columns.borrow().len() > len {
            let Some(column) = self.columns.borrow_mut().pop() else {
                break;
            };
            column
                .animation_generation
                .set(column.animation_generation.get().saturating_add(1));
            self.columns_widget.remove(&column.shell);
        }
        let retained = self
            .columns
            .borrow()
            .last()
            .map(|column| column.shell.clone());
        if let Some(retained) = retained {
            self.reveal_column(retained);
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

fn entry_prefix(entry: &FileEntry) -> &'static str {
    if entry.is_broken_symbolic_link() {
        "×  "
    } else if entry.is_directory() && entry.is_symbolic_link() {
        "▸↗ "
    } else if entry.is_directory() {
        "▸  "
    } else if entry.is_symbolic_link() {
        " ↗ "
    } else {
        "   "
    }
}

fn append_entries(
    model: &gtk::StringList,
    stored_count: &Rc<Cell<usize>>,
    entries: Vec<FileEntry>,
    limit: Option<usize>,
) {
    let remaining = limit
        .map(|limit| limit.max(1).saturating_sub(stored_count.get()))
        .unwrap_or(entries.len());
    let mut appended = 0;
    for entry in entries.into_iter().take(remaining) {
        model.append(&format!("{}{}", entry_prefix(&entry), entry.display_name));
        appended += 1;
    }
    stored_count.set(stored_count.get() + appended);
}

fn cancel_source(source: &RefCell<Option<glib::SourceId>>) {
    if let Some(source) = source.take() {
        source.remove();
    }
}

fn animate_column_entry(shell: &gtk::Box, column: &gtk::Box, generation: &Rc<Cell<u64>>) {
    let animation_id = generation.get().saturating_add(1);
    generation.set(animation_id);
    if !animations_enabled() {
        column.set_opacity(1.0);
        column.set_margin_start(0);
        return;
    }

    column.set_opacity(0.0);
    column.set_margin_start(COLUMN_OFFSET);
    let started = Instant::now();
    let shell = shell.clone();
    let column = column.clone();
    let generation = generation.clone();
    let _tick = shell.add_tick_callback(move |_, _| {
        if generation.get() != animation_id {
            return glib::ControlFlow::Break;
        }
        let progress =
            (started.elapsed().as_secs_f64() / COLUMN_TRANSITION.as_secs_f64()).clamp(0.0, 1.0);
        let eased = emphasized_deceleration(progress);
        column.set_opacity(eased);
        column.set_margin_start((f64::from(COLUMN_OFFSET) * (1.0 - eased)).round() as i32);
        if progress >= 1.0 {
            column.set_opacity(1.0);
            column.set_margin_start(0);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn horizontal_reveal_target(
    current: f64,
    page_size: f64,
    lower: f64,
    upper: f64,
    item_left: f64,
    item_right: f64,
) -> f64 {
    let viewport_right = current + page_size;
    let target = if item_right > viewport_right {
        item_right - page_size
    } else if item_left < current {
        item_left
    } else {
        current
    };
    target.clamp(lower, (upper - page_size).max(lower))
}

fn animate_horizontal_scroll(
    scroller: &gtk::ScrolledWindow,
    adjustment: &gtk::Adjustment,
    target: f64,
    generation: &Rc<Cell<u64>>,
    animation_id: u64,
) {
    let start = adjustment.value();
    if !animations_enabled() || (target - start).abs() < 0.5 {
        adjustment.set_value(target);
        return;
    }

    let started = Instant::now();
    let adjustment = adjustment.clone();
    let generation = generation.clone();
    let _tick = scroller.add_tick_callback(move |_, _| {
        if generation.get() != animation_id {
            return glib::ControlFlow::Break;
        }
        let progress =
            (started.elapsed().as_secs_f64() / COLUMN_TRANSITION.as_secs_f64()).clamp(0.0, 1.0);
        let eased = emphasized_deceleration(progress);
        adjustment.set_value(start + (target - start) * eased);
        if progress >= 1.0 {
            adjustment.set_value(target);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn open_location(location: &Location, parent: &impl IsA<gtk::Widget>) {
    let file = location
        .native_path()
        .map(gio::File::for_path)
        .unwrap_or_else(|| gio::File::for_uri(location.uri_value().unwrap_or_default()));
    let uri = file.uri();
    if let Err(error) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        tracing::warn!(location = %location.display_path(), error = %error, "unable to open file");
        show_error_dialog(parent, "Unable to open file", &error.to_string());
    }
}

fn show_error_dialog(parent: &impl IsA<gtk::Widget>, message: &str, detail: &str) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .build();
    let window = parent.root().and_downcast::<gtk::Window>();
    dialog.show(window.as_ref());
}

#[cfg(test)]
mod tests;
