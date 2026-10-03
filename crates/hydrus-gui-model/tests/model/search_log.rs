//! A gallery search's or watcher's search log against the reference's,
//! recorded by `oracle/record_search_log.py`: each page's row, the whole
//! log's menu for logs of many counts and kinds, the right-click menu on
//! selected rows of a search's and a watcher's log, and the delete
//! question.

use serde_json::{Value as Json, json};

use hydrus_gui_model::file_log::Entry;
use hydrus_gui_model::search_log::{Action, LogFacts, delete_question, log_menu, row, row_menu};
use hydrus_store::queues::{
    GallerySeed, GallerySeedMeta, SeedStatus, StatusCounts, search_log_status,
};

fn status(code: i64) -> SeedStatus {
    SeedStatus::from_code(code).unwrap()
}

fn seed(recorded: &Json, now: i64) -> GallerySeed {
    GallerySeed {
        id: 0,
        queue_id: 0,
        url: recorded[0].as_str().unwrap().into(),
        can_generate_more_pages: true,
        created: now - recorded[2].as_i64().unwrap(),
        modified: now - recorded[3].as_i64().unwrap(),
        status: status(recorded[1].as_i64().unwrap()),
        note: recorded[4].as_str().unwrap().into(),
        referral_url: None,
        meta: GallerySeedMeta::default(),
    }
}

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

#[test]
fn the_search_log_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("search_log.json");
    let now = recorded["now"].as_i64().unwrap();
    let seeds: Vec<GallerySeed> = recorded["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| seed(s, now))
        .collect();
    for (i, (s, want)) in seeds
        .iter()
        .zip(recorded["rows"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(json!(row(s, i, now)), *want, "row {i}");
    }
    let mut counts = StatusCounts::new();
    for s in &seeds {
        *counts.entry(s.status).or_default() += 1;
    }
    assert_eq!(search_log_status(&counts).0, recorded["header"]);

    for menu in recorded["menus"].as_array().unwrap() {
        let mut counts = StatusCounts::new();
        for (code, n) in menu["counts"].as_object().unwrap() {
            counts.insert(status(code.parse().unwrap()), n.as_u64().unwrap() as usize);
        }
        let log = LogFacts {
            len: counts.values().sum(),
            counts,
            last_failed: menu["last_failed"].as_bool().unwrap(),
            read_only: menu["read_only"].as_bool().unwrap(),
            can_generate_more_pages: menu["can_generate"].as_bool().unwrap(),
        };
        assert_eq!(tree(&log_menu(&log, false)), menu["menu"], "{menu}");
    }

    for menu in recorded["row_menus"].as_array().unwrap() {
        let log = LogFacts {
            len: seeds.len(),
            counts: counts.clone(),
            // (the last page, unknown, didn't fail)
            last_failed: false,
            read_only: false,
            can_generate_more_pages: menu["kind"] == "search",
        };
        let selected: Vec<&GallerySeed> = menu["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| &seeds[i.as_u64().unwrap() as usize])
            .collect();
        assert_eq!(
            tree(&row_menu(&selected, &log)),
            menu["menu"],
            "{} {}",
            menu["kind"],
            menu["selected"]
        );
    }

    assert_eq!(
        delete_question(SeedStatus::Error, "search"),
        recorded["delete_question"]
    );
}
