//! A page's status bar text, as the reference writes it
//! (`MediaResultsPanel._GetPrettyStatusForStatusBar`): how many files and
//! of what type (`GetMediasFiletypeSummaryString`: "14 jpegs", "27
//! images", "36 files"), and their total size and duration; with files
//! selected, the same of them and how many are in the inbox, or for one
//! file its interesting info lines. An empty page says why instead. Plain
//! Rust, tested against the reference.

use std::collections::BTreeSet;

use hydrus_core::numbers::human_int;
use hydrus_core::time::duration_ms_to_pretty;
use hydrus_core::{HashId, Mime};
use hydrus_media::mimes;
use hydrus_store::Store;
use hydrus_store::settings::GuiFormatting;

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

/// How many items some files are shown as, and how many of those are
/// collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Items {
    pub items: usize,
    pub collections: usize,
}

impl Items {
    /// Each file shown alone.
    pub fn files(count: usize) -> Self {
        Self {
            items: count,
            collections: 0,
        }
    }
}

/// `GetMediasFiletypeSummaryString`: "1 png", "14 jpegs", "3 animations",
/// "36 files", "12 files in 3 collections" (of `files`, shown as `items`).
pub fn filetype_summary(files: &[Facts], items: Items) -> String {
    let count = files.len();
    let summary = if count > 100_000 {
        "files".to_owned()
    } else if items.collections > 0 {
        let suffix = if items.items > 1 || count > 1 {
            "s"
        } else {
            ""
        };
        if items.collections == items.items {
            let collections = if items.collections > 1 { "s" } else { "" };
            format!(
                "file{suffix} in {} collection{collections}",
                human_int(items.collections as u64)
            )
        } else {
            // (a collection's type is none of the others)
            format!("file{suffix}")
        }
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
pub fn total_size(files: &[Facts]) -> String {
    total_size_with_format(files, &GuiFormatting::default())
}

pub fn total_size_with_format(files: &[Facts], formatting: &GuiFormatting) -> String {
    let total: u64 = files.iter().filter_map(|f| f.size).sum();
    let unknown = files.iter().any(|f| f.size.is_none());
    match (total, unknown) {
        (0, true) => "unknown size".to_owned(),
        (total, true) => format!(
            "{} + some unknown size",
            crate::gui_format::bytes(formatting, total)
        ),
        (total, false) => crate::gui_format::bytes(formatting, total),
    }
}

/// `_GetPrettyTotalDuration`: nothing unless every file has a duration.
pub fn total_duration(files: &[Facts]) -> Option<String> {
    let durations: Option<Vec<u64>> = files
        .iter()
        .map(|f| f.duration_ms.filter(|&d| d > 0))
        .collect();
    let total: u64 = durations.filter(|d| !d.is_empty())?.iter().sum();
    Some(duration_ms_to_pretty(total))
}

/// The status bar's text for a page of `files` (shown as `items`) with
/// `selected` (as `selected_items`) selected. An empty page says `empty`
/// if it has something to say; one file selected is described by
/// `single_line` (its interesting info lines joined with ", ", if the
/// options show them).
pub fn status(
    (files, items): (&[Facts], Items),
    (selected, selected_items): (&[Facts], Items),
    empty: Option<&str>,
    single_line: Option<&str>,
) -> String {
    status_with_format(
        (files, items),
        (selected, selected_items),
        empty,
        single_line,
        &GuiFormatting::default(),
    )
}

pub fn status_with_format(
    (files, items): (&[Facts], Items),
    (selected, selected_items): (&[Facts], Items),
    empty: Option<&str>,
    single_line: Option<&str>,
    formatting: &GuiFormatting,
) -> String {
    if files.is_empty()
        && let Some(empty) = empty
    {
        return empty.to_owned();
    }
    let mut s = filetype_summary(files, items);
    if selected.is_empty() {
        if !files.is_empty() {
            s += &format!(" - totalling {}", total_size_with_format(files, formatting));
            if let Some(duration) = total_duration(files) {
                s += &format!(", {duration}");
            }
        }
        return s;
    }
    s += &format!(
        " - {} selected, ",
        filetype_summary(selected, selected_items)
    );
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
    s += &format!(
        "{phrase}, totalling {}",
        total_size_with_format(selected, formatting)
    );
    if let Some(duration) = total_duration(selected) {
        s += &format!(", {duration}");
    }
    s
}

/// The main window's status bar's network part, as the reference's
/// `REPEATINGBandwidth` writes it: the data read since the client opened,
/// what it is reading a second if anything, and whether subscriptions or
/// all new network traffic are paused ("12.3 MB (45 KB/s), subs paused").
pub fn bandwidth_status(
    read: u64,
    per_second: u64,
    pauses: &hydrus_store::settings::Pauses,
) -> String {
    bandwidth_status_with_format(read, per_second, pauses, &GuiFormatting::default())
}

pub fn bandwidth_status_with_format(
    read: u64,
    per_second: u64,
    pauses: &hydrus_store::settings::Pauses,
    formatting: &GuiFormatting,
) -> String {
    let mut status = crate::gui_format::bytes(formatting, read);
    if per_second > 0 {
        status.push_str(&format!(
            " ({}/s)",
            crate::gui_format::bytes(formatting, per_second)
        ));
    }
    if pauses.subscriptions {
        status.push_str(", subs paused");
    }
    if pauses.network_traffic {
        status.push_str(", network paused");
    }
    status
}

/// What the daemon has read since the client opened, from what it says
/// of its network engine now and then (`DaemonLive`): a daemon started
/// again counts from nothing.
#[derive(Debug, Clone, Default)]
pub struct SessionBytes {
    last: Option<(i64, u64)>,
    read: u64,
}

impl SessionBytes {
    /// Take what the daemon says (if it has said anything), at `now`:
    /// what has been read since the client opened, and a second's worth
    /// now (none if the daemon hasn't said so lately).
    pub fn read(&mut self, live: Option<hydrus_store::live::DaemonLive>, now: i64) -> (u64, u64) {
        let Some(live) = live else {
            return (self.read, 0);
        };
        match self.last {
            Some((started, bytes)) if started == live.started => {
                self.read += live.bytes.saturating_sub(bytes);
            }
            // (a daemon started since: all it has read is new)
            Some(_) => self.read += live.bytes,
            // (what was read before the client opened isn't counted)
            None => {}
        }
        self.last = Some((live.started, live.bytes));
        let per_second = if now - live.at <= 10 { live.speed } else { 0 };
        (self.read, per_second)
    }
}

/// The tooltips of the status bar's idle and CPU-busy fields
/// (`_RefreshStatusBar`); none while the field is empty.
pub fn activity_tooltips(idle: bool, cpu_busy: bool) -> (&'static str, &'static str) {
    (
        if idle {
            "client is idle, it can do maintenance work"
        } else {
            ""
        },
        if cpu_busy {
            "this computer has been doing work recently, so some hydrus maintenance will not start"
        } else {
            ""
        },
    )
}

/// The status bar's idle and CPU-busy fields (`_RefreshStatusBar`).
pub fn activity(idle: bool, cpu_busy: bool) -> (&'static str, &'static str) {
    (
        if idle { "idle" } else { "" },
        if cpu_busy { "CPU busy" } else { "" },
    )
}

#[cfg(test)]
mod bandwidth_tests {
    use hydrus_store::live::DaemonLive;
    use hydrus_store::settings::Pauses;

    use super::*;

    #[test]
    fn the_network_part_reads_as_the_reference_s() {
        let mut pauses = Pauses::default();
        assert_eq!(bandwidth_status(0, 0, &pauses), "0B");
        pauses.network_traffic = true;
        // (as the reference's own client showed it, booted paused)
        assert_eq!(bandwidth_status(0, 0, &pauses), "0B, network paused");
        pauses.subscriptions = true;
        assert_eq!(
            bandwidth_status(12_900_000, 46_080, &pauses),
            "12.3 MB (45 KB/s), subs paused, network paused"
        );
    }

    #[test]
    fn only_what_is_read_while_the_client_is_open_counts() {
        let live = |started, bytes, speed, at| {
            Some(DaemonLive {
                started,
                bytes,
                speed,
                at,
            })
        };
        let mut session = SessionBytes::default();
        assert_eq!(session.read(None, 100), (0, 0));
        // the daemon had read 5000 before the client opened
        assert_eq!(session.read(live(10, 5000, 300, 100), 100), (0, 300));
        assert_eq!(session.read(live(10, 7000, 0, 101), 101), (2000, 0));
        // a second's worth said long ago isn't now
        assert_eq!(session.read(live(10, 7000, 900, 101), 200), (2000, 0));
        // a daemon started again: all it reads counts
        assert_eq!(session.read(live(150, 400, 0, 160), 160), (2400, 0));
        assert_eq!(session.read(live(150, 1000, 0, 161), 161), (3000, 0));
        assert_eq!(session.read(None, 162), (3000, 0));
    }
}
