// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    app::peek::PeekState,
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
    active_column: Option<usize>,
    peek: Option<PeekState>,
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
        self.peek = None;
        self.columns.truncate(parent_depth + 1);
        self.push_column(location, request_id);
        self.active_column = self.columns.len().checked_sub(1);
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
        self.peek = None;
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
        self.active_column = self.columns.len().checked_sub(1);
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

    pub fn begin_peek(
        &mut self,
        origin_depth: usize,
        location: Location,
        request_id: RequestId,
    ) -> bool {
        if origin_depth >= self.columns.len() {
            return false;
        }
        self.peek = Some(PeekState::new(origin_depth, location, request_id));
        true
    }

    pub fn peek_target(&self) -> Option<(usize, Location)> {
        self.peek
            .as_ref()
            .map(|peek| (peek.origin_depth, peek.location.clone()))
    }

    pub fn clear_peek(&mut self) -> bool {
        self.peek.take().is_some()
    }

    pub fn apply_peek_batch(&mut self, request_id: RequestId, entries: &[FileEntry]) -> bool {
        let Some(peek) = self.peek.as_mut().filter(|peek| peek.accepts(request_id)) else {
            return false;
        };
        peek.append(entries);
        true
    }

    pub fn finish_peek(&mut self, request_id: RequestId) -> bool {
        let Some(peek) = self.peek.as_mut().filter(|peek| peek.accepts(request_id)) else {
            return false;
        };
        peek.finish();
        true
    }

    pub fn fail_peek(&mut self, request_id: RequestId, message: String) -> bool {
        let Some(peek) = self.peek.as_mut().filter(|peek| peek.accepts(request_id)) else {
            return false;
        };
        peek.fail(message);
        true
    }

    pub fn select(&mut self, depth: usize, position: usize) -> bool {
        let Some(column) = self.columns.get_mut(depth) else {
            return false;
        };
        if position >= column.entries.len() {
            return false;
        }
        column.selected = Some(position);
        self.active_column = Some(depth);
        true
    }

    pub fn move_selection(&mut self, direction: i32) -> Option<(usize, usize)> {
        let depth = self
            .active_column
            .or_else(|| self.columns.len().checked_sub(1))?;
        let column = self.columns.get_mut(depth)?;
        if column.entries.is_empty() {
            return None;
        }

        let last = column.entries.len() - 1;
        let position = match (column.selected, direction.cmp(&0)) {
            (None, std::cmp::Ordering::Less) => last,
            (None, _) => 0,
            (Some(position), std::cmp::Ordering::Less) => position.saturating_sub(1),
            (Some(position), std::cmp::Ordering::Greater) => (position + 1).min(last),
            (Some(position), std::cmp::Ordering::Equal) => position,
        };
        column.selected = Some(position);
        self.active_column = Some(depth);
        Some((depth, position))
    }

    pub fn focus_parent(&mut self) -> Option<(usize, Option<usize>)> {
        let depth = self.active_column?;
        let parent_depth = depth.checked_sub(1)?;
        self.active_column = Some(parent_depth);
        Some((parent_depth, self.columns[parent_depth].selected))
    }

    pub fn close_deepest(&mut self) -> Option<(usize, Option<usize>)> {
        if self.columns.len() <= 1 {
            return None;
        }
        self.record_navigation();
        self.peek = None;
        self.columns.truncate(self.columns.len() - 1);
        let depth = self.columns.len() - 1;
        self.active_column = Some(depth);
        Some((depth, self.columns[depth].selected))
    }

    pub fn entry_at(&self, depth: usize, position: usize) -> Option<FileEntry> {
        self.columns.get(depth)?.entries.get(position).cloned()
    }

    pub fn focused_entry(&self) -> Option<(usize, usize, FileEntry)> {
        let depth = self.active_column?;
        let column = self.columns.get(depth)?;
        let position = column.selected?;
        let entry = column.entries.get(position)?.clone();
        Some((depth, position, entry))
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
