//! Flow (`oracle/record_flow_tag_relationships.py`): a sibling (`doggo` →
//! `dog`) and a parent (`dog` → `animal`) added on "my tags" through the
//! tags menu's windows; one file tagged `doggo` and another `dog`; searches
//! for `dog`, `animal` and `doggo`. After each step what the user sees must
//! be what the reference showed: the windows' rows and status, each file's
//! tags as stored and as displayed, the search results, the sidebar's tag
//! list and the autocomplete's rows with their counts.

use std::collections::BTreeSet;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Sha256;
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, TagRelationshipsWindow, bind, headless};
use hydrus_store::Store;

use super::{Known, compare};
use crate::common::{main_menu, widgets};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[Known {
    // the reference words its background sibling and parent sync ("are fully
    // synced", "set to sync all the time in the background"); hydrus-rs
    // applies them as it writes
    path: "/dialog/sync status",
    why: "Tags > sync: the siblings and parents windows' status line",
}];

/// tags > siblings or parents, opened through the menu.
fn open(ui: &MainWindow, bound: &Bound, kind: &str) -> TagRelationshipsWindow {
    let lines = main_menu::open(ui, &["tags"]);
    let label = lines
        .iter()
        .find(|l| l.0.starts_with(kind))
        .unwrap_or_else(|| panic!("tags > {kind}"))
        .0
        .clone();
    main_menu::choose(ui, &["tags"], &label);
    bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn sha(hex_text: &str) -> Sha256 {
    Sha256::from_slice(&hex::decode(hex_text).unwrap()).unwrap()
}

#[test]
fn siblings_and_parents_reach_tags_and_search_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_tag_relationships.json");
    let (_dirs, store) = super::basic_store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let tagged: Vec<String> = recording["tagged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let mine = store.snapshot().services.by_name("my tags").unwrap().id;

    // a search page of every file
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();

    let hash_id = |hash: &str| {
        let sha = sha(hash);
        store
            .read(move |c| hydrus_store::master::hash_id(c, &sha))
            .unwrap()
            .unwrap()
    };
    let position = |hash: &str| {
        let id = hash_id(hash);
        let page = bound.current.borrow();
        let page = page.borrow();
        i32::try_from(page.results().iter().position(|&r| r == id).unwrap()).unwrap()
    };
    // a file's tags in my tags: as stored, and as displayed (its ideals
    // and their parents)
    let tags_of = |hash: &str| -> Json {
        let id = hash_id(hash);
        let snapshot = store.snapshot();
        store
            .read(|conn| {
                let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
                let tags = batch.results[0]
                    .tags
                    .get(&mine)
                    .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                    .cloned()
                    .unwrap_or_default();
                let graph = snapshot.display.get(mine);
                let mut stored = BTreeSet::new();
                let mut displayed = BTreeSet::new();
                for t in tags {
                    stored.insert(batch.tags[&t].as_str().to_owned());
                    for d in graph.display_tags(t) {
                        displayed.insert(
                            hydrus_store::master::tag(conn, d)?
                                .unwrap()
                                .as_str()
                                .to_owned(),
                        );
                    }
                }
                Ok(json!({"stored": stored, "displayed": displayed}))
            })
            .unwrap()
    };
    let step = |name: &str, dialog: Json, extra: Json| -> Json {
        let mut step = json!({
            "step": name,
            "dialog": dialog,
            "files": tagged.iter().map(|h| (h.clone(), tags_of(h))).collect::<serde_json::Map<_, _>>(),
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            step[k] = v;
        }
        step
    };
    // the sidebar's rows for this flow's tags
    let mine_tags = ["dog", "doggo", "animal"];
    let sidebar = || -> Json {
        let mut rows: Vec<(String, String)> = ui
            .get_tags()
            .iter()
            .filter_map(|r| {
                let text = r.text.to_string();
                let tag = text.split(" (").next().unwrap().to_owned();
                mine_tags.contains(&tag.as_str()).then_some((tag, text))
            })
            .collect();
        rows.sort();
        Json::Array(
            rows.into_iter()
                .map(|(tag, text)| json!({"tag": tag, "rows": [text]}))
                .collect(),
        )
    };
    let result_hashes = || -> Json {
        let ids = bound.current.borrow().borrow().results().to_vec();
        let mut hashes: Vec<String> = store
            .read(|c| hydrus_store::master::hashes(c, &ids))
            .unwrap()
            .values()
            .map(|h| hex::encode(h.as_bytes()))
            .collect();
        hashes.sort();
        json!(hashes)
    };
    let mut steps = Vec::new();

    // 1. siblings and 2. parents: the windows, "my tags", a pair added, apply
    for (kind, name, left, right) in [
        ("siblings", "siblings", "doggo", "dog"),
        ("parents", "parents", "dog", "animal"),
    ] {
        let w = open(&ui, &bound, kind);
        let services = w.get_service_names();
        let at = (0..services.row_count())
            .find(|&i| services.row_data(i).unwrap() == "my tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        widgets::settle();
        let seen_services: Vec<String> = services.iter().map(|s| s.to_string()).collect();
        let sync_status = w.get_sync_status().to_string();
        w.invoke_enter_tags(false, left.into());
        w.invoke_enter_tags(true, right.into());
        w.invoke_add();
        widgets::settle();
        let rows: Vec<Vec<String>> = w
            .get_rows()
            .iter()
            .map(|r| r.cells.iter().map(|c| c.to_string()).collect())
            .collect();
        let seen = json!({
            // (without the application's name, which Qt adds to the window)
            "title": w.get_composed_title().split(" - ").next().unwrap(),
            "services": seen_services,
            "sync status": sync_status,
            "rows": rows,
        });
        w.invoke_apply();
        widgets::settle();
        steps.push(step(name, seen, json!({})));
    }

    // 3. tag: the first file selected and `doggo` added, the second `dog`
    for (hash, tag) in [(&tagged[0], "doggo"), (&tagged[1], "dog")] {
        ui.invoke_select_none();
        ui.invoke_thumbnail_clicked(position(hash), false, false);
        ui.invoke_manage_tags_selected();
        let manage = bound
            .manage_tags
            .borrow()
            .as_ref()
            .expect("the manage tags dialog")
            .clone_strong();
        let at = manage
            .get_service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        manage.invoke_service_chosen(i32::try_from(at).unwrap());
        // the tag itself chosen among the suggestions (a sibling's old tag
        // shows as "doggo → dog"; Enter would take the ideal)
        manage.invoke_text_edited(tag.into());
        widgets::settle();
        let suggestions = manage.get_suggestions();
        let exact = suggestions
            .iter()
            .position(|s| s.text == tag || s.text.starts_with(&format!("{tag} \u{2192} ")));
        match exact {
            Some(i) => manage.invoke_suggestion_chosen(i32::try_from(i).unwrap()),
            None => manage.invoke_entered(),
        }
        manage.invoke_apply();
        widgets::settle();
    }
    ui.invoke_select_all();
    widgets::settle();
    steps.push(step("tag", json!({}), json!({"sidebar": sidebar()})));

    // 4. search for each, all its files selected; then what typing offers
    let mut searches = serde_json::Map::new();
    for text in ["dog", "animal", "doggo"] {
        ui.invoke_search_edited(text.into());
        ui.invoke_search_accepted();
        widgets::settle();
        ui.invoke_select_all();
        widgets::settle();
        searches.insert(
            text.to_owned(),
            json!({"hashes": result_hashes(), "sidebar": sidebar()}),
        );
    }
    let mut typed = serde_json::Map::new();
    for text in ["dog", "doggo", "ani"] {
        ui.invoke_search_edited(text.into());
        // (the dropdown fetches after a short pause, as the reference's does)
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while ui.get_suggestions().row_count() == 0 && std::time::Instant::now() < until {
            widgets::settle();
        }
        widgets::settle();
        let rows: Vec<Json> = ui
            .get_suggestions()
            .iter()
            .map(|r| r.text.to_string())
            .filter(|t| mine_tags.iter().any(|m| t.starts_with(m)))
            .map(|t| json!({"predicate": t, "rows": [t]}))
            .collect();
        typed.insert(text.to_owned(), Json::Array(rows));
    }
    steps.push(step(
        "search",
        json!({}),
        json!({"searches": searches, "autocomplete": typed}),
    ));

    compare(&steps, &recording, KNOWN);
    let _: &Store = &store;
}
