// SPDX-License-Identifier: GPL-3.0-or-later

use std::{ffi::OsString, path::PathBuf};

/// A browsable destination. Paths remain native and are only converted for display.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Location {
    path: PathBuf,
}

impl Location {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    Directory,
    File,
    SymbolicLink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataValue<T> {
    Unknown,
    Known(T),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    pub location: Location,
    pub native_name: OsString,
    pub display_name: String,
    pub kind: EntryKind,
    pub size: MetadataValue<u64>,
    pub modified_unix_seconds: MetadataValue<i64>,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        self.kind == EntryKind::Directory
    }
}
