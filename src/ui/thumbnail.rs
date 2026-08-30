// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use gdk_pixbuf::prelude::*;
use gtk::{gdk, gio, glib, prelude::*};

use crate::model::{FileEntry, MetadataValue};

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
const MAX_CACHE_ENTRIES: usize = 256;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;

thread_local! {
    static ACTIVE_REQUESTS: RefCell<HashMap<usize, (u64, glib::WeakRef<gtk::Image>)>> =
        RefCell::new(HashMap::new());
    static THUMBNAIL_CACHE: RefCell<ThumbnailCache> = RefCell::new(ThumbnailCache::default());
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailKey {
    path: PathBuf,
    modified: Option<i64>,
    file_size: Option<u64>,
    thumbnail_size: i32,
}

#[derive(Default)]
struct ThumbnailCache {
    entries: HashMap<ThumbnailKey, glib::Bytes>,
    recent: VecDeque<ThumbnailKey>,
    byte_count: usize,
}

impl ThumbnailCache {
    fn get(&mut self, key: &ThumbnailKey) -> Option<glib::Bytes> {
        let bytes = self.entries.get(key)?.clone();
        self.recent.retain(|candidate| candidate != key);
        self.recent.push_back(key.clone());
        Some(bytes)
    }

    fn insert(&mut self, key: ThumbnailKey, bytes: glib::Bytes) {
        if let Some(previous) = self.entries.remove(&key) {
            self.byte_count = self.byte_count.saturating_sub(previous.len());
        }
        self.recent.retain(|candidate| candidate != &key);
        self.byte_count = self.byte_count.saturating_add(bytes.len());
        self.recent.push_back(key.clone());
        self.entries.insert(key, bytes);
        while self.entries.len() > MAX_CACHE_ENTRIES || self.byte_count > MAX_CACHE_BYTES {
            let Some(oldest) = self.recent.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.byte_count = self.byte_count.saturating_sub(removed.len());
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThumbnailKind {
    Image,
    RawImage,
    Pdf,
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
    let thumbnail_size = thumbnail_size.clamp(16, 256);
    let key = ThumbnailKey {
        path: path.clone(),
        modified: known_metadata(&entry.modified_unix_seconds),
        file_size: known_metadata(&entry.size),
        thumbnail_size,
    };
    if let Some(bytes) = THUMBNAIL_CACHE.with(|cache| cache.borrow_mut().get(&key)) {
        apply_thumbnail(image, &bytes, thumbnail_size);
        return;
    }

    let weak_image = glib::WeakRef::new();
    weak_image.set(Some(image));
    glib::MainContext::default().spawn_local(async move {
        let result =
            gio::spawn_blocking(move || render_thumbnail(&path, kind, thumbnail_size)).await;
        let Ok(Some(png)) = result else {
            return;
        };
        let bytes = glib::Bytes::from_owned(png);
        THUMBNAIL_CACHE.with(|cache| cache.borrow_mut().insert(key, bytes.clone()));
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
        apply_thumbnail(&image, &bytes, thumbnail_size);
    });
}

fn known_metadata<T: Copy>(value: &MetadataValue<T>) -> Option<T> {
    match value {
        MetadataValue::Known(value) => Some(*value),
        MetadataValue::Unknown | MetadataValue::Unavailable => None,
    }
}

fn apply_thumbnail(image: &gtk::Image, bytes: &glib::Bytes, thumbnail_size: i32) {
    if let Ok(texture) = gdk::Texture::from_bytes(bytes) {
        crate::assets::remove_primary_icon(image);
        image.set_pixel_size(thumbnail_size);
        image.set_size_request(thumbnail_size, thumbnail_size);
        image.set_paintable(Some(&texture));
        image.set_opacity(1.0);
    }
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
        "3fr" | "arw" | "cr2" | "cr3" | "dcr" | "dng" | "erf" | "kdc" | "mef" | "mos" | "mrw"
        | "nef" | "nrw" | "orf" | "pef" | "raf" | "raw" | "rw2" | "rwl" | "sr2" | "srf" | "srw"
        | "x3f" => Some(ThumbnailKind::RawImage),
        "pdf" => Some(ThumbnailKind::Pdf),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mpeg" | "mpg" | "ogv" => {
            Some(ThumbnailKind::Video)
        }
        _ => None,
    }
}

fn render_thumbnail(path: &Path, kind: ThumbnailKind, size: i32) -> Option<Vec<u8>> {
    let render_size = size.clamp(16, 256);
    match kind {
        ThumbnailKind::Image => render_pixbuf_thumbnail(path, render_size),
        ThumbnailKind::RawImage => render_pixbuf_thumbnail(path, render_size)
            .or_else(|| render_imagemagick_thumbnail(path, render_size))
            .or_else(|| render_dcraw_thumbnail(path, render_size)),
        ThumbnailKind::Pdf => render_pdf_thumbnail(path, render_size),
        ThumbnailKind::Video => render_video_thumbnail(path, render_size),
    }
}

fn render_pixbuf_thumbnail(path: &Path, size: i32) -> Option<Vec<u8>> {
    gdk_pixbuf::Pixbuf::from_file_at_scale(path, size, size, true)
        .ok()?
        .save_to_bufferv("png", &[])
        .ok()
}

fn render_imagemagick_thumbnail(path: &Path, size: i32) -> Option<Vec<u8>> {
    for executable in ["magick", "convert"] {
        let output = Command::new(executable)
            .arg(path)
            .args(["-auto-orient", "-thumbnail"])
            .arg(format!("{size}x{size}"))
            .arg("png:-")
            .output();
        if let Ok(output) = output
            && output.status.success()
            && !output.stdout.is_empty()
        {
            return Some(output.stdout);
        }
    }
    None
}

fn render_dcraw_thumbnail(path: &Path, size: i32) -> Option<Vec<u8>> {
    for executable in ["dcraw_emu", "dcraw"] {
        let output = Command::new(executable)
            .args(["-e", "-c"])
            .arg(path)
            .output();
        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() || output.stdout.is_empty() {
            continue;
        }
        let loader = gdk_pixbuf::PixbufLoader::new();
        if loader.write(&output.stdout).is_err() || loader.close().is_err() {
            continue;
        }
        let Some(pixbuf) = loader.pixbuf() else {
            continue;
        };
        let width = pixbuf.width().max(1);
        let height = pixbuf.height().max(1);
        let scale = (f64::from(size) / f64::from(width))
            .min(f64::from(size) / f64::from(height))
            .min(1.0);
        let scaled = pixbuf.scale_simple(
            (f64::from(width) * scale).round().max(1.0) as i32,
            (f64::from(height) * scale).round().max(1.0) as i32,
            gdk_pixbuf::InterpType::Bilinear,
        )?;
        if let Ok(png) = scaled.save_to_bufferv("png", &[]) {
            return Some(png);
        }
    }
    None
}

fn render_pdf_thumbnail(path: &Path, size: i32) -> Option<Vec<u8>> {
    let uri = gio::File::for_path(path).uri();
    let document = poppler::Document::from_file(&uri, None).ok()?;
    let page = document.page(0)?;
    let (page_width, page_height) = page.size();
    if page_width <= 0.0 || page_height <= 0.0 {
        return None;
    }
    let scale = (f64::from(size) / page_width).min(f64::from(size) / page_height);
    let width = (page_width * scale).ceil().max(1.0) as i32;
    let height = (page_height * scale).ceil().max(1.0) as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let context = cairo::Context::new(&surface).ok()?;
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.paint().ok()?;
    context.scale(scale, scale);
    page.render(&context);
    surface.flush();
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    Some(png)
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
