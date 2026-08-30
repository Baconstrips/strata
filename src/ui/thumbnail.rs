// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    collections::HashMap,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use gtk::{gdk, gio, glib, prelude::*};

use crate::model::FileEntry;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ACTIVE_REQUESTS: RefCell<HashMap<usize, (u64, glib::WeakRef<gtk::Image>)>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThumbnailKind {
    Image,
    Video,
}

pub(super) fn set_thumbnail_or_icon(
    image: &gtk::Image,
    entry: &FileEntry,
    fallback_icon: &str,
    icon_size: i32,
    thumbnail_size: i32,
) {
    let (image_id, request) = set_fallback_icon(image, fallback_icon, icon_size);

    let Some(path) = entry.location.native_path().map(Path::to_path_buf) else {
        return;
    };
    let Some(kind) = thumbnail_kind(&path) else {
        return;
    };

    let weak_image = glib::WeakRef::new();
    weak_image.set(Some(image));
    glib::MainContext::default().spawn_local(async move {
        let result =
            gio::spawn_blocking(move || render_thumbnail(&path, kind, thumbnail_size)).await;
        let Ok(Some(png)) = result else {
            return;
        };
        let is_current = ACTIVE_REQUESTS.with(|requests| {
            requests
                .borrow()
                .get(&image_id)
                .is_some_and(|active| active.0 == request)
        });
        if !is_current {
            return;
        }
        let Some(image) = weak_image.upgrade() else {
            ACTIVE_REQUESTS.with(|requests| {
                requests.borrow_mut().remove(&image_id);
            });
            return;
        };
        let bytes = glib::Bytes::from_owned(png);
        if let Ok(texture) = gdk::Texture::from_bytes(&bytes) {
            crate::assets::remove_primary_icon(&image);
            image.set_pixel_size(thumbnail_size);
            image.set_size_request(thumbnail_size, thumbnail_size);
            image.set_paintable(Some(&texture));
            image.set_opacity(1.0);
        }
    });
}

pub(super) fn show_fallback_icon(image: &gtk::Image, icon: &str, size: i32) {
    set_fallback_icon(image, icon, size);
}

fn set_fallback_icon(image: &gtk::Image, icon: &str, size: i32) -> (usize, u64) {
    let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let image_id = image.as_ptr() as usize;
    let weak_image = glib::WeakRef::new();
    weak_image.set(Some(image));
    ACTIVE_REQUESTS.with(|requests| {
        let mut requests = requests.borrow_mut();
        requests.retain(|_, (_, image)| image.upgrade().is_some());
        requests.insert(image_id, (request, weak_image));
    });
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    crate::assets::set_primary_icon(image, icon);
    (image_id, request)
}

fn thumbnail_kind(path: &Path) -> Option<ThumbnailKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => {
            Some(ThumbnailKind::Image)
        }
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mpeg" | "mpg" | "ogv" => {
            Some(ThumbnailKind::Video)
        }
        _ => None,
    }
}

fn render_thumbnail(path: &Path, kind: ThumbnailKind, size: i32) -> Option<Vec<u8>> {
    let render_size = size.clamp(16, 256);
    match kind {
        ThumbnailKind::Image => {
            gdk_pixbuf::Pixbuf::from_file_at_scale(path, render_size, render_size, true)
                .ok()?
                .save_to_bufferv("png", &[])
                .ok()
        }
        ThumbnailKind::Video => render_video_thumbnail(path, render_size),
    }
}

fn render_video_thumbnail(path: &Path, size: i32) -> Option<Vec<u8>> {
    let output = Command::new("ffmpegthumbnailer")
        .arg("-i")
        .arg(path)
        .args(["-o", "/dev/stdout", "-s"])
        .arg(size.to_string())
        .args(["-q", "8"])
        .output()
        .ok()?;
    (output.status.success() && !output.stdout.is_empty()).then_some(output.stdout)
}

#[cfg(test)]
mod tests;
