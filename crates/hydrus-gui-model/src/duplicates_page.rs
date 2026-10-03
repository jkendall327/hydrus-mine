//! A duplicates page's sidebar tabs besides filtering, as the reference's
//! `ClientGUISidebarDuplicates`: the preparation tab (`PreparationPanel`),
//! saying how far the similar files search has got at the search distance
//! and naming the tab after it, and its questions. Recorded by
//! `oracle/record_duplicates_preparation.py`.

use std::collections::BTreeMap;

use hydrus_core::numbers::{float_to_percentage, human_int, value_range};
use hydrus_store::duplicates::auto::{OperationMode, PairStatus, Rule};

/// The search distances with names (`hamming_string_lookup`), as the
/// distance button's menu offers them.
pub const DISTANCES: [(u32, &str); 4] = [
    (0, "exact match"),
    (2, "very similar"),
    (4, "similar"),
    (8, "speculative"),
];

/// The distance button's label: the distance's name, or "custom".
pub fn distance_label(distance: u32) -> &'static str {
    DISTANCES
        .iter()
        .find(|(d, _)| *d == distance)
        .map_or("custom", |(_, name)| name)
}

/// What the preparation tab says of the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preparation {
    /// "1,200 eligible files in the system."
    pub eligible: String,
    /// "Searched 6/10 files at this distance.", and the gauge's value and
    /// range.
    pub searched: String,
    pub gauge: (u64, u64),
    /// Whether there is work left for "work harder".
    pub can_start: bool,
    /// The tab's name: "preparation", or "preparation (60% done)".
    pub page_name: String,
}

/// The preparation tab, from how many files have been searched at each
/// distance (`None`: not yet) and the search distance
/// (`_InitialiseMaintenanceStatusUpdater`'s publisher); with `hide_caught_up`
/// (`hide_duplicates_needs_work_message_when_reasonably_caught_up`) the
/// tab's name drops its percentage once over 99% is done.
pub fn preparation(
    searched: &BTreeMap<Option<u32>, usize>,
    distance: u32,
    hide_caught_up: bool,
) -> Preparation {
    let total: usize = searched.values().sum();
    let done: usize = searched
        .iter()
        .filter(|(d, _)| d.is_some_and(|d| d >= distance))
        .map(|(_, n)| n)
        .sum();
    let eligible = format!("{} eligible files in the system.", human_int(total as u64));
    let (total, done) = (total as u64, done as u64);
    if done >= total {
        return Preparation {
            eligible,
            searched: "All potential duplicates found at this distance.".into(),
            // (an empty gauge's range is 1)
            gauge: (total, total.max(1)),
            can_start: false,
            page_name: "preparation".into(),
        };
    }
    let searched_text = if done == 0 {
        "Have not yet searched at this distance.".to_owned()
    } else {
        format!(
            "Searched {} files at this distance.",
            value_range(done, total)
        )
    };
    #[allow(clippy::cast_precision_loss)] // (file counts)
    let fraction = done as f64 / total as f64;
    let page_name = if hide_caught_up && fraction > 0.99 {
        "preparation".to_owned()
    } else {
        let mut percent = float_to_percentage(fraction);
        // (never "100.0%" while some are left)
        if percent == "100.0%" {
            percent = "99.9%".into();
        }
        format!("preparation ({percent} done)")
    };
    Preparation {
        eligible,
        searched: searched_text,
        gauge: (done, total),
        can_start: true,
        page_name,
    }
}

/// The preparation tab's cog menu's questions.
pub const RESET_QUESTION: &str = "ADVANCED TOOL: This will delete all the current potential duplicate pairs and queue every eligible file up for another re-search.\n\nThis can be useful if you know you have database damage and need to reset and re-search everything, or if you have accidentally searched too broadly and are now swamped with too many false positives. It is not useful for much else.";

/// The auto-resolution tab's rules list's column titles.
pub const RULE_COLUMNS: [&str; 3] = ["name", "progress", "status"];

/// A rule's progress (`GetSearchSummary`): what is left to search, test
/// and resolve, and what it resolved, failed, was denied and didn't match.
pub fn rule_progress(counts: &BTreeMap<PairStatus, u64>) -> String {
    let count = |s: PairStatus| counts.get(&s).copied().unwrap_or(0);
    if counts.values().sum::<u64>() == 0 {
        return "no pairs".into();
    }
    let not_searched = count(PairStatus::NotSearched);
    let not_tested = count(PairStatus::MatchesSearchNotTested);
    let ready = count(PairStatus::ReadyToAction);
    let mut text = String::new();
    if not_searched > 0 {
        text.push_str(&format!("{} to search, ", human_int(not_searched)));
    }
    if not_tested > 0 {
        text.push_str(&format!("{} still to test, ", human_int(not_tested)));
    }
    if ready > 0 {
        text.push_str(&format!("{} ready to resolve, ", human_int(ready)));
    }
    if not_searched + not_tested + ready == 0 {
        text.push_str("Done! ");
    }
    text.push_str(&format!(
        "{} pairs resolved",
        human_int(count(PairStatus::Actioned))
    ));
    for (status, what) in [
        (PairStatus::FailedTest, "failed the test"),
        (PairStatus::Denied, "denied by user"),
        (PairStatus::DoesNotMatchSearch, "did not match the search"),
    ] {
        let n = count(status);
        if n > 0 {
            text.push_str(&format!(" ({} {what})", human_int(n)));
        }
    }
    text
}

/// A rule's status (the manager's `GetRunningStatus`, which in hydrus-rs
/// can't see which rule `hydrus serve` is working on): paused; working,
/// or waiting when auto-resolution may not work now (`able_to_work`),
/// while it has pairs to search or test; queued while pairs wait on a
/// human; else done.
pub fn rule_status(rule: &Rule, counts: &BTreeMap<PairStatus, u64>, able_to_work: bool) -> String {
    let count = |s: PairStatus| counts.get(&s).copied().unwrap_or(0);
    let ready = count(PairStatus::ReadyToAction);
    let resolution_work = count(PairStatus::MatchesSearchNotTested) > 0
        && !(rule.mode == OperationMode::SemiAutomatic
            && rule
                .max_pending_pairs
                .is_some_and(|most| ready >= u64::from(most)));
    let search_work = count(PairStatus::NotSearched) > 0;
    if rule.paused {
        "paused"
    } else if search_work || resolution_work {
        if able_to_work { "working" } else { "waiting" }
    } else if ready > 0 {
        "queued"
    } else {
        "done"
    }
    .into()
}

/// What the auto-resolution tab's cog menu's resets ask, of `n` rules.
pub fn reset_question(which: Reset, n: usize) -> String {
    let n = human_int(n as u64);
    match which {
        Reset::Search => format!(
            "This will command the database to re-search these {n} rules. It will not undo any user-denied decisions. There is no point to running this unless you suspect a miscount or other sync bug."
        ),
        Reset::Test => format!(
            "This will command the database to re-test the pending/fails for these {n} rules. It will not undo any user-denied decisions. There is no point to running this unless you suspect a miscount or other sync bug."
        ),
        Reset::Denied => format!(
            "This will command the database to repeal all user-denied decisions for these {n} rules.\n\nDo this only if the rules have tens of thousands of denied pairs and you do not want to undo them manually in the \"review actions\" window."
        ),
    }
}

/// The auto-resolution tab's cog menu's resets, of the selected rules (or
/// all, with none selected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reset {
    Search,
    Test,
    Denied,
}
