// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gtk::{gio, glib, prelude::*};

use crate::{
    model::Location,
    services::{
        LoadHandle, Preview, PreviewContent, PreviewEvent, PreviewProvider, PreviewRequest,
        content_family, has_plain_text_extension,
    },
};

#[derive(Default)]
pub struct LocalPreviewProvider;

impl PreviewProvider for LocalPreviewProvider {
    fn load(&self, request: PreviewRequest, emit: Rc<dyn Fn(PreviewEvent)>) -> LoadHandle {
        let request_id = request.id;
        let entry = request.entry.clone();
        let task = glib::MainContext::default().spawn_local(async move {
            let file = file_for_location(&entry.location);
            let info = match file
                .query_info_future(
                    "standard::content-type",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await
            {
                Ok(info) => info,
                Err(error) => {
                    emit(PreviewEvent::Failed {
                        request_id,
                        entry,
                        message: error.to_string(),
                    });
                    return;
                }
            };
            let content_type = info
                .content_type()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            let mut content = content_family(&content_type);
            if matches!(content, PreviewContent::Unsupported)
                && (gio::content_type_is_a(&content_type, "text/plain")
                    || has_plain_text_extension(&entry.native_name))
            {
                content = PreviewContent::Text {
                    content: String::new(),
                    truncated: false,
                };
            }

            if matches!(content, PreviewContent::Pdf { .. }) {
                content = match render_pdf(&file, request.pdf_page).await {
                    Ok((png, page, pages)) => PreviewContent::Pdf { png, page, pages },
                    Err(message) => {
                        emit(PreviewEvent::Failed {
                            request_id,
                            entry,
                            message,
                        });
                        return;
                    }
                };
            } else if matches!(content, PreviewContent::Text { .. }) {
                content = match read_text(&file, request.text_byte_limit).await {
                    Ok((content, truncated)) => PreviewContent::Text { content, truncated },
                    Err(error) => {
                        emit(PreviewEvent::Failed {
                            request_id,
                            entry,
                            message: error.to_string(),
                        });
                        return;
                    }
                };
            }

            emit(PreviewEvent::Ready(Preview {
                request_id,
                entry,
                content_type,
                content,
            }));
        });

        LoadHandle::new(move || task.abort())
    }
}

fn file_for_location(location: &Location) -> gio::File {
    location
        .native_path()
        .map(gio::File::for_path)
        .unwrap_or_else(|| gio::File::for_uri(location.uri_value().unwrap_or_default()))
}

async fn render_pdf(file: &gio::File, page: i32) -> Result<(Vec<u8>, i32, i32), String> {
    let uri = file.uri().to_string();
    gio::spawn_blocking(move || render_pdf_blocking(&uri, page))
        .await
        .map_err(|_| "The PDF renderer stopped unexpectedly".to_owned())?
}

fn render_pdf_blocking(uri: &str, requested_page: i32) -> Result<(Vec<u8>, i32, i32), String> {
    const MAX_WIDTH: f64 = 1400.0;
    const MAX_HEIGHT: f64 = 1800.0;
    const MAX_PIXELS: f64 = 2_500_000.0;

    let document = poppler::Document::from_file(uri, None).map_err(|error| error.to_string())?;
    let pages = document.n_pages();
    if pages <= 0 {
        return Err("This PDF has no pages".to_owned());
    }
    let page_index = requested_page.clamp(0, pages - 1);
    let page = document
        .page(page_index)
        .ok_or_else(|| "Unable to load that PDF page".to_owned())?;
    let (page_width, page_height) = page.size();
    if page_width <= 0.0 || page_height <= 0.0 {
        return Err("The PDF page has invalid dimensions".to_owned());
    }

    let scale = (MAX_WIDTH / page_width)
        .min(MAX_HEIGHT / page_height)
        .min((MAX_PIXELS / (page_width * page_height)).sqrt());
    let width = (page_width * scale).ceil() as i32;
    let height = (page_height * scale).ceil() as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|error| error.to_string())?;
    let context = cairo::Context::new(&surface).map_err(|error| error.to_string())?;
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.paint().map_err(|error| error.to_string())?;
    context.scale(scale, scale);
    page.render(&context);
    surface.flush();

    let mut png = Vec::new();
    surface
        .write_to_png(&mut png)
        .map_err(|error| error.to_string())?;
    Ok((png, page_index, pages))
}

async fn read_text(file: &gio::File, byte_limit: usize) -> Result<(String, bool), glib::Error> {
    let stream = file.read_future(glib::Priority::DEFAULT).await?;
    let bytes = stream
        .read_bytes_future(byte_limit.saturating_add(1), glib::Priority::DEFAULT)
        .await?;
    let bytes = bytes.as_ref();
    let truncated = bytes.len() > byte_limit;
    let sample = &bytes[..bytes.len().min(byte_limit)];
    Ok((String::from_utf8_lossy(sample).into_owned(), truncated))
}

#[cfg(test)]
mod tests;
