//! What a duplicate decision would change, said as the reference's
//! `GetMergeSummaryOnPair` says it (`oracle/record_merge_summaries.py`): two
//! files given tags, ratings, notes and URLs, under the client's merge
//! options and custom ones, each way round, with deletes, in and out of
//! auto-resolution.

use std::collections::HashMap;
use std::sync::Arc;

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::{HashId, Tag};
use hydrus_gui_model::merge_summary::summary;
use hydrus_store::Store;
use hydrus_store::content::{DomainRoles, MappingAction};
use hydrus_store::delete_lock::Reinbox;
use hydrus_store::duplicates::merge::{
    self, ArchiveSync, DuplicateMergeSettings, MergeAction, MergeOptions, RatingMerge, SyncAction,
    TagMerge,
};
use serde_json::Value;

fn store() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

/// The files named, by the basic fixture's manifest.
fn files(store: &Store, names: &[Value]) -> Vec<HashId> {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hashes: HashMap<String, String> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_owned(),
                f["hash"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    names
        .iter()
        .map(|n| {
            let hash = hydrus_core::Sha256::from_slice(
                &hex::decode(&hashes[n.as_str().unwrap()]).unwrap(),
            )
            .unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}

/// The recorder's setup, done again.
fn set_up(store: &Store, files: &[HashId], setup: &[Value]) {
    let setup = setup.to_vec();
    let files = files.to_vec();
    store
        .write_content(move |w| {
            for op in &setup {
                let file = files[usize::try_from(op[1].as_u64().unwrap()).unwrap()];
                let service = |name: &Value| {
                    w.snapshot()
                        .services
                        .by_name(name.as_str().unwrap())
                        .unwrap()
                        .clone()
                };
                match op[0].as_str().unwrap() {
                    "tags" => {
                        let service = service(&op[2]).id;
                        for tag in op[3].as_array().unwrap() {
                            let tag = Tag::new(tag.as_str().unwrap()).unwrap();
                            let id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                            w.update_mappings(service, &MappingAction::Add, id, &[file])?;
                        }
                    }
                    "rating" => {
                        let service = service(&op[2]);
                        if let Some(n) = op[3].as_i64() {
                            w.set_incdec(service.id, &[file], n)?;
                        } else {
                            w.set_rating(service.id, &[file], op[3].as_f64())?;
                        }
                    }
                    "note" => w.set_note(file, op[2].as_str().unwrap(), op[3].as_str().unwrap())?,
                    "url" => w.add_urls(&[file], &[op[2].as_str().unwrap().to_owned()])?,
                    "archive" => w.archive(&[file])?,
                    other => panic!("{other}"),
                }
            }
            Ok(())
        })
        .unwrap();
}

fn merge_action(code: &Value) -> Option<MergeAction> {
    match code.as_u64()? {
        0 => Some(MergeAction::Copy),
        1 => Some(MergeAction::Move),
        2 => Some(MergeAction::TwoWay),
        _ => None,
    }
}

fn sync_action(code: &Value) -> Option<SyncAction> {
    match code.as_u64()? {
        0 => Some(SyncAction::Copy),
        2 => Some(SyncAction::TwoWay),
        _ => None,
    }
}

/// The options described.
fn options(store: &Store, description: &Value) -> MergeOptions {
    let client: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    match description["client"].as_u64() {
        Some(4) => return client.better,
        Some(2) => return client.same_quality,
        Some(other) => panic!("{other}"),
        None => {}
    }
    let snapshot = store.snapshot();
    let key = |name: &Value| {
        snapshot
            .services
            .by_name(name.as_str().unwrap())
            .unwrap()
            .key
            .clone()
    };
    let none = Vec::new();
    MergeOptions {
        tags: description["tags"]
            .as_array()
            .unwrap_or(&none)
            .iter()
            .map(|t| {
                let mut filter = TagFilter::default();
                if let Some(namespace) = t[2].as_str() {
                    filter.set_rule(format!("{namespace}:"), FilterRule::Blacklist);
                }
                TagMerge {
                    service: key(&t[0]),
                    action: merge_action(&t[1]).unwrap(),
                    filter,
                }
            })
            .collect(),
        ratings: description["ratings"]
            .as_array()
            .unwrap_or(&none)
            .iter()
            .map(|r| RatingMerge {
                service: key(&r[0]),
                action: merge_action(&r[1]).unwrap(),
            })
            .collect(),
        notes: merge_action(&description["notes"]),
        note_merge: client.better.note_merge,
        note_names: client.better.note_names.clone(),
        archive: match description["archive"].as_u64() {
            Some(1) => ArchiveSync::IfEither,
            Some(2) => ArchiveSync::Always,
            _ => ArchiveSync::Never,
        },
        urls: sync_action(&description["urls"]),
        file_modified: sync_action(&description["modified"]),
    }
}

/// A summary with each "add urls" line's URLs sorted (the reference joins
/// a set, in no fixed order).
fn urls_sorted(summary: &str) -> String {
    summary
        .lines()
        .map(|line| {
            line.split(" | ")
                .map(|part| match part.find("add urls: ") {
                    Some(at) => {
                        let (head, urls) = part.split_at(at + "add urls: ".len());
                        let mut urls: Vec<&str> = urls.split(", ").collect();
                        urls.sort_unstable();
                        format!("{head}{}", urls.join(", "))
                    }
                    None => part.to_owned(),
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn merges_are_summarised_as_the_reference() {
    let recorded = hydrus_testkit::fixture_json("merge_summaries.json");
    let (_dir, store) = store();
    let files = files(&store, recorded["files"].as_array().unwrap());
    set_up(&store, &files, recorded["setup"].as_array().unwrap());
    let snapshot = store.snapshot();
    let combined_local = DomainRoles::new(&snapshot.services)
        .unwrap()
        .combined_local_media;
    for case in recorded["options"].as_array().unwrap() {
        let description = &case["options"];
        let options = options(&store, description);
        for c in case["cases"].as_array().unwrap() {
            let at = |k: &str| files[usize::try_from(c[k].as_u64().unwrap()).unwrap()];
            let (a, b) = (at("a"), at("b"));
            let deletes = [
                c["delete_a"].as_bool().unwrap(),
                c["delete_b"].as_bool().unwrap(),
            ];
            let in_auto = c["in_auto_resolution"].as_bool().unwrap();
            let mut options = options.clone();
            // (auto-resolution archives both only if one was archived)
            if in_auto && options.archive == ArchiveSync::Always {
                options.archive = ArchiveSync::IfEither;
            }
            let reinbox = if in_auto {
                Reinbox::InAutoResolution
            } else {
                Reinbox::AfterDuplicateFilter
            };
            let ours = store
                .read(|conn| {
                    let changes = merge::plan(
                        conn,
                        &snapshot.services,
                        combined_local,
                        a,
                        b,
                        Some(&options),
                        deletes,
                        reinbox,
                    )?;
                    summary(conn, &snapshot, &changes, a, b)
                })
                .unwrap();
            assert_eq!(
                urls_sorted(&ours),
                urls_sorted(c["summary"].as_str().unwrap()),
                "{description} {c}"
            );
        }
    }
}
