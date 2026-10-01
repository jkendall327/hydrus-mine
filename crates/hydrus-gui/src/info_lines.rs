//! A file's info lines, as the reference writes them
//! (`ClientMediaResultPrettyInfo.GetPrettyMediaResultInfoLines`): its size,
//! type, resolution, duration and frames, where it is and was, and when it
//! was imported, modified and archived. The media viewer's top hover
//! frame shows the "interesting" ones, joined with ` | `. Plain Rust,
//! tested against the reference.

use hydrus_core::Mime;
use hydrus_core::media_viewer::InfoLineSettings;
use hydrus_core::numbers::{human_bytes, human_bytes_ratio, human_int, resolution_text};
use hydrus_core::time::TimestampMs;
use hydrus_core::time::{duration_ms_to_pretty, timestamp_to_pretty_time_delta};
use hydrus_store::media::{FileFlags, MediaResult};
use hydrus_store::services::{ServiceKind, ServiceRegistry};

/// One line: its text, whether it is interesting, and, for a submenu, its
/// lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoLine {
    pub text: String,
    pub interesting: bool,
    pub submenu: Option<Vec<InfoLine>>,
}

impl InfoLine {
    fn new(text: impl Into<String>, interesting: bool) -> Self {
        Self {
            text: text.into(),
            interesting,
            submenu: None,
        }
    }
}

/// The top hover frame's line: the interesting lines, but submenus,
/// joined with ` | `.
pub fn top_line(
    media: &MediaResult,
    services: &ServiceRegistry,
    settings: &InfoLineSettings,
    now_ms: i64,
) -> String {
    info_lines(media, services, settings, now_ms, true)
        .iter()
        .filter(|l| l.submenu.is_none())
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// A file's info lines at `now_ms`; with `only_interesting`, just those
/// (worded as the options say for the top hover frame).
#[allow(clippy::too_many_lines)]
pub fn info_lines(
    media: &MediaResult,
    services: &ServiceRegistry,
    settings: &InfoLineSettings,
    now_ms: i64,
    only_interesting: bool,
) -> Vec<InfoLine> {
    let now = now_ms.div_euclid(1000);
    // (`TimestampToPrettyTimeDelta` of whole seconds)
    let ago = |ms: Option<TimestampMs>| match ms.map(|t| t.0) {
        Some(ms) => timestamp_to_pretty_time_delta(ms.div_euclid(1000), now, " ago"),
        None => "at an unknown time".to_owned(),
    };
    let is_interesting = |a: TimestampMs, b: TimestampMs| {
        let (d1, d2) = ((a.0 - now_ms).abs(), (b.0 - now_ms).abs());
        (d1.min(d2) as f64) / (d1.max(d2).max(1) as f64) < 0.9
    };
    let mut lines = Vec::new();
    let Some(info) = media.info.as_ref() else {
        return lines;
    };

    // size, type, resolution, duration, frames, audio, words
    let mut text = format!("{} {}", human_bytes(info.size), info.mime.human_name());
    if let (Some(width), Some(height)) = (info.width, info.height) {
        let resolution = if settings.nice_resolutions {
            resolution_text(u64::from(width), u64::from(height))
        } else {
            format!("{}x{}", human_int(width.into()), human_int(height.into()))
        };
        text += &format!(" ({resolution})");
    }
    let mut bitrate_duration_ms = None;
    if let Some(duration) = info.duration_ms {
        bitrate_duration_ms = Some(duration);
        text += &format!(", {}", duration_ms_to_pretty(duration));
    } else if let Some((simulated, source)) = simulated_duration(media) {
        bitrate_duration_ms = Some(simulated);
        text += &format!(", {} ({source})", duration_ms_to_pretty(simulated));
    }
    if let Some(frames) = info.num_frames {
        let framerate = info
            .duration_ms
            .filter(|&d| d > 0 && frames > 0)
            .map(|d| frames as f64 / (d as f64 / 1000.0));
        let insert = match framerate {
            None => String::new(),
            Some(f) if f < 1.0 => format!(", {f:.2}fps"),
            Some(f) if f < 10.0 => format!(", {f:.1}fps"),
            // (Python's round, halves to even)
            Some(f) => format!(", {}fps", f.round_ties_even()),
        };
        text += &format!(" ({} frames{insert})", human_int(frames));
    }
    if info.has_audio {
        text += &format!(", {}", settings.has_audio_label);
    }
    if let Some(words) = info.num_words {
        text += &format!(" ({} words)", human_int(words));
    }
    lines.push(InfoLine::new(text, true));
    lines.push(InfoLine::new(
        format!("{} bytes", human_int(info.size)),
        false,
    ));
    if let Some(duration) = bitrate_duration_ms
        && duration > 0
    {
        let rate = human_bytes_ratio(u128::from(info.size) * 1000, u128::from(duration));
        lines.push(InfoLine::new(format!("approx bitrate: {rate}/s"), false));
    }
    if let Some(original) = info.original_mime {
        lines.push(InfoLine::new(
            format!("filetype was originally: {}", original.human_name()),
            false,
        ));
    }

    // where it is and was
    let kind_of =
        |kind: fn(&ServiceKind) -> bool| services.all().find(|s| kind(&s.kind)).map(|s| s.id);
    let storage = kind_of(|k| matches!(k, ServiceKind::LocalFileStorage));
    let trash = kind_of(|k| matches!(k, ServiceKind::Trash));
    let current = |id: Option<hydrus_core::ServiceId>| id.is_some_and(|id| media.is_current_in(id));
    let deleted =
        |id: Option<hydrus_core::ServiceId>| id.is_some_and(|id| media.is_deleted_from(id));
    let deleted_at = |id: hydrus_core::ServiceId| {
        media
            .deleted
            .iter()
            .find(|d| d.service == id)
            .and_then(|d| d.deleted)
    };
    // (as the reference, the trash's line takes the time last looked at)
    let mut last_time: Option<TimestampMs> = None;
    let mut seen_local_times: Vec<TimestampMs> = Vec::new();
    if current(storage) {
        let added = storage.and_then(|s| media.added_to(s));
        lines.push(InfoLine::new(format!("imported: {}", ago(added)), true));
        last_time = added;
        seen_local_times.extend(added);
    } else {
        // (its size known: "you have never had it" is for files without)
        let text = if deleted(storage) {
            "you do not have this file, but you did once"
        } else {
            "you do not have this file, but your client has heard a bit about it"
        };
        lines.push(InfoLine::new(text, true));
    }
    let mut local: Vec<_> = services
        .all()
        .filter(|s| matches!(s.kind, ServiceKind::LocalFiles))
        .collect();
    local.sort_by(|a, b| a.name.cmp(&b.name));
    let show_import_times = !only_interesting || settings.file_services_import_times_interesting;
    for service in local.iter().filter(|s| media.is_current_in(s.id)) {
        let added = media.added_to(service.id);
        let text = if show_import_times {
            format!("added to {}: {}", service.name, ago(added))
        } else {
            service.name.clone()
        };
        lines.push(InfoLine::new(text, settings.file_services_interesting));
        last_time = added;
        seen_local_times.extend(added);
    }
    let reason = media
        .deletion_reason
        .clone()
        .unwrap_or_else(|| "Unknown deletion reason.".to_owned());
    if deleted(storage) {
        let at = storage.and_then(deleted_at);
        lines.push(InfoLine::new(
            format!("deleted from this client {} ({reason})", ago(at)),
            true,
        ));
        last_time = at;
    } else if current(trash) {
        for service in local.iter().filter(|s| media.is_deleted_from(s.id)) {
            let at = deleted_at(service.id);
            lines.push(InfoLine::new(
                format!("removed from {} {}", service.name, ago(at)),
                false,
            ));
            last_time = at;
        }
    }
    if current(trash) {
        let mut text = if !only_interesting || settings.trash_time_interesting {
            format!("sent to trash {}", ago(last_time))
        } else {
            "in the trash".to_owned()
        };
        if !only_interesting || settings.trash_reason_interesting {
            text += &format!(" ({reason})");
        }
        lines.push(InfoLine::new(text, true));
    }
    let mut remote: Vec<_> = services
        .all()
        .filter(|s| {
            matches!(
                s.kind,
                ServiceKind::FileRepository(_) | ServiceKind::Ipfs(_)
            ) && media.is_current_in(s.id)
        })
        .collect();
    remote.sort_by(|a, b| a.name.cmp(&b.name));
    for service in remote {
        let label = if matches!(service.kind, ServiceKind::Ipfs(_)) {
            "pinned"
        } else {
            "uploaded"
        };
        let added = media.added_to(service.id);
        let text = if show_import_times {
            format!("{label} to {} {}", service.name, ago(added))
        } else {
            format!("{label} to {}", service.name)
        };
        lines.push(InfoLine::new(text, settings.file_services_interesting));
    }

    // when it was modified
    if let Some(modified) = media.aggregate_modified() {
        let interesting = !settings.hide_uninteresting_modified_time
            || seen_local_times
                .iter()
                .all(|&t| is_interesting(t, modified));
        lines.push(InfoLine::new(
            format!("modified: {}", ago(Some(modified))),
            interesting,
        ));
        let mut all = Vec::new();
        if let Some(file) = info.file_modified {
            all.push(InfoLine::new(format!("local: {}", ago(Some(file))), false));
        }
        let mut domains = media.domain_modified.clone();
        domains.sort();
        for (domain, at) in domains {
            all.push(InfoLine::new(format!("{domain}: {}", ago(Some(at))), false));
        }
        if all.len() > 1 {
            lines.push(InfoLine {
                text: "all modified times".into(),
                interesting: false,
                submenu: Some(all),
            });
        }
    }

    // archived
    if current(storage) && !media.inbox {
        let show_time = !only_interesting || settings.archived_time_interesting;
        let text = match (media.archived, show_time) {
            (_, false) => "archived".to_owned(),
            (None, true) => "archived: unknown time".to_owned(),
            (Some(at), true) => format!("archived: {}", ago(Some(at))),
        };
        lines.push(InfoLine::new(text, settings.archived_interesting));
    }

    // what it has
    if info.has_audio {
        lines.push(InfoLine::new("has audio", false));
    }
    for (flag, text) in [
        (FileFlags::TRANSPARENCY, "has transparency"),
        (FileFlags::EXIF, "has exif metadata"),
        (FileFlags::XMP, "has xmp metadata"),
        (FileFlags::IPTC, "has iptc metadata"),
        (
            FileFlags::HUMAN_READABLE_METADATA,
            "has human-readable metadata",
        ),
        (FileFlags::SOFTWARE_SOURCE, "has software/source metadata"),
        (FileFlags::ICC_PROFILE, "has icc profile"),
    ] {
        if info.flags.has(flag) {
            lines.push(InfoLine::new(text, false));
        }
    }
    if only_interesting {
        lines.retain(|l| l.interesting);
    }
    lines
}

/// A ugoira without a duration of its own: its frames' durations from its
/// note, else 125ms a frame (`GetSimulatedDurationMSAndSource`).
fn simulated_duration(media: &MediaResult) -> Option<(u64, &'static str)> {
    let info = media.info.as_ref()?;
    let frames = info.num_frames.filter(|&n| n > 1)?;
    if info.mime != Mime::AnimationUgoira || info.duration_ms.is_some_and(|d| d > 0) {
        return None;
    }
    if hydrus_media::MediaTools::has_ugoira_frame_times_note(&media.notes) {
        return Some(
            match hydrus_media::MediaTools::ugoira_note_frame_durations(&media.notes) {
                Some(durations) => (durations.iter().map(|&d| u64::from(d)).sum(), "note-based"),
                None => (0, "unknown simulated duration request"),
            },
        );
    }
    Some((
        frames * u64::from(hydrus_media::UGOIRA_DEFAULT_FRAME_DURATION_MS),
        "speculated",
    ))
}
