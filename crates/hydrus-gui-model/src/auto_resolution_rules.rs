//! What the duplicates auto-resolution rule editor says of rules (the
//! reference's `EditDuplicatesAutoResolutionRulesPanel` and its rule
//! editor): each rule's row in the "edit rules" list, each comparator's
//! summary and whether it can tell A from B, and the editor's choices.
//! Recorded by `oracle/record_auto_resolution_summaries.py`.

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::search::comparable::Comparable;
use hydrus_core::search::context::FileSearchContext;
use hydrus_core::search::number::{NumberOp, NumberTest};
use hydrus_search::{TextContext, predicate_text};
use hydrus_store::duplicates::auto::{
    Comparator, LookingAt, OneFileTest, OperationMode, PairTest, Rule, RuleAction,
};

/// The "edit rules" list's column titles.
pub const COLUMNS: [&str; 6] = [
    "name",
    "search",
    "comparison",
    "action",
    "progress",
    "operation",
];

/// The operation choices, and what they set.
pub const OPERATION_CHOICES: [(&str, OperationMode); 2] = [
    (
        "semi-automatic: will search and test, but no action without human approval",
        OperationMode::SemiAutomatic,
    ),
    (
        "fully automatic: will search and test and action",
        OperationMode::FullyAutomatic,
    ),
];

/// The actions the editor offers, in order.
pub const ACTION_CHOICES: [RuleAction; 4] = [
    RuleAction::Better,
    RuleAction::SameQuality,
    RuleAction::Alternate,
    RuleAction::FalsePositive,
];

/// The visual duplicates comparator's confidences, in order.
pub const VISUAL_CHOICES: [u8; 3] = [60, 85, 100];

/// An action, as the editor and the list say it.
pub fn action_text(action: RuleAction) -> &'static str {
    match action {
        RuleAction::FalsePositive => "set as not related/false positive",
        RuleAction::SameQuality => "set as same quality",
        RuleAction::Alternate => "set as alternates",
        RuleAction::Better => "set as duplicates--A better",
        RuleAction::Worse => "set as duplicates--B better",
    }
}

/// A visual duplicates confidence, as said.
pub fn visual_text(confidence: u8) -> &'static str {
    match confidence {
        0..40 => "not duplicates",
        40..60 => "probably visual duplicates",
        60..85 => "very probably visual duplicates",
        85..100 => "almost certainly visual duplicates",
        _ => "near-perfect visual duplicates",
    }
}

/// A file search's summary (`FileSearchContext.GetSummary`).
pub fn search_summary(search: &FileSearchContext, context: &TextContext) -> String {
    if search.predicates.is_empty() {
        return "allows all files".into();
    }
    let mut texts: Vec<String> = search
        .predicates
        .iter()
        .map(|p| predicate_text(p, context))
        .collect();
    texts.sort();
    if texts.len() > 3 {
        format!(
            "{} predicates",
            hydrus_core::numbers::human_int(texts.len() as u64)
        )
    } else {
        texts.join(", ")
    }
}

/// A rule's search, as the list's "search" column says it
/// (`PotentialDuplicatesSearchContext.GetSummary`).
pub fn rule_search_summary(search: &DuplicatesSearch, context: &TextContext) -> String {
    let mut parts = Vec::new();
    match search.pixel_duplicates {
        PixelDuplicates::Required => parts.push("pixel duplicates".to_owned()),
        PixelDuplicates::Excluded => parts.push("not pixel duplicates".to_owned()),
        PixelDuplicates::Allowed => {}
    }
    let one = search_summary(&search.search_1, context);
    parts.push(match search.kind {
        PairSearchKind::BothFilesMatchDifferentSearches => format!(
            "files matching [{one}] and [{}]",
            search_summary(&search.search_2, context)
        ),
        PairSearchKind::BothFilesMatchOneSearch => format!("both files matching [{one}]"),
        PairSearchKind::OneFileMatchesOneSearch => format!("one file matching [{one}]"),
    });
    if search.pixel_duplicates != PixelDuplicates::Required {
        parts.push(format!(
            "max search distance: {}",
            search.max_hamming_distance
        ));
    }
    parts.join(", ")
}

/// A comparable property, as its system predicate names it.
pub fn property_text(property: Comparable) -> &'static str {
    match property {
        Comparable::Size => "system:filesize",
        Comparable::Width => "system:width",
        Comparable::Height => "system:height",
        Comparable::NumPixels => "system:number of pixels",
        Comparable::Ratio => "system:ratio",
        Comparable::Duration => "system:duration",
        Comparable::Framerate => "system:framerate",
        Comparable::NumFrames => "system:number of frames",
        Comparable::NumTags => "system:number of tags",
        Comparable::NumUrls => "system:number of urls",
        Comparable::ImportTime => "system:import time",
        Comparable::ModifiedTime => "system:modified time",
        Comparable::LastViewedTime => "system:last viewed time",
        Comparable::ArchivedTime => "system:archived time",
    }
}

fn is_time(property: Comparable) -> bool {
    matches!(
        property,
        Comparable::ImportTime
            | Comparable::ModifiedTime
            | Comparable::LastViewedTime
            | Comparable::ArchivedTime
            | Comparable::Duration
    )
}

/// An operator as a relative comparator says it for a property.
fn operator_text(op: NumberOp, property: Comparable) -> &'static str {
    use NumberOp as O;
    if is_time(property) {
        return match op {
            O::Less => "earlier than",
            O::Greater => "later than",
            O::Equal => "exactly the same time as",
            O::NotEqual => "not exactly the same time as",
            O::ApproxPercent { .. } => "you should not see this",
            O::ApproxAbsolute { .. } => "roughly the same time as",
            O::LessOrEqual => "earlier than or exactly the same time as",
            O::GreaterOrEqual => "later than or exactly the same time as",
        };
    }
    if property == Comparable::Ratio {
        return match op {
            O::Less => "taller than",
            O::Greater => "wider than",
            O::Equal => "is",
            O::NotEqual => "is not",
            O::ApproxPercent { .. } => "is about",
            O::ApproxAbsolute { .. } => "you should not see this",
            O::LessOrEqual => "taller than or exactly",
            O::GreaterOrEqual => "wider than or exactly",
        };
    }
    match op {
        O::Less => "<",
        O::Greater => ">",
        O::Equal => "=",
        O::ApproxPercent { .. } | O::ApproxAbsolute { .. } => "\u{2248}",
        O::NotEqual => "\u{2260}",
        O::LessOrEqual => "\u{2264}",
        O::GreaterOrEqual => "\u{2265}",
    }
}

/// A relative comparator's summary (`PairComparatorRelativeFileInfo.
/// GetSummary`): `A has "system:filesize" > 1.50x B +3`.
pub fn relative_summary(
    property: Comparable,
    test: &NumberTest,
    multiplier: f64,
    delta: i64,
) -> String {
    #[allow(clippy::cast_precision_loss)] // (milliseconds to seconds, for words)
    let time = |ms: i64| hydrus_core::time::pretty_time_delta_f64(ms as f64 / 1000.0);
    let delta_text = if is_time(property) {
        time(delta)
    } else {
        delta.to_string()
    };
    let mut what = "B".to_owned();
    #[allow(clippy::float_cmp)] // (as the reference compares)
    if multiplier != 1.0 {
        what = format!("{multiplier:.2}x {what}");
    }
    if delta > 0 {
        what = format!("{what} +{delta_text}");
    } else if delta < 0 {
        what = format!("{what} {delta_text}");
    }
    let mut text = format!("{} {what}", operator_text(test.op, property));
    match test.op {
        NumberOp::ApproxPercent { percent } => {
            text.push_str(&format!(
                " \u{b1}{}",
                hydrus_core::numbers::float_to_percentage(f64::from(percent) / 100.0)
            ));
        }
        NumberOp::ApproxAbsolute { tolerance } => {
            let tolerance = i64::try_from(tolerance).unwrap_or(i64::MAX);
            let shown = if is_time(property) {
                time(tolerance)
            } else {
                hydrus_core::numbers::human_int(tolerance.unsigned_abs())
            };
            text.push_str(&format!(" \u{b1}{shown}"));
        }
        _ => {}
    }
    format!("A has \"{}\" {text}", property_text(property))
}

fn looking_text(looking_at: LookingAt) -> &'static str {
    match looking_at {
        LookingAt::A => "A will match: ",
        LookingAt::B => "B will match: ",
        LookingAt::Either => "either will match: ",
    }
}

/// A comparator's summary (`GetSummary`), as the comparator list shows it.
pub fn comparator_summary(comparator: &Comparator, context: &TextContext) -> String {
    match comparator {
        Comparator::OneFileMetadata {
            looking_at,
            predicates,
        } => {
            let search = FileSearchContext {
                predicates: predicates.clone(),
                ..FileSearchContext::default()
            };
            format!(
                "{}{}",
                looking_text(*looking_at),
                search_summary(&search, context)
            )
        }
        Comparator::OneFileHardcoded { looking_at, test } => format!(
            "{}{}",
            looking_text(*looking_at),
            match test {
                OneFileTest::JpegIsProgressive => "is a progressive jpeg",
                OneFileTest::JpegIsNotProgressive => "is a non-progressive jpeg",
            }
        ),
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => relative_summary(*property, test, *multiplier, *delta),
        Comparator::Pair(test) => match test {
            PairTest::FiletypeSame => "A and B have the same filetype",
            PairTest::FiletypeDiffers => "A and B have different filetypes",
            PairTest::HasExifSame => "A and B have the same \"has exif\" value",
            PairTest::HasIccProfileSame => "A and B have the same \"has icc profile\" value",
            PairTest::AHasClearlyBetterJpegQuality => "A has clearly better jpeg quality than B",
            PairTest::AHasSameOrBetterMetadataFlags => "A has same or better metadata flags to B",
            PairTest::AHasIccProfileIfBDoes => "A has ICC Profile if B does",
        }
        .to_owned(),
        Comparator::VisualDuplicates { confidence } => {
            format!("A and B are {}", visual_text(*confidence))
        }
        Comparator::Or(subs) => {
            if subs.is_empty() {
                return "Empty OR Comparator - will always fail".into();
            }
            format!("({})", sorted_summaries(subs, context).join(") OR ("))
        }
        Comparator::And(subs) => {
            if subs.is_empty() {
                return "Empty AND Comparator - will always fail".into();
            }
            sorted_summaries(subs, context).join(", ")
        }
    }
}

fn sorted_summaries(comparators: &[Comparator], context: &TextContext) -> Vec<String> {
    let mut texts: Vec<String> = comparators
        .iter()
        .map(|c| comparator_summary(c, context))
        .collect();
    texts.sort();
    texts
}

/// Whether a comparator can tell A from B (`CanDetermineBetter`): a
/// "better/worse" action needs one that can.
pub fn can_determine_better(comparator: &Comparator) -> bool {
    match comparator {
        Comparator::OneFileMetadata { looking_at, .. }
        | Comparator::OneFileHardcoded { looking_at, .. } => *looking_at != LookingAt::Either,
        Comparator::RelativeFileInfo {
            test,
            multiplier,
            delta,
            ..
        } => {
            #[allow(clippy::float_cmp)] // (as the reference compares)
            let scaled = *multiplier != 1.0;
            matches!(test.op, NumberOp::Less | NumberOp::Greater) || scaled || *delta != 0
        }
        Comparator::Pair(test) => matches!(
            test,
            PairTest::AHasClearlyBetterJpegQuality
                | PairTest::AHasSameOrBetterMetadataFlags
                | PairTest::AHasIccProfileIfBDoes
        ),
        Comparator::VisualDuplicates { .. } => false,
        Comparator::Or(subs) => !subs.is_empty() && subs.iter().all(can_determine_better),
        Comparator::And(subs) => subs.iter().any(can_determine_better),
    }
}

/// Whether a rule's comparators, together, can tell A from B.
pub fn rule_can_determine_better(comparators: &[Comparator]) -> bool {
    comparators.iter().any(can_determine_better)
}

/// What the editor says on "OK" for a "better/worse" rule whose
/// comparators can't tell A from B.
pub const CANNOT_DETERMINE_BETTER: &str = "Hey, you have the action set to \"better/worse duplicate\" but the comparison step has no way to figure out A or B! You need to add at least one step that specifies either A or B so the rule knows which way around to apply the action. If there is no easy determination, perhaps \"set as same quality\" is more appropriate?";

/// The rule's action, as the list's "action" column says it.
pub fn action_summary(rule: &Rule) -> String {
    let mut text = action_text(rule.action).to_owned();
    if rule.delete_a {
        text.push_str(", delete A");
    }
    if rule.delete_b {
        text.push_str(", delete B");
    }
    text.push_str(if rule.custom_merge.is_some() {
        ", custom merge options"
    } else {
        ", default merge options"
    });
    text
}

/// The rule's operation, as the list says it.
pub fn operation_text(rule: &Rule) -> String {
    let mut text = match rule.mode {
        OperationMode::SemiAutomatic => "semi-automatic",
        OperationMode::FullyAutomatic => "fully automatic",
    }
    .to_owned();
    if rule.paused {
        text.push_str(" - paused");
    }
    text
}

/// A rule's row in the "edit rules" list; `progress` is its search
/// progress (`duplicates_page::rule_progress`).
pub fn row(rule: &Rule, progress: &str, context: &TextContext) -> [String; 6] {
    [
        rule.name.clone(),
        rule_search_summary(&rule.search, context),
        sorted_summaries(&rule.comparators, context).join(", "),
        action_summary(rule),
        progress.to_owned(),
        operation_text(rule),
    ]
}
