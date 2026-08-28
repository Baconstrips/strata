// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;

use super::*;
use crate::{
    model::{EntryKind, MetadataValue},
    services::LoadHandle,
};

struct FakeFileSource;

impl FileSource for FakeFileSource {
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
fn file_source_can_be_replaced_without_constructing_the_ui() {
    let browser = Browser::new(Rc::new(FakeFileSource));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    browser.observe(move |event| observed.borrow_mut().push(event));

    browser.navigate(Location::local("/fixture"));

    assert!(events.borrow().iter().any(|event| matches!(
        event,
        BrowserEvent::EntriesAdded { entries, .. } if entries.len() == 1
    )));
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, BrowserEvent::LoadFinished { .. }))
    );
}
