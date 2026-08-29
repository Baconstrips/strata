// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use super::{reorder_places, should_show_standard_place};

#[test]
fn places_can_move_before_an_earlier_item() {
    let mut places = vec!["desktop", "documents", "downloads", "pictures", "videos"];

    assert!(reorder_places(&mut places, "videos", "documents", false));
    assert_eq!(
        places,
        vec!["desktop", "videos", "documents", "downloads", "pictures"]
    );
}

#[test]
fn places_can_move_after_a_later_item() {
    let mut places = vec!["desktop", "documents", "downloads", "pictures", "videos"];

    assert!(reorder_places(&mut places, "documents", "pictures", true));
    assert_eq!(
        places,
        vec!["desktop", "downloads", "pictures", "documents", "videos"]
    );
}

#[test]
fn invalid_place_reorders_leave_the_order_unchanged() {
    let original = vec!["desktop", "documents", "downloads"];
    let mut places = original.clone();

    assert!(!reorder_places(&mut places, "missing", "desktop", false));
    assert!(!reorder_places(&mut places, "desktop", "missing", false));
    assert!(!reorder_places(&mut places, "desktop", "desktop", false));
    assert_eq!(places, original);
}

#[test]
fn desktop_is_hidden_when_it_points_to_home() {
    let home = Path::new("/home/user");

    assert!(!should_show_standard_place("desktop", home, home));
    assert!(should_show_standard_place(
        "desktop",
        Path::new("/home/user/Desktop"),
        home
    ));
    assert!(should_show_standard_place("documents", home, home));
}
