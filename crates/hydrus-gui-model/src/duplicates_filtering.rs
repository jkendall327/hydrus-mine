//! A duplicates page's filtering tab (`ClientGUISidebarDuplicates.FilterPanel`
//! and `EditPotentialDuplicatesSearchContextPanel`): the pair search's kind,
//! file searches, pixel-dupe preference and distance, the pair count, the
//! filter's pair sort and mixed/group mode, and "quick and dirty processing"
//! (a random potential group, and setting the shown files' relationship,
//! the media panel's `_SetDuplicates`).

use hydrus_core::HashId;
use hydrus_core::duplicates::{PairOrder, PairSearchKind, PixelDuplicates};
use hydrus_core::numbers::human_int;
use hydrus_core::pages::DuplicatesPage;
use hydrus_search::{Predicate, parse_api_search};
use hydrus_store::Store;
use hydrus_store::delete_lock::Reinbox;
use hydrus_store::duplicates::{DuplicateMergeSettings, PairDecision, PairRelationship};

/// The search kinds, in the reference's order.
pub const KINDS: [(PairSearchKind, &str); 3] = [
    (
        PairSearchKind::OneFileMatchesOneSearch,
        "at least one file matches the search",
    ),
    (
        PairSearchKind::BothFilesMatchOneSearch,
        "both files match the search",
    ),
    (
        PairSearchKind::BothFilesMatchDifferentSearches,
        "the two files match different searches",
    ),
];

/// The pixel-dupe preferences (`similar_files_pixel_dupes_string_lookup`).
pub const PIXEL: [(PixelDuplicates, &str); 3] = [
    (PixelDuplicates::Required, "must be pixel dupes"),
    (PixelDuplicates::Allowed, "can be pixel dupes"),
    (PixelDuplicates::Excluded, "must not be pixel dupes"),
];

/// The distance row's label.
pub const DISTANCE: &str = "maximum search distance of pair: ";

/// The pair sorts, in the sort button's order.
pub const SORTS: [(PairOrder, &str); 4] = [
    (PairOrder::MaxFilesize, "sort by: filesize of larger file"),
    (PairOrder::MinFilesize, "sort by: filesize of smaller file"),
    (
        PairOrder::Similarity,
        "sort by: similarity (distance/filesize ratio)",
    ),
    (PairOrder::Random, "sort by: random"),
];

/// The direction choices for a sort (none for random): label, ascending.
pub fn directions(order: PairOrder) -> &'static [(&'static str, bool)] {
    match order {
        PairOrder::MaxFilesize | PairOrder::MinFilesize => {
            &[("largest first", false), ("smallest first", true)]
        }
        PairOrder::Similarity => &[("most similar first", true), ("least similar first", false)],
        PairOrder::Random => &[],
    }
}

/// The mixed-pairs/group-mode choices.
pub const GROUP_MODES: [(&str, bool); 2] = [("mixed pairs", false), ("group mode", true)];

/// The "quick and dirty processing" buttons that set a relationship for
/// every file shown.
pub const SET_BUTTONS: [(&str, PairRelationship); 3] = [
    (
        "set current media as duplicates of the same quality",
        PairRelationship::SameQuality,
    ),
    (
        "set current media as all related alternates",
        PairRelationship::Alternate,
    ),
    (
        "set current media as not related/false positive",
        PairRelationship::FalsePositive,
    ),
];

/// A file search of the page (the second is used only for different searches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    First,
    Second,
}

fn search_mut(
    page: &mut DuplicatesPage,
    which: Which,
) -> &mut hydrus_core::search::context::FileSearchContext {
    match which {
        Which::First => &mut page.search.search_1,
        Which::Second => &mut page.search.search_2,
    }
}

/// Add typed predicates (a tag or system predicate) to one search.
///
/// # Errors
/// The text isn't a predicate.
pub fn add_predicates(page: &mut DuplicatesPage, which: Which, text: &str) -> Result<(), String> {
    let predicates = parse_api_search(&serde_json::json!([text])).map_err(|e| e.to_string())?;
    let search = search_mut(page, which);
    for predicate in predicates {
        if !search.predicates.contains(&predicate) {
            search.predicates.push(predicate);
        }
    }
    Ok(())
}

/// Remove one search's predicate.
pub fn remove_predicate(
    page: &mut DuplicatesPage,
    which: Which,
    index: usize,
) -> Option<Predicate> {
    let search = search_mut(page, which);
    (index < search.predicates.len()).then(|| search.predicates.remove(index))
}

/// Whether the second search shows (`_UpdateControls`).
pub fn second_search_shown(page: &DuplicatesPage) -> bool {
    page.search.kind == PairSearchKind::BothFilesMatchDifferentSearches
}

/// Whether the distance can be edited: not when pixel dupes are required.
pub fn distance_enabled(page: &DuplicatesPage) -> bool {
    page.search.pixel_duplicates != PixelDuplicates::Required
}

/// The pair count line (`_UpdateCountLabel`, for a finished count).
pub fn count_text(pairs_in_domain: usize, matching: usize) -> String {
    if pairs_in_domain == 0 {
        "no potential pairs in this file domain!".into()
    } else {
        format!(
            "{} pairs searched; {} match",
            human_int(pairs_in_domain as u64),
            human_int(matching as u64)
        )
    }
}

/// Count the pairs in the search's domain and those matching it.
pub fn count(store: &Store, page: &DuplicatesPage) -> hydrus_store::Result<(usize, usize)> {
    let snapshot = store.snapshot();
    let query =
        hydrus_duplicates::potentials::PotentialsQuery::from_search(&snapshot, &page.search)?;
    let mut everything = query.clone();
    everything.kind = PairSearchKind::OneFileMatchesOneSearch;
    everything.pixel_duplicates = PixelDuplicates::Allowed;
    everything.max_hamming_distance = u32::MAX;
    everything.search_1.predicates.clear();
    store.read(|conn| {
        let total = everything.count(conn, &snapshot)?.unwrap_or(0);
        let matching = query.count(conn, &snapshot)?.unwrap_or(0);
        Ok((total, matching))
    })
}

/// A random potential group in the search (`ShowRandomPotentialDupes`).
pub fn random_group(store: &Store, page: &DuplicatesPage) -> hydrus_store::Result<Vec<HashId>> {
    let snapshot = store.snapshot();
    let query =
        hydrus_duplicates::potentials::PotentialsQuery::from_search(&snapshot, &page.search)?;
    store.read(|conn| {
        Ok(query
            .with_search(conn, &snapshot, |search| {
                hydrus_store::duplicates::random_potential_group(conn, &snapshot, search)
            })?
            .unwrap_or_default())
    })
}

/// The pairs `_SetDuplicates` makes of `files`: every combination for false
/// positives and alternates, else the first file with each other.
pub fn pairs(files: &[HashId], relationship: PairRelationship) -> Vec<(HashId, HashId)> {
    let mut out = Vec::new();
    match relationship {
        PairRelationship::FalsePositive
        | PairRelationship::Alternate
        | PairRelationship::Potential => {
            for (i, a) in files.iter().enumerate() {
                for b in &files[i + 1..] {
                    out.push((*a, *b));
                }
            }
        }
        _ => {
            if let Some((first, rest)) = files.split_first() {
                out.extend(rest.iter().filter(|b| *b != first).map(|b| (*first, *b)));
            }
        }
    }
    out
}

/// The relationship's words (`duplicate_type_string_lookup`).
pub const fn relationship_text(relationship: PairRelationship) -> &'static str {
    match relationship {
        PairRelationship::FalsePositive => "not related/false positive",
        PairRelationship::Alternate => "alternates",
        PairRelationship::Better => "this is a better duplicate",
        PairRelationship::SameQuality => "same quality",
        PairRelationship::Potential => "potential duplicates",
    }
}

/// What `_SetDuplicates` does in words ("apply \"same quality\" (with default
/// duplicate metadata merge options)").
pub fn yes_no_text(relationship: PairRelationship, advanced: bool) -> String {
    if relationship == PairRelationship::Potential {
        return "queue all possible and valid pair combinations into the duplicate filter".into();
    }
    let mut text = format!("apply \"{}\"", relationship_text(relationship));
    if matches!(
        relationship,
        PairRelationship::Better | PairRelationship::SameQuality
    ) || (advanced && relationship == PairRelationship::Alternate)
    {
        text.push_str(" (with default duplicate metadata merge options)");
    }
    text
}

/// The question asked before setting `relationship` for `num_files` files
/// making `num_pairs` pairs: message, yes label and no label.
pub fn question(
    relationship: PairRelationship,
    advanced: bool,
    num_files: usize,
    num_pairs: usize,
) -> (String, &'static str, &'static str) {
    let files = human_int(num_files as u64);
    let what = yes_no_text(relationship, advanced);
    if num_pairs > 1
        && matches!(
            relationship,
            PairRelationship::FalsePositive | PairRelationship::Alternate
        )
    {
        let pairs = human_int(num_pairs as u64);
        if num_pairs > 100 {
            if relationship == PairRelationship::FalsePositive {
                return (
                    format!(
                        "False positive records are complicated, and setting that relationship for {files} files ({pairs} pairs) at once is likely a mistake.\n\nAre you sure all of these files are all potential duplicates and that they are all false positive matches with each other? If not, I recommend you step back for now."
                    ),
                    "I know what I am doing",
                    "step back for now",
                );
            }
            return (
                format!(
                    "Are you certain all these {files} files are alternates with every other member of the selection, and that none are duplicates?\n\nIf some of them may be duplicates, I recommend you either deselect the possible duplicates and try again, or just leave this group to be processed in the normal duplicate filter."
                ),
                "they are all alternates",
                "some may be duplicates",
            );
        }
        return (
            format!(
                "Are you sure you want to {what} for the {files} selected files? The relationship will be applied between every pair combination in the file selection ({pairs} pairs)."
            ),
            "yes",
            "no",
        );
    }
    (
        format!("Are you sure you want to {what} for the {files} selected files?"),
        "yes",
        "no",
    )
}

/// Set `relationship` between `files`' pairs, merging metadata with the
/// default merge options where the reference does.
pub fn set_duplicates(
    store: &Store,
    files: &[HashId],
    relationship: PairRelationship,
    advanced: bool,
) -> hydrus_store::Result<usize> {
    let pairs = pairs(files, relationship);
    let reason = format!(
        "Deleted from duplicate action on Media Page ({}).",
        yes_no_text(relationship, advanced)
    );
    let count = pairs.len();
    store.write_content(move |w| {
        let settings: DuplicateMergeSettings = hydrus_store::settings::get(w.conn())?;
        let merge = match relationship {
            PairRelationship::Alternate if !advanced => None,
            other => settings.for_relationship(other),
        };
        for (a, b) in &pairs {
            hydrus_store::duplicates::apply_decision(
                w,
                &PairDecision {
                    relationship,
                    a: *a,
                    b: *b,
                    merge,
                    delete_a: false,
                    delete_b: false,
                    deletion_reason: &reason,
                    // (only the merge path inboxes, as in the reference)
                    reinbox: if merge.is_some() {
                        Reinbox::AfterDuplicateFilter
                    } else {
                        Reinbox::Never
                    },
                },
            )?;
        }
        Ok(())
    })?;
    Ok(count)
}
