// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use crate::model::{FileEntry, Location};

use super::LoadHandle;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OperationRequestId(pub u64);

#[derive(Clone, Debug)]
pub struct RenameRequest {
    pub id: OperationRequestId,
    pub entry: FileEntry,
    pub new_name: String,
}

#[derive(Clone, Debug)]
pub struct CreateDirectoryRequest {
    pub id: OperationRequestId,
    pub parent: Location,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct PasteRequest {
    pub id: OperationRequestId,
    pub destination: Location,
    pub sources: Vec<Location>,
}

#[derive(Clone, Debug)]
pub enum OperationEvent {
    Renamed {
        request_id: OperationRequestId,
    },
    Created {
        request_id: OperationRequestId,
    },
    Pasted {
        request_id: OperationRequestId,
    },
    Failed {
        request_id: OperationRequestId,
        message: String,
    },
}

pub trait OperationProvider {
    fn rename(&self, request: RenameRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle;
    fn create_directory(
        &self,
        request: CreateDirectoryRequest,
        emit: Rc<dyn Fn(OperationEvent)>,
    ) -> LoadHandle;
    fn paste(&self, request: PasteRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle;
}
