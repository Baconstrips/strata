// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn file_sizes_use_compact_decimal_units() {
    assert_eq!(format_file_size(999), "999 B");
    assert_eq!(format_file_size(1_200), "1.2 kB");
    assert_eq!(format_file_size(1_000_000), "1 MB");
    assert_eq!(format_file_size(2_500_000_000), "2.5 GB");
}

#[test]
fn inline_rename_selects_the_stem_but_keeps_the_extension() {
    assert_eq!(rename_stem_end("report.txt"), 6);
    assert_eq!(rename_stem_end("archive.tar.gz"), 11);
    assert_eq!(rename_stem_end("README"), 6);
    assert_eq!(rename_stem_end(".gitignore"), 10);
}

#[test]
fn file_names_map_to_specific_lucide_icons() {
    assert_eq!(icon_for_name("setup.sh"), crate::assets::icons::TERMINAL);
    assert_eq!(icon_for_name("photo.webp"), crate::assets::icons::PICTURES);
    assert_eq!(icon_for_name("movie.mkv"), crate::assets::icons::VIDEOS);
    assert_eq!(icon_for_name("source.rs"), crate::assets::icons::FILE_CODE);
    assert_eq!(
        icon_for_name("backup.tar"),
        crate::assets::icons::FILE_ARCHIVE
    );
    assert_eq!(icon_for_name("README.md"), crate::assets::icons::DOCUMENTS);
}

#[test]
fn reveal_target_scrolls_only_enough_to_show_the_new_column() {
    assert_eq!(
        horizontal_reveal_target(0.0, 900.0, 0.0, 1_200.0, 900.0, 1_200.0),
        300.0
    );
}

#[test]
fn reveal_target_is_stable_when_the_column_is_already_visible() {
    assert_eq!(
        horizontal_reveal_target(300.0, 900.0, 0.0, 1_500.0, 900.0, 1_200.0),
        300.0
    );
}

#[test]
fn reveal_target_can_scroll_back_to_an_earlier_column() {
    assert_eq!(
        horizontal_reveal_target(600.0, 900.0, 0.0, 1_500.0, 300.0, 600.0),
        300.0
    );
}
