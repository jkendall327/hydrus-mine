//! The about window (help > about, the reference's `ShowAboutWindow`):
//! the name, the version, the site, and four tabs: a description with the
//! platform, library versions, boot time, directories and database
//! settings; the optional libraries; the credits; and the license.

use hydrus_store::settings::GuiFormatting;

pub const TITLE: &str = "about hydrus";
pub const SITE: &str = "https://hydrusnetwork.github.io/hydrus/";
pub const TABS: [&str; 4] = ["Description", "Optional Libraries", "Credits", "License"];
pub const NAME: &str = "hydrus-rs";

/// What the window says, gathered by the GUI.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// hydrus-rs's own version.
    pub version: String,
    /// The architecture and system ("x86_64", "Linux").
    pub arch: String,
    pub os: String,
    /// ffmpeg's version, if it runs.
    pub ffmpeg: Option<String>,
    pub sqlite: String,
    /// When hydrus-rs started, and now (milliseconds).
    pub boot_ms: i64,
    pub now_ms: i64,
    pub install_dir: String,
    pub db_dir: String,
    pub temp_dir: String,
    pub cache_mb: i64,
    pub journal_mode: String,
    pub synchronous: i64,
}

/// The window's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    pub name: String,
    pub version: String,
    pub site: String,
    /// Each tab's name and text, in [`TABS`]' order.
    pub tabs: Vec<(String, String)>,
}

/// `HydrusTime.TimestampMSToPrettyTime`: `YYYY-MM-DD HH:MM:SS.mmm` (UTC
/// here; the reference's is local time).
fn pretty_time_ms(ms: i64) -> String {
    let seconds = hydrus_import::status::pretty_time(hydrus_core::TimestampMs(ms));
    format!("{seconds}.{:03}", ms.rem_euclid(1000))
}

/// `render_availability_line`.
fn availability(name: &str, ok: bool) -> String {
    if ok {
        format!("{name}: yes")
    } else {
        format!("{name}: not available")
    }
}

/// The about window's texts, with the license's text if there is one.
pub fn about(facts: &Facts, license: Option<&str>) -> About {
    about_with_format(facts, license, &GuiFormatting::default())
}
pub fn about_with_format(
    facts: &Facts,
    license: Option<&str>,
    formatting: &GuiFormatting,
) -> About {
    let mut lines = vec![
        format!("running on {} {}", facts.arch, facts.os),
        format!("FFMPEG: {}", facts.ffmpeg.as_deref().unwrap_or("unknown")),
        format!("sqlite: {}", facts.sqlite),
        String::new(),
    ];
    lines.push(format!(
        "boot time: {} ({})",
        crate::gui_format::timestamp(
            formatting,
            Some(facts.boot_ms.div_euclid(1000)),
            facts.now_ms.div_euclid(1000)
        ),
        pretty_time_ms(facts.boot_ms)
    ));
    lines.push(String::new());
    lines.push(format!("install dir: {}", facts.install_dir));
    lines.push(format!("db dir: {}", facts.db_dir));
    lines.push(format!("temp dir: {}", facts.temp_dir));
    lines.push(String::new());
    lines.push(format!("db cache size per file: {}MB", facts.cache_mb));
    lines.push(format!("db journal mode: {}", facts.journal_mode));
    lines.push(format!("db synchronous mode: {}", facts.synchronous));
    let description = format!(
        "This is the media management application of the hydrus software suite, ported to Rust.\n\n{}",
        lines.join("\n")
    );
    let libraries = availability("ffmpeg", facts.ffmpeg.is_some());
    let credits = "Created by Anonymous\n\nhydrus-rs: a port of it to Rust".to_owned();
    let license = license.map_or_else(|| "no license file found!".to_owned(), str::to_owned);
    About {
        name: NAME.to_owned(),
        version: format!(
            "v{}, porting hydrus v{}, using network version 20",
            facts.version,
            hydrus_core::REFERENCE_VERSION
        ),
        site: SITE.to_owned(),
        tabs: TABS
            .iter()
            .map(|t| (*t).to_owned())
            .zip([description, libraries, credits, license])
            .collect(),
    }
}
