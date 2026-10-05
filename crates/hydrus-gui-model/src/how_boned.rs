//! Database > how boned am I? (`ReviewHowBonedAmI`): a search's file, view
//! and duplicate statistics, worded as the reference's panel words them,
//! with Mr. Bones's special messages.

use hydrus_core::numbers::{human_bytes, human_int};
use hydrus_core::service::builtin_keys;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, LocationContext, Predicate, SystemPredicate,
    search_files_without_implicit_limit,
};
use hydrus_store::Store;
use hydrus_store::error::StoreError;
use hydrus_store::stats::{self, BonedStats, FileSet};

/// The statistics of a search, as the reference's `boned_stats` reads them:
/// a single plain local domain also counts the files deleted from it.
pub fn boned(store: &Store, search: &FileSearchContext) -> hydrus_store::Result<BonedStats> {
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let everything = search.predicates.is_empty()
        || search.predicates == [Predicate::System(SystemPredicate::Everything)];
    let find = |conn: &rusqlite::Connection, search: &FileSearchContext| {
        search_files_without_implicit_limit(
            conn,
            &snapshot,
            search,
            FileSort::default(),
            &Clock::system(),
        )
        .map(FileSet::Files)
        .map_err(|e| StoreError::Invalid(e.to_string()))
    };
    let location = &search.location;
    let single = match (
        location.current().iter().collect::<Vec<_>>().as_slice(),
        location.deleted().iter().next(),
    ) {
        ([key], None)
            if key.as_bytes() != builtin_keys::TRASH
                && key.as_bytes() != builtin_keys::COMBINED_FILE =>
        {
            Some((*key).clone())
        }
        _ => None,
    };
    store.read(|conn| {
        let current = if everything {
            let mut domains = Vec::new();
            for key in location.current() {
                domains.push((services.by_key(key)?.id, false));
            }
            for key in location.deleted() {
                domains.push((services.by_key(key)?.id, true));
            }
            FileSet::Domains(domains)
        } else {
            find(conn, search)?
        };
        let (times, deleted) = match &single {
            None => (None, None),
            Some(key) => {
                let service = services.by_key(key)?.id;
                let deleted = if everything {
                    FileSet::Domains(vec![(service, true)])
                } else {
                    let in_deleted = FileSearchContext {
                        location: LocationContext::new(Vec::new(), [key.clone()]),
                        ..search.clone()
                    };
                    find(conn, &in_deleted)?
                };
                (Some(service), Some(deleted))
            }
        };
        stats::boned_stats(conn, &current, times, deleted.as_ref(), times)
    })
}

/// `ClientData.ConvertZoomToPercentage`: two decimal places, none for a
/// whole percentage.
pub fn percentage(fraction: f64) -> String {
    let text = format!("{:.2}%", fraction * 100.0);
    if text.ends_with("00%") {
        format!("{:.0}%", fraction * 100.0)
    } else {
        text
    }
}

/// The files tab: its table's rows (label, files, %, size, %, average; an
/// empty row is a gap), or "No files!", and the earliest import line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesTab {
    pub rows: Vec<[String; 6]>,
    pub earliest: Option<String>,
}

#[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)] // (non-negative counts, ratios)
fn row(
    label: &str,
    num: i64,
    num_percent: Option<f64>,
    size: i64,
    size_percent: Option<f64>,
) -> [String; 6] {
    let average = if num > 0 { size / num } else { 0 };
    [
        label.to_owned(),
        human_int(num as u64),
        num_percent.map(percentage).unwrap_or_default(),
        human_bytes(size as u64),
        size_percent.map(percentage).unwrap_or_default(),
        human_bytes(average as u64),
    ]
}

/// The files tab at `now` (seconds), in the local `offset`.
#[allow(clippy::cast_precision_loss)] // (ratios)
pub fn files_tab(stats: &BonedStats, now: i64, offset: jiff::tz::Offset) -> FilesTab {
    let num_total = stats.num_inbox + stats.num_archive;
    let size_total = stats.size_inbox + stats.size_archive;
    if num_total == 0 || size_total == 0 {
        return FilesTab {
            rows: Vec::new(),
            earliest: None,
        };
    }
    let fraction = |a: i64, b: i64| a as f64 / b as f64;
    let header = ["", "Files", "%", "Size", "%", "Average"].map(str::to_owned);
    let gap = || std::array::from_fn(|_| String::new());
    let inbox = row(
        "Inbox:",
        stats.num_inbox,
        Some(fraction(stats.num_inbox, num_total)),
        stats.size_inbox,
        Some(fraction(stats.size_inbox, size_total)),
    );
    let archive = row(
        "Archive:",
        stats.num_archive,
        Some(fraction(stats.num_archive, num_total)),
        stats.size_archive,
        Some(fraction(stats.size_archive, size_total)),
    );
    let rows = match stats.deleted {
        Some((num_deleted, size_deleted)) => {
            let (num_super, size_super) = (num_total + num_deleted, size_total + size_deleted);
            vec![
                header,
                row("Total Ever Imported:", num_super, None, size_super, None),
                gap(),
                row(
                    "Current:",
                    num_total,
                    Some(fraction(num_total, num_super)),
                    size_total,
                    Some(fraction(size_total, size_super)),
                ),
                row(
                    "Deleted:",
                    num_deleted,
                    Some(fraction(num_deleted, num_super)),
                    size_deleted,
                    Some(fraction(size_deleted, size_super)),
                ),
                gap(),
                inbox,
                archive,
            ]
        }
        None => vec![
            header,
            row("Current:", num_total, None, size_total, None),
            gap(),
            inbox,
            archive,
        ],
    };
    let earliest = stats.earliest_import_ms.map(|ms| {
        let seconds = ms.div_euclid(1000);
        format!(
            "Earliest file import: {} ({})",
            hydrus_core::time::timestamp_to_iso_with_offset(Some(seconds), offset),
            hydrus_core::time::timestamp_to_pretty_time_delta(seconds, now, " ago")
        )
    });
    FilesTab { rows, earliest }
}

/// The views tab's two lines.
#[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)] // (non-negative totals)
pub fn views_tab(stats: &BonedStats) -> [String; 2] {
    let line = |what: &str, views: i64, ms: i64| {
        format!(
            "Total {what} views: {}, totalling {}",
            human_int(views as u64),
            hydrus_core::time::pretty_time_delta_f64(ms as f64 / 1000.0)
        )
    };
    [
        line("media", stats.media_views, stats.media_viewtime_ms),
        line("preview", stats.preview_views, stats.preview_viewtime_ms),
    ]
}

/// The duplicates tab's note and two lines.
pub const DUPLICATES_NOTE: &str = "Since many duplicate files get deleted, this will not give nice \"all-time\" numbers unless you change the file domain to \"all files ever imported or deleted\".";

#[allow(clippy::cast_sign_loss)] // (non-negative counts)
pub fn duplicates_tab(stats: &BonedStats) -> [String; 2] {
    [
        format!(
            "Total files in duplicate groups: {}",
            human_int(stats.duplicate_files as u64)
        ),
        format!(
            "Total files in alternate groups: {} ({} groups)",
            human_int(stats.alternate_files as u64),
            human_int(stats.alternate_groups as u64)
        ),
    ]
}

/// Mr. Bones's words in place of his picture, for the plain search of
/// combined local file domains (`_SetTheMasterOfCeremonies`).
pub fn special_message(stats: &BonedStats, search: &FileSearchContext) -> Option<&'static str> {
    let plain = search.predicates.is_empty()
        && search.location
            == LocationContext::new(
                [hydrus_core::ServiceKey::new(
                    builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
                )],
                [],
            );
    if !plain {
        return None;
    }
    let num_total = stats.num_inbox + stats.num_archive;
    let num_deleted = stats.deleted.map_or(0, |d| d.0);
    if num_total == 0 && num_deleted == 0 {
        Some("You have yet to board the ride.")
    } else if num_total + num_deleted < 1000 {
        Some(
            "I hope you enjoy my software. You might like to think about workflows to import your backlog and new files. :^)",
        )
    } else if stats.num_inbox * 99 <= stats.num_archive {
        Some("CONGRATULATIONS. YOU APPEAR TO BE UNBONED--BUT REMAIN EVER VIGILANT")
    } else {
        None
    }
}
