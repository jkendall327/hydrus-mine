//! A page's status bar text, as the reference writes it
//! (`MediaResultsPanel._GetPrettyStatusForStatusBar`): how many files and
//! of what type (`GetMediasFiletypeSummaryString`: "14 jpegs", "27
//! images", "36 files"), and their total size and duration; with files
//! selected, the same of them and how many are in the inbox, or for one
//! file its interesting info lines. An empty page says why instead. Plain
//! Rust, tested against the reference.

use std::collections::BTreeSet;

use hydrus_core::numbers::{human_bytes, human_int};
use hydrus_core::time::duration_ms_to_pretty;
use hydrus_core::{HashId, Mime};
use hydrus_media::mimes;
use hydrus_store::Store;

/// What the status bar knows of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub mime: Mime,
    /// `None` if not known.
    pub size: Option<u64>,
    pub duration_ms: Option<u64>,
    pub inbox: bool,
}

/// The facts of `files`, each with its file (those the store doesn't
/// know left out).
pub fn facts(store: &Store, files: &[HashId]) -> Vec<(HashId, Facts)> {
    let read = store.read(|conn| {
        let basic = hydrus_store::media::load_basic(conn, files)?;
        let inbox = hydrus_store::media::inboxed(conn, files)?;
        Ok((basic, inbox))
    });
    let Ok((basic, inbox)) = read else {
        return Vec::new();
    };
    basic
        .iter()
        .map(|m| {
            let facts = Facts {
                mime: m.info.as_ref().map_or(Mime::ApplicationUnknown, |i| i.mime),
                size: m.info.as_ref().map(|i| i.size),
                duration_ms: m.info.as_ref().and_then(|i| i.duration_ms),
                inbox: inbox.contains(&m.hash_id),
            };
            (m.hash_id, facts)
        })
        .collect()
}

/// `GetMediasFiletypeSummaryString`: "1 png", "14 jpegs", "3 animations",
/// "36 files".
pub fn filetype_summary(files: &[Facts]) -> String {
    let count = files.len();
    let summary = if count > 100_000 {
        "files".to_owned()
    } else {
        let suffix = if count > 1 { "s" } else { "" };
        let mimes: BTreeSet<Mime> = files.iter().map(|f| f.mime).collect();
        let all = |class: fn(Mime) -> bool| mimes.iter().all(|&m| class(m));
        match mimes.iter().next() {
            None => format!("file{suffix}"),
            Some(mime) if mimes.len() == 1 => format!("{}{suffix}", mime.human_name()),
            _ if all(mimes::is_image) => format!("image{suffix}"),
            _ if all(mimes::is_animation) => format!("animation{suffix}"),
            _ if all(mimes::is_video) => format!("video{suffix}"),
            _ if all(mimes::is_audio) => format!("audio file{suffix}"),
            _ => format!("file{suffix}"),
        }
    };
    format!("{} {summary}", human_int(count as u64))
}

/// `_GetPrettyTotalSize`.
pub(crate) fn total_size(files: &[Facts]) -> String {
    let total: u64 = files.iter().filter_map(|f| f.size).sum();
    let unknown = files.iter().any(|f| f.size.is_none());
    match (total, unknown) {
        (0, true) => "unknown size".to_owned(),
        (total, true) => format!("{} + some unknown size", human_bytes(total)),
        (total, false) => human_bytes(total),
    }
}

/// `_GetPrettyTotalDuration`: nothing unless every file has a duration.
pub(crate) fn total_duration(files: &[Facts]) -> Option<String> {
    let durations: Option<Vec<u64>> = files
        .iter()
        .map(|f| f.duration_ms.filter(|&d| d > 0))
        .collect();
    let total: u64 = durations.filter(|d| !d.is_empty())?.iter().sum();
    Some(duration_ms_to_pretty(total))
}

/// The status bar's text for a page of `files` with `selected` selected.
/// An empty page says `empty` if it has something to say; one file
/// selected is described by `single_line` (its interesting info lines
/// joined with ", ", if the options show them).
pub fn status(
    files: &[Facts],
    selected: &[Facts],
    empty: Option<&str>,
    single_line: Option<&str>,
) -> String {
    if files.is_empty()
        && let Some(empty) = empty
    {
        return empty.to_owned();
    }
    let mut s = filetype_summary(files);
    if selected.is_empty() {
        if !files.is_empty() {
            s += &format!(" - totalling {}", total_size(files));
            if let Some(duration) = total_duration(files) {
                s += &format!(", {duration}");
            }
        }
        return s;
    }
    s += &format!(" - {} selected, ", filetype_summary(selected));
    if let (1, Some(line)) = (selected.len(), single_line) {
        s += line;
        return s;
    }
    let (count, inbox) = (selected.len(), selected.iter().filter(|f| f.inbox).count());
    let phrase = if inbox == count {
        if inbox > 1 {
            "all in inbox"
        } else {
            "in inbox"
        }
        .to_owned()
    } else if inbox == 0 {
        if count > 1 {
            "all archived"
        } else {
            "archived"
        }
        .to_owned()
    } else {
        format!(
            "{} in inbox and {} archived",
            human_int(inbox as u64),
            human_int((count - inbox) as u64)
        )
    };
    s += &format!("{phrase}, totalling {}", total_size(selected));
    if let Some(duration) = total_duration(selected) {
        s += &format!(", {duration}");
    }
    s
}
