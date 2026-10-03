//! The duplicates auto-resolution rule editor's "preview" tab (the
//! reference's `PreviewPanel`): a sample of the pairs the rule's search
//! matches, tested, those that pass (with what would be done to them) and
//! those that fail, and what it says as it works.

use hydrus_core::numbers::human_int;
use hydrus_store::duplicates::auto::Rule;

/// The boxes' titles.
pub const PREVIEW_BOX: &str = "preview of rule work";
pub const SEARCH_BOX: &str = "search";
pub const PASS_BOX: &str = "pairs that will be actioned";
pub const FAIL_BOX: &str = "pairs that will be skipped";

pub const READY_TO_FETCH: &str = "ready to fetch pairs";
pub const READY_TO_TEST: &str = "ready to test new pairs";
pub const READY_TO_PREVIEW: &str = "ready to generate preview";
pub const FETCHING: &str = "fetching pairs\u{2026}";
pub const TESTING: &str = "testing pairs\u{2026}";
pub const NO_PAIRS: &str = "no potential pairs in this file domain!";
pub const FETCH_TOOLTIP: &str = "Fetch a sample of pairs";
pub const RETEST_TOOLTIP: &str = "Retest the fetched pairs";

/// When the rule being edited can't be had (it can't tell A from B,
/// say).
pub fn problem(error: &str) -> String {
    format!("Problem fetching the current rule! {error}")
}

/// The search's label, once done: the pairs in the domain, and how many
/// the search matched.
pub fn searched(searched: usize, matched: usize) -> String {
    if searched == 0 {
        return NO_PAIRS.to_owned();
    }
    format!(
        "{} pairs searched; {} matched",
        human_int(searched as u64),
        human_int(matched as u64)
    )
}

/// The pairs fetched but not yet tested.
pub fn still_to_test(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("{} pairs still to test", human_int(n as u64))
    }
}

/// A list's label: its pairs, or "None!".
pub fn listed(n: usize) -> String {
    if n == 0 {
        "None!".to_owned()
    } else {
        format!(
            "{} pairs - double-click to open a media viewer",
            human_int(n as u64)
        )
    }
}

/// A passing pair's "action" (`GetActionSummaryOnMatchingPair`, either way
/// round tested): which way round it passes, what the rule does, and
/// what it would change.
pub fn pass_cell(rule: &Rule, both_ways: bool, merge_summary: Option<&str>) -> String {
    format!(
        "{}\n{}",
        if both_ways {
            "either way around"
        } else {
            "this way around"
        },
        crate::auto_resolution_review::pending_cell(rule, merge_summary)
    )
}
