// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use crate::model::FileEntry;

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
pub enum OperationEvent {
    Renamed {
        request_id: OperationRequestId,
    },
    Failed {
        request_id: OperationRequestId,
        message: String,
    },
}

pub trait OperationProvider {
    fn rename(&self, request: RenameRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle;
}
