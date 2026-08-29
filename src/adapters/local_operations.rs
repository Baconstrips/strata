// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gtk::{gio, glib, prelude::*};

use crate::services::{LoadHandle, OperationEvent, OperationProvider, RenameRequest};

#[derive(Default)]
pub struct LocalOperationProvider;

impl OperationProvider for LocalOperationProvider {
    fn rename(&self, request: RenameRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle {
        let task = glib::MainContext::default().spawn_local(async move {
            let file = request
                .entry
                .location
                .native_path()
                .map(gio::File::for_path)
                .unwrap_or_else(|| {
                    gio::File::for_uri(request.entry.location.uri_value().unwrap_or_default())
                });
            match file
                .set_display_name_future(&request.new_name, glib::Priority::DEFAULT)
                .await
            {
                Ok(_) => emit(OperationEvent::Renamed {
                    request_id: request.id,
                }),
                Err(error) => emit(OperationEvent::Failed {
                    request_id: request.id,
                    message: error.to_string(),
                }),
            }
        });
        LoadHandle::new(move || task.abort())
    }
}
