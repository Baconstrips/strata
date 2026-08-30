// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    fs,
    path::{Path, PathBuf},
};

use gtk::glib;

use super::{
    MAX_CACHE_ENTRIES, ThumbnailCache, ThumbnailKey, ThumbnailKind, render_pdf_thumbnail,
    thumbnail_kind,
};

#[test]
fn recognizes_mainstream_image_and_video_formats() {
    assert_eq!(
        thumbnail_kind(Path::new("photo.JPEG")),
        Some(ThumbnailKind::Image)
    );
    assert_eq!(
        thumbnail_kind(Path::new("animation.webp")),
        Some(ThumbnailKind::Image)
    );
    assert_eq!(
        thumbnail_kind(Path::new("capture.CR3")),
        Some(ThumbnailKind::RawImage)
    );
    assert_eq!(
        thumbnail_kind(Path::new("photo.nef")),
        Some(ThumbnailKind::RawImage)
    );
    assert_eq!(
        thumbnail_kind(Path::new("document.PDF")),
        Some(ThumbnailKind::Pdf)
    );
    assert_eq!(
        thumbnail_kind(Path::new("clip.mkv")),
        Some(ThumbnailKind::Video)
    );
    assert_eq!(
        thumbnail_kind(Path::new("clip.ogv")),
        Some(ThumbnailKind::Video)
    );
}

#[test]
fn renders_the_first_pdf_page_as_a_bounded_thumbnail() {
    let path = std::env::temp_dir().join(format!(
        "strata-thumbnail-{}-{}.pdf",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let surface = cairo::PdfSurface::new(612.0, 792.0, &path).expect("create PDF surface");
    let context = cairo::Context::new(&surface).expect("create PDF context");
    context.set_source_rgb(0.2, 0.4, 0.8);
    context.paint().expect("paint PDF page");
    context.show_page().expect("finish PDF page");
    drop(context);
    surface.finish();

    let png = render_pdf_thumbnail(&path, 64).expect("render PDF thumbnail");
    let _removed = fs::remove_file(path);

    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn thumbnail_cache_evicts_the_least_recent_entry() {
    let mut cache = ThumbnailCache::default();
    for index in 0..=MAX_CACHE_ENTRIES {
        cache.insert(
            ThumbnailKey {
                path: PathBuf::from(format!("image-{index}.png")),
                modified: Some(1),
                file_size: Some(1),
                thumbnail_size: 64,
            },
            glib::Bytes::from_static(&[1]),
        );
    }

    let oldest = ThumbnailKey {
        path: PathBuf::from("image-0.png"),
        modified: Some(1),
        file_size: Some(1),
        thumbnail_size: 64,
    };
    assert!(cache.get(&oldest).is_none());
    assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);
}

#[test]
fn rejects_files_without_a_thumbnail_provider() {
    assert_eq!(thumbnail_kind(Path::new("README.md")), None);
    assert_eq!(thumbnail_kind(Path::new("no-extension")), None);
}
