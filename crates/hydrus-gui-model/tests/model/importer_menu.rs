//! A gallery and a watcher downloader page's list menus against the
//! reference's, recorded by `oracle/record_importer_menus.py`, with the
//! first importer's logs holding a file of each status and a successful
//! and a failed page; and presentation options' summaries.

use serde_json::{Value as Json, json};

use hydrus_core::import_options::{PresentationInbox, PresentationOptions, PresentationStatus};
use hydrus_core::service::builtin_keys;
use hydrus_gui_model::file_log::{self, Entry};
use hydrus_gui_model::importer_menu::{
    Action, Selected, Single, gallery_menu, presentation_summary, watcher_menu,
};
use hydrus_gui_model::search_log;
use hydrus_store::queues::{SeedStatus, StatusCounts};

fn tree(entries: &[Entry<Action>]) -> Json {
    Json::Array(
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(label, _) | Entry::Label(label) => json!(label),
                Entry::Separator => json!("---"),
                Entry::Menu(title, entries) => json!({"menu": title, "entries": tree(entries)}),
            })
            .collect(),
    )
}

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/importer_menus.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn counts(statuses: &[SeedStatus]) -> StatusCounts {
    let mut counts = StatusCounts::new();
    for s in statuses {
        *counts.entry(*s).or_default() += 1;
    }
    counts
}

/// The selection of rows `rows`: the first importer has the logs, and
/// each gallery query its first page (pended with the query).
fn selected(rows: &[usize], watcher: bool) -> Selected {
    let first = rows.contains(&0);
    let single = (rows.len() == 1).then(|| {
        let mut pages: Vec<SeedStatus> = if watcher {
            Vec::new()
        } else {
            vec![SeedStatus::Unknown]
        };
        let files = if first {
            pages.extend([SeedStatus::SuccessfulAndNew, SeedStatus::Error]);
            counts(&[
                SeedStatus::SuccessfulAndNew,
                SeedStatus::Error,
                SeedStatus::Vetoed,
                SeedStatus::Unknown,
            ])
        } else {
            StatusCounts::new()
        };
        let pages = counts(&pages);
        Single {
            presentation: None,
            files: file_log::LogFacts {
                len: files.values().sum(),
                counts: files,
                urls: true,
            },
            searches: search_log::LogFacts {
                len: pages.values().sum(),
                counts: pages,
                last_failed: first,
                read_only: watcher,
                can_generate_more_pages: !watcher,
            },
        }
    });
    Selected {
        count: rows.len(),
        single,
        any_failed: first,
        any_ignored: first,
    }
}

#[test]
fn the_list_menus_are_the_references() {
    let recorded = recorded();
    for (kind, watcher) in [("gallery", false), ("watcher", true)] {
        for case in recorded[kind].as_array().unwrap() {
            let rows: Vec<usize> = case["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| usize::try_from(r.as_u64().unwrap()).unwrap())
                .collect();
            let sel = selected(&rows, watcher);
            let menu = if watcher {
                watcher_menu(&sel)
            } else {
                gallery_menu(&sel)
            };
            assert_eq!(tree(&menu), case["menu"], "{kind} {rows:?}");
        }
    }
    assert!(gallery_menu(&Selected::default()).is_empty());
}

#[test]
fn presentation_summaries_are_the_references() {
    for case in recorded()["summaries"].as_array().unwrap() {
        let options = PresentationOptions {
            status: match case["status"].as_i64().unwrap() {
                0 => PresentationStatus::AnyGood,
                1 => PresentationStatus::NewOnly,
                _ => PresentationStatus::None,
            },
            inbox: match case["inbox"].as_i64().unwrap() {
                0 => PresentationInbox::Agnostic,
                1 => PresentationInbox::RequireInbox,
                _ => PresentationInbox::AndIncludeAllInbox,
            },
            location: vec![hex::encode(if case["location"] == "storage" {
                builtin_keys::HYDRUS_LOCAL_FILE_STORAGE
            } else {
                builtin_keys::COMBINED_LOCAL_FILE_DOMAINS
            })],
        };
        assert_eq!(presentation_summary(&options), case["downloader"], "{case}");
    }
}

#[test]
fn an_importers_own_presentation_is_named_and_not_offered_again() {
    let own = PresentationOptions {
        status: PresentationStatus::NewOnly,
        ..PresentationOptions::default()
    };
    let mut sel = selected(&[1], false);
    sel.single.as_mut().unwrap().presentation = Some(own);
    let menu = tree(&gallery_menu(&sel));
    assert_eq!(
        menu[2]["entries"],
        json!([
            "default presented files (presenting new files)",
            "presenting inbox files",
            "presenting all files",
            "presenting all files, including if trashed"
        ])
    );
}
