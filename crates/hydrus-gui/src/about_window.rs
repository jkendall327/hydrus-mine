//! The about window, bound (`ui/about.slint`): what hydrus-gui-model's
//! [`about`](crate::about) says, from what this process and its store
//! can tell.

use std::sync::OnceLock;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_gui_model::about::{Availability, Facts, Library, about_with_format};
use hydrus_store::Store;

use crate::AboutWindow;

/// The license hydrus-rs is distributed under (the reference's).
const LICENSE: &str = include_str!("../../../LICENSE");

static BOOT_MS: OnceLock<i64> = OnceLock::new();

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Note the time hydrus-rs started (its boot time), if not already.
pub(crate) fn note_boot() {
    BOOT_MS.get_or_init(now_ms);
}

/// `ffmpeg -version`'s version ("ffmpeg version 6.1.1 Copyright ...").
fn ffmpeg_version(store: &Store) -> Option<String> {
    let policy = store
        .read(hydrus_store::ffmpeg_policy::load)
        .unwrap_or_default();
    hydrus_media::Ffmpeg::default()
        .timeout(policy.timeout())
        .version()
        .ok()
        .flatten()
}

fn library(name: &str, state: Availability) -> Library {
    Library {
        name: name.to_owned(),
        state,
    }
}

/// The optional parts and whether they are there: what the reference checks
/// of its Python modules, for what hydrus-rs does without or with built in.
fn libraries(ffmpeg: bool) -> Vec<Vec<Library>> {
    let native = || Availability::Yes(Some("native".to_owned()));
    vec![
        vec![library("PDF", native()), library("SVG", native())],
        vec![library("mpv", Availability::Missing)],
        vec![
            library(
                "ffmpeg",
                if ffmpeg {
                    Availability::Yes(None)
                } else {
                    Availability::Missing
                },
            ),
            library("lz4", native()),
            library("olefile", native()),
            library("HEIF", Availability::Missing),
            library("AVIF", Availability::Missing),
            library("JpegXL", Availability::Missing),
        ],
    ]
}

/// The locale from the environment (`en_US` of `en_US.UTF-8`).
fn locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .map_or_else(
            || "C".to_owned(),
            |value| value.split(['.', '@']).next().unwrap_or("C").to_owned(),
        )
}

/// What the window says, from this process and its store.
pub fn facts(store: &Store) -> Facts {
    let pragmas = store.read(|conn| {
        let sqlite: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
        let journal: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        let cache: i64 = conn.query_row("PRAGMA cache_size", [], |r| r.get(0))?;
        let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
        Ok((sqlite, journal, synchronous, cache, temp_store))
    });
    let (sqlite, journal_mode, synchronous, cache, temp_store) =
        pragmas.unwrap_or_else(|_| (String::new(), String::new(), 0, 0, 0));
    let ffmpeg = ffmpeg_version(store);
    // (a negative cache size is in KiB)
    let cache_mb = if cache < 0 { -cache / 1024 } else { cache };
    let os = match std::env::consts::OS {
        "linux" => "Linux".to_owned(),
        "windows" => "Windows".to_owned(),
        "macos" => "macOS".to_owned(),
        other => other.to_owned(),
    };
    Facts {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        os,
        libraries: libraries(ffmpeg.is_some()),
        ffmpeg,
        sqlite,
        running_as: if cfg!(debug_assertions) {
            "from a debug build"
        } else {
            "from a release build"
        }
        .to_owned(),
        locale: locale(),
        sqlite_temp_dir: std::env::var("SQLITE_TMPDIR").ok(),
        // (2 is MEMORY)
        temp_in_memory: temp_store == 2,
        boot_ms: *BOOT_MS.get_or_init(now_ms),
        now_ms: now_ms(),
        install_dir: std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_default(),
        db_dir: store.dir().to_string_lossy().into_owned(),
        temp_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        cache_mb,
        journal_mode,
        synchronous,
    }
}

/// Open the about window.
pub fn open(store: &Store) -> Result<AboutWindow, slint::PlatformError> {
    let window = crate::app_title::new::<crate::AboutWindow>()?;
    let about = about_with_format(
        &facts(store),
        Some(LICENSE),
        &hydrus_gui_model::gui_format::preferences(store),
    );
    window.set_name(about.name.into());
    window.set_version(about.version.into());
    window.set_site(about.site.as_str().into());
    let (tabs, texts): (Vec<SharedString>, Vec<SharedString>) = about
        .tabs
        .into_iter()
        .map(|(n, t)| (n.into(), t.into()))
        .unzip();
    window.set_tabs(ModelRc::new(VecModel::from(tabs)));
    window.set_texts(ModelRc::new(VecModel::from(texts)));
    let site = about.site;
    window.on_open_site(move || crate::launch(&site));
    window.on_close_clicked({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    window.show()?;
    Ok(window)
}
