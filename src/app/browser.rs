// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
};

use crate::{
    app::navigation::{EntryInsertion, EntrySplice, NavigationPath, NavigationState},
    model::{FileEntry, Location, SortDirection, SortKey, ViewPreferences},
    services::{
        DirectoryChange, DirectoryEvent, DirectoryRequest, FileSource, LoadHandle,
        LocationValidationError, RequestId,
    },
};

#[derive(Clone, Debug)]
pub enum BrowserEvent {
    Reset,
    ColumnsTruncated {
        len: usize,
    },
    ColumnAdded {
        depth: usize,
        location: Location,
    },
    EntriesInserted {
        depth: usize,
        insertions: Vec<EntryInsertion>,
    },
    EntriesReplaced {
        depth: usize,
        entries: Vec<FileEntry>,
    },
    EntriesSpliced {
        depth: usize,
        splices: Vec<EntrySplice>,
        selected: Option<usize>,
    },
    ColumnReloaded {
        depth: usize,
    },
    LoadFinished {
        depth: usize,
    },
    LoadFailed {
        depth: usize,
        message: String,
    },
    PeekStarted {
        location: Location,
    },
    PeekEntriesAdded {
        entries: Vec<FileEntry>,
    },
    PeekFinished,
    PeekFailed {
        message: String,
    },
    PeekClosed,
    FocusChanged {
        depth: usize,
        position: Option<usize>,
    },
    SelectionChanged {
        depth: usize,
        position: usize,
    },
    OpenRequested {
        location: Location,
    },
    NavigationRejected {
        message: String,
    },
}

type Observer = Rc<dyn Fn(BrowserEvent)>;

pub struct Browser {
    source: Rc<dyn FileSource>,
    state: RefCell<NavigationState>,
    loads: RefCell<Vec<LoadHandle>>,
    monitors: RefCell<Vec<Option<LoadHandle>>>,
    peek_load: RefCell<Option<LoadHandle>>,
    next_request: Cell<u64>,
    preferences: Cell<ViewPreferences>,
    observer: RefCell<Option<Observer>>,
}

impl Browser {
    pub fn new(source: Rc<dyn FileSource>) -> Rc<Self> {
        Rc::new(Self {
            source,
            state: RefCell::new(NavigationState::default()),
            loads: RefCell::new(Vec::new()),
            monitors: RefCell::new(Vec::new()),
            peek_load: RefCell::new(None),
            next_request: Cell::new(1),
            preferences: Cell::new(ViewPreferences::default()),
            observer: RefCell::new(None),
        })
    }

    pub fn observe(&self, observer: impl Fn(BrowserEvent) + 'static) {
        self.observer.replace(Some(Rc::new(observer)));
    }

    pub fn clear_observer(&self) {
        self.observer.take();
    }

    pub fn navigate_input(self: &Rc<Self>, input: &str) -> Result<(), LocationValidationError> {
        if input.is_empty() {
            return Err(LocationValidationError::Empty);
        }

        let location = self
            .active_location()
            .filter(|current| current.display_path() == input)
            .unwrap_or_else(|| {
                if input == "trash:///" {
                    Location::uri(input)
                } else {
                    Location::local(PathBuf::from(input))
                }
            });
        if location.native_path().is_some() && !location.is_absolute_native() {
            return Err(LocationValidationError::NotAbsolute);
        }
        self.source.validate_location(&location)?;
        self.navigate(location);
        Ok(())
    }

    pub fn active_location(&self) -> Option<Location> {
        self.state.borrow().active_location()
    }

    pub fn focus_active(&self) {
        if let Some((depth, position)) = self.state.borrow().active_focus() {
            self.emit(BrowserEvent::FocusChanged { depth, position });
        }
    }

    pub fn navigate(self: &Rc<Self>, location: Location) {
        self.close_peek();
        self.loads.borrow_mut().clear();
        self.monitors.borrow_mut().clear();
        let request_id = self.new_request_id();
        self.state
            .borrow_mut()
            .navigate(location.clone(), request_id);
        self.emit(BrowserEvent::Reset);
        self.emit(BrowserEvent::ColumnAdded {
            depth: 0,
            location: location.clone(),
        });
        self.emit(BrowserEvent::FocusChanged {
            depth: 0,
            position: None,
        });
        self.start_load(0, location, request_id);
    }

    pub fn descend(self: &Rc<Self>, parent_depth: usize, location: Location) {
        self.close_peek();
        if let Err(error) = self.source.validate_location(&location) {
            self.emit(BrowserEvent::NavigationRejected {
                message: error.to_string(),
            });
            self.focus_active();
            return;
        }
        let request_id = self.new_request_id();
        if !self
            .state
            .borrow_mut()
            .descend(parent_depth, location.clone(), request_id)
        {
            return;
        }

        let retained = parent_depth + 1;
        self.loads.borrow_mut().truncate(retained);
        self.monitors.borrow_mut().truncate(retained);
        self.emit(BrowserEvent::ColumnsTruncated { len: retained });
        self.emit(BrowserEvent::ColumnAdded {
            depth: retained,
            location: location.clone(),
        });
        self.emit(BrowserEvent::FocusChanged {
            depth: retained,
            position: None,
        });
        self.start_load(retained, location, request_id);
    }

    pub fn begin_peek(self: &Rc<Self>, origin_depth: usize, location: Location) {
        self.close_peek();
        let request_id = self.new_request_id();
        if !self
            .state
            .borrow_mut()
            .begin_peek(origin_depth, location.clone(), request_id)
        {
            return;
        }

        self.emit(BrowserEvent::PeekStarted {
            location: location.clone(),
        });
        let weak: Weak<Self> = Rc::downgrade(self);
        let emit = Rc::new(move |event| {
            if let Some(browser) = weak.upgrade() {
                browser.handle_directory_event(event);
            }
        });
        let handle = self.source.enumerate(
            DirectoryRequest {
                id: request_id,
                location,
                batch_size: 128,
                include_hidden: self.preferences.get().show_hidden,
            },
            emit,
        );
        self.peek_load.replace(Some(handle));
    }

    pub fn close_peek(&self) -> bool {
        self.peek_load.take();
        let closed = self.state.borrow_mut().clear_peek();
        if closed {
            self.emit(BrowserEvent::PeekClosed);
        }
        closed
    }

    pub fn escape(&self) {
        if self.close_peek() {
            return;
        }

        let closed = self.state.borrow_mut().close_deepest();
        if let Some((depth, position)) = closed {
            let len = depth + 1;
            self.loads.borrow_mut().truncate(len);
            self.monitors.borrow_mut().truncate(len);
            self.emit(BrowserEvent::ColumnsTruncated { len });
            self.emit(BrowserEvent::FocusChanged { depth, position });
        }
    }

    pub fn close_column(&self, depth: usize) {
        self.close_peek();
        let closed = self.state.borrow_mut().close_from(depth);
        if let Some((parent_depth, position)) = closed {
            self.loads.borrow_mut().truncate(depth);
            self.monitors.borrow_mut().truncate(depth);
            self.emit(BrowserEvent::ColumnsTruncated { len: depth });
            self.emit(BrowserEvent::FocusChanged {
                depth: parent_depth,
                position,
            });
        }
    }

    pub fn commit_peek(self: &Rc<Self>) {
        let target = self.state.borrow().peek_target();
        if let Some((origin_depth, location)) = target {
            self.close_peek();
            self.descend(origin_depth, location);
        }
    }

    pub fn set_sort_key(&self, depth: usize, sort_key: SortKey) {
        self.apply_column_preferences(depth, |preferences| preferences.sort_key = sort_key);
    }

    pub fn set_sort_direction(&self, depth: usize, sort_direction: SortDirection) {
        self.apply_column_preferences(depth, |preferences| {
            preferences.sort_direction = sort_direction;
        });
    }

    pub fn set_folders_first(&self, depth: usize, folders_first: bool) {
        self.apply_column_preferences(depth, |preferences| {
            preferences.folders_first = folders_first;
        });
    }

    pub fn toggle_hidden(self: &Rc<Self>) {
        let mut preferences = self.preferences.get();
        preferences.show_hidden = !preferences.show_hidden;
        self.preferences.set(preferences);

        let locations = {
            let mut state = self.state.borrow_mut();
            state.set_show_hidden(preferences.show_hidden);
            state
                .columns
                .iter()
                .map(|column| column.location.clone())
                .collect::<Vec<_>>()
        };
        for (depth, location) in locations.into_iter().enumerate() {
            self.refresh_column(depth);
            let monitor = self.install_monitor(depth, location);
            if let Some(slot) = self.monitors.borrow_mut().get_mut(depth) {
                *slot = monitor;
            }
        }
    }

    fn apply_column_preferences(&self, depth: usize, update: impl FnOnce(&mut ViewPreferences)) {
        let result = {
            let mut state = self.state.borrow_mut();
            let Some(mut preferences) = state.column_preferences(depth) else {
                return;
            };
            update(&mut preferences);
            state.set_column_preferences(depth, preferences)
        };
        if let Some((entries, selected)) = result {
            self.emit(BrowserEvent::EntriesReplaced { depth, entries });
            if let Some(position) = selected {
                self.emit(BrowserEvent::SelectionChanged { depth, position });
            }
        }
    }

    pub fn back(self: &Rc<Self>) {
        let target = self.state.borrow_mut().go_back();
        if let Some(target) = target {
            self.restore_path(target);
        }
    }

    pub fn forward(self: &Rc<Self>) {
        let target = self.state.borrow_mut().go_forward();
        if let Some(target) = target {
            self.restore_path(target);
        }
    }

    pub fn parent(self: &Rc<Self>) {
        let target = self.state.borrow_mut().go_parent();
        if let Some(target) = target {
            self.restore_path(target);
        }
    }

    pub fn select(&self, depth: usize, position: usize) {
        if self.state.borrow_mut().select(depth, position) {
            self.emit(BrowserEvent::FocusChanged {
                depth,
                position: Some(position),
            });
        }
    }

    pub fn entry_at(&self, depth: usize, position: usize) -> Option<FileEntry> {
        self.state.borrow().entry_at(depth, position)
    }

    pub fn activate(self: &Rc<Self>, depth: usize, position: usize) {
        self.select(depth, position);
        self.activate_focused();
    }

    pub fn move_selection(&self, direction: i32) {
        if let Some((depth, position)) = self.state.borrow_mut().move_selection(direction) {
            self.emit(BrowserEvent::FocusChanged {
                depth,
                position: Some(position),
            });
        }
    }

    pub fn focus_parent(&self) {
        if let Some((depth, position)) = self.state.borrow_mut().focus_parent() {
            self.emit(BrowserEvent::FocusChanged { depth, position });
        }
    }

    pub fn activate_focused(self: &Rc<Self>) {
        let focused = self.state.borrow().focused_entry();
        let Some((depth, _, entry)) = focused else {
            self.move_selection(1);
            return;
        };

        if entry.is_directory() {
            self.descend(depth, entry.location);
        } else {
            self.emit(BrowserEvent::OpenRequested {
                location: entry.location,
            });
        }
    }

    fn restore_path(self: &Rc<Self>, path: NavigationPath) {
        self.close_peek();
        self.loads.borrow_mut().clear();
        self.monitors.borrow_mut().clear();
        let loads: Vec<_> = path
            .locations()
            .iter()
            .cloned()
            .map(|location| {
                let request_id = self.new_request_id();
                (location, request_id)
            })
            .collect();
        self.state
            .borrow_mut()
            .restore(path, loads.iter().map(|(_, request_id)| *request_id));

        self.emit(BrowserEvent::Reset);
        let active_depth = loads.len().checked_sub(1);
        for (depth, (location, request_id)) in loads.into_iter().enumerate() {
            self.emit(BrowserEvent::ColumnAdded {
                depth,
                location: location.clone(),
            });
            self.start_load(depth, location, request_id);
        }
        if let Some(depth) = active_depth {
            self.emit(BrowserEvent::FocusChanged {
                depth,
                position: None,
            });
        }
    }

    fn start_load(self: &Rc<Self>, depth: usize, location: Location, request_id: RequestId) {
        let handle = self.request_directory(location.clone(), request_id);
        self.loads.borrow_mut().push(handle);

        let monitor = self.install_monitor(depth, location);
        self.monitors.borrow_mut().push(monitor);
    }

    fn install_monitor(self: &Rc<Self>, depth: usize, location: Location) -> Option<LoadHandle> {
        let weak: Weak<Self> = Rc::downgrade(self);
        let watched = location.clone();
        let notify = Rc::new(move |change| {
            if let Some(browser) = weak.upgrade() {
                browser.handle_directory_change(depth, &watched, change);
            }
        });
        self.source
            .watch(location, self.preferences.get().show_hidden, notify)
    }

    fn request_directory(self: &Rc<Self>, location: Location, request_id: RequestId) -> LoadHandle {
        let weak: Weak<Self> = Rc::downgrade(self);
        let emit = Rc::new(move |event| {
            if let Some(browser) = weak.upgrade() {
                browser.handle_directory_event(event);
            }
        });
        self.source.enumerate(
            DirectoryRequest {
                id: request_id,
                location,
                batch_size: 128,
                include_hidden: self.preferences.get().show_hidden,
            },
            emit,
        )
    }

    pub fn retry_column(self: &Rc<Self>, depth: usize) {
        self.refresh_column(depth);
    }

    fn refresh_column(self: &Rc<Self>, depth: usize) {
        let request_id = self.new_request_id();
        let location = self.state.borrow_mut().reload_column(depth, request_id);
        let Some(location) = location else {
            return;
        };
        self.emit(BrowserEvent::ColumnReloaded { depth });
        let handle = self.request_directory(location, request_id);
        if let Some(load) = self.loads.borrow_mut().get_mut(depth) {
            *load = handle;
        }
    }

    fn handle_directory_change(
        self: &Rc<Self>,
        depth: usize,
        watched: &Location,
        change: DirectoryChange,
    ) {
        if matches!(&change, DirectoryChange::Rescan) {
            self.refresh_column(depth);
            return;
        }
        let path_update = self
            .state
            .borrow()
            .path_after_external_change(depth, &change);
        if let Some(path) = path_update {
            self.restore_path(path);
            return;
        }

        let application = self
            .state
            .borrow_mut()
            .apply_directory_change(depth, watched, change);
        if let Some((splices, selected)) = application {
            self.emit(BrowserEvent::EntriesSpliced {
                depth,
                splices,
                selected,
            });
        }
    }

    fn handle_directory_event(&self, event: DirectoryEvent) {
        match event {
            DirectoryEvent::Batch {
                request_id,
                entries,
            } => {
                let mut state = self.state.borrow_mut();
                let application = state.apply_batch(request_id, entries.clone());
                if let Some((depth, insertions)) = application {
                    tracing::debug!(
                        request_id = request_id.0,
                        location = %state.columns[depth].location.display_path(),
                        entries = entries.len(),
                        "directory batch accepted"
                    );
                    let selected = state.columns[depth].selected;
                    drop(state);
                    self.emit(BrowserEvent::EntriesInserted { depth, insertions });
                    if let Some(position) = selected {
                        self.emit(BrowserEvent::SelectionChanged { depth, position });
                    }
                } else if state.apply_peek_batch(request_id, &entries) {
                    drop(state);
                    self.emit(BrowserEvent::PeekEntriesAdded { entries });
                }
            }
            DirectoryEvent::Finished { request_id } => {
                let mut state = self.state.borrow_mut();
                if let Some(depth) = state.finish(request_id) {
                    drop(state);
                    self.emit(BrowserEvent::LoadFinished { depth });
                } else if state.finish_peek(request_id) {
                    drop(state);
                    self.emit(BrowserEvent::PeekFinished);
                }
            }
            DirectoryEvent::Failed {
                request_id,
                message,
            } => {
                let mut state = self.state.borrow_mut();
                if let Some(depth) = state.fail(request_id, message.clone()) {
                    drop(state);
                    self.emit(BrowserEvent::LoadFailed { depth, message });
                } else if state.fail_peek(request_id, message.clone()) {
                    drop(state);
                    self.emit(BrowserEvent::PeekFailed { message });
                }
            }
        }
    }

    fn emit(&self, event: BrowserEvent) {
        let observer = self.observer.borrow().clone();
        if let Some(observer) = observer {
            observer(event);
        }
    }

    fn new_request_id(&self) -> RequestId {
        let id = self.next_request.get();
        self.next_request.set(id.saturating_add(1));
        RequestId(id)
    }
}

#[cfg(test)]
mod tests;
