//! GUI sessions and the downloaders in their pages, against
//! `oracle/fixtures/gui_sessions.json` (made by
//! `oracle/dump_gui_sessions.py`): every field lands where the reference
//! puts it, and a session's tree and pages read as the reference reads them.

use serde_json::{Value as Json, json};

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::CheckerOptions;
use hydrus_legacy::objects::auto_resolution::PotentialsSearch;
use hydrus_legacy::objects::gui_sessions::{
    LegacyGalleryImport, LegacyMultipleGalleryImport, LegacyMultipleWatcherImport, LegacyPage,
    LegacyUrlsImport, LegacyWatcherImport, PageContent, SessionNode, gallery_import,
    multiple_gallery_import, multiple_watcher_import, page, page_data, session, urls_import,
    watcher_import,
};
use hydrus_legacy::objects::import_options::slice;
use hydrus_legacy::objects::subscriptions::checker_options;
use hydrus_legacy::objects::{FileSearchContext, MediaSort};

mod common;
use common::seeds::{file_seed_facts, gallery_seed_facts, object, service_tags, sorted};

/// The expected tuple if ours decodes the same, so facts compare whole.
fn same_options(ours: &ImportOptionsSlice, expected: &Json) -> Json {
    if *ours == slice(&object(expected)).unwrap() {
        expected.clone()
    } else {
        json!(format!("{ours:?}"))
    }
}

fn same_checker(ours: &CheckerOptions, expected: &Json) -> Json {
    if *ours == checker_options(&object(expected)).unwrap() {
        expected.clone()
    } else {
        json!(format!("{ours:?}"))
    }
}

fn seeds(
    files: &[hydrus_legacy::objects::subscriptions::LegacyFileSeed],
    galleries: &[hydrus_legacy::objects::subscriptions::LegacyGallerySeed],
) -> (Json, Json) {
    (
        files.iter().map(file_seed_facts).collect(),
        galleries.iter().map(gallery_seed_facts).collect(),
    )
}

fn urls_facts(u: &LegacyUrlsImport, expected: &Json) -> Json {
    let (file_seeds, gallery_seeds) = seeds(&u.file_seeds, &u.gallery_seeds);
    json!({
        "file_seeds": file_seeds,
        "gallery_seeds": gallery_seeds,
        "paused": u.paused,
        "import_options": same_options(&u.import_options, &expected["import_options"]),
    })
}

fn gallery_facts(g: &LegacyGalleryImport, expected: &Json) -> Json {
    let (file_seeds, gallery_seeds) = seeds(&g.file_seeds, &g.gallery_seeds);
    json!({
        "file_seeds": file_seeds,
        "gallery_seeds": gallery_seeds,
        "key": g.key,
        "created": g.created,
        "query": g.query,
        "source_name": g.source_name,
        "current_page_index": g.current_page_index,
        "num_urls_found": g.num_urls_found,
        "num_new_urls_found": g.num_new_urls_found,
        "file_limit": g.file_limit,
        "gallery_paused": g.gallery_paused,
        "files_paused": g.files_paused,
        "no_work_until": g.no_work_until,
        "no_work_until_reason": g.no_work_until_reason,
        "import_options": same_options(&g.import_options, &expected["import_options"]),
    })
}

fn gallery_page_facts(m: &LegacyMultipleGalleryImport, expected: &Json) -> Json {
    json!({
        "gug_key": m.gug_key,
        "gug_name": m.gug_name,
        "highlighted": m.highlighted,
        "file_limit": m.file_limit,
        "start_file_queues_paused": m.start_file_queues_paused,
        "start_gallery_queues_paused": m.start_gallery_queues_paused,
        "do_not_allow_new_dupes": m.do_not_allow_new_dupes,
        "merge_simultaneous_pends_to_one_importer": m.merge_simultaneous_pends_to_one_importer,
        "import_options": same_options(&m.import_options, &expected["import_options"]),
        "gallery_imports": m.gallery_imports.iter().zip(expected["gallery_imports"].as_array().unwrap())
            .map(|(g, e)| gallery_facts(g, e)).collect::<Vec<_>>(),
    })
}

fn watcher_facts(w: &LegacyWatcherImport, expected: &Json) -> Json {
    let (file_seeds, gallery_seeds) = seeds(&w.file_seeds, &w.gallery_seeds);
    json!({
        "file_seeds": file_seeds,
        "gallery_seeds": gallery_seeds,
        "url": w.url,
        "filterable": sorted(&w.external_filterable_tags),
        "additional": service_tags(&w.external_additional_tags),
        "checker_options": same_checker(&w.checker, &expected["checker_options"]),
        "import_options": same_options(&w.import_options, &expected["import_options"]),
        "last_check_time": w.last_check_time,
        "files_paused": w.files_paused,
        "checking_paused": w.checking_paused,
        "checking_status": w.checking_status,
        "subject": w.subject,
        "no_work_until": w.no_work_until,
        "no_work_until_reason": w.no_work_until_reason,
        "created": w.created,
    })
}

fn watcher_page_facts(m: &LegacyMultipleWatcherImport, expected: &Json) -> Json {
    json!({
        "highlighted": m.highlighted,
        "checker_options": same_checker(&m.checker, &expected["checker_options"]),
        "import_options": same_options(&m.import_options, &expected["import_options"]),
        "watchers": m.watchers.iter().zip(expected["watchers"].as_array().unwrap())
            .map(|(w, e)| watcher_facts(w, e)).collect::<Vec<_>>(),
    })
}

/// The expected value if ours decodes the same (`decode` reads the
/// expected, stored form), so facts compare whole.
fn same<T: PartialEq + std::fmt::Debug>(
    ours: &T,
    expected: &Json,
    decode: impl Fn(&hydrus_legacy::serialisable::SerialisableObject) -> T,
) -> Json {
    if *ours == decode(&object(expected)) {
        expected.clone()
    } else {
        json!(format!("{ours:?}"))
    }
}

/// A page's name, type, sort, and its search or downloader's facts where
/// it has one.
fn page_facts(p: &LegacyPage, expected: &Json) -> Json {
    let variables = &expected["variables"];
    let mut content = match &p.content {
        PageContent::Query(q) => json!({
            "file_search_context": same(&q.search, &variables["file_search_context"], |o| {
                FileSearchContext::from_object(o).unwrap()
            }),
            "synchronised": q.synchronised,
            "system_hash_locked": q.hash_locked,
            "system_hash_locked_syncs_new": q.lock_syncs.syncs_new,
            "system_hash_locked_syncs_removes": q.lock_syncs.syncs_removes,
        }),
        PageContent::Urls(u) => json!({ "urls_import": urls_facts(u, &variables["urls_import"]) }),
        PageContent::Gallery(m) => json!({
            "multiple_gallery_import": gallery_page_facts(m, &variables["multiple_gallery_import"])
        }),
        PageContent::Watchers(m) => json!({
            "multiple_watcher_import": watcher_page_facts(m, &variables["multiple_watcher_import"])
        }),
        PageContent::Duplicates(d) => json!({
            "potential_duplicates_search_context": same(
                &d.search,
                &variables["potential_duplicates_search_context"],
                |o| PotentialsSearch::from_object(o).unwrap()
            ),
            "synchronised": d.synchronised,
            "duplicate_pair_sort_type": d.sort_type,
            "duplicate_pair_sort_asc": d.sort_ascending,
            "filter_group_mode": d.group_mode,
        }),
        PageContent::Other => json!({}),
    };
    if let Some(sort) = &p.sort {
        content["media_sort"] = same(sort, &variables["media_sort"], |o| {
            MediaSort::from_object(o).unwrap()
        });
    }
    json!({ "name": p.name, "type": p.page_type, "content": content })
}

/// The expected facts cut down to what [`page_facts`] reads.
fn expected_page_facts(expected: &Json) -> Json {
    let variables = &expected["variables"];
    let content: serde_json::Map<String, Json> = [
        "urls_import",
        "multiple_gallery_import",
        "multiple_watcher_import",
        "file_search_context",
        "synchronised",
        "system_hash_locked",
        "system_hash_locked_syncs_new",
        "system_hash_locked_syncs_removes",
        "media_sort",
        "potential_duplicates_search_context",
        "duplicate_pair_sort_type",
        "duplicate_pair_sort_asc",
        "filter_group_mode",
    ]
    .into_iter()
    .filter_map(|name| Some((name.to_owned(), variables.get(name)?.clone())))
    .collect();
    json!({ "name": expected["name"], "type": expected["type"], "content": content })
}

fn cases(name: &str) -> Vec<Json> {
    let fixture = hydrus_testkit::fixture_json("gui_sessions.json");
    let cases = fixture[name].as_array().unwrap().clone();
    assert!(!cases.is_empty(), "{name}");
    cases
}

#[test]
fn downloaders_read_as_the_reference_reads_them() {
    for case in cases("urls_imports") {
        let ours = urls_import(&object(&case["stored"])).unwrap();
        assert_eq!(urls_facts(&ours, &case["facts"]), case["facts"]);
    }
    for case in cases("gallery_imports") {
        let ours = gallery_import(&object(&case["stored"])).unwrap();
        assert_eq!(gallery_facts(&ours, &case["facts"]), case["facts"]);
    }
    for case in cases("multiple_gallery_imports") {
        let ours = multiple_gallery_import(&object(&case["stored"])).unwrap();
        assert_eq!(gallery_page_facts(&ours, &case["facts"]), case["facts"]);
    }
    for case in cases("watcher_imports") {
        let ours = watcher_import(&object(&case["stored"])).unwrap();
        assert_eq!(watcher_facts(&ours, &case["facts"]), case["facts"]);
    }
    for case in cases("multiple_watcher_imports") {
        let ours = multiple_watcher_import(&object(&case["stored"])).unwrap();
        assert_eq!(watcher_page_facts(&ours, &case["facts"]), case["facts"]);
    }
}

/// (search pages locked to a `system:hash`, with what the hash follows,
/// among them)
#[test]
fn pages_read_as_the_reference_reads_them() {
    for case in cases("page_managers")
        .into_iter()
        .chain(cases("duplicates_pages"))
        .chain(cases("locked_pages"))
    {
        let ours = page(&object(&case["stored"])).unwrap();
        assert_eq!(
            page_facts(&ours, &case["facts"]),
            expected_page_facts(&case["facts"])
        );
    }
}

fn tree_facts(node: &SessionNode) -> Json {
    match node {
        SessionNode::Notebook { name, pages } => {
            json!({ "name": name, "pages": pages.iter().map(tree_facts).collect::<Vec<_>>() })
        }
        SessionNode::Page {
            name,
            page_data_hash,
        } => json!({ "name": name, "page_data_hash": hex::encode(page_data_hash) }),
    }
}

#[test]
fn sessions_read_as_the_reference_reads_them() {
    for case in cases("sessions") {
        let facts = &case["facts"];
        let ours = session(&object(&case["container"])).unwrap();
        assert_eq!(ours.name, facts["name"]);
        assert_eq!(tree_facts(&ours.top), facts["tree"]);
        for hash in ours.top.page_data_hashes() {
            let hash = hex::encode(hash);
            let stored = &case["page_data"][&hash];
            let expected = &facts["pages"][&hash];
            let data = page_data(&object(stored)).unwrap();
            assert_eq!(
                page_facts(&data.page, &expected["page"]),
                expected_page_facts(&expected["page"])
            );
            let hashes: Vec<String> = data.hashes.iter().map(hex::encode).collect();
            assert_eq!(json!(hashes), expected["hashes"]);
        }
    }
}
