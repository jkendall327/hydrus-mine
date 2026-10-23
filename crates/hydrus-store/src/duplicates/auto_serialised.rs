//! Auto-resolution rules and comparators in the reference's serialised form
//! (`DuplicatesAutoResolutionRule` v3 and its `PairComparator`s), so the
//! reference can import what hydrus-rs exports: the inverse of
//! [`crate::import::auto_resolution_rule`] and
//! [`crate::import::auto_resolution_comparator`].
//!
//! The text is what the reference's `DumpToString` writes (`json.dumps` of
//! the tuple). Where hydrus-rs keeps less than the reference, the
//! reference's defaults stand in: a file search's search type (AND) and
//! "search complete" flag (false), a metadata conditional's location and tag
//! context (combined local file domains, all known tags), and merge options'
//! services that merge nothing (left out, as they merge nothing either way).

use hydrus_core::ServiceKey;
use hydrus_core::search::context::{FileSearchContext, LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_core::tag_filter::FilterRule;
use hydrus_legacy::objects::predicate_encode;
use hydrus_legacy::pyjson::PyJson;

use super::auto::{Comparator, LookingAt, OneFileTest, OperationMode, PairTest, Rule};
use super::merge::{ArchiveSync, MergeAction, MergeOptions, SyncAction};

/// What the encoder looks up in the client (services' keys and scales, the
/// default viewing canvases).
pub use predicate_encode::Context;

const LIST: i64 = 26;

fn int(n: impl Into<i64>) -> PyJson {
    PyJson::Int(n.into())
}

fn object(kind: i64, version: i64, info: PyJson) -> PyJson {
    PyJson::List(vec![int(kind), int(version), info])
}

fn hex(key: &ServiceKey) -> PyJson {
    PyJson::Str(key.to_hex())
}

/// A `SerialisableList` of objects.
fn serialisable_list(objects: Vec<PyJson>) -> PyJson {
    object(
        LIST,
        3,
        PyJson::List(
            objects
                .into_iter()
                .map(|o| PyJson::List(vec![int(2), o]))
                .collect(),
        ),
    )
}

fn location(l: &LocationContext) -> PyJson {
    object(
        103,
        1,
        PyJson::List(vec![
            PyJson::List(l.current().iter().map(hex).collect()),
            PyJson::List(l.deleted().iter().map(hex).collect()),
        ]),
    )
}

fn tag_context(t: &TagContext) -> PyJson {
    object(
        80,
        2,
        PyJson::List(vec![
            hex(&t.service),
            PyJson::Bool(t.include_current),
            PyJson::Bool(t.include_pending),
            hex(&t.display_service),
        ]),
    )
}

fn file_search(
    location_context: PyJson,
    tags: PyJson,
    predicates: &[hydrus_core::search::predicate::Predicate],
    ctx: &Context<'_>,
) -> Result<PyJson, String> {
    let predicates = predicates
        .iter()
        .map(|p| predicate_encode::predicate(p, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(object(
        15,
        5,
        PyJson::List(vec![
            location_context,
            tags,
            // SEARCH_TYPE_AND, and not "search complete"
            int(0),
            PyJson::List(predicates),
            PyJson::Bool(false),
        ]),
    ))
}

fn search_context(f: &FileSearchContext, ctx: &Context<'_>) -> Result<PyJson, String> {
    file_search(
        location(&f.location),
        tag_context(&f.tags),
        &f.predicates,
        ctx,
    )
}

fn looking_at(l: LookingAt) -> PyJson {
    int(match l {
        LookingAt::A => 0,
        LookingAt::B => 1,
        LookingAt::Either => 2,
    })
}

/// A comparator, as the reference serialises it.
///
/// # Errors
/// If one of its predicates can't be written (see
/// [`predicate_encode::predicate`]).
pub fn comparator(c: &Comparator, ctx: &Context<'_>) -> Result<PyJson, String> {
    Ok(match c {
        Comparator::OneFileMetadata {
            looking_at: l,
            predicates,
        } => {
            let any_file = ServiceKey::new(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec());
            let all_tags = ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec());
            let search = file_search(
                location(&LocationContext::single(any_file)),
                tag_context(&TagContext::new(all_tags, true, true)),
                predicates,
                ctx,
            )?;
            object(
                130,
                1,
                PyJson::List(vec![looking_at(*l), object(132, 1, search)]),
            )
        }
        Comparator::OneFileHardcoded {
            looking_at: l,
            test,
        } => object(
            152,
            1,
            PyJson::List(vec![
                looking_at(*l),
                int(match test {
                    OneFileTest::JpegIsProgressive => 0,
                    OneFileTest::JpegIsNotProgressive => 1,
                }),
            ]),
        ),
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => object(
            131,
            1,
            PyJson::List(vec![
                predicate_encode::bare(property.predicate_type()),
                predicate_encode::number_test(test)?,
                PyJson::Float(*multiplier),
                int(*delta),
            ]),
        ),
        Comparator::Pair(test) => object(
            137,
            1,
            int(match test {
                PairTest::FiletypeSame => 0,
                PairTest::FiletypeDiffers => 1,
                PairTest::HasExifSame => 2,
                PairTest::HasIccProfileSame => 3,
                PairTest::AHasClearlyBetterJpegQuality => 4,
                PairTest::AHasSameOrBetterMetadataFlags => 5,
                PairTest::AHasIccProfileIfBDoes => 6,
            }),
        ),
        Comparator::VisualDuplicates { confidence } => object(138, 1, int(*confidence)),
        Comparator::Or(members) | Comparator::And(members) => object(
            if matches!(c, Comparator::Or(_)) {
                140
            } else {
                141
            },
            1,
            comparator_list(members, ctx)?,
        ),
    })
}

fn comparator_list(comparators: &[Comparator], ctx: &Context<'_>) -> Result<PyJson, String> {
    Ok(serialisable_list(
        comparators
            .iter()
            .map(|c| comparator(c, ctx))
            .collect::<Result<_, _>>()?,
    ))
}

fn merge_action(a: Option<MergeAction>) -> PyJson {
    // HC.CONTENT_MERGE_ACTION_*
    int(match a {
        Some(MergeAction::Copy) => 0,
        Some(MergeAction::Move) => 1,
        Some(MergeAction::TwoWay) => 2,
        None => 3,
    })
}

fn sync_action(a: Option<SyncAction>) -> PyJson {
    merge_action(a.map(|a| match a {
        SyncAction::Copy => MergeAction::Copy,
        SyncAction::TwoWay => MergeAction::TwoWay,
    }))
}

fn merge_options(o: &MergeOptions) -> PyJson {
    let tags = o
        .tags
        .iter()
        .map(|t| {
            let rules = t
                .filter
                .rules()
                .map(|(slice, rule)| {
                    // TAG_FILTER_BLACKLIST 0, TAG_FILTER_WHITELIST 1
                    PyJson::List(vec![
                        PyJson::Str(slice.to_owned()),
                        int(i64::from(rule != FilterRule::Blacklist)),
                    ])
                })
                .collect();
            PyJson::List(vec![
                hex(&t.service),
                merge_action(Some(t.action)),
                object(44, 1, PyJson::List(rules)),
            ])
        })
        .collect();
    let ratings = o
        .ratings
        .iter()
        .map(|r| PyJson::List(vec![hex(&r.service), merge_action(Some(r.action))]))
        .collect();
    // the note import options: what merges is held, the rest is the
    // reference's default (`NoteImportOptions()`: get, extend, rename)
    let (get, extend, conflict) = match &o.note_merge {
        Some(m) => (true, m.extend_existing, m.conflict as i64),
        None => (o.notes.is_none(), true, 3),
    };
    let names = &o.note_names;
    let note_import = object(
        153,
        1,
        PyJson::List(vec![
            PyJson::Bool(get),
            PyJson::Bool(extend),
            int(conflict),
            PyJson::List(
                names
                    .whitelist
                    .iter()
                    .map(|n| PyJson::Str(n.clone()))
                    .collect(),
            ),
            names
                .all_override
                .as_ref()
                .map_or(PyJson::Null, |n| PyJson::Str(n.clone())),
            PyJson::List(
                names
                    .overrides
                    .iter()
                    .map(|(a, b)| {
                        PyJson::List(vec![PyJson::Str(a.clone()), PyJson::Str(b.clone())])
                    })
                    .collect(),
            ),
        ]),
    );
    object(
        43,
        8,
        PyJson::List(vec![
            PyJson::List(tags),
            PyJson::List(ratings),
            merge_action(o.notes),
            note_import,
            int(match o.archive {
                ArchiveSync::Never => 0,
                ArchiveSync::IfEither => 1,
                ArchiveSync::Always => 2,
            }),
            sync_action(o.urls),
            sync_action(o.file_modified),
        ]),
    )
}

/// A rule, as the reference serialises it. `id` is the reference's rule id
/// (its database id, or a negative session id for a new rule); the
/// reference replaces it when it imports a rule.
///
/// # Errors
/// If one of its predicates can't be written.
pub fn rule(r: &Rule, id: i64, ctx: &Context<'_>) -> Result<PyJson, String> {
    let search = object(
        134,
        1,
        PyJson::List(vec![
            search_context(&r.search.search_1, ctx)?,
            search_context(&r.search.search_2, ctx)?,
            int(r.search.kind.code()),
            int(r.search.pixel_duplicates.code()),
            int(r.search.max_hamming_distance),
        ]),
    );
    Ok(PyJson::List(vec![
        int(128),
        PyJson::Str(r.name.clone()),
        int(3),
        PyJson::List(vec![
            int(id),
            PyJson::Bool(r.paused),
            int(match r.mode {
                OperationMode::SemiAutomatic => 1,
                OperationMode::FullyAutomatic => 2,
            }),
            r.max_pending_pairs.map_or(PyJson::Null, int),
            search,
            object(129, 1, comparator_list(&r.comparators, ctx)?),
            int(r.action.duplicate_type().code()),
            PyJson::Bool(r.delete_a),
            PyJson::Bool(r.delete_b),
            r.custom_merge.as_ref().map_or(PyJson::Null, merge_options),
        ]),
    ]))
}

/// One object alone, or a `SerialisableList` of several, as the
/// reference's list exports them (`_GetExportObject`), as text.
pub fn export_text(objects: Vec<PyJson>) -> String {
    let value = if objects.len() == 1 {
        objects.into_iter().next().expect("one object")
    } else {
        serialisable_list(objects)
    };
    value.to_python_string()
}
