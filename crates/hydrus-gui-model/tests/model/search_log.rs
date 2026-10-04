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

#[test]
fn exchange_questions_replay_actual_reference_answers_and_complete_objects() {
    use hydrus_gui_model::search_log::{ImportStep, export_objects};
    let recorded = hydrus_testkit::fixture_json("search_log_exchange.json");
    let classes = hydrus_core::url::UrlClasses::default();
    let existing = GallerySeed {
        id: 1,
        queue_id: 1,
        url: "https://gallery-exchange.example/a".into(),
        can_generate_more_pages: true,
        created: 0,
        modified: 0,
        status: SeedStatus::Error,
        note: "old failure".into(),
        referral_url: None,
        meta: GallerySeedMeta::default(),
    };
    let mut q = 0;
    for case in recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["text"].is_string())
    {
        let mut step = ImportStep::start(
            case["text"].as_str().unwrap(),
            std::slice::from_ref(&existing),
            &classes,
            true,
        );
        for answer in case["answers"].as_array().unwrap() {
            let (text, choices) = step.question().unwrap();
            assert_eq!(json!(text), recorded["questions"][q]["text"]);
            assert_eq!(json!(choices), recorded["questions"][q]["choices"]);
            step = step.answer(
                match answer.as_str().unwrap() {
                    "yes" => 0,
                    "no" => 1,
                    "cancel" => -1,
                    _ => unreachable!(),
                },
                true,
            );
            q += 1;
        }
        let mut rows = vec![json!([existing.url, true, 4, "old failure"])];
        if let ImportStep::Ready { urls, more } = step {
            let mut seen = std::collections::BTreeSet::new();
            rows.extend(urls.into_iter().filter_map(|url| {
                let url = classes.normalise(&url, true).unwrap_or(url);
                if seen.insert(url.clone()) {
                    Some(json!([url, more, 0, ""]))
                } else {
                    None
                }
            }));
        } else {
            assert_eq!(step, ImportStep::Cancelled);
        }
        assert_eq!(json!(rows), case["seeds"], "{}", case["name"]);
    }
    let s = GallerySeed {
        url: recorded["png"]["payload"].as_str().unwrap().into(),
        can_generate_more_pages: false,
        created: 1_700_000_000,
        modified: 1_700_000_100,
        note: "note 日本".into(),
        referral_url: Some("https://gallery-exchange.example/ref".into()),
        meta: GallerySeedMeta {
            request_headers: vec![("X-Synthetic".into(), "header".into())],
            external_filterable_tags: ["filter:tag".into()].into(),
            external_additional_tags: vec![("11".repeat(32), ["extra:tag".into()].into())],
            run_token: "not serialized".into(),
            force_next_page_url_generation: true,
        },
        ..existing
    };
    assert_eq!(json!(export_objects(&[&s]).unwrap()), recorded["objects"]);
    let png = std::fs::read(hydrus_testkit::fixture_path("search_log_urls.png")).unwrap();
    assert_eq!(
        json!(hydrus_downloader_exchange::text_png::decode(&png).unwrap()),
        recorded["png"]["payload"]
    );
}
