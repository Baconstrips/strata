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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NavigationPath {
    locations: Vec<Location>,
}

impl NavigationPath {
    pub fn from_locations(locations: Vec<Location>) -> Self {
        Self { locations }
    }

    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    fn parent(&self) -> Option<Self> {
        if self.locations.len() > 1 {
            let mut locations = self.locations.clone();
            locations.pop();
            return Some(Self { locations });
        }

        let current = self.locations.first()?;
        let parent = current.path().parent()?;
        if parent == current.path() {
            return None;
        }
        Some(Self::from_locations(vec![Location::local(parent)]))
    }
}

#[derive(Default)]
pub struct NavigationState {
    pub columns: Vec<ColumnState>,
    back_history: Vec<NavigationPath>,
    forward_history: Vec<NavigationPath>,
}

impl NavigationState {
    pub fn navigate(&mut self, location: Location, request_id: RequestId) {
        self.record_navigation();
        self.restore(NavigationPath::from_locations(vec![location]), [request_id]);
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

        self.record_navigation();
        self.columns.truncate(parent_depth + 1);
        self.push_column(location, request_id);
        true
    }

    pub fn go_back(&mut self) -> Option<NavigationPath> {
        let target = self.back_history.pop()?;
        if let Some(current) = self.current_path() {
            self.forward_history.push(current);
        }
        Some(target)
    }

    pub fn go_forward(&mut self) -> Option<NavigationPath> {
        let target = self.forward_history.pop()?;
        if let Some(current) = self.current_path() {
            self.back_history.push(current);
        }
        Some(target)
    }

    pub fn go_parent(&mut self) -> Option<NavigationPath> {
        let target = self.current_path()?.parent()?;
        self.record_navigation();
        Some(target)
    }

    pub fn restore(
        &mut self,
        path: NavigationPath,
        request_ids: impl IntoIterator<Item = RequestId>,
    ) {
        self.columns = path
            .locations
            .into_iter()
            .zip(request_ids)
            .map(|(location, request_id)| ColumnState {
                location,
                entries: Vec::new(),
                selected: None,
                load_state: LoadState::Loading,
                request_id,
            })
            .collect();
    }

    fn current_path(&self) -> Option<NavigationPath> {
        (!self.columns.is_empty()).then(|| {
            NavigationPath::from_locations(
                self.columns
                    .iter()
                    .map(|column| column.location.clone())
                    .collect(),
            )
        })
    }

    fn record_navigation(&mut self) {
        if let Some(current) = self.current_path() {
            self.back_history.push(current);
            self.forward_history.clear();
        }
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
mod tests;
