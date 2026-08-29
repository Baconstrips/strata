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
    model::{FileEntry, Location, SortDirection, SortKey},
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

struct BoundRow {
    item: glib::WeakRef<gtk::ListItem>,
    row: glib::WeakRef<gtk::Box>,
}

#[derive(Clone)]
struct ColumnView {
    shell: gtk::Box,
    animation_generation: Rc<Cell<u64>>,
    presentation: LoadPresentation,
    model: gtk::StringList,
    filtered_model: gtk::FilterListModel,
    filter_entry: gtk::Entry,
    filter_button: gtk::ToggleButton,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    bound_rows: Rc<RefCell<Vec<BoundRow>>>,
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
    peek_enabled: Cell<bool>,
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
        let confirm_location = gtk::Button::builder()
            .icon_name(crate::assets::icons::CHECK)
            .tooltip_text("Navigate (Enter)")
            .build();
        confirm_location.add_css_class("location-action");
        let cancel_location = gtk::Button::builder()
            .icon_name(crate::assets::icons::X)
            .tooltip_text("Cancel (Escape)")
            .build();
        cancel_location.add_css_class("location-action");
        let entry_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        entry_row.append(&location_entry);
        entry_row.append(&confirm_location);
        entry_row.append(&cancel_location);
        let entry_control = gtk::Box::new(gtk::Orientation::Vertical, 0);
        entry_control.append(&entry_row);
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
        location_stack.set_valign(gtk::Align::Center);

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
            peek_enabled: Cell::new(true),
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
        let weak_state = Rc::downgrade(&state);
        confirm_location.connect_clicked(move |_| {
            if let Some(state) = weak_state.upgrade() {
                state.submit_location();
            }
        });
        let weak_state = Rc::downgrade(&state);
        cancel_location.connect_clicked(move |_| {
            if let Some(state) = weak_state.upgrade() {
                state.cancel_location_edit();
            }
        });
        breadcrumb_scroller.set_cursor_from_name(Some("text"));
        let edit_location = gtk::GestureClick::new();
        let weak_state = Rc::downgrade(&state);
        edit_location.connect_released(move |gesture, _, x, y| {
            let clicked_button = gesture
                .widget()
                .and_then(|widget| widget.pick(x, y, gtk::PickFlags::DEFAULT))
                .is_some_and(is_breadcrumb_target);
            if !clicked_button {
                if let Some(state) = weak_state.upgrade() {
                    state.begin_location_edit();
                }
            }
        });
        breadcrumb_scroller.add_controller(edit_location);

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
        self.state.begin_location_edit();
    }

    pub fn location_has_focus(&self) -> bool {
        self.state.location_entry.has_focus()
    }

    pub fn cancel_location_edit(&self) {
        self.state.cancel_location_edit();
    }

    pub fn set_peek_enabled(&self, enabled: bool) {
        self.state.peek_enabled.set(enabled);
        if !enabled {
            cancel_source(&self.state.pending_peek);
            self.state.browser.close_peek();
        }
    }

    pub fn dismiss_focused_filter(&self) -> bool {
        let focused = self.state.overlay.root().and_then(|root| root.focus());
        let columns = self.state.columns.borrow();
        let Some(column) = columns.iter().find(|column| {
            column.filter_entry.has_focus()
                || focused.as_ref().is_some_and(|focused| {
                    focused == column.filter_entry.upcast_ref::<gtk::Widget>()
                        || focused.is_ancestor(&column.filter_entry)
                })
        }) else {
            return false;
        };
        column.filter_button.set_active(false);
        column.list.grab_focus();
        true
    }
}

impl ViewState {
    fn begin_location_edit(&self) {
        self.clear_location_error();
        self.location_stack.set_visible_child_name("entry");
        self.location_entry.grab_focus();
        self.location_entry.select_region(0, -1);
    }

    fn cancel_location_edit(&self) {
        self.restore_location_text();
        self.clear_location_error();
        self.location_stack.set_visible_child_name("breadcrumbs");
        self.browser.focus_active();
    }

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
        let starts_at_root = locations
            .first()
            .and_then(Location::native_path)
            .is_some_and(|path| path == Path::new("/"));
        let last = locations.len().saturating_sub(1);
        for (index, crumb) in locations.into_iter().enumerate() {
            if index > 0 && !(starts_at_root && index == 1) {
                let separator = gtk::Label::new(Some("/"));
                separator.add_css_class("breadcrumb-separator");
                self.breadcrumbs.append(&separator);
            }

            let label = if crumb == home {
                "~".to_owned()
            } else {
                crumb.display_name()
            };
            if index == last {
                let current = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                current.add_css_class("current-breadcrumb");
                let current_label = gtk::Label::new(Some(&label));
                current_label.add_css_class("breadcrumb");
                current_label.add_css_class("current");
                current_label.set_tooltip_text(Some(&crumb.display_path()));
                let copy = gtk::Button::builder()
                    .icon_name(crate::assets::icons::COPY)
                    .tooltip_text("Copy path")
                    .build();
                copy.add_css_class("copy-path");
                copy.set_has_frame(false);
                copy.set_cursor_from_name(Some("pointer"));
                let copied_path = location.display_path();
                let feedback_generation = Rc::new(Cell::new(0_u64));
                copy.connect_clicked(move |button| {
                    if let Some(display) = gtk::gdk::Display::default() {
                        display.clipboard().set_text(&copied_path);
                    }
                    let generation = feedback_generation.get().saturating_add(1);
                    feedback_generation.set(generation);
                    button.set_icon_name(crate::assets::icons::CHECK);
                    button.set_tooltip_text(Some("Path copied"));
                    let button = button.clone();
                    let feedback_generation = feedback_generation.clone();
                    glib::timeout_add_local_once(Duration::from_secs(2), move || {
                        if feedback_generation.get() == generation {
                            button.set_icon_name(crate::assets::icons::COPY);
                            button.set_tooltip_text(Some("Copy path"));
                        }
                    });
                });
                current.append(&current_label);
                current.append(&copy);
                self.breadcrumbs.append(&current);
            } else {
                let button = gtk::Button::with_label(&label);
                button.add_css_class("breadcrumb");
                if crumb
                    .native_path()
                    .is_some_and(|path| path == Path::new("/"))
                {
                    button.add_css_class("breadcrumb-root");
                }
                button.set_has_frame(false);
                button.set_tooltip_text(Some(&crumb.display_path()));
                button.set_cursor_from_name(Some("pointer"));
                let weak = Rc::downgrade(self);
                button.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.browser.navigate(crumb.clone());
                    }
                });
                self.breadcrumbs.append(&button);
            }
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
                        let labels: Vec<_> =
                            insertion.entries.iter().map(entry_model_value).collect();
                        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                        column.model.splice(insertion.position as u32, 0, &labels);
                    }
                    let count = column.entry_count.get() + entry_count;
                    column.entry_count.set(count);
                    set_filter_placeholder(&column, count);
                    crate::metrics::mark_batch_rendered(entry_count, render_started);
                }
            }
            BrowserEvent::EntriesReplaced { depth, entries } => {
                if let Some(column) = self.columns.borrow().get(depth).cloned() {
                    if !entries.is_empty() {
                        column.presentation.show_content();
                    }
                    let labels: Vec<_> = entries.iter().map(entry_model_value).collect();
                    let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                    column.model.splice(0, column.model.n_items(), &labels);
                    column.entry_count.set(entries.len());
                    set_filter_placeholder(&column, entries.len());
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
                        let labels: Vec<_> = splice.entries.iter().map(entry_model_value).collect();
                        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
                        column
                            .model
                            .splice(splice.position as u32, splice.removed as u32, &labels);
                        count = count
                            .saturating_sub(splice.removed)
                            .saturating_add(splice.entries.len());
                    }
                    column.entry_count.set(count);
                    set_filter_placeholder(column, count);
                    column.selection.set_selected(
                        selected
                            .and_then(|position| filtered_position_for_source(column, position))
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
                    set_filter_placeholder(column, 0);
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
                    if let Some(filtered_position) = filtered_position_for_source(column, position)
                    {
                        column.selection.set_selected(filtered_position);
                        column
                            .list
                            .scroll_to(filtered_position, gtk::ListScrollFlags::NONE, None);
                    }
                }
            }
            BrowserEvent::FocusChanged { depth, position } => {
                if let Some(column) = self.columns.borrow().get(depth) {
                    if let Some(filtered_position) =
                        position.and_then(|position| filtered_position_for_source(column, position))
                    {
                        column.selection.set_selected(filtered_position);
                        column
                            .list
                            .scroll_to(filtered_position, gtk::ListScrollFlags::FOCUS, None);
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
        self.refresh_active_path_rows();
    }

    fn refresh_active_path_rows(&self) {
        for (depth, column) in self.columns.borrow().iter().enumerate() {
            let active = self
                .browser
                .active_child_position(depth)
                .and_then(|position| filtered_position_for_source(column, position));
            column.bound_rows.borrow_mut().retain(|bound| {
                let (Some(item), Some(row)) = (bound.item.upgrade(), bound.row.upgrade()) else {
                    return false;
                };
                set_active_path_style(&row, active == Some(item.position()));
                true
            });
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
        let header_actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header_actions.add_css_class("column-header-actions");
        header_actions.append(&column_sort_direction_toggle(&self.browser, depth));
        header_actions.append(&column_sort_menu(&self.browser, depth));

        let filter_entry = gtk::Entry::builder()
            .placeholder_text("Filter 0 items…")
            .has_frame(false)
            .hexpand(true)
            .build();
        filter_entry.add_css_class("column-filter-entry");
        let filter_icon = gtk::Image::from_icon_name(crate::assets::icons::FUNNEL);
        filter_icon.set_pixel_size(16);
        let filter_control = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        filter_control.add_css_class("column-filter");
        filter_control.append(&filter_icon);
        filter_control.append(&filter_entry);
        let filter_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .child(&filter_control)
            .build();
        let filter_button = gtk::ToggleButton::builder()
            .icon_name(crate::assets::icons::FUNNEL)
            .tooltip_text("Filter this pane")
            .build();
        filter_button.add_css_class("column-header-action");
        let shown_filter = filter_revealer.clone();
        let focused_filter = filter_entry.clone();
        filter_button.connect_toggled(move |button| {
            shown_filter.set_reveal_child(button.is_active());
            if button.is_active() {
                focused_filter.grab_focus();
            } else {
                focused_filter.set_text("");
            }
        });
        header_actions.append(&filter_button);
        if depth > 0 {
            let close = gtk::Button::builder()
                .icon_name(crate::assets::icons::X)
                .tooltip_text("Close this pane")
                .build();
            close.add_css_class("column-header-action");
            let weak_browser = Rc::downgrade(&self.browser);
            close.connect_clicked(move |_| {
                if let Some(browser) = weak_browser.upgrade() {
                    browser.close_column(depth);
                }
            });
            header_actions.append(&close);
        }
        header.append(&header_actions);
        column.append(&header);
        column.append(&filter_revealer);

        let entry_count = Rc::new(Cell::new(0));
        let model = gtk::StringList::new(&[]);
        let filter_query = Rc::new(RefCell::new(String::new()));
        let query = filter_query.clone();
        let filter = gtk::CustomFilter::new(move |item| {
            let Some(item) = item.downcast_ref::<gtk::StringObject>() else {
                return false;
            };
            let query = query.borrow();
            query.is_empty()
                || model_display_name(&item.string())
                    .to_lowercase()
                    .contains(query.as_str())
        });
        let filtered_model = gtk::FilterListModel::new(Some(model.clone()), Some(filter.clone()));
        let selection = gtk::SingleSelection::new(Some(filtered_model.clone()));
        selection.set_autoselect(false);
        filter_entry.connect_changed(move |entry| {
            *filter_query.borrow_mut() = entry.text().to_lowercase();
            filter.changed(gtk::FilterChange::Different);
        });

        let factory = gtk::SignalListItemFactory::new();
        let bound_rows: Rc<RefCell<Vec<BoundRow>>> = Rc::new(RefCell::new(Vec::new()));
        let rows_for_setup = bound_rows.clone();
        let weak_state = Rc::downgrade(self);
        let source_for_hover = model.clone();
        let filtered_for_hover = filtered_model.clone();
        factory.connect_setup(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("file-row");
            let icon = gtk::Image::new();
            icon.add_css_class("file-icon");
            icon.set_pixel_size(17);
            let label = gtk::Label::builder()
                .halign(gtk::Align::Fill)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let size = gtk::Label::new(None);
            size.add_css_class("file-size");
            size.set_xalign(1.0);
            let chevron = gtk::Image::from_icon_name(crate::assets::icons::CHEVRON_RIGHT);
            chevron.add_css_class("file-chevron");
            chevron.set_pixel_size(15);
            row.append(&icon);
            row.append(&label);
            row.append(&size);
            row.append(&chevron);
            let motion = gtk::EventControllerMotion::new();
            let list_item = item.clone();
            let anchor: gtk::Widget = row.clone().upcast();
            let weak_state_for_enter = weak_state.clone();
            let source_for_enter = source_for_hover.clone();
            let filtered_for_enter = filtered_for_hover.clone();
            motion.connect_enter(move |_, _, _| {
                if let Some(state) = weak_state_for_enter.upgrade() {
                    let source_position = source_position_for_filtered(
                        &source_for_enter,
                        &filtered_for_enter,
                        list_item.position(),
                    );
                    let entry = source_position
                        .and_then(|position| state.browser.entry_at(depth, position));
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
            row.add_controller(motion);
            item.set_child(Some(&row));
            let weak_item = glib::WeakRef::new();
            weak_item.set(Some(item));
            let weak_row = glib::WeakRef::new();
            weak_row.set(Some(&row));
            rows_for_setup.borrow_mut().push(BoundRow {
                item: weak_item,
                row: weak_row,
            });
        });
        let source_for_bind = model.clone();
        let filtered_for_bind = filtered_model.clone();
        let weak_browser_for_bind = Rc::downgrade(&self.browser);
        factory.connect_bind(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(value) = item.item().and_downcast::<gtk::StringObject>() else {
                return;
            };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            let Some(icon) = row.first_child().and_downcast::<gtk::Image>() else {
                return;
            };
            let Some(label) = icon.next_sibling().and_downcast::<gtk::Label>() else {
                return;
            };
            let Some(size) = label.next_sibling().and_downcast::<gtk::Label>() else {
                return;
            };
            let Some(chevron) = size.next_sibling().and_downcast::<gtk::Image>() else {
                return;
            };
            label.set_label(model_display_name(&value.string()));
            let source_position =
                source_position_for_filtered(&source_for_bind, &filtered_for_bind, item.position());
            let browser = weak_browser_for_bind.upgrade();
            let entry =
                source_position.and_then(|position| browser.as_ref()?.entry_at(depth, position));
            let active = source_position.is_some_and(|position| {
                browser
                    .as_ref()
                    .and_then(|browser| browser.active_child_position(depth))
                    == Some(position)
            });
            set_active_path_style(&row, active);
            if let Some(entry) = entry.as_ref() {
                crate::assets::set_primary_icon(&icon, entry_icon(entry));
                icon.set_opacity(if entry.is_directory() { 1.0 } else { 0.72 });
                chevron.set_visible(entry.is_directory());
            } else {
                crate::assets::set_primary_icon(&icon, crate::assets::icons::DOCUMENTS);
                icon.set_opacity(0.72);
                chevron.set_visible(false);
            }
            let size_text = entry
                .filter(|entry| !entry.is_directory())
                .and_then(|entry| match entry.size {
                    crate::model::MetadataValue::Known(bytes) => Some(format_file_size(bytes)),
                    crate::model::MetadataValue::Unknown
                    | crate::model::MetadataValue::Unavailable => None,
                })
                .unwrap_or_default();
            size.set_label(&size_text);
        });

        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        list.add_css_class("file-list");
        list.set_single_click_activate(true);
        list.set_vexpand(true);

        let weak_browser = Rc::downgrade(&self.browser);
        let source_for_activation = model.clone();
        let filtered_for_activation = filtered_model.clone();
        list.connect_activate(move |_, position| {
            let source_position = source_position_for_filtered(
                &source_for_activation,
                &filtered_for_activation,
                position,
            );
            if let (Some(browser), Some(source_position)) =
                (weak_browser.upgrade(), source_position)
            {
                browser.activate(depth, source_position);
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
            filtered_model,
            filter_entry,
            filter_button,
            selection,
            list,
            bound_rows,
            entry_count,
            spinner,
        });

        self.refresh_active_path_rows();
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
        if !self.peek_enabled.get() {
            return;
        }
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
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::new();
        icon.add_css_class("file-icon");
        icon.set_pixel_size(17);
        let label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let chevron = gtk::Image::from_icon_name(crate::assets::icons::CHEVRON_RIGHT);
        chevron.add_css_class("file-chevron");
        chevron.set_pixel_size(15);
        row.append(&icon);
        row.append(&label);
        row.append(&chevron);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(value) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(row) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(icon) = row.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(label) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(chevron) = label.next_sibling().and_downcast::<gtk::Image>() else {
            return;
        };
        let value = value.string();
        let name = model_display_name(&value);
        let directory = model_is_directory(&value);
        label.set_label(name);
        crate::assets::set_primary_icon(
            &icon,
            if directory {
                crate::assets::icons::FOLDER
            } else {
                icon_for_name(name)
            },
        );
        icon.set_opacity(if directory { 1.0 } else { 0.72 });
        chevron.set_visible(directory);
    });
    factory
}

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if bytes < 1_000 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1_000.0 && unit < UNITS.len() - 1 {
        value /= 1_000.0;
        unit += 1;
    }
    let formatted = format!("{value:.1}");
    format!("{} {}", formatted.trim_end_matches(".0"), UNITS[unit])
}

fn set_filter_placeholder(column: &ColumnView, count: usize) {
    let noun = if count == 1 { "item" } else { "items" };
    column
        .filter_entry
        .set_placeholder_text(Some(&format!("Filter {count} {noun}…")));
}

fn source_position_for_filtered(
    source: &gtk::StringList,
    filtered: &gtk::FilterListModel,
    filtered_position: u32,
) -> Option<usize> {
    let item = filtered.item(filtered_position)?;
    (0..source.n_items())
        .find(|position| {
            source
                .item(*position)
                .is_some_and(|candidate| candidate == item)
        })
        .map(|position| position as usize)
}

fn filtered_position_for_source(column: &ColumnView, source_position: usize) -> Option<u32> {
    let item = column.model.item(source_position as u32)?;
    (0..column.filtered_model.n_items()).find(|position| {
        column
            .filtered_model
            .item(*position)
            .is_some_and(|candidate| candidate == item)
    })
}

fn column_sort_menu(browser: &Rc<Browser>, depth: usize) -> gtk::MenuButton {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.add_css_class("column-menu");
    let heading = gtk::Label::new(Some("SORT BY"));
    heading.set_xalign(0.0);
    heading.add_css_class("menu-heading");
    content.append(&heading);

    let selected_checks: Rc<RefCell<Vec<gtk::Image>>> = Rc::new(RefCell::new(Vec::new()));
    for (label, key, selected) in [
        ("Name", SortKey::Name, true),
        ("Size", SortKey::Size, false),
        ("Modified", SortKey::Modified, false),
        ("Type", SortKey::Type, false),
    ] {
        let (option, check) = column_menu_option(label, selected);
        selected_checks.borrow_mut().push(check.clone());
        let index = selected_checks.borrow().len() - 1;
        let checks = selected_checks.clone();
        let weak_browser = Rc::downgrade(browser);
        option.connect_clicked(move |_| {
            for (check_index, check) in checks.borrow().iter().enumerate() {
                check.set_visible(check_index == index);
            }
            if let Some(browser) = weak_browser.upgrade() {
                browser.set_sort_key(depth, key);
            }
        });
        content.append(&option);
    }

    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let (folders_first, folders_check) = column_menu_option("Folders first", true);
    let folders_enabled = Rc::new(Cell::new(true));
    let weak_browser = Rc::downgrade(browser);
    folders_first.connect_clicked(move |_| {
        let enabled = !folders_enabled.get();
        folders_enabled.set(enabled);
        folders_check.set_visible(enabled);
        if let Some(browser) = weak_browser.upgrade() {
            browser.set_folders_first(depth, enabled);
        }
    });
    content.append(&folders_first);

    let popover = gtk::Popover::builder()
        .child(&content)
        .has_arrow(false)
        .halign(gtk::Align::End)
        .position(gtk::PositionType::Bottom)
        .build();
    popover.add_css_class("column-popover");
    let button = gtk::MenuButton::builder()
        .icon_name(crate::assets::icons::SETTINGS_2)
        .tooltip_text("Choose sort field")
        .popover(&popover)
        .build();
    button.add_css_class("column-header-action");
    button
}

fn column_sort_direction_toggle(browser: &Rc<Browser>, depth: usize) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .icon_name(crate::assets::icons::ARROW_UP_NARROW_WIDE)
        .tooltip_text("Ascending — click to reverse")
        .build();
    button.add_css_class("column-header-action");
    let weak_browser = Rc::downgrade(browser);
    button.connect_toggled(move |button| {
        let direction = if button.is_active() {
            button.set_icon_name(crate::assets::icons::ARROW_DOWN_WIDE_NARROW);
            button.set_tooltip_text(Some("Descending — click to reverse"));
            SortDirection::Descending
        } else {
            button.set_icon_name(crate::assets::icons::ARROW_UP_NARROW_WIDE);
            button.set_tooltip_text(Some("Ascending — click to reverse"));
            SortDirection::Ascending
        };
        if let Some(browser) = weak_browser.upgrade() {
            browser.set_sort_direction(depth, direction);
        }
    });
    button
}

fn column_menu_option(label: &str, selected: bool) -> (gtk::Button, gtk::Image) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let check = gtk::Image::from_icon_name(crate::assets::icons::CHECK);
    check.set_pixel_size(16);
    check.set_visible(selected);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);
    row.append(&check);
    let option = gtk::Button::builder().child(&row).build();
    option.add_css_class("column-menu-option");
    option.set_has_frame(false);
    (option, check)
}

fn is_breadcrumb_target(mut target: gtk::Widget) -> bool {
    loop {
        if target.is::<gtk::Button>()
            || target.has_css_class("breadcrumb")
            || target.has_css_class("breadcrumb-separator")
            || target.has_css_class("current-breadcrumb")
        {
            return true;
        }
        let Some(parent) = target.parent() else {
            return false;
        };
        if parent.has_css_class("breadcrumbs") {
            return false;
        }
        target = parent;
    }
}

fn set_active_path_style(row: &gtk::Box, active: bool) {
    if active {
        row.add_css_class("active-path");
    } else {
        row.remove_css_class("active-path");
    }
}

fn entry_model_value(entry: &FileEntry) -> String {
    let kind = if entry.is_broken_symbolic_link() {
        'x'
    } else if entry.is_directory() {
        'd'
    } else if entry.is_symbolic_link() {
        's'
    } else {
        'f'
    };
    format!("{kind}\t{}", entry.display_name)
}

fn model_display_name(value: &str) -> &str {
    value.split_once('\t').map_or(value, |(_, name)| name)
}

fn model_is_directory(value: &str) -> bool {
    value.starts_with("d\t")
}

fn entry_icon(entry: &FileEntry) -> &'static str {
    if entry.is_broken_symbolic_link() {
        return crate::assets::icons::X;
    }
    if entry.is_directory() {
        return crate::assets::icons::FOLDER;
    }
    icon_for_name(&entry.display_name)
}

fn icon_for_name(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("sh" | "bash" | "zsh" | "fish") => crate::assets::icons::TERMINAL,
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif") => {
            crate::assets::icons::PICTURES
        }
        Some("mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v") => crate::assets::icons::VIDEOS,
        Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst") => {
            crate::assets::icons::FILE_ARCHIVE
        }
        Some(
            "rs" | "c" | "h" | "cpp" | "go" | "py" | "rb" | "java" | "js" | "jsx" | "ts" | "tsx"
            | "lua" | "php" | "html" | "css" | "scss" | "json",
        ) => crate::assets::icons::FILE_CODE,
        _ => crate::assets::icons::DOCUMENTS,
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
        model.append(&entry_model_value(&entry));
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
