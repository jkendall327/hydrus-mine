//! Pages collect their files as the reference's pages do, against
//! `oracle/fixtures/media_collect.json` (made by
//! `oracle/record_media_collect.py`), and the collect control offers what
//! the reference's does: the namespaces of the options' namespace sorts,
//! then the star rating services, and whether unmatched files collect.

use std::sync::Arc;

use serde_json::Value;
use slint::Model as _;

use hydrus_core::HashId;
use hydrus_core::pages::{PageCollect, PageSort, PageSortBy};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext, SortOrder};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

struct Fixture {
    _legacy: tempfile::TempDir,
    _native: tempfile::TempDir,
    store: Arc<Store>,
}

fn fixture() -> Fixture {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    Fixture {
        _legacy: legacy,
        _native: native,
        store,
    }
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// A recorded sort as we keep one.
fn page_sort(recorded: &Value) -> PageSort {
    let data = &recorded["data"];
    PageSort {
        by: match recorded["type"].as_str().unwrap() {
            "system" => PageSortBy::System(data.as_i64().unwrap()),
            "namespaces" => PageSortBy::Namespaces {
                namespaces: data["namespaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_str().unwrap().to_owned())
                    .collect(),
                tag_display_type: data["tag_display_type"].as_i64().unwrap(),
            },
            "rating" => {
                PageSortBy::Rating(hydrus_core::ServiceKey::new(unhex(data.as_str().unwrap())))
            }
            other => panic!("{other}"),
        },
        ascending: recorded["order"] == 0,
    }
}

/// A recorded collect as we keep one.
fn page_collect(recorded: &Value) -> PageCollect {
    PageCollect {
        namespaces: recorded["namespaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect(),
        ratings: recorded["ratings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| hydrus_core::ServiceKey::new(unhex(k.as_str().unwrap())))
            .collect(),
        collect_unmatched: recorded["collect_unmatched"].as_bool().unwrap(),
        tag_context: hydrus_core::search::context::TagContext::default(),
    }
}

/// A page's items as the oracle records them: a file's hash, or a
/// collection's files' hashes.
fn items(store: &Store, page: &SearchPage) -> Value {
    let hex = |files: &[HashId]| -> Vec<Value> {
        let hashes = store
            .read(|c| hydrus_store::master::hashes(c, files))
            .unwrap();
        files
            .iter()
            .map(|f| Value::String(hashes[f].to_string()))
            .collect()
    };
    page.results()
        .iter()
        .map(|&item| match page.collection(item) {
            Some(files) => Value::Array(hex(files)),
            None => hex(&[item]).remove(0),
        })
        .collect()
}

#[test]
fn pages_collect_as_the_reference_s_pages_collect() {
    let recorded = hydrus_testkit::fixture_json("media_collect.json");
    let fixture = fixture();
    let store = &fixture.store;
    let files: Vec<HashId> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            store
                .read(|c| hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap()))
                .unwrap()
                .unwrap()
        })
        .collect();
    let location = LocationContext::single(hydrus_core::ServiceKey::new(unhex(
        recorded["service_key"].as_str().unwrap(),
    )));
    let mut checked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let collect = page_collect(&case["collect"]);
        // a page showing the files as the reference's did, collected, then
        // sorted each way (the collections staying as collected)
        let mut ours = SearchPage::restored(
            store.clone(),
            FileSearchContext {
                location: location.clone(),
                ..FileSearchContext::default()
            },
            true,
            None,
            files.clone(),
        );
        ours.set_collect(collect.clone());
        for sorted in case["sorts"].as_array().unwrap() {
            let sort = page_sort(&sorted["sort"]);
            ours.set_sort_type(sort.by.clone());
            ours.set_sort_order(if sort.ascending {
                SortOrder::Ascending
            } else {
                SortOrder::Descending
            });
            assert_eq!(
                items(store, &ours),
                sorted["media"],
                "{:?} {:?}",
                case["collect"],
                sorted["sort"]
            );
            checked += 1;
        }
    }
    // ten collects, by every system sort but random, the options' two
    // namespace sorts and three rating services, both ways
    assert_eq!(checked, 640);
}

#[test]
fn the_collect_control_collects_the_page() {
    let fixture = fixture();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(fixture.store.clone())));
    ui.invoke_columns_changed(4);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().files().len();
    assert!(files > 0);

    // the namespaces of the options' two namespace sorts, then the
    // like/dislike and numerical rating services (not inc/dec)
    let choices = || -> Vec<(String, bool)> {
        let rows = ui.get_collect_choices();
        (0..rows.row_count())
            .map(|i| {
                let row = rows.row_data(i).unwrap();
                (row.name.to_string(), row.checked)
            })
            .collect()
    };
    let names: Vec<String> = choices().into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "chapter",
            "creator",
            "page",
            "series",
            "title",
            "volume",
            "favourites",
            "stars"
        ]
    );
    assert_eq!(ui.get_collect_label(), "no collections");
    assert!(page.borrow().results().len() == files);

    // collect by series: every item a collection, unmatched files together
    let at = |name: &str| i32::try_from(names.iter().position(|n| n == name).unwrap()).unwrap();
    ui.invoke_collect_toggled(at("series"), true);
    assert_eq!(page.borrow().collect().namespaces, ["series"]);
    assert_eq!(ui.get_collect_label(), "collect by series");
    assert!(choices().contains(&("series".into(), true)));
    let collected = |ui: &MainWindow| {
        let page = page.borrow();
        let items = page.results().len();
        let collections = page
            .results()
            .iter()
            .filter(|&&item| page.collection(item).is_some())
            .count();
        assert_eq!(page.files().len(), files, "{}", ui.get_status());
        (items, collections)
    };
    let (items, collections) = collected(&ui);
    assert!(items < files);
    assert_eq!(collections, items);
    let status = ui.get_status().to_string();
    assert!(
        status.contains(&format!("in {collections} collection")),
        "{status}"
    );
    // the grid badges each collection with its number of files
    let badges = || -> Vec<String> {
        let rows = ui.get_thumbnail_rows();
        (0..rows.row_count())
            .flat_map(|r| {
                let row = rows.row_data(r).unwrap();
                (0..row.thumbnails.row_count())
                    .map(move |i| row.thumbnails.row_data(i).unwrap().files.to_string())
            })
            .collect()
    };
    let expected: Vec<String> = {
        let page = page.borrow();
        page.results()
            .iter()
            .map(|&item| {
                page.collection(item)
                    .map_or(String::new(), |c| c.len().to_string())
            })
            .collect()
    };
    assert_eq!(badges(), expected);
    assert!(badges().iter().all(|b| !b.is_empty()));

    // unmatched files left separate: they are single files
    ui.invoke_collect_unmatched_chosen(false);
    assert!(!page.borrow().collect().collect_unmatched);
    assert!(ui.get_collect_separate());
    assert!(!ui.get_collect_unmatched());
    let (separate_items, separate_collections) = collected(&ui);
    assert!(separate_collections < separate_items);
    assert!(badges().iter().any(String::is_empty));
    assert_eq!(page.borrow().collect().namespaces, ["series"]);

    // the media viewer opens over all the page's files, from a
    // collection's first
    let (index, first) = {
        let page = page.borrow();
        page.results()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| page.collection(**item).is_some())
            .map(|(i, &item)| (i, item))
            .unwrap()
    };
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let position = page
        .borrow()
        .files()
        .iter()
        .position(|&f| f == first)
        .unwrap();
    assert_eq!(viewer.get_caption(), format!("{}/{files}", position + 1));

    // and selecting a collection selects its files
    ui.invoke_select_all();
    assert_eq!(page.borrow().selected_files().len(), files);

    // and a sort keeps the selection on the collections
    let sorts = ui.get_sort_names().row_count();
    ui.invoke_sort_chosen(i32::try_from(sorts).unwrap() - 1);
    assert_eq!(page.borrow().selected_files().len(), files);
    assert_eq!(page.borrow().selected_items().len(), separate_items);

    // unchecked: no collections, and (as collecting does) nothing selected
    ui.invoke_collect_toggled(at("series"), false);
    assert_eq!(ui.get_collect_label(), "no collections");
    assert!(!page.borrow().collect().collects());
    assert_eq!(page.borrow().results().len(), files);
    assert!(badges().iter().all(String::is_empty));
    assert!(page.borrow().selected_files().is_empty());
    assert!(page.borrow().focused().is_none());
}

#[test]
fn a_page_opened_from_a_collected_page_collects_as_it_did() {
    let fixture = fixture();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(fixture.store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    page.borrow_mut().set_collect(PageCollect {
        namespaces: vec!["creator".into()],
        ratings: Vec::new(),
        collect_unmatched: false,
        tag_context: hydrus_core::search::context::TagContext::default(),
    });
    let files = page.borrow().files();
    bound.pages.borrow_mut().open_files(
        page.borrow().location().clone(),
        files.clone(),
        None,
        Some(page.borrow().collect()),
    );
    let opened = bound.pages.borrow_mut().current();
    let opened = opened.borrow();
    assert_eq!(opened.collect(), page.borrow().collect());
    assert_eq!(opened.results().len(), page.borrow().results().len());
    let mut ours = opened.files();
    let mut theirs = files;
    ours.sort_unstable();
    theirs.sort_unstable();
    assert_eq!(ours, theirs);
}

#[test]
fn a_session_page_keeps_how_it_collects() {
    use hydrus_core::pages::{Page, PageContent, PageKey, Session};
    use hydrus_store::sessions::{self, LAST_SESSION};

    let fixture = fixture();
    let store = &fixture.store;
    let files: Vec<HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    let by_series = PageCollect {
        namespaces: vec!["series".into()],
        ratings: Vec::new(),
        collect_unmatched: false,
        tag_context: hydrus_core::search::context::TagContext::default(),
    };
    // a page collecting by series, and one collecting nothing (though
    // leaving unmatched files separate)
    let page = |name: &str, collect: PageCollect| Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: Some(collect),
        },
    };
    let collected = page("collected", by_series.clone());
    let single = page(
        "single",
        PageCollect {
            collect_unmatched: false,
            ..PageCollect::default()
        },
    );
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![collected.clone(), single.clone()],
    };
    let (keys, kept) = ([collected.key, single.key], files.clone());
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 0)?;
            for key in &keys {
                sessions::set_page_files(ctx.conn(), key, &kept)?;
            }
            Ok(())
        })
        .unwrap();

    let mut pages = Pages::open(store.clone()).unwrap();
    {
        let opened = pages.current();
        let opened = opened.borrow();
        assert_eq!(opened.collect(), &by_series);
        // collected (and sorted) on opening, as the reference's page does
        assert!(opened.results().len() < files.len());
        assert_eq!(opened.files().len(), files.len());
    }
    pages.select(0, 1);
    {
        let opened = pages.current();
        let opened = opened.borrow();
        assert!(!opened.collect().collects());
        assert!(!opened.collect().collect_unmatched);
        // as kept
        assert_eq!(opened.results(), files);
    }
    pages.save(1).unwrap();
    let saved = store
        .read(|conn| sessions::load(conn, LAST_SESSION))
        .unwrap()
        .unwrap();
    let collects: Vec<Option<PageCollect>> = saved
        .pages
        .iter()
        .map(|p| match &p.content {
            PageContent::Search { collect, .. } => collect.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(
        collects,
        [
            Some(by_series),
            Some(PageCollect {
                collect_unmatched: false,
                ..PageCollect::default()
            })
        ]
    );
}

#[test]
fn restoring_and_refreshing_searches_preserves_absent_and_explicit_collect_settings() {
    use hydrus_core::pages::PageContent;
    let f = fixture();
    for collect in [None, Some(PageCollect::default())] {
        let original = PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: collect.clone(),
        };
        let mut page = SearchPage::restored(
            f.store.clone(),
            FileSearchContext::default(),
            true,
            None,
            Vec::new(),
        )
        .with_collect(collect);
        assert_eq!(page.content(&original), original);
        page.refresh();
        assert_eq!(page.content(&original), original);
        // An explicit collect action is serialized even when no files are shown.
        let changed = PageCollect {
            namespaces: vec!["series".into()],
            collect_unmatched: false,
            ..PageCollect::default()
        };
        page.set_collect(changed.clone());
        let PageContent::Search { collect, .. } = page.content(&original) else {
            panic!("a restored search remains a search");
        };
        assert_eq!(collect, Some(changed));
    }
}
