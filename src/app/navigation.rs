// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    model::{FileEntry, Location},
    services::RequestId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadState {
    Loading,
    Ready,
    Empty,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct ColumnState {
    pub location: Location,
    pub entries: Vec<FileEntry>,
    pub selected: Option<usize>,
    pub load_state: LoadState,
    request_id: RequestId,
}

#[derive(Default)]
pub struct NavigationState {
    pub columns: Vec<ColumnState>,
}

impl NavigationState {
    pub fn reset(&mut self, location: Location, request_id: RequestId) {
        self.columns.clear();
        self.push_column(location, request_id);
    }

    pub fn descend(
        &mut self,
        parent_depth: usize,
        location: Location,
        request_id: RequestId,
    ) -> bool {
        if parent_depth >= self.columns.len() {
            return false;
        }

        self.columns.truncate(parent_depth + 1);
        self.push_column(location, request_id);
        true
    }

    fn push_column(&mut self, location: Location, request_id: RequestId) {
        self.columns.push(ColumnState {
            location,
            entries: Vec::new(),
            selected: None,
            load_state: LoadState::Loading,
            request_id,
        });
    }

    pub fn apply_batch(&mut self, request_id: RequestId, entries: &[FileEntry]) -> Option<usize> {
        let (depth, column) = self.column_for_request_mut(request_id)?;
        column.entries.extend_from_slice(entries);
        Some(depth)
    }

    pub fn finish(&mut self, request_id: RequestId) -> Option<usize> {
        let (depth, column) = self.column_for_request_mut(request_id)?;
        column.load_state = if column.entries.is_empty() {
            LoadState::Empty
        } else {
            LoadState::Ready
        };
        Some(depth)
    }

    pub fn fail(&mut self, request_id: RequestId, message: String) -> Option<usize> {
        let (depth, column) = self.column_for_request_mut(request_id)?;
        column.load_state = LoadState::Error(message);
        Some(depth)
    }

    pub fn select(&mut self, depth: usize, position: usize) -> bool {
        let Some(column) = self.columns.get_mut(depth) else {
            return false;
        };
        if position >= column.entries.len() {
            return false;
        }
        column.selected = Some(position);
        true
    }

    fn column_for_request_mut(
        &mut self,
        request_id: RequestId,
    ) -> Option<(usize, &mut ColumnState)> {
        self.columns
            .iter_mut()
            .enumerate()
            .find(|(_, column)| column.request_id == request_id)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::model::{EntryKind, MetadataValue};

    fn location(path: &str) -> Location {
        Location::local(path)
    }

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            location: location(path),
            native_name: OsString::from("child"),
            display_name: "child".into(),
            kind: EntryKind::Directory,
            size: MetadataValue::Unknown,
            modified_unix_seconds: MetadataValue::Unknown,
        }
    }

    #[test]
    fn selecting_a_sibling_replaces_deeper_columns() {
        let mut state = NavigationState::default();
        state.reset(location("/home"), RequestId(1));
        assert!(state.descend(0, location("/home/one"), RequestId(2)));
        assert!(state.descend(1, location("/home/one/deep"), RequestId(3)));

        assert!(state.descend(0, location("/home/two"), RequestId(4)));

        assert_eq!(state.columns.len(), 2);
        assert_eq!(state.columns[1].location, location("/home/two"));
    }

    #[test]
    fn stale_batches_are_rejected() {
        let mut state = NavigationState::default();
        state.reset(location("/home"), RequestId(1));
        state.reset(location("/tmp"), RequestId(2));

        assert_eq!(
            state.apply_batch(RequestId(1), &[entry("/home/child")]),
            None
        );
        assert!(state.columns[0].entries.is_empty());
    }

    #[test]
    fn empty_is_distinct_from_loading_and_error() {
        let mut state = NavigationState::default();
        state.reset(location("/empty"), RequestId(1));
        assert_eq!(state.columns[0].load_state, LoadState::Loading);

        assert_eq!(state.finish(RequestId(1)), Some(0));
        assert_eq!(state.columns[0].load_state, LoadState::Empty);
    }
}
