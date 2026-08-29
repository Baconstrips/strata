// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn breadcrumbs_preserve_each_native_ancestor() {
    let location = Location::local("/home/user/project");

    assert_eq!(
        location.breadcrumbs(),
        vec![
            Location::local("/"),
            Location::local("/home"),
            Location::local("/home/user"),
            Location::local("/home/user/project"),
        ]
    );
}

#[test]
fn root_has_one_breadcrumb() {
    assert_eq!(
        Location::local("/").breadcrumbs(),
        vec![Location::local("/")]
    );
}
