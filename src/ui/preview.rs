// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gio, glib, prelude::*};
use sourceview5::prelude::*;

use crate::{
    model::{FileEntry, MetadataValue},
    services::{
        LoadHandle, Preview, PreviewContent, PreviewEvent, PreviewProvider, PreviewRequest,
        PreviewRequestId,
    },
};

const DEFAULT_WIDTH: i32 = 384;
const MIN_WIDTH: i32 = 280;
const MAX_WIDTH: i32 = 720;
const TEXT_BYTE_LIMIT: usize = 1024 * 1024;
const TRANSITION: Duration = Duration::from_millis(260);

struct PreviewState {
    provider: Rc<dyn PreviewProvider>,
    revealer: gtk::Revealer,
    pane: gtk::Box,
    title: gtk::Label,
    size: gtk::Label,
    modified: gtk::Label,
    content_type: gtk::Label,
    content: gtk::Box,
    split: RefCell<Option<gtk::Paned>>,
    current: RefCell<Option<FileEntry>>,
    load: RefCell<Option<LoadHandle>>,
    current_request: Cell<Option<PreviewRequestId>>,
    next_request: Cell<u64>,
    opened: Cell<bool>,
    width: Cell<i32>,
    animating: Cell<bool>,
    animation_generation: Rc<Cell<u64>>,
}

#[derive(Clone)]
pub struct PreviewDrawer {
    state: Rc<PreviewState>,
}

impl PreviewDrawer {
    pub fn new(provider: Rc<dyn PreviewProvider>) -> Self {
        let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pane.add_css_class("preview-pane");
        pane.set_size_request(MIN_WIDTH, -1);
        pane.set_hexpand(true);
        pane.set_vexpand(true);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("preview-header");
        let icon = crate::assets::primary_icon(crate::assets::icons::DOCUMENTS, 18);
        let title = gtk::Label::new(None);
        title.add_css_class("preview-title");
        title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        title.set_hexpand(true);
        title.set_xalign(0.0);
        let close = gtk::Button::builder()
            .tooltip_text("Close preview (Space)")
            .build();
        close.set_child(Some(&crate::assets::primary_icon(
            crate::assets::icons::X,
            16,
        )));
        close.add_css_class("preview-close");
        header.append(&icon);
        header.append(&title);
        header.append(&close);
        pane.append(&header);

        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        metadata.add_css_class("preview-metadata");
        let (size_group, size) = metadata_value("SIZE");
        let (modified_group, modified) = metadata_value("MODIFIED");
        let (type_group, content_type) = metadata_value("TYPE");
        metadata.append(&size_group);
        metadata.append(&modified_group);
        metadata.append(&type_group);
        pane.append(&metadata);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add_css_class("preview-content");
        content.set_vexpand(true);
        pane.append(&content);

        let revealer = gtk::Revealer::builder()
            .child(&pane)
            .transition_duration(0)
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .reveal_child(false)
            .build();

        let state = Rc::new(PreviewState {
            provider,
            revealer,
            pane,
            title,
            size,
            modified,
            content_type,
            content,
            split: RefCell::new(None),
            current: RefCell::new(None),
            load: RefCell::new(None),
            current_request: Cell::new(None),
            next_request: Cell::new(1),
            opened: Cell::new(false),
            width: Cell::new(DEFAULT_WIDTH),
            animating: Cell::new(false),
            animation_generation: Rc::new(Cell::new(0)),
        });
        let weak = Rc::downgrade(&state);
        close.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.close();
            }
        });

        Self { state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.state.revealer.clone().upcast()
    }

    pub fn attach_split(&self, split: &gtk::Paned) {
        self.state.split.replace(Some(split.clone()));
        let weak = Rc::downgrade(&self.state);
        split.connect_position_notify(move |split| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if state.opened.get() && !state.animating.get() {
                let available = split.width();
                let width = available.saturating_sub(split.position());
                let bounded = width.clamp(MIN_WIDTH, MAX_WIDTH);
                state.width.set(bounded);
                if width != bounded {
                    split.set_position(available.saturating_sub(bounded));
                }
            }
        });
    }

    pub fn is_open(&self) -> bool {
        self.state.opened.get()
    }

    pub fn show(&self, entry: FileEntry) {
        self.state.show(entry);
    }

    pub fn close(&self) {
        self.state.close();
    }

    pub fn toggle(&self, entry: Option<FileEntry>) {
        if self.is_open() {
            self.close();
        } else if let Some(entry) = entry {
            self.show(entry);
        }
    }
}

impl PreviewState {
    fn show(self: &Rc<Self>, entry: FileEntry) {
        let was_open = self.opened.replace(true);
        let already_showing = self.current.borrow().as_ref() == Some(&entry);
        if !was_open {
            self.revealer.set_transition_duration(0);
            self.pane.set_size_request(0, -1);
            self.revealer.set_reveal_child(true);
            if let Some(split) = self.split.borrow().as_ref() {
                self.animate_open(split);
            }
        }
        if !was_open || !already_showing {
            self.load(entry);
        }
    }

    fn animate_open(self: &Rc<Self>, split: &gtk::Paned) {
        let available = split.width();
        if available <= MIN_WIDTH {
            return;
        }
        let target = available.saturating_sub(self.width.get().clamp(MIN_WIDTH, MAX_WIDTH));
        let start = available;
        split.set_position(start);
        let animation_id = self.animation_generation.get().saturating_add(1);
        self.animation_generation.set(animation_id);
        self.animating.set(true);

        if !super::motion::animations_enabled() {
            split.set_position(target);
            self.pane.set_size_request(MIN_WIDTH, -1);
            self.animating.set(false);
            return;
        }

        let started = Instant::now();
        let split = split.clone();
        let pane = self.pane.clone();
        let generation = self.animation_generation.clone();
        let weak = Rc::downgrade(self);
        let _tick = split.clone().add_tick_callback(move |_, _| {
            if generation.get() != animation_id {
                return glib::ControlFlow::Break;
            }
            let progress =
                (started.elapsed().as_secs_f64() / TRANSITION.as_secs_f64()).clamp(0.0, 1.0);
            let eased = super::motion::emphasized_deceleration(progress);
            let position = f64::from(start) + f64::from(target - start) * eased;
            split.set_position(position.round() as i32);
            if progress >= 1.0 {
                split.set_position(target);
                pane.set_size_request(MIN_WIDTH, -1);
                if let Some(state) = weak.upgrade() {
                    state.animating.set(false);
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    fn close(self: &Rc<Self>) {
        self.opened.set(false);
        self.animating.set(false);
        self.animation_generation
            .set(self.animation_generation.get().saturating_add(1));
        self.current_request.set(None);
        self.load.borrow_mut().take();
        self.revealer.set_transition_duration(0);
        self.revealer.set_reveal_child(false);
        if let Some(split) = self.split.borrow().as_ref() {
            split.set_position(split.width());
        }
        self.pane.set_size_request(MIN_WIDTH, -1);
    }

    fn load(self: &Rc<Self>, entry: FileEntry) {
        self.current.replace(Some(entry.clone()));
        self.title.set_text(&entry.display_name);
        self.title
            .set_tooltip_text(Some(&entry.location.display_path()));
        self.size.set_text(&metadata_size(&entry));
        self.modified.set_text(&metadata_modified(&entry));
        self.content_type.set_text(file_extension(&entry));
        self.show_loading();
        self.load.borrow_mut().take();

        let request_id = PreviewRequestId(self.next_request.get());
        self.next_request
            .set(self.next_request.get().saturating_add(1));
        self.current_request.set(Some(request_id));
        let weak = Rc::downgrade(self);
        let emit = Rc::new(move |event| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            state.handle_event(request_id, event);
        });
        let load = self.provider.load(
            PreviewRequest {
                id: request_id,
                entry,
                text_byte_limit: TEXT_BYTE_LIMIT,
            },
            emit,
        );
        self.load.replace(Some(load));
    }

    fn handle_event(&self, expected: PreviewRequestId, event: PreviewEvent) {
        if self.current_request.get() != Some(expected) {
            return;
        }
        match event {
            PreviewEvent::Ready(preview) if preview.request_id == expected => {
                self.current_request.set(None);
                self.load.borrow_mut().take();
                self.render(preview);
            }
            PreviewEvent::Failed {
                request_id,
                entry,
                message,
            } if request_id == expected => {
                self.current_request.set(None);
                self.load.borrow_mut().take();
                self.title.set_text(&entry.display_name);
                self.show_message("Preview unavailable", &message);
            }
            PreviewEvent::Ready(_) | PreviewEvent::Failed { .. } => {}
        }
    }

    fn render(&self, preview: Preview) {
        self.content_type.set_text(&preview.content_type);
        clear_box(&self.content);
        match preview.content {
            PreviewContent::Text { content, truncated } => {
                let buffer = sourceview5::Buffer::new(None);
                let languages = sourceview5::LanguageManager::default();
                let language = languages.guess_language(
                    preview.entry.location.native_path(),
                    Some(&preview.content_type),
                );
                buffer.set_language(language.as_ref());
                buffer.set_style_scheme(super::theme::source_style_scheme().as_ref());
                buffer.set_highlight_syntax(true);
                buffer.set_text(&content);
                let view = sourceview5::View::builder()
                    .buffer(&buffer)
                    .cursor_visible(false)
                    .editable(false)
                    .highlight_current_line(false)
                    .left_margin(14)
                    .right_margin(14)
                    .top_margin(12)
                    .bottom_margin(12)
                    .monospace(true)
                    .show_line_numbers(true)
                    .wrap_mode(gtk::WrapMode::None)
                    .build();
                view.add_css_class("preview-text");
                let scroll = gtk::ScrolledWindow::builder()
                    .child(&view)
                    .hscrollbar_policy(gtk::PolicyType::Automatic)
                    .vscrollbar_policy(gtk::PolicyType::Automatic)
                    .hexpand(true)
                    .vexpand(true)
                    .build();
                self.content.append(&scroll);
                if truncated {
                    let notice = gtk::Label::new(Some("Preview limited to the first 1 MB"));
                    notice.add_css_class("preview-note");
                    self.content.append(&notice);
                }
            }
            PreviewContent::Image => {
                let file = file_for_entry(&preview.entry);
                let picture = gtk::Picture::for_file(&file);
                picture.add_css_class("preview-image");
                picture.set_can_shrink(true);
                picture.set_content_fit(gtk::ContentFit::Contain);
                picture.set_hexpand(true);
                picture.set_vexpand(true);
                self.content.append(&picture);
            }
            PreviewContent::Media => {
                let file = file_for_entry(&preview.entry);
                let video = gtk::Video::new();
                video.add_css_class("preview-media");
                video.set_autoplay(false);
                video.set_loop(false);
                video.set_file(Some(&file));
                video.set_hexpand(true);
                video.set_vexpand(true);
                self.content.append(&video);
            }
            PreviewContent::Unsupported => {
                self.show_message(
                    "No visual preview",
                    "Metadata is available for this file type.",
                );
            }
        }
    }

    fn show_loading(&self) {
        clear_box(&self.content);
        let spinner = gtk::Spinner::new();
        spinner.add_css_class("preview-spinner");
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        spinner.set_vexpand(true);
        spinner.start();
        self.content.append(&spinner);
    }

    fn show_message(&self, title: &str, detail: &str) {
        clear_box(&self.content);
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 7);
        box_.add_css_class("preview-feedback");
        box_.set_halign(gtk::Align::Center);
        box_.set_valign(gtk::Align::Center);
        box_.set_vexpand(true);
        let heading = gtk::Label::new(Some(title));
        heading.add_css_class("preview-feedback-title");
        let detail = gtk::Label::new(Some(detail));
        detail.add_css_class("preview-feedback-detail");
        detail.set_justify(gtk::Justification::Center);
        detail.set_wrap(true);
        box_.append(&heading);
        box_.append(&detail);
        self.content.append(&box_);
    }
}

fn metadata_value(label: &str) -> (gtk::Box, gtk::Label) {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 2);
    group.set_hexpand(true);
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("preview-metadata-label");
    heading.set_xalign(0.0);
    let value = gtk::Label::new(Some("—"));
    value.add_css_class("preview-metadata-value");
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    value.set_xalign(0.0);
    group.append(&heading);
    group.append(&value);
    (group, value)
}

fn clear_box(box_: &gtk::Box) {
    while let Some(child) = box_.first_child() {
        box_.remove(&child);
    }
}

fn file_for_entry(entry: &FileEntry) -> gio::File {
    entry
        .location
        .native_path()
        .map(gio::File::for_path)
        .unwrap_or_else(|| gio::File::for_uri(entry.location.uri_value().unwrap_or_default()))
}

fn metadata_size(entry: &FileEntry) -> String {
    match entry.size {
        MetadataValue::Known(bytes) => format_file_size(bytes),
        MetadataValue::Unknown | MetadataValue::Unavailable => "—".to_owned(),
    }
}

fn metadata_modified(entry: &FileEntry) -> String {
    let MetadataValue::Known(seconds) = entry.modified_unix_seconds else {
        return "—".to_owned();
    };
    glib::DateTime::from_unix_local(seconds)
        .and_then(|date| date.format("%Y-%m-%d %H:%M"))
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "—".to_owned())
}

fn file_extension(entry: &FileEntry) -> &str {
    entry
        .location
        .native_path()
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or("file")
}

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 || value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests;
