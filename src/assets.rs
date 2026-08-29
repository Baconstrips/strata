// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    ffi::CString,
    fs, io,
    io::Cursor,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use gtk::{gdk, gio, glib};

pub mod icons {
    pub const ARROW_DOWN_WIDE_NARROW: &str = "strata-arrow-down-wide-narrow";
    pub const ARROW_UP_NARROW_WIDE: &str = "strata-arrow-up-narrow-wide";
    pub const CHECK: &str = "strata-check";
    pub const CHECK_ON_PRIMARY: &str = "strata-check-on-primary";
    pub const CHEVRON_RIGHT: &str = "strata-chevron-right";
    pub const COPY: &str = "strata-copy";
    pub const DOCUMENTS: &str = "strata-file-text";
    pub const DOWNLOADS: &str = "strata-download";
    pub const EYE: &str = "strata-eye";
    pub const EYE_OFF: &str = "strata-eye-off";
    pub const FILE_ARCHIVE: &str = "strata-file-archive";
    pub const FILE_CODE: &str = "strata-file-code";
    pub const FOLDER: &str = "strata-folder";
    pub const HARD_DRIVE: &str = "strata-hard-drive";
    pub const FUNNEL: &str = "strata-funnel";
    pub const GRID: &str = "strata-grid";
    pub const HOME: &str = "strata-house";
    pub const LIST: &str = "strata-list";
    pub const LIST_ACTIVE: &str = "strata-list-active";
    pub const KEYBOARD: &str = "strata-keyboard";
    pub const MONITOR: &str = "strata-monitor";
    pub const PALETTE: &str = "strata-palette";
    pub const PANEL_LEFT: &str = "strata-panel-left-symbolic";
    pub const PLUS: &str = "strata-plus";
    pub const PICTURES: &str = "strata-image";
    pub const ROWS: &str = "strata-rows";
    pub const SEARCH: &str = "strata-search";
    pub const SETTINGS: &str = "strata-settings";
    pub const SETTINGS_2: &str = "strata-settings-2";
    pub const SETTINGS_ACTIVE: &str = "strata-settings-active";
    pub const SLIDERS: &str = "strata-sliders-horizontal";
    pub const TERMINAL: &str = "strata-terminal";
    pub const TRASH: &str = "strata-trash";
    pub const VIDEOS: &str = "strata-video";
    pub const X: &str = "strata-x";
}

const FONT_VERSION: &str = "2.304";
const JETBRAINS_MONO: &[u8] = include_bytes!("../data/fonts/JetBrainsMono[wght].ttf");

struct PrimaryIcon {
    image: glib::WeakRef<gtk::Image>,
    name: String,
}

thread_local! {
    static PRIMARY_ICON_COLOR: RefCell<String> = RefCell::new("#8bc9eb".to_owned());
    static PRIMARY_ICONS: RefCell<Vec<PrimaryIcon>> = const { RefCell::new(Vec::new()) };
}

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

pub fn primary_icon(name: &str, pixel_size: i32) -> gtk::Image {
    let image = gtk::Image::new();
    image.set_pixel_size(pixel_size);
    set_primary_icon(&image, name);
    image
}

pub fn set_primary_icon(image: &gtk::Image, name: &str) {
    let color = PRIMARY_ICON_COLOR.with(|color| color.borrow().clone());
    apply_primary_icon(image, name, &color);
    PRIMARY_ICONS.with(|icons| {
        let mut icons = icons.borrow_mut();
        icons.retain(|icon| icon.image.upgrade().is_some());
        if let Some(icon) = icons
            .iter_mut()
            .find(|icon| icon.image.upgrade().as_ref() == Some(image))
        {
            icon.name = name.to_owned();
            return;
        }
        let image_ref = glib::WeakRef::new();
        image_ref.set(Some(image));
        icons.push(PrimaryIcon {
            image: image_ref,
            name: name.to_owned(),
        });
    });
}

pub fn set_primary_icon_color(color: &str) {
    PRIMARY_ICON_COLOR.with(|current| current.replace(color.to_owned()));
    PRIMARY_ICONS.with(|icons| {
        icons.borrow_mut().retain(|icon| {
            let Some(image) = icon.image.upgrade() else {
                return false;
            };
            apply_primary_icon(&image, &icon.name, color);
            true
        });
    });
}

fn apply_primary_icon(image: &gtk::Image, name: &str, color: &str) {
    image.set_icon_name(Some(name));
    let Some(texture) = primary_icon_texture(name, color) else {
        return;
    };
    image.set_paintable(Some(&texture));
}

fn primary_icon_texture(name: &str, color: &str) -> Option<gdk::Texture> {
    let path = format!("/io/github/lgse/Strata/icons/scalable/actions/{name}.svg");
    let data = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE).ok()?;
    let source = std::str::from_utf8(data.as_ref()).ok()?;
    let mut source = source.replace("#8bc9eb", color);
    if name == icons::FOLDER {
        source = source.replacen(
            "fill=\"none\"",
            &format!("fill=\"{color}\" fill-opacity=\"0.15\""),
            1,
        );
    }
    let pixbuf = gdk_pixbuf::Pixbuf::from_read(Cursor::new(source.into_bytes())).ok()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
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
