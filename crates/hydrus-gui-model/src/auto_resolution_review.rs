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

/// The pending tab's "action" cell: what the rule would do. (The
/// reference adds the content merge's summary below.)
pub fn pending_cell(rule: &Rule) -> String {
    action_text(rule.action).to_owned()
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

/// After approving or denying, the row selected: the earliest of those
/// taken away, or the last left. `None` when none are left.
pub fn reselect(earliest: usize, left: usize) -> Option<usize> {
    (left > 0).then(|| earliest.min(left - 1))
}
