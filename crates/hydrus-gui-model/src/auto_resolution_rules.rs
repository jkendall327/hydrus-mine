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

/// The "edit rules" dialog's warning, and its note on order.
pub const RULES_WARNING: &str = "THIS IS AN ADVANCED SYSTEM. READ THE HELP DOCUMENTATION. DO NOT CREATE A NEW RULE WITHOUT UNDERSTANDING AND PREVIEWING WHAT IT WILL DO";
pub const RULES_ORDER_NOTE: &str = "Rules are worked on in smart alphabetical name order, so if you have overlapping test domains and want to force precedence, try naming them \"1 - \" and \"2 - \" etc..";

/// A rule as the dialog holds it, with its id in the store if it is
/// there yet.
#[derive(Debug, Clone)]
pub struct RuleEdit {
    pub id: Option<i64>,
    pub rule: Rule,
}

impl crate::folders::Name for RuleEdit {
    fn name(&self) -> &str {
        &self.rule.name
    }

    fn set_name(&mut self, name: String) {
        self.rule.name = name;
    }
}

/// The rule "add" starts from (`_Add`): "new rule", searching both files
/// of pairs among images over 128x128 in all my files, pixel duplicates
/// allowed at distance 0; no comparators, "better", semi-automatic.
/// `like` is a suggested rule whose search is that (the reference's
/// "pixel-perfect pairs").
pub fn new_rule(like: &Rule) -> Rule {
    let mut search = like.search.clone();
    search.kind = PairSearchKind::BothFilesMatchOneSearch;
    search.pixel_duplicates = PixelDuplicates::Allowed;
    search.max_hamming_distance = 0;
    Rule {
        name: "new rule".into(),
        paused: false,
        mode: OperationMode::SemiAutomatic,
        max_pending_pairs: Some(500),
        search,
        comparators: Vec::new(),
        action: RuleAction::Better,
        delete_a: false,
        delete_b: false,
        custom_merge: None,
    }
}

/// The search tab's choices: what pairs match, in order, and pixel
/// duplicates.
pub const PAIR_SEARCH_CHOICES: [(&str, PairSearchKind); 3] = [
    (
        "at least one file matches the search",
        PairSearchKind::OneFileMatchesOneSearch,
    ),
    (
        "both files match the search",
        PairSearchKind::BothFilesMatchOneSearch,
    ),
    (
        "the two files match different searches",
        PairSearchKind::BothFilesMatchDifferentSearches,
    ),
];

pub const PIXEL_CHOICES: [(&str, PixelDuplicates); 3] = [
    ("must be pixel dupes", PixelDuplicates::Required),
    ("can be pixel dupes", PixelDuplicates::Allowed),
    ("must not be pixel dupes", PixelDuplicates::Excluded),
];

/// The tabs' texts.
pub const SEARCH_TEXT: &str = "First we have to find some duplicate pairs to test. This can be system:everything if you like, but it is best to narrow it down if you can.\n\nIt is a good idea to keep a \"system:filetype is image\" in here to ensure you do not include some PSD files by accident etc..";
pub const COMPARISON_TEXT: &str = "Now, for each pair that matches our search, we need to determine if their differences (or similarities!) are clear enough that we can confidently make an automatic decision. The pairs are also unordered, so if we are setting one file to be a better duplicate of the other, we also need to define which is the A (usually the better) and the B (usually the worse).\n\nThe client will test the incoming pair both ways around ([1,2] and [2,1]) against these rules, and if they fit either way, that \"AB\" pair order is set and the action is applied. If the pair cannot fit into the rules either way, the test is considered failed and no changes are made. If there are no rules, all pairs will be actioned--but remember you need at least one clear A- or B-defining rule for a \"better/worse duplicates\" action.";
pub const ACTION_TEXT: &str = "And now we have pairs to action, what should we do?\n\nNote that in the auto-resolution filter, \"always archive both\" will be treated as \"if one is archived, archive the other\".";

/// What "add" in a comparator list offers ("Which type of comparator?"):
/// each choice's label and description, and the comparator it starts.
pub fn comparator_choices() -> Vec<(String, &'static str, Comparator)> {
    let mut out: Vec<(String, &'static str, Comparator)> = vec![
        (
            "test A or B using search terms".into(),
            "A comparator that tests one file at a time using system predicates.",
            Comparator::OneFileMetadata {
                looking_at: LookingAt::A,
                predicates: Vec::new(),
            },
        ),
        (
            "test A or B using other file info".into(),
            "A comparator that tests one file at a time using a special routine.",
            Comparator::OneFileHardcoded {
                looking_at: LookingAt::A,
                test: OneFileTest::JpegIsProgressive,
            },
        ),
        (
            "test A against B using file info".into(),
            "A comparator that performs a number test on the width, filesize, etc.. of A vs B.",
            Comparator::RelativeFileInfo {
                property: Comparable::Size,
                test: NumberTest {
                    op: NumberOp::Greater,
                    value: 1,
                },
                multiplier: 1.0,
                delta: 0,
            },
        ),
        (
            "test if A and B are visual duplicates".into(),
            "A comparator that examines the differences in the images' shape and colour to determine if they are visual duplicates.",
            Comparator::VisualDuplicates { confidence: 85 },
        ),
    ];
    let pairs: [(PairTest, &'static str); 7] = [
        (
            PairTest::FiletypeSame,
            "A comparator that tests if the two files share the same filetype.",
        ),
        (
            PairTest::FiletypeDiffers,
            "A comparator that tests if the two files have different filetype.",
        ),
        (
            PairTest::AHasClearlyBetterJpegQuality,
            "A comparator that tests if A has a non-trivially higher apparent jpeg quality than B. The difference corresponds to about one label-step of quality as you see in the duplicate filter. If either file is not a jpeg, it fails.",
        ),
        (
            PairTest::AHasSameOrBetterMetadataFlags,
            "Easy one-shot comparator that wants to preserve rich metadata in A. If B has a \"has_x\" flag, A must have it too. The flags tested are: EXIF, XMP, IPTC, software/source, human-readable. The actual contents are not compared, only the presence.",
        ),
        (
            PairTest::AHasIccProfileIfBDoes,
            "Easy one-shot comparator that wants to keep ICC Profiles in A. If B has an ICC Profile, A must have one too. The actual contents are not compared, only the presence.",
        ),
        (
            PairTest::HasExifSame,
            "A comparator that tests if the two files either both have or both do not have some amount of EXIF data.",
        ),
        (
            PairTest::HasIccProfileSame,
            "A comparator that tests if the two files either both have or both do not have some amount of ICC Profile data.",
        ),
    ];
    let context = TextContext::default();
    for (test, description) in pairs {
        let comparator = Comparator::Pair(test);
        out.push((
            comparator_summary(&comparator, &context),
            description,
            comparator,
        ));
    }
    out.push((
        "OR Comparator".into(),
        "A comparator that tests an OR of several sub-comparators.",
        Comparator::Or(Vec::new()),
    ));
    out.push((
        "AND Comparator".into(),
        "A comparator that tests an AND of several sub-comparators. Use when you need to mix several different comparators within an OR.",
        Comparator::And(Vec::new()),
    ));
    out
}

/// The looking-at choices of a one-file comparator, in order: the search
/// one's wording, or the hardcoded one's.
pub fn looking_choices(search: bool) -> [(&'static str, LookingAt); 3] {
    if search {
        [
            ("A will match these", LookingAt::A),
            ("B will match these", LookingAt::B),
            ("either will match these", LookingAt::Either),
        ]
    } else {
        [
            ("A will match", LookingAt::A),
            ("B will match", LookingAt::B),
            ("either will match", LookingAt::Either),
        ]
    }
}

/// The one-file hardcoded tests, in order.
pub const ONE_FILE_TESTS: [(&str, OneFileTest); 2] = [
    ("is a progressive jpeg", OneFileTest::JpegIsProgressive),
    (
        "is a non-progressive jpeg",
        OneFileTest::JpegIsNotProgressive,
    ),
];

/// The properties a relative comparator can test, in the reference's
/// order.
pub const PROPERTIES: [Comparable; 14] = [
    Comparable::Size,
    Comparable::Width,
    Comparable::Height,
    Comparable::NumPixels,
    Comparable::Ratio,
    Comparable::Duration,
    Comparable::Framerate,
    Comparable::NumFrames,
    Comparable::NumTags,
    Comparable::NumUrls,
    Comparable::ImportTime,
    Comparable::ModifiedTime,
    Comparable::LastViewedTime,
    Comparable::ArchivedTime,
];

/// The operators offered for a property (the reference's panel for its
/// kind), approximate ones with a default range.
pub fn operator_choices(property: Comparable) -> Vec<NumberOp> {
    use NumberOp as O;
    let time = matches!(
        property,
        Comparable::ImportTime
            | Comparable::ModifiedTime
            | Comparable::LastViewedTime
            | Comparable::ArchivedTime
    );
    let mut ops = vec![O::Less, O::Greater];
    match property {
        Comparable::Framerate => {
            ops.push(O::ApproxAbsolute { tolerance: 1 });
            ops.push(O::ApproxPercent { percent: 5 });
            return ops;
        }
        _ => ops.extend([O::LessOrEqual, O::GreaterOrEqual, O::Equal, O::NotEqual]),
    }
    if time {
        ops.push(O::ApproxAbsolute { tolerance: 60_000 });
    } else if property == Comparable::Duration {
        ops.push(O::ApproxAbsolute { tolerance: 1000 });
        ops.push(O::ApproxPercent { percent: 5 });
    } else if property == Comparable::Ratio {
        ops.push(O::ApproxPercent { percent: 5 });
    } else {
        ops.push(O::ApproxAbsolute { tolerance: 1 });
        ops.push(O::ApproxPercent { percent: 5 });
    }
    ops
}

/// Whether two operators are the same kind (ignoring their ranges).
pub fn same_operator(a: NumberOp, b: NumberOp) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// An operator as its choice shows it, for a property.
pub fn operator_label(op: NumberOp, property: Comparable) -> String {
    let words = operator_text(op, property);
    match op {
        NumberOp::ApproxPercent { .. } => format!("{words} (within a percentage)"),
        NumberOp::ApproxAbsolute { .. } => format!("{words} (within a range)"),
        _ => words.to_owned(),
    }
}

/// Predicates as lines of text, as the search box shows them.
pub fn predicate_lines(
    predicates: &[hydrus_core::search::predicate::Predicate],
    context: &TextContext,
) -> String {
    predicates
        .iter()
        .map(|p| predicate_text(p, context))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Predicates typed a line each (as the Client API's search reads tags
/// and system predicates), or what is wrong.
pub fn parse_predicate_lines(
    text: &str,
) -> Result<Vec<hydrus_core::search::predicate::Predicate>, String> {
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::Value::String(l.to_owned()))
        .collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    hydrus_search::parse_api_search(&serde_json::Value::Array(lines)).map_err(|e| e.to_string())
}
