// SPDX-License-Identifier: GPL-3.0-or-later

use super::is_newer;

#[test]
fn newer_patch_version_is_detected() {
    assert!(is_newer("0.2.1", "0.2.0"));
}

#[test]
fn newer_minor_version_is_detected() {
    assert!(is_newer("0.3.0", "0.2.9"));
}

#[test]
fn equal_version_is_not_newer() {
    assert!(!is_newer("0.2.0", "0.2.0"));
}

#[test]
fn older_version_is_not_newer() {
    assert!(!is_newer("0.1.9", "0.2.0"));
}

#[test]
fn missing_or_malformed_segments_fall_back_to_zero() {
    assert!(!is_newer("0.2", "0.2.0"));
    assert!(!is_newer("0.2.x", "0.2.1"));
}
