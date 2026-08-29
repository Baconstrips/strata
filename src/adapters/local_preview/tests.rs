// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;

use gtk::gio::prelude::FileExt;

use super::render_pdf_blocking;

#[test]
fn renders_requested_pdf_pages_within_the_pixel_budget() {
    let path = std::env::temp_dir().join(format!(
        "strata-preview-{}-{}.pdf",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let surface = cairo::PdfSurface::new(612.0, 792.0, &path).expect("create PDF surface");
    {
        let context = cairo::Context::new(&surface).expect("create PDF context");
        context.set_source_rgb(0.2, 0.4, 0.8);
        context.paint().expect("paint PDF page");
        context.show_page().expect("finish first PDF page");
        context.set_source_rgb(0.8, 0.4, 0.2);
        context.paint().expect("paint second PDF page");
        context.show_page().expect("finish second PDF page");
    }
    surface.finish();

    let uri = gtk::gio::File::for_path(&path).uri();
    let (png, page, pages) = render_pdf_blocking(&uri, 1).expect("render second PDF page");
    let _removed = fs::remove_file(path);

    assert_eq!(page, 1);
    assert_eq!(pages, 2);
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}
