// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    ffi::CString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use gtk::{gdk, gio, glib};

pub mod icons {
    pub const ARROW_UP_DOWN: &str = "strata-arrow-up-down";
    pub const CHECK: &str = "strata-check";
    pub const COPY: &str = "strata-copy";
    pub const DOCUMENTS: &str = "strata-file-text";
    pub const EYE: &str = "strata-eye";
    pub const EYE_OFF: &str = "strata-eye-off";
    pub const DOWNLOADS: &str = "strata-download";
    pub const FOLDER: &str = "strata-folder";
    pub const HARD_DRIVE: &str = "strata-hard-drive";
    pub const FUNNEL: &str = "strata-funnel";
    pub const GRID: &str = "strata-grid";
    pub const HOME: &str = "strata-house";
    pub const LIST: &str = "strata-list";
    pub const LIST_ACTIVE: &str = "strata-list-active";
    pub const PANEL_LEFT: &str = "strata-panel-left-symbolic";
    pub const PICTURES: &str = "strata-image";
    pub const ROWS: &str = "strata-rows";
    pub const SEARCH: &str = "strata-search";
    pub const SETTINGS: &str = "strata-settings";
    pub const SETTINGS_ACTIVE: &str = "strata-settings-active";
    pub const TRASH: &str = "strata-trash";
    pub const VIDEOS: &str = "strata-video";
    pub const X: &str = "strata-x";
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

#[expect(
    unsafe_code,
    reason = "Fontconfig exposes application-font registration only through its C FFI"
)]
fn register_application_fonts(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: This no-argument Fontconfig call returns a borrowed process-global
    // configuration. We check it for null before passing it to any other FFI call.
    let config = unsafe { fontconfig_sys::FcConfigGetCurrent() };
    if config.is_null() {
        return Err("Fontconfig did not provide a current configuration".into());
    }

    for path in paths {
        let path = CString::new(path.as_os_str().as_bytes())?;

        // SAFETY: `config` was checked above. `path` is a valid, NUL-terminated C string
        // that remains alive for the call, and Fontconfig copies rather than retains it.
        let registered =
            unsafe { fontconfig_sys::FcConfigAppFontAddFile(config, path.as_ptr().cast()) };
        if registered == 0 {
            return Err("Fontconfig could not register a bundled font".into());
        }
    }

    // SAFETY: `config` is the same checked process-global configuration. Registration
    // runs during single-threaded startup before GTK/Pango creates the application's map.
    let rebuilt = unsafe { fontconfig_sys::FcConfigBuildFonts(config) };
    if rebuilt == 0 {
        return Err("Fontconfig could not rebuild the application font set".into());
    }

    Ok(())
}
