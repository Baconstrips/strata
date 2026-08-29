// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use crate::model::{FileEntry, Location};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(pub u64);

#[derive(Clone, Debug)]
pub struct DirectoryRequest {
    pub id: RequestId,
    pub location: Location,
    pub batch_size: usize,
    pub include_hidden: bool,
}

#[derive(Clone, Debug)]
pub enum DirectoryEvent {
    Batch {
        request_id: RequestId,
        entries: Vec<FileEntry>,
    },
    Finished {
        request_id: RequestId,
    },
    Failed {
        request_id: RequestId,
        message: String,
    },
}

/// A cancellable directory load. Dropping it cancels any unfinished provider work.
pub struct LoadHandle {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl LoadHandle {
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }
}

impl Drop for LoadHandle {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

pub trait FileSource {
    fn enumerate(&self, request: DirectoryRequest, emit: Rc<dyn Fn(DirectoryEvent)>) -> LoadHandle;

    fn watch(&self, _location: Location, _notify: Rc<dyn Fn()>) -> Option<LoadHandle> {
        None
    }
}
