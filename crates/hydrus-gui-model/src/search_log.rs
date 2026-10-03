//! A gallery search's or watcher's search log window (the reference's
//! `EditGallerySeedLogPanel`; a watcher's is its "check log") and its
//! menus: each page's row, the right-click menu on selected rows, and the
//! whole log's menu (`PopulateGallerySeedLogButton`). Menus are the file
//! log's [`Entry`] trees. Recorded by `oracle/record_search_log.py`.

use hydrus_core::numbers::human_int;
use hydrus_core::time::timestamp_to_pretty_time_delta;
use hydrus_store::queues::{GallerySeed, SeedStatus, StatusCounts};

use crate::file_log::{Entry, human_url, status_text};

/// The search log's column titles.
pub const COLUMNS: [&str; 6] = ["#", "url", "status", "added", "last modified", "note"];

/// A page's row (`_ConvertGallerySeedToDisplayTuple`).
pub fn row(seed: &GallerySeed, index: usize, now: i64) -> Vec<String> {
    vec![
        human_int(index as u64),
        human_url(&seed.url),
        status_text(seed.status).to_owned(),
        timestamp_to_pretty_time_delta(seed.created, now, " ago"),
        timestamp_to_pretty_time_delta(seed.modified, now, " ago"),
        seed.note.lines().next().unwrap_or_default().to_owned(),
    ]
}

/// What a search log menu's item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Remove entries with this status, after [`delete_question`].
    DeleteStatus(SeedStatus),
    /// Read the last, failed page again and carry on from it.
    RestartFailedSearch,
    /// Every URL to the clipboard, a line each.
    ExportToClipboard,
    CopyUrls,
    CopyNotes,
    OpenUrls,
    /// The selected read again, a page each; `true` letting the search
    /// carry on to next pages.
    TryAgain(bool),
    Skip,
    /// Not in hydrus-rs yet (png, importing URLs, the advanced entries).
    NotYet,
}

/// What a log's menus need to know of it.
#[derive(Debug, Clone, Default)]
pub struct LogFacts {
    pub counts: StatusCounts,
    pub len: usize,
    /// The last page failed (so the search can be restarted).
    pub last_failed: bool,
    /// A subscription query's log, which can't be changed here.
    pub read_only: bool,
    /// A gallery search's (not a watcher's), whose pages lead on.
    pub can_generate_more_pages: bool,
}

fn item(label: impl Into<String>, action: Action) -> Entry<Action> {
    Entry::Item(label.into(), action)
}

/// The whole log's menu (`PopulateGallerySeedLogButton`).
pub fn log_menu(log: &LogFacts, any_selected: bool) -> Vec<Entry<Action>> {
    let count = |s: SeedStatus| log.counts.get(&s).copied().unwrap_or(0);
    let mut menu = Vec::new();
    for (status, what) in [
        (SeedStatus::SuccessfulAndNew, "successful"),
        (SeedStatus::Error, "failed"),
        (SeedStatus::Vetoed, "ignored"),
        (SeedStatus::Skipped, "skipped"),
    ] {
        let n = count(status);
        if n > 0 {
            menu.push(item(
                format!(
                    "delete {} '{what}' gallery log entries from the log",
                    human_int(n as u64)
                ),
                Action::DeleteStatus(status),
            ));
        }
    }
    menu.push(Entry::Separator);
    if log.len > 0 {
        if !log.read_only && log.last_failed {
            menu.push(item(
                "restart and resume failed search",
                Action::RestartFailedSearch,
            ));
            menu.push(Entry::Separator);
        }
        menu.push(Entry::Menu(
            "export all urls".into(),
            vec![
                item("to clipboard", Action::ExportToClipboard),
                item("to png", Action::NotYet),
            ],
        ));
    }
    if !log.read_only {
        menu.push(Entry::Menu(
            "ADVANCED: import new urls".into(),
            vec![
                item("from clipboard", Action::NotYet),
                item("from png", Action::NotYet),
            ],
        ));
    }
    if any_selected {
        menu.push(Entry::Menu(
            "advanced".into(),
            vec![item(
                "export selected page objects to clipboard",
                Action::NotYet,
            )],
        ));
    }
    crate::file_log::tidy(menu)
}

/// The right-click menu on the selected pages (`_GetListCtrlMenu`); with
/// none selected, the whole log's.
pub fn row_menu(selected: &[&GallerySeed], log: &LogFacts) -> Vec<Entry<Action>> {
    if selected.is_empty() {
        return log_menu(log, false);
    }
    let mut menu = Vec::new();
    if let [seed] = selected {
        menu.push(item("copy url", Action::CopyUrls));
        if !seed.note.is_empty() {
            menu.push(item("copy note", Action::CopyNotes));
        }
    } else {
        menu.push(item("copy urls", Action::CopyUrls));
        menu.push(item("copy notes", Action::CopyNotes));
    }
    menu.push(Entry::Separator);
    menu.push(item("open urls", Action::OpenUrls));
    if let [seed] = selected {
        menu.push(Entry::Separator);
        menu.push(match &seed.referral_url {
            Some(referral) => Entry::Menu(
                "additional urls".into(),
                vec![Entry::Label(format!("referral url: {referral}"))],
            ),
            None => Entry::Label("no additional urls".into()),
        });
        let headers: Vec<Entry<Action>> = seed
            .meta
            .request_headers
            .iter()
            .map(|(k, v)| Entry::Label(format!("{k}: {v}")))
            .collect();
        menu.push(if headers.is_empty() {
            Entry::Label("no additional headers".into())
        } else {
            Entry::Menu("additional headers".into(), headers)
        });
        let mut inherited: Vec<String> =
            seed.meta.external_filterable_tags.iter().cloned().collect();
        inherited.sort_by_key(|t| hydrus_core::sort::human_sort_key(t));
        menu.push(if inherited.is_empty() {
            Entry::Label("no inherited tags".into())
        } else {
            Entry::Menu(
                "inherited tags".into(),
                inherited.into_iter().map(Entry::Label).collect(),
            )
        });
    }
    menu.push(Entry::Separator);
    if !log.read_only {
        menu.push(item(
            "try again (just this one page)",
            Action::TryAgain(false),
        ));
        if log.can_generate_more_pages {
            menu.push(item(
                "try again (and allow search to continue)",
                Action::TryAgain(true),
            ));
        }
    }
    menu.push(item("skip", Action::Skip));
    menu.push(Entry::Separator);
    menu.push(Entry::Menu("whole log".into(), log_menu(log, true)));
    crate::file_log::tidy(menu)
}

/// What deleting a status's entries asks (`ClearGallerySeeds`), of a
/// "search" or "check" log.
pub fn delete_question(status: SeedStatus, log_kind: &str) -> String {
    let what = match status {
        SeedStatus::SuccessfulAndNew => "successful",
        SeedStatus::Error => "error",
        SeedStatus::Vetoed => "ignored",
        SeedStatus::Skipped => "skipped",
        _ => "unknown",
    };
    format!(
        "Are you sure you want to delete all the {what} {log_kind} log entries? This is useful for cleaning up and de-laggifying a very large list, but be careful you aren't removing something you would want to revisit."
    )
}
