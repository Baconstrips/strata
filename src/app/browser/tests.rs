// SPDX-License-Identifier: GPL-3.0-or-later

use std::{cell::Cell, ffi::OsString};

use super::*;
use crate::{
    model::{EntryKind, MetadataValue},
    services::LoadHandle,
};

struct FakeFileSource;

struct RejectingFileSource;

struct TrackingFileSource {
    cancellations: Rc<Cell<usize>>,
}

struct RecordingFileSource {
    include_hidden: Rc<RefCell<Vec<bool>>>,
}

type WatchCallback = Rc<dyn Fn()>;

struct WatchingFileSource {
    notify: Rc<RefCell<Option<WatchCallback>>>,
}

impl FileSource for WatchingFileSource {
    fn validate_location(&self, _location: &Location) -> Result<(), LocationValidationError> {
        Ok(())
    }

    fn enumerate(&self, request: DirectoryRequest, emit: Rc<dyn Fn(DirectoryEvent)>) -> LoadHandle {
        emit(DirectoryEvent::Batch {
            request_id: request.id,
            entries: vec![FileEntry {
                location: Location::local("/fixture/child"),
                native_name: OsString::from("child"),
                display_name: "child".into(),
                kind: EntryKind::Directory,
                size: MetadataValue::Unknown,
                modified_unix_seconds: MetadataValue::Unknown,
            }],
        });
        emit(DirectoryEvent::Finished {
            request_id: request.id,
        });
        LoadHandle::new(|| {})
    }

    fn watch(&self, _location: Location, notify: Rc<dyn Fn()>) -> Option<LoadHandle> {
        self.notify.replace(Some(notify));
        Some(LoadHandle::new(|| {}))
    }
}

impl FileSource for RecordingFileSource {
    fn validate_location(&self, _location: &Location) -> Result<(), LocationValidationError> {
        Ok(())
    }

    fn enumerate(
        &self,
        request: DirectoryRequest,
        _emit: Rc<dyn Fn(DirectoryEvent)>,
    ) -> LoadHandle {
        self.include_hidden
            .borrow_mut()
            .push(request.include_hidden);
        LoadHandle::new(|| {})
    }
}

impl FileSource for TrackingFileSource {
    fn validate_location(&self, _location: &Location) -> Result<(), LocationValidationError> {
        Ok(())
    }

    fn enumerate(
        &self,
        _request: DirectoryRequest,
        _emit: Rc<dyn Fn(DirectoryEvent)>,
    ) -> LoadHandle {
        let cancellations = self.cancellations.clone();
        LoadHandle::new(move || cancellations.set(cancellations.get() + 1))
    }
}

impl FileSource for RejectingFileSource {
    fn validate_location(&self, _location: &Location) -> Result<(), LocationValidationError> {
        Err(LocationValidationError::Inaccessible)
    }

    fn enumerate(
        &self,
        _request: DirectoryRequest,
        _emit: Rc<dyn Fn(DirectoryEvent)>,
    ) -> LoadHandle {
        LoadHandle::new(|| {})
    }
}

impl FileSource for FakeFileSource {
    fn validate_location(&self, _location: &Location) -> Result<(), LocationValidationError> {
        Ok(())
    }

    fn enumerate(&self, request: DirectoryRequest, emit: Rc<dyn Fn(DirectoryEvent)>) -> LoadHandle {
        emit(DirectoryEvent::Batch {
            request_id: request.id,
            entries: vec![FileEntry {
                location: Location::local("/fixture/child"),
                native_name: OsString::from("child"),
                display_name: "child".into(),
                kind: EntryKind::Directory,
                size: MetadataValue::Unknown,
                modified_unix_seconds: MetadataValue::Unknown,
            }],
        });
        emit(DirectoryEvent::Finished {
            request_id: request.id,
        });
        LoadHandle::new(|| {})
    }
}

#[test]
fn filesystem_notifications_reload_the_affected_column() {
    let notify = Rc::new(RefCell::new(None::<WatchCallback>));
    let browser = Browser::new(Rc::new(WatchingFileSource {
        notify: notify.clone(),
    }));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));
    events.borrow_mut().clear();

    let callback = notify
        .borrow()
        .clone()
        .expect("the directory watcher should be installed");
    callback();

    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::ColumnReloaded { depth: 0 }))
    );
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::EntriesInserted { depth: 0, .. }))
    );
}

#[test]
fn hidden_file_preference_is_applied_to_reloaded_requests() {
    let include_hidden = Rc::new(RefCell::new(Vec::new()));
    let browser = Browser::new(Rc::new(RecordingFileSource {
        include_hidden: include_hidden.clone(),
    }));

    browser.navigate(Location::local("/fixture"));
    browser.toggle_hidden();

    assert_eq!(*include_hidden.borrow(), vec![false, true]);
}

#[test]
fn navigating_away_cancels_the_previous_directory_request() {
    let cancellations = Rc::new(Cell::new(0));
    let browser = Browser::new(Rc::new(TrackingFileSource {
        cancellations: cancellations.clone(),
    }));

    browser.navigate(Location::local("/first"));
    browser.navigate(Location::local("/second"));

    assert_eq!(cancellations.get(), 1);
}

#[test]
fn file_source_can_be_replaced_without_constructing_the_ui() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));

    browser.navigate(Location::local("/fixture"));

    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::EntriesInserted { insertions, .. }
            if insertions.iter().map(|insertion| insertion.entries.len()).sum::<usize>() == 1
    )));
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::LoadFinished { .. }))
    );
}

#[test]
fn valid_location_input_navigates_through_the_controller() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));
    events.borrow_mut().clear();

    assert_eq!(browser.navigate_input("/accepted"), Ok(()));

    assert_eq!(
        browser.active_location(),
        Some(Location::local("/accepted"))
    );
    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::ColumnAdded { depth: 0, location }
            if location == &Location::local("/accepted")
    )));
}

#[test]
fn rejected_location_input_preserves_navigation_state() {
    let browser = Browser::new(Rc::new(RejectingFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));
    events.borrow_mut().clear();

    assert_eq!(
        browser.navigate_input("/restricted"),
        Err(LocationValidationError::Inaccessible)
    );

    assert_eq!(browser.active_location(), Some(Location::local("/fixture")));
    assert!(events.borrow().is_empty());
}

#[test]
fn invalid_location_text_is_rejected_before_the_provider() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    browser.navigate(Location::local("/fixture"));

    assert_eq!(
        browser.navigate_input(""),
        Err(LocationValidationError::Empty)
    );
    assert_eq!(
        browser.navigate_input("relative/path"),
        Err(LocationValidationError::NotAbsolute)
    );
    assert_eq!(browser.active_location(), Some(Location::local("/fixture")));
}

#[test]
fn peeking_streams_results_without_committing_navigation_history() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));

    browser.begin_peek(0, Location::local("/fixture/child"));

    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::PeekStarted { .. }))
    );
    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::PeekEntriesAdded { entries } if entries.len() == 1
    )));
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::PeekFinished))
    );

    browser.back();
    let resets = events
        .borrow()
        .iter()
        .filter(|event| matches!(event, BrowserEvent::Reset))
        .count();
    assert_eq!(resets, 1, "a peek must not create a history entry");
}

#[test]
fn committing_a_peek_descends_and_creates_history() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));
    browser.begin_peek(0, Location::local("/fixture/child"));

    browser.commit_peek();

    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::ColumnAdded { depth: 1, location }
            if location == &Location::local("/fixture/child")
    )));
    browser.back();
    let resets = events
        .borrow()
        .iter()
        .filter(|event| matches!(event, BrowserEvent::Reset))
        .count();
    assert_eq!(resets, 2, "committing a peek must create a history entry");
}

#[test]
fn keyboard_selection_and_activation_descend_without_the_ui() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));

    browser.move_selection(1);
    browser.activate_focused();

    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::FocusChanged {
            depth: 0,
            position: Some(0)
        }
    )));
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::ColumnAdded { depth: 1, .. }))
    );
}

#[test]
fn escape_closes_a_peek_before_the_deepest_column() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));
    browser.navigate(Location::local("/fixture"));
    browser.move_selection(1);
    browser.activate_focused();
    browser.begin_peek(1, Location::local("/fixture/child/child"));
    events.borrow_mut().clear();

    browser.escape();
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::PeekClosed))
    );
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::ColumnsTruncated { .. }))
    );

    events.borrow_mut().clear();
    browser.escape();
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::ColumnsTruncated { len: 1 }))
    );
}
