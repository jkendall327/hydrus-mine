//! Duplicates auto-resolution rules (`json_dumps_named` type 128): a
//! potential-duplicates search, a pair selector of comparators that decides
//! which file of a pair is A (the better), and what to do with the pair.
//!
//! File searches keep their predicates as stored objects (decode them with
//! [`crate::objects::predicates`]); everything else is decoded here.

use crate::objects::duplicates::DuplicateMergeOptions;
use crate::objects::favourites::FileSearchContext;
use crate::objects::predicates::number_test;
use crate::objects::util::{
    DecodeResult, boolean, float, int, list, list_items, malformed, nested, opt_int, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableError, SerialisableObject, SerialisableType};
use hydrus_core::search::number::NumberTest;

const RULE: SerialisableType = SerialisableType::DUPLICATES_AUTO_RESOLUTION_RULE;
const SELECTOR: SerialisableType = SerialisableType(129);
const ONE_FILE_METADATA: SerialisableType = SerialisableType(130);
const RELATIVE_FILE_INFO: SerialisableType = SerialisableType(131);
const METADATA_CONDITIONAL: SerialisableType = SerialisableType(132);
const POTENTIALS_SEARCH: SerialisableType = SerialisableType(134);
const RELATIVE_HARDCODED: SerialisableType = SerialisableType(137);
const VISUAL_DUPLICATES: SerialisableType = SerialisableType(138);
const OR: SerialisableType = SerialisableType(140);
const AND: SerialisableType = SerialisableType(141);
const ONE_FILE_HARDCODED: SerialisableType = SerialisableType(152);

/// A rule, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoResolutionRule {
    pub name: String,
    /// The database's id for the rule (the key of its pair status tables).
    pub id: i64,
    pub paused: bool,
    /// 1: semi-automatic (search and test, a human approves), 2: fully
    /// automatic.
    pub operation_mode: i64,
    /// Semi-automatic rules stop testing when this many pairs await approval.
    pub max_pending_pairs: Option<i64>,
    pub search: PotentialsSearch,
    pub comparators: Vec<Comparator>,
    /// The duplicate action (`HC.DUPLICATE_*`) applied to A and B.
    pub action: i64,
    pub delete_a: bool,
    pub delete_b: bool,
    /// `None`: the client's default merge options for the action.
    pub custom_merge_options: Option<DuplicateMergeOptions>,
}

/// A potential-duplicates search (`PotentialDuplicatesSearchContext`).
#[derive(Debug, Clone, PartialEq)]
pub struct PotentialsSearch {
    pub search_1: FileSearchContext,
    pub search_2: FileSearchContext,
    /// `DUPE_SEARCH_*`: 0 one file matches the first search, 1 both match
    /// it, 2 they match different searches.
    pub dupe_search_type: i64,
    /// `SIMILAR_FILES_PIXEL_DUPES_*`: 0 required, 1 allowed, 2 excluded.
    pub pixel_dupes: i64,
    pub max_hamming_distance: i64,
}

/// A comparator of a pair selector.
#[derive(Debug, Clone, PartialEq)]
pub enum Comparator {
    /// One file (A, B or either: `looking_at` 0, 1, 2) matches a file search
    /// (a metadata conditional).
    OneFileMetadata {
        looking_at: i64,
        search: FileSearchContext,
    },
    /// One file passes a hardcoded test (0: is a progressive jpeg, 1: is
    /// not).
    OneFileHardcoded {
        looking_at: i64,
        test: i64,
    },
    /// A's property compared with B's times `multiplier` plus `delta`.
    RelativeFileInfo {
        /// The predicate type of the property (`PREDICATE_TYPE_SYSTEM_*`).
        property: i64,
        test: NumberTest,
        multiplier: f64,
        delta: i64,
    },
    /// A hardcoded two-file test (`HARDCODED_COMPARATOR_TYPE_TWO_FILES_*`).
    RelativeHardcoded(i64),
    /// A and B are visual duplicates with at least this confidence.
    VisualDuplicates(i64),
    Or(Vec<Comparator>),
    And(Vec<Comparator>),
}

impl AutoResolutionRule {
    /// Decode a stored rule (v1-3).
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(RULE)?;
        object.check_not_future()?;
        let name = object
            .name
            .clone()
            .ok_or_else(|| malformed(RULE, "a rule has no name"))?;
        let info = object.info();
        let items = list(RULE, &info, "rule")?;
        // v1 had no max pending pairs (500) and no paused flag; v2 no paused
        // flag, with "paused" an operation mode (0)
        let (id, paused, mode, max_pending, search, selector, action, delete_a, delete_b, merge) =
            match (object.version, items) {
                (1, [id, mode, search, selector, action, a, b, merge]) => {
                    let mode = int(RULE, mode, "operation mode")?;
                    (
                        id,
                        mode == 0,
                        mode,
                        &PyJson::Int(500),
                        search,
                        selector,
                        action,
                        a,
                        b,
                        merge,
                    )
                }
                (2, [id, mode, max_pending, search, selector, action, a, b, merge]) => {
                    let mode = int(RULE, mode, "operation mode")?;
                    (
                        id,
                        mode == 0,
                        mode,
                        max_pending,
                        search,
                        selector,
                        action,
                        a,
                        b,
                        merge,
                    )
                }
                (
                    3,
                    [
                        id,
                        paused,
                        mode,
                        max_pending,
                        search,
                        selector,
                        action,
                        a,
                        b,
                        merge,
                    ],
                ) => (
                    id,
                    boolean(RULE, paused, "paused")?,
                    int(RULE, mode, "operation mode")?,
                    max_pending,
                    search,
                    selector,
                    action,
                    a,
                    b,
                    merge,
                ),
                (version, _) => {
                    return Err(SerialisableError::UnsupportedVersion {
                        kind: RULE,
                        version,
                        detail: "unexpected layout for this version",
                    });
                }
            };
        // the defunct "paused" mode became a paused semi-automatic rule
        let operation_mode = if mode == 0 { 1 } else { mode };
        Ok(AutoResolutionRule {
            name,
            id: int(RULE, id, "rule id")?,
            paused,
            operation_mode,
            max_pending_pairs: opt_int(RULE, max_pending, "max pending pairs")?,
            search: potentials_search(&nested(RULE, search, "search")?)?,
            comparators: selector_comparators(&nested(RULE, selector, "pair selector")?)?,
            action: int(RULE, action, "action")?,
            delete_a: boolean(RULE, delete_a, "delete a")?,
            delete_b: boolean(RULE, delete_b, "delete b")?,
            custom_merge_options: match merge {
                PyJson::Null => None,
                other => Some(DuplicateMergeOptions::from_object(&nested(
                    RULE,
                    other,
                    "merge options",
                )?)?),
            },
        })
    }
}

fn potentials_search(object: &SerialisableObject) -> DecodeResult<PotentialsSearch> {
    let k = POTENTIALS_SEARCH;
    object.expect_kind(k)?;
    object.check_not_future()?;
    let info = object.info();
    let [one, two, kind, pixels, distance] = tuple::<5>(k, &info, "potentials search")?;
    Ok(PotentialsSearch {
        search_1: FileSearchContext::from_object(&nested(k, one, "file search 1")?)?,
        search_2: FileSearchContext::from_object(&nested(k, two, "file search 2")?)?,
        dupe_search_type: int(k, kind, "search type")?,
        pixel_dupes: int(k, pixels, "pixel duplicates")?,
        max_hamming_distance: int(k, distance, "max distance")?,
    })
}

fn selector_comparators(object: &SerialisableObject) -> DecodeResult<Vec<Comparator>> {
    object.expect_kind(SELECTOR)?;
    object.check_not_future()?;
    let info = object.info();
    comparator_list(SELECTOR, &nested(SELECTOR, &info, "comparators")?)
}

/// The comparators of a serialisable list.
fn comparator_list(
    kind: SerialisableType,
    object: &SerialisableObject,
) -> DecodeResult<Vec<Comparator>> {
    list_items(object)?
        .iter()
        .map(|item| match item {
            Meta::Object(object) => comparator(object),
            _ => Err(malformed(kind, "a comparator is not an object")),
        })
        .collect()
}

fn comparator(object: &SerialisableObject) -> DecodeResult<Comparator> {
    object.check_not_future()?;
    let info = object.info();
    let k = object.kind;
    Ok(match k {
        ONE_FILE_METADATA => {
            let [looking_at, conditional] = tuple::<2>(k, &info, "metadata comparator")?;
            let conditional = nested(k, conditional, "metadata conditional")?;
            conditional.expect_kind(METADATA_CONDITIONAL)?;
            conditional.check_not_future()?;
            let search = conditional.info();
            Comparator::OneFileMetadata {
                looking_at: int(k, looking_at, "looking at")?,
                search: FileSearchContext::from_object(&nested(k, &search, "file search")?)?,
            }
        }
        ONE_FILE_HARDCODED => {
            let [looking_at, test] = tuple::<2>(k, &info, "hardcoded comparator")?;
            Comparator::OneFileHardcoded {
                looking_at: int(k, looking_at, "looking at")?,
                test: int(k, test, "test")?,
            }
        }
        RELATIVE_FILE_INFO => {
            let [predicate, test, multiplier, delta] = tuple::<4>(k, &info, "relative comparator")?;
            // the predicate only names the property: its value is unused
            let predicate = nested(k, predicate, "predicate")?;
            let predicate_info = predicate.info();
            let [property, _, _] = tuple::<3>(k, &predicate_info, "predicate")?;
            Comparator::RelativeFileInfo {
                property: int(k, property, "property")?,
                test: number_test(&nested(k, test, "number test")?)?,
                multiplier: match multiplier {
                    PyJson::Int(n) => *n as f64,
                    other => float(k, other, "multiplier")?,
                },
                delta: int(k, delta, "delta")?,
            }
        }
        RELATIVE_HARDCODED => Comparator::RelativeHardcoded(int(k, &info, "hardcoded test")?),
        VISUAL_DUPLICATES => Comparator::VisualDuplicates(int(k, &info, "confidence")?),
        OR | AND => {
            let members = comparator_list(k, &nested(k, &info, "comparators")?)?;
            if k == OR {
                Comparator::Or(members)
            } else {
                Comparator::And(members)
            }
        }
        other => {
            return Err(malformed(
                SELECTOR,
                format!("unknown comparator type {other:?}"),
            ));
        }
    })
}
