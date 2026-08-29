// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::{
    app::navigation::{NavigationPath, NavigationState},
    model::{FileEntry, Location},
    services::{DirectoryEvent, DirectoryRequest, FileSource, LoadHandle, RequestId},
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
    EntriesAdded {
        depth: usize,
        entries: Vec<FileEntry>,
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
    OpenRequested {
        location: Location,
    },
}

type Observer = Rc<dyn Fn(BrowserEvent)>;

pub struct Browser {
    source: Rc<dyn FileSource>,
    state: RefCell<NavigationState>,
    loads: RefCell<Vec<LoadHandle>>,
    peek_load: RefCell<Option<LoadHandle>>,
    next_request: Cell<u64>,
    observer: RefCell<Option<Observer>>,
}

impl Browser {
    pub fn new(source: Rc<dyn FileSource>) -> Rc<Self> {
        Rc::new(Self {
            source,
            state: RefCell::new(NavigationState::default()),
            loads: RefCell::new(Vec::new()),
            peek_load: RefCell::new(None),
            next_request: Cell::new(1),
            observer: RefCell::new(None),
        })
    }

    pub fn observe(&self, observer: impl Fn(BrowserEvent) + 'static) {
        self.observer.replace(Some(Rc::new(observer)));
    }

    pub fn clear_observer(&self) {
        self.observer.take();
    }

    pub fn navigate(self: &Rc<Self>, location: Location) {
        self.close_peek();
        self.loads.borrow_mut().clear();
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
        self.start_load(location, request_id);
    }

    pub fn descend(self: &Rc<Self>, parent_depth: usize, location: Location) {
        self.close_peek();
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
        self.emit(BrowserEvent::ColumnsTruncated { len: retained });
        self.emit(BrowserEvent::ColumnAdded {
            depth: retained,
            location: location.clone(),
        });
        self.emit(BrowserEvent::FocusChanged {
            depth: retained,
            position: None,
        });
        self.start_load(location, request_id);
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
            self.emit(BrowserEvent::ColumnsTruncated { len });
            self.emit(BrowserEvent::FocusChanged { depth, position });
        }
    }

    pub fn commit_peek(self: &Rc<Self>) {
        let target = self.state.borrow().peek_target();
        if let Some((origin_depth, location)) = target {
            self.close_peek();
            self.descend(origin_depth, location);
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
            self.start_load(location, request_id);
        }
        if let Some(depth) = active_depth {
            self.emit(BrowserEvent::FocusChanged {
                depth,
                position: None,
            });
        }
    }

    fn start_load(self: &Rc<Self>, location: Location, request_id: RequestId) {
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
            },
            emit,
        );
        self.loads.borrow_mut().push(handle);
    }

    fn handle_directory_event(&self, event: DirectoryEvent) {
        match event {
            DirectoryEvent::Batch {
                request_id,
                entries,
            } => {
                let mut state = self.state.borrow_mut();
                let depth = state.apply_batch(request_id, &entries);
                if let Some(depth) = depth {
                    tracing::debug!(
                        request_id = request_id.0,
                        location = %state.columns[depth].location.path().display(),
                        entries = entries.len(),
                        "directory batch accepted"
                    );
                    drop(state);
                    self.emit(BrowserEvent::EntriesAdded { depth, entries });
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
