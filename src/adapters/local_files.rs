// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    io::ErrorKind,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gio, glib, prelude::*};

use crate::{
    model::{EntryKind, FileEntry, MetadataValue},
    services::{DirectoryEvent, DirectoryRequest, FileSource, LoadHandle, LocationValidationError},
};

#[derive(Default)]
pub struct LocalFileSource;

fn map_validation_error(error: std::io::Error) -> LocationValidationError {
    match error.kind() {
        ErrorKind::NotFound => LocationValidationError::Missing,
        ErrorKind::PermissionDenied => LocationValidationError::Inaccessible,
        _ => LocationValidationError::Unavailable(error.to_string()),
    }
}

impl FileSource for LocalFileSource {
    fn validate_location(
        &self,
        location: &crate::model::Location,
    ) -> Result<(), LocationValidationError> {
        let metadata = std::fs::metadata(location.path()).map_err(map_validation_error)?;
        if !metadata.is_dir() {
            return Err(LocationValidationError::NotDirectory);
        }
        std::fs::read_dir(location.path())
            .map(|_| ())
            .map_err(map_validation_error)
    }

    fn enumerate(&self, request: DirectoryRequest, emit: Rc<dyn Fn(DirectoryEvent)>) -> LoadHandle {
        let request_id = request.id;
        let path = request.location.path().to_path_buf();
        let started = Instant::now();
        tracing::info!(request_id = request_id.0, path = %path.display(), "directory load started");

        let task = glib::MainContext::default().spawn_local(async move {
            let directory = gio::File::for_path(&path);
            let enumerator = match directory
                .enumerate_children_future(
                    "standard::display-name,standard::name,standard::type,standard::is-hidden,standard::size,time::modified",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await
            {
                Ok(enumerator) => enumerator,
                Err(error) => {
                    tracing::warn!(request_id = request_id.0, error = %error, "directory load failed");
                    emit(DirectoryEvent::Failed {
                        request_id,
                        message: error.to_string(),
                    });
                    return;
                }
            };

            let mut total_entries = 0usize;
            let mut first_batch = true;
            loop {
                match enumerator
                    .next_files_future(request.batch_size as i32, glib::Priority::DEFAULT)
                    .await
                {
                    Ok(files) if files.is_empty() => {
                        tracing::info!(
                            request_id = request_id.0,
                            entries = total_entries,
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            "directory load finished"
                        );
                        emit(DirectoryEvent::Finished { request_id });
                        break;
                    }
                    Ok(files) => {
                        let entries: Vec<_> = files
                            .into_iter()
                            .filter(|info| request.include_hidden || !info.is_hidden())
                            .map(|info| {
                                let native_path = info.name();
                                let native_name = native_path.into_os_string();
                                let kind = match info.file_type() {
                                    gio::FileType::Directory => EntryKind::Directory,
                                    gio::FileType::Regular => EntryKind::File,
                                    gio::FileType::SymbolicLink => EntryKind::SymbolicLink,
                                    _ => EntryKind::Other,
                                };
                                FileEntry {
                                    location: crate::model::Location::local(path.join(&native_name)),
                                    native_name,
                                    display_name: info.display_name().to_string(),
                                    kind,
                                    size: if kind == EntryKind::Directory {
                                        MetadataValue::Unknown
                                    } else {
                                        u64::try_from(info.size())
                                            .map(MetadataValue::Known)
                                            .unwrap_or(MetadataValue::Unavailable)
                                    },
                                    modified_unix_seconds: info
                                        .modification_date_time()
                                        .map(|modified| MetadataValue::Known(modified.to_unix()))
                                        .unwrap_or(MetadataValue::Unavailable),
                                }
                            })
                            .collect();
                        total_entries += entries.len();
                        if first_batch {
                            tracing::info!(
                                request_id = request_id.0,
                                entries = entries.len(),
                                elapsed_ms = started.elapsed().as_millis() as u64,
                                "first directory batch ready"
                            );
                            first_batch = false;
                        }
                        emit(DirectoryEvent::Batch {
                            request_id,
                            entries,
                        });
                    }
                    Err(error) => {
                        tracing::warn!(request_id = request_id.0, error = %error, "directory load interrupted");
                        emit(DirectoryEvent::Failed {
                            request_id,
                            message: error.to_string(),
                        });
                        break;
                    }
                }
            }
        });

        LoadHandle::new(move || {
            tracing::debug!(request_id = request_id.0, "directory load cancelled");
            task.abort();
        })
    }

    fn watch(&self, location: crate::model::Location, notify: Rc<dyn Fn()>) -> Option<LoadHandle> {
        let file = gio::File::for_path(location.path());
        let monitor = match file.monitor_directory(
            gio::FileMonitorFlags::WATCH_MOVES,
            None::<&gio::Cancellable>,
        ) {
            Ok(monitor) => monitor,
            Err(error) => {
                tracing::warn!(
                    path = %location.path().display(),
                    error = %error,
                    "directory monitoring unavailable"
                );
                return None;
            }
        };
        let pending = Rc::new(RefCell::new(None::<glib::SourceId>));
        let pending_for_change = pending.clone();
        monitor.connect_changed(move |_, _, _, _| {
            if let Some(source) = pending_for_change.take() {
                source.remove();
            }
            let pending_for_timeout = pending_for_change.clone();
            let notify = notify.clone();
            let source = glib::timeout_add_local_once(Duration::from_millis(100), move || {
                pending_for_timeout.take();
                notify();
            });
            pending_for_change.replace(Some(source));
        });
        Some(LoadHandle::new(move || {
            if let Some(source) = pending.take() {
                source.remove();
            }
            let _cancelled = monitor.cancel();
        }))
    }
}

#[cfg(test)]
mod tests;
