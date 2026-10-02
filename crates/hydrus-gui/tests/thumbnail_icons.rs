//! The icons over thumbnails against the reference's
//! (`oracle/record_thumbnail_icons.py`): for every file of the `basic`
//! fixture, and its files collected by series, the same icons (inbox,
//! trash, notes, sound, play, collection...) where the reference draws
//! them; and the grid's thumbnails carrying them.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use slint::Model as _;

use hydrus_core::{HashId, Sha256};
use hydrus_gui::thumbnail_icons::{self, IconFacts, placed};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn file(store: &Store, hex: &str) -> HashId {
    let hash: Sha256 = hex.parse().unwrap();
    store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap_or_else(|| panic!("no file {hex}"))
}

fn icons_json(icons: &[thumbnail_icons::Placed]) -> Json {
    icons
        .iter()
        .map(|p| json!({ "name": p.icon.name(), "x": p.x, "y": p.y }))
        .collect()
}

#[test]
fn the_icons_are_the_references() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("thumbnail_icons.json");
    let border = i32::try_from(recorded["thumbnail_border"].as_i64().unwrap()).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for page in ["files", "collected_by_series"] {
        for thumbnail in recorded[page].as_array().unwrap() {
            let files: Vec<HashId> = thumbnail["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| file(&store, h.as_str().unwrap()))
                .collect();
            let facts = thumbnail_icons::facts(&store, &files);
            let collection = files.len() > 1;
            let facts = if collection {
                let members: Vec<IconFacts> = files.iter().map(|f| facts[f].clone()).collect();
                IconFacts::of_collection(&members)
            } else {
                facts[&files[0]].clone()
            };
            let size = &thumbnail["size"];
            let width = i32::try_from(size[0].as_i64().unwrap()).unwrap();
            let height = i32::try_from(size[1].as_i64().unwrap()).unwrap();
            let ours = placed(&facts, collection, border, width, height, border);
            assert_eq!(
                icons_json(&ours),
                thumbnail["icons"],
                "{page} {}",
                thumbnail["files"]
            );
            seen.extend(ours.iter().map(|p| p.icon.name()));
        }
    }
    // (the fixture has each of these)
    for name in ["inbox", "trash", "notes", "sound", "play", "collection"] {
        assert!(seen.contains(name), "{name} {seen:?}");
    }
}

/// The grid's thumbnails carry their icons, read for the rows shown, and
/// read again when the files change (archived, here).
#[test]
fn the_grid_draws_each_thumbnail_s_icons() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let first = page.borrow().results()[0];
    let row = bound.rows.row_data(0).unwrap();
    let icons = row.thumbnails.row_data(0).unwrap().icons;
    let kinds: Vec<i32> = (0..icons.row_count())
        .map(|i| icons.row_data(i).unwrap().kind)
        .collect();
    assert!(
        kinds.contains(&thumbnail_icons::Icon::Inbox.code()),
        "{kinds:?}"
    );
    let inbox = icons
        .iter()
        .find(|i| i.kind == thumbnail_icons::Icon::Inbox.code())
        .unwrap();
    // the default thumbnail, 152 wide with its border: a pixel in from it
    assert_eq!((inbox.x, inbox.y), (134.0, 1.0));
    // archived: no inbox icon once the rows are shown again
    hydrus_gui::media_actions::archive(&store, &[first]).unwrap();
    bound.rows.reset();
    let row = bound.rows.row_data(0).unwrap();
    let icons = row.thumbnails.row_data(0).unwrap().icons;
    assert!(
        icons
            .iter()
            .all(|i| i.kind != thumbnail_icons::Icon::Inbox.code())
    );
}
