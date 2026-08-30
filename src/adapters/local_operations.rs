// SPDX-License-Identifier: GPL-3.0-or-later

use std::{future::Future, pin::Pin, rc::Rc};

use gtk::{gio, glib, prelude::*};

use crate::{
    model::Location,
    services::{
        CreateDirectoryRequest, DeleteRequest, LoadHandle, OperationEvent, OperationProvider,
        PasteRequest, RenameRequest,
    },
};

fn gio_file(location: &Location) -> gio::File {
    location
        .native_path()
        .map(gio::File::for_path)
        .unwrap_or_else(|| gio::File::for_uri(location.uri_value().unwrap_or_default()))
}

fn permanently_delete(
    file: gio::File,
    directory: bool,
) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>>>> {
    Box::pin(async move {
        if directory {
            let enumerator = file
                .enumerate_children_future(
                    "standard::name,standard::type",
                    gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                    glib::Priority::DEFAULT,
                )
                .await?;
            loop {
                let children = enumerator
                    .next_files_future(64, glib::Priority::DEFAULT)
                    .await?;
                if children.is_empty() {
                    break;
                }
                for child in children {
                    permanently_delete(
                        file.child(child.name()),
                        child.file_type() == gio::FileType::Directory,
                    )
                    .await?;
                }
            }
        }
        file.delete_future(glib::Priority::DEFAULT).await
    })
}

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

    fn create_directory(
        &self,
        request: CreateDirectoryRequest,
        emit: Rc<dyn Fn(OperationEvent)>,
    ) -> LoadHandle {
        let task = glib::MainContext::default().spawn_local(async move {
            let folder = gio_file(&request.parent).child(&request.name);
            match folder.make_directory_future(glib::Priority::DEFAULT).await {
                Ok(()) => emit(OperationEvent::Created {
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

    fn paste(&self, request: PasteRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle {
        let task = glib::MainContext::default().spawn_local(async move {
            let destination = gio_file(&request.destination);
            for source in &request.sources {
                let source = gio_file(source);
                let Some(name) = source.basename() else {
                    emit(OperationEvent::Failed {
                        request_id: request.id,
                        message: "A clipboard item has no file name".to_owned(),
                    });
                    return;
                };
                let target = destination.child(name);
                let (copy, _progress) =
                    source.copy_future(&target, gio::FileCopyFlags::NONE, glib::Priority::DEFAULT);
                if let Err(error) = copy.await {
                    emit(OperationEvent::Failed {
                        request_id: request.id,
                        message: error.to_string(),
                    });
                    return;
                }
            }
            emit(OperationEvent::Pasted {
                request_id: request.id,
            });
        });
        LoadHandle::new(move || task.abort())
    }

    fn delete(&self, request: DeleteRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle {
        let task = glib::MainContext::default().spawn_local(async move {
            for entry in &request.entries {
                let file = gio_file(&entry.location);
                let result = if request.permanent {
                    permanently_delete(file, entry.is_directory()).await
                } else {
                    file.trash_future(glib::Priority::DEFAULT).await
                };
                if let Err(error) = result {
                    emit(OperationEvent::Failed {
                        request_id: request.id,
                        message: format!("{}: {error}", entry.display_name),
                    });
                    return;
                }
            }
            emit(OperationEvent::Deleted {
                request_id: request.id,
            });
        });
        LoadHandle::new(move || task.abort())
    }
}
