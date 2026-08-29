// SPDX-License-Identifier: GPL-3.0-or-later

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
    state.navigate(location("/home"), RequestId(1));
    assert!(state.descend(0, location("/home/one"), RequestId(2)));
    assert!(state.descend(1, location("/home/one/deep"), RequestId(3)));

    assert!(state.descend(0, location("/home/two"), RequestId(4)));

    assert_eq!(state.columns.len(), 2);
    assert_eq!(state.columns[1].location, location("/home/two"));
}

#[test]
fn stale_batches_are_rejected() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    state.navigate(location("/tmp"), RequestId(2));

    assert_eq!(
        state.apply_batch(RequestId(1), &[entry("/home/child")]),
        None
    );
    assert!(state.columns[0].entries.is_empty());
}

#[test]
fn empty_is_distinct_from_loading_and_error() {
    let mut state = NavigationState::default();
    state.navigate(location("/empty"), RequestId(1));
    assert_eq!(state.columns[0].load_state, LoadState::Loading);

    assert_eq!(state.finish(RequestId(1)), Some(0));
    assert_eq!(state.columns[0].load_state, LoadState::Empty);
}

#[test]
fn back_and_forward_restore_committed_paths() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    assert!(state.descend(0, location("/home/projects"), RequestId(2)));

    let back = state
        .go_back()
        .expect("a committed path should be available");
    assert_eq!(back.locations(), &[location("/home")]);
    state.restore(back, [RequestId(3)]);

    let forward = state
        .go_forward()
        .expect("the descended path should be available");
    assert_eq!(
        forward.locations(),
        &[location("/home"), location("/home/projects")]
    );
}

#[test]
fn parent_removes_the_deepest_committed_column() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    assert!(state.descend(0, location("/home/projects"), RequestId(2)));

    let parent = state.go_parent().expect("the path has a parent");
    assert_eq!(parent.locations(), &[location("/home")]);
}

#[test]
fn keyboard_selection_is_bounded_and_tracks_the_active_column() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    state.apply_batch(RequestId(1), &[entry("/home/one"), entry("/home/two")]);

    assert_eq!(state.move_selection(1), Some((0, 0)));
    assert_eq!(state.move_selection(1), Some((0, 1)));
    assert_eq!(state.move_selection(1), Some((0, 1)));
    assert_eq!(state.move_selection(-1), Some((0, 0)));
    assert_eq!(state.move_selection(-1), Some((0, 0)));
}

#[test]
fn moving_to_the_parent_column_restores_its_selection() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    state.apply_batch(RequestId(1), &[entry("/home/projects")]);
    assert!(state.select(0, 0));
    assert!(state.descend(0, location("/home/projects"), RequestId(2)));

    assert_eq!(state.focus_parent(), Some((0, Some(0))));
    let (depth, position, focused) = state.focused_entry().expect("parent entry remains focused");
    assert_eq!((depth, position), (0, 0));
    assert_eq!(focused.location, location("/home/projects"));
}

#[test]
fn closing_the_deepest_column_preserves_the_parent_selection() {
    let mut state = NavigationState::default();
    state.navigate(location("/home"), RequestId(1));
    state.apply_batch(RequestId(1), &[entry("/home/projects")]);
    assert!(state.select(0, 0));
    assert!(state.descend(0, location("/home/projects"), RequestId(2)));

    assert_eq!(state.close_deepest(), Some((0, Some(0))));
    assert_eq!(state.columns.len(), 1);
    assert_eq!(state.close_deepest(), None);
}
