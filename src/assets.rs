// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    ffi::CString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use gtk::{gdk, gio, glib};

pub mod icons {
    pub const DOCUMENTS: &str = "strata-file-text-symbolic";
    pub const DOWNLOADS: &str = "strata-download-symbolic";
    pub const HOME: &str = "strata-house-symbolic";
    pub const PICTURES: &str = "strata-image-symbolic";
    pub const SEARCH: &str = "strata-search-symbolic";
    pub const VIDEOS: &str = "strata-video-symbolic";
}

const FONT_VERSION: &str = "2.304";
const JETBRAINS_MONO: &[u8] = include_bytes!("../data/fonts/JetBrainsMono[wght].ttf");

pub fn prepare() -> Result<(), Box<dyn std::error::Error>> {
    gio::resources_register_include!("strata.gresource")?;

    let font_directory = glib::user_cache_dir()
        .join("strata")
        .join("fonts")
        .join(FONT_VERSION);
    fs::create_dir_all(&font_directory)?;

    let regular = font_directory.join("JetBrainsMono.ttf");
    write_if_changed(&regular, JETBRAINS_MONO)?;
    register_application_fonts([regular])?;

    Ok(())
}

pub fn register_icon_theme() {
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_resource_path("/io/github/lgse/Strata/icons");
    }
}

fn write_if_changed(path: &Path, contents: &[u8]) -> io::Result<()> {
    let is_current = path
        .metadata()
        .map(|metadata| metadata.len() == contents.len() as u64)
        .unwrap_or(false);

    if !is_current {
        fs::write(path, contents)?;
    }

    Ok(())
}

#[allow(unsafe_code)]
fn register_application_fonts(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: Font registration happens during single-threaded startup, before GTK/Pango
    // creates a font map. Fontconfig owns the current config; each C string lives through
    // its call, and Fontconfig copies the path rather than retaining the pointer.
    unsafe {
        let config = fontconfig_sys::FcConfigGetCurrent();
        if config.is_null() {
            return Err("Fontconfig did not provide a current configuration".into());
        }

        for path in paths {
            let path = CString::new(path.as_os_str().as_bytes())?;
            if fontconfig_sys::FcConfigAppFontAddFile(config, path.as_ptr().cast()) == 0 {
                return Err("Fontconfig could not register a bundled font".into());
            }
        }

        if fontconfig_sys::FcConfigBuildFonts(config) == 0 {
            return Err("Fontconfig could not rebuild the application font set".into());
        }
    }

    Ok(())
}
