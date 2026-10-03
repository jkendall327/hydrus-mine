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

/// The tag banners' texts against the reference's: the top one drawn a
/// pixel under the border, the bottom right one the other text that isn't
/// a collection's count.
#[test]
fn the_banners_are_the_references() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("thumbnail_icons.json");
    let border = recorded["thumbnail_border"].as_i64().unwrap();
    let summaries = hydrus_core::tag_summary::TagSummaries::default();
    let mut banners = 0;
    for page in ["files", "collected_by_series"] {
        for thumbnail in recorded[page].as_array().unwrap() {
            let files: Vec<HashId> = thumbnail["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| file(&store, h.as_str().unwrap()))
                .collect();
            let count = (files.len() > 1).then(|| files.len().to_string());
            let mut top = String::new();
            let mut bottom = String::new();
            for text in thumbnail["texts"].as_array().unwrap() {
                let said = text["text"].as_str().unwrap().to_owned();
                if text["y"] == border + 1 {
                    top = said;
                } else if Some(&said) != count.as_ref() {
                    bottom = said;
                }
            }
            banners += usize::from(!top.is_empty()) + usize::from(!bottom.is_empty());
            assert_eq!(
                thumbnail_icons::banners(&store, &files, &summaries),
                (top, bottom),
                "{page} {}",
                thumbnail["files"]
            );
        }
    }
    assert!(banners > 10, "{banners}");
}

/// The banners show the tags a single file shows: hiding the series from
/// single files (in every tag service) takes them out of the top banner;
/// hiding them from the selection list doesn't.
#[test]
fn the_banners_obey_the_single_file_tag_display_filter() {
    use hydrus_core::ServiceKey;
    use hydrus_core::service::builtin_keys;
    use hydrus_core::tag_filter::{FilterRule, TagFilter};
    use hydrus_store::tag_display::{TagDisplayFilters, TagView};
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("thumbnail_icons.json");
    let summaries = hydrus_core::tag_summary::TagSummaries::default();
    // a file whose top banner has its series
    let (files, before) = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|t| {
            let files = vec![file(&store, t["files"][0].as_str().unwrap())];
            let (top, _) = thumbnail_icons::banners(&store, &files, &summaries);
            top.contains("metroid").then_some((files, top))
        })
        .expect("a file of series metroid");
    let hide_series = |view: TagView| {
        let mut filters = TagDisplayFilters::default();
        filters.for_view_mut(view).insert(
            ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()).to_hex(),
            TagFilter::new().with_rule("series:", FilterRule::Blacklist),
        );
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &filters))
            .unwrap();
    };
    hide_series(TagView::SelectionList);
    assert_eq!(
        thumbnail_icons::banners(&store, &files, &summaries).0,
        before
    );
    hide_series(TagView::SingleMedia);
    let (after, _) = thumbnail_icons::banners(&store, &files, &summaries);
    assert!(!after.contains("metroid"), "{after}");
}

/// The banners summarise pending tags too: the `repositories` fixture's
/// file with "pended tag" pending to its tag repository shows it, with a
/// summary of unnamespaced tags.
#[test]
fn the_banners_show_pending_tags() {
    use hydrus_core::tag_summary::{NamespaceInfo, TagSummaries};
    let legacy = hydrus_testkit::legacy_fixture("repositories");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut summaries = TagSummaries::default();
    summaries.thumbnail_top.namespace_info = vec![NamespaceInfo {
        namespace: String::new(),
        prefix: String::new(),
        separator: ", ".into(),
    }];
    let pended = file(
        &store,
        "03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b",
    );
    let (top, _) = thumbnail_icons::banners(&store, &[pended], &summaries);
    assert!(top.contains("pended tag"), "{top}");
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
    // and its tag banners
    let thumbnail = row.thumbnails.row_data(0).unwrap();
    let (top, bottom) = thumbnail_icons::banners(
        &store,
        &[first],
        &hydrus_core::tag_summary::TagSummaries::default(),
    );
    assert_eq!(
        (thumbnail.top.as_str(), thumbnail.bottom.as_str()),
        (top.as_str(), bottom.as_str())
    );
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

/// A collection's banners summarise all its files' tags.
#[test]
fn a_collection_s_banners_say_what_all_its_files_do() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    // a collection's banners: all its files'
    page.borrow_mut()
        .set_collect(hydrus_core::pages::PageCollect {
            namespaces: vec!["series".into()],
            ratings: Vec::new(),
            collect_unmatched: false,
        });
    bound.rows.reset();
    let summaries = hydrus_core::tag_summary::TagSummaries::default();
    // (one whose files say different things, so taking its first file's
    // alone would show)
    let (at, top) = {
        let page = page.borrow();
        page.results()
            .iter()
            .enumerate()
            .find_map(|(i, &item)| {
                let members = page.collection(item)?.to_vec();
                let (top, _) = thumbnail_icons::banners(&store, &members, &summaries);
                let (first, _) = thumbnail_icons::banners(&store, &members[..1], &summaries);
                (top != first).then_some((i, top))
            })
            .expect("a collection whose files differ")
    };
    let columns = usize::try_from(ui.get_grid_columns()).unwrap().max(1);
    let collected = bound
        .rows
        .row_data(at / columns)
        .unwrap()
        .thumbnails
        .row_data(at % columns)
        .unwrap();
    assert_eq!(collected.top.as_str(), top);
}
