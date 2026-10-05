//! The "review actions" window for a duplicates auto-resolution rule (the
//! reference's `ReviewActionsPanel`, opened by the "edit rules" dialog's
//! "review actions"): its tabs of pairs waiting for approval, actions taken
//! and pairs denied, what each says and asks.

use hydrus_core::DuplicateType;
use hydrus_core::TimestampMs;
use hydrus_core::time::timestamp_to_pretty_time_delta;
use hydrus_store::duplicates::auto::{OperationMode, Rule};

use crate::auto_resolution_rules::action_text;

pub const TITLE: &str = "review duplicate auto-resolution actions";

/// The tabs' names.
pub const TABS: [&str; 3] = ["pending actions", "actions taken", "actions denied"];

/// The tabs' boxes' titles.
pub const BOXES: [&str; 3] = ["pending actions", "actioned pairs", "denied pairs"];

/// Each tab's refresh button's tooltip.
pub const REFRESH_TOOLTIPS: [&str; 3] = [
    "Refresh the pending pairs",
    "Refresh the actioned pairs",
    "Refresh the denied pairs",
];

/// Each tab's columns.
pub const COLUMNS: [[&str; 3]; 3] = [
    ["A", "B", "action"],
    ["A", "B", "action"],
    ["1", "2", "time"],
];

/// How many pairs each tab fetches unless told otherwise; "fetch all" is
/// no limit.
pub const DEFAULT_FETCH: usize = 250;
pub const FETCH_LABEL: &str = "only sample this many: ";
pub const FETCH_ALL: &str = "fetch all";

pub const FETCHING: &str = "fetching and calculating pairs\u{2026}";
pub const FULLY_AUTOMATIC: &str =
    "This rule is fully automatic; it will not wait for human approval.";

pub const NOTHING_LOCAL_IN_FILTER: &str = "Sorry, but it seems there is nothing to show! Every pair I saw had at least one non-local file. Try refreshing the panel!";
pub const NOTHING_LOCAL_IN_VIEWER: &str = "Sorry, but neither of those files is local (they were probably deleted), so they cannot be displayed in the media viewer!";

/// The tab the window opens on: pending actions for a rule that waits for
/// approval, else actions taken.
pub fn start_tab(rule: &Rule) -> usize {
    match rule.mode {
        OperationMode::SemiAutomatic => 0,
        OperationMode::FullyAutomatic => 1,
    }
}

fn human_int(n: usize) -> String {
    hydrus_core::numbers::human_int(n as u64)
}

/// A tab's label once its pairs are fetched.
pub fn found(n: usize) -> String {
    format!("Found {} pairs.", human_int(n))
}

/// The pending tab's label after approving or denying some.
pub fn remaining(n: usize) -> String {
    format!("{} pairs remaining.", human_int(n))
}

/// Approving or denying more than five pairs asks first.
pub fn approve_question(n: usize) -> Option<String> {
    (n > 5).then(|| {
        format!(
            "Are you sure you want to approve the {} pairs?",
            human_int(n)
        )
    })
}

pub fn deny_question(n: usize) -> Option<String> {
    (n > 5).then(|| format!("Are you sure you want to deny the {} pairs?", human_int(n)))
}

/// What "approve" or "deny" says while it works.
pub fn working(approving: bool, done: usize, total: usize) -> String {
    format!(
        "{}: {}/{}",
        if approving { "approving" } else { "denying" },
        human_int(done),
        human_int(total)
    )
}

/// "undo" on actions taken, covering `files` distinct files.
pub fn undo_actioned_question(files: usize) -> String {
    format!(
        "Are you sure you want to undo the auto-resolution actions covering these {} files? This is a serious action and will reset all the duplicate file relationships these files have.\n\nThe only way to do this reliably is to completely dissolve the respective duplicate group(s), which may undo many other decisions. All the files in the duplicate group(s) (not just what you selected) will be queued up for search in the potential duplicates system once more. Any files that are in trash will be undeleted. This action will not remove the entries from this audit log nor undo any content merge.",
        human_int(files)
    )
}

/// "undo" on actions denied.
pub fn undo_denied_question(pairs: usize) -> String {
    format!(
        "Are you sure you want to undo your deny decisions for these {} pairs? They will be queued up for re-search.",
        human_int(pairs)
    )
}

/// An action's words, by the relationship it set
/// (`duplicate_type_auto_resolution_action_description_lookup`).
pub fn action_description(duplicate_type: DuplicateType) -> &'static str {
    match duplicate_type {
        DuplicateType::FalsePositive => "set as not related/false positive",
        DuplicateType::SameQuality => "set as same quality",
        DuplicateType::Alternate => "set as alternates",
        DuplicateType::Worse => "set as duplicates--B better",
        _ => "set as duplicates--A better",
    }
}

/// When, as the lists say it: the time, then how long ago, a line each.
fn when(timestamp_ms: i64, now: i64) -> String {
    let seconds = timestamp_ms.div_euclid(1000);
    format!(
        "{}\n{}",
        hydrus_import::status::pretty_time(TimestampMs(timestamp_ms)),
        timestamp_to_pretty_time_delta(seconds, now, " ago")
    )
}

/// The pending tab's "action" cell (`GetActionSummaryOnMatchingPair`,
/// not either way round): what the rule does, then what it would change
/// ([`merge_summary`](crate::merge_summary)), or that it couldn't say.
pub fn pending_cell(rule: &Rule, merge_summary: Option<&str>) -> String {
    format!(
        "{}\n{}",
        action_text(rule.action),
        merge_summary.unwrap_or(COULD_NOT_SUMMARISE)
    )
}

pub const COULD_NOT_SUMMARISE: &str =
    "Could not summarise the duplicate merge! Please tell hydrus dev.";

/// What approving `a` and `b` under `rule` would change, summarised.
pub fn pending_summary(
    store: &hydrus_store::Store,
    rule: &Rule,
    a: hydrus_core::HashId,
    b: hydrus_core::HashId,
) -> hydrus_store::Result<String> {
    let snapshot = store.snapshot();
    let rule = rule.clone();
    store.read(move |conn| {
        let changes = hydrus_duplicates::engine::planned_changes(conn, &snapshot, &rule, a, b)?;
        crate::merge_summary::summary(conn, &snapshot, &changes, a, b)
    })
}

/// The actions taken tab's "action" cell.
pub fn actioned_cell(duplicate_type: DuplicateType, timestamp_ms: i64, now: i64) -> String {
    format!(
        "{}\n{}",
        action_description(duplicate_type),
        when(timestamp_ms, now)
    )
}

/// The actions denied tab's "time" cell.
pub fn denied_cell(timestamp_ms: i64, now: i64) -> String {
    when(timestamp_ms, now)
}

/// The lists' right-click entry, for `n` selected rows.
pub fn show_in_page_label(n: usize) -> String {
    if n == 1 {
        "show selected row in a new page".to_owned()
    } else {
        format!("show {} rows in a new page", human_int(n))
    }
}

/// Where "show in a new page" shows pairs' files
/// (`ShowMediaResultsInNewPageWithAppropriateLocationContext`): all known
/// files if any isn't stored, all of local storage if any is in the trash,
/// else all local files.
pub fn show_location(
    store: &hydrus_store::Store,
    files: &[hydrus_core::HashId],
) -> hydrus_store::Result<hydrus_search::LocationContext> {
    use hydrus_core::service::builtin_keys::{
        COMBINED_FILE, COMBINED_LOCAL_FILE_DOMAINS, HYDRUS_LOCAL_FILE_STORAGE, TRASH,
    };
    let snapshot = store.snapshot();
    let (storage, trash) = (
        snapshot.services.builtin(HYDRUS_LOCAL_FILE_STORAGE)?.id,
        snapshot.services.builtin(TRASH)?.id,
    );
    let (stored, trashed) = store.read(|c| {
        Ok((
            hydrus_store::media::current_in(c, storage, files)?,
            hydrus_store::media::current_in(c, trash, files)?,
        ))
    })?;
    let key = if files.iter().any(|f| !stored.contains(f)) {
        COMBINED_FILE
    } else if !trashed.is_empty() {
        HYDRUS_LOCAL_FILE_STORAGE
    } else {
        COMBINED_LOCAL_FILE_DOMAINS
    };
    Ok(hydrus_search::LocationContext::single(
        hydrus_core::ServiceKey::new(key.to_vec()),
    ))
}

/// After approving or denying, the row selected: the earliest of those
/// taken away, or the last left. `None` when none are left.
pub fn reselect(earliest: usize, left: usize) -> Option<usize> {
    (left > 0).then(|| earliest.min(left - 1))
}

/// How long approving or denying works before its popup shows
/// (`CallLater( 4, ... )`).
pub const POPUP_AFTER: std::time::Duration = std::time::Duration::from_secs(4);

/// The popup's first text (`approving auto-resolution decisions`).
pub fn action_title(approve: bool) -> &'static str {
    if approve {
        "approving auto-resolution decisions"
    } else {
        "denying auto-resolution decisions"
    }
}

/// The progress shown on the button and popup before each chunk of four
/// (`ActionAutoResolutionReviewPairs`): "approving: 4/12".
pub fn action_progress(approve: bool, done: usize, total: usize) -> String {
    format!(
        "{}: {}",
        if approve { "approving" } else { "denying" },
        hydrus_core::numbers::value_range(done as u64, total as u64)
    )
}

/// Approve or deny `pairs` four at a time, as the reference's worker does:
/// `status` takes each chunk's progress, and once `POPUP_AFTER` has passed
/// a popup shows it, finished and dismissed when done.
pub fn action_pairs(
    store: &hydrus_store::Store,
    rule_id: i64,
    pairs: &[(hydrus_core::HashId, hydrus_core::HashId)],
    approve: bool,
    status: &std::sync::Mutex<String>,
) -> hydrus_store::Result<()> {
    use hydrus_store::popups;
    let started = std::time::Instant::now();
    let now = || hydrus_core::TimestampMs::now().millis() / 1000;
    let mut popup: Option<[u8; 32]> = None;
    let mut result = Ok(());
    for (i, chunk) in pairs.chunks(4).enumerate() {
        let text = action_progress(approve, i * 4, pairs.len());
        text.clone_into(
            &mut status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        if popup.is_none() && started.elapsed() >= POPUP_AFTER {
            #[allow(clippy::cast_precision_loss)] // (seconds)
            let job = popups::Job::text(action_title(approve), now() as f64);
            popup = Some(job.key);
            let at = now();
            store.write(move |ctx| popups::add(ctx.conn(), &job, at))?;
        }
        if let Some(key) = popup {
            let at = now();
            store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| job.status_text_1 = Some(text))
                    .map(|_| ())
            })?;
        }
        let done = if approve {
            hydrus_duplicates::engine::approve(store, rule_id, chunk)
        } else {
            hydrus_duplicates::engine::deny(store, rule_id, chunk)
        };
        if let Err(e) = done {
            result = Err(e);
            break;
        }
    }
    if let Some(key) = popup {
        let at = now();
        store.write(move |ctx| {
            popups::update(ctx.conn(), &key, at, |job| job.finish_and_dismiss(None, at)).map(|_| ())
        })?;
    }
    result
}
