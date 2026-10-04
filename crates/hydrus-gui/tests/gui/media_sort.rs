//! Pages sort their files as the reference's pages do, against
//! `oracle/fixtures/media_sort.json` (made by `oracle/record_media_sort.py`):
//! the options' fallback sort first, then the sort chosen (a system
//! sort, by namespaces' tags, or by rating), each stably, with the page's
//! defaults for files with no value; and the options'
//! default, fallback and namespace sorts come across from hydrus.

use std::sync::Arc;

use serde_json::Value;

use hydrus_core::HashId;
use hydrus_core::pages::{PageSort, PageSortBy, SortSettings};
use hydrus_gui::SearchPage;
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
        tag_context: hydrus_core::search::context::TagContext::default(),
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

#[test]
fn the_sort_options_come_across_from_hydrus() {
    let recorded = hydrus_testkit::fixture_json("media_sort.json");
    let fixture = fixture();
    let sorts: SortSettings = fixture.store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(sorts.default_sort, page_sort(&recorded["default_sort"]));
    assert_eq!(sorts.fallback_sort, page_sort(&recorded["fallback_sort"]));
    let namespace_sorts: Vec<PageSort> = recorded["namespace_sorts"]
        .as_array()
        .unwrap()
        .iter()
        .map(page_sort)
        .collect();
    assert_eq!(sorts.namespace_sorts, namespace_sorts);
    // and a new page sorts by the default sort
    let page = SearchPage::new(fixture.store.clone());
    assert_eq!(page.sort().clone(), sorts.default_sort);
}

#[test]
fn pages_sort_as_the_reference_s_pages_sort() {
    let recorded = hydrus_testkit::fixture_json("media_sort.json");
    let fixture = fixture();
    let store = &fixture.store;
    let ids = |hashes: &Value| -> Vec<HashId> {
        hashes
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                store
                    .read(|c| {
                        hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap())
                    })
                    .unwrap()
                    .unwrap()
            })
            .collect()
    };
    let mut checked = 0;
    for page in recorded["pages"].as_array().unwrap() {
        let location = LocationContext::single(hydrus_core::ServiceKey::new(unhex(
            page["service_key"].as_str().unwrap(),
        )));
        let files = ids(&page["files"]);
        for case in page["sorts"].as_array().unwrap() {
            let sort = page_sort(&case["sort"]);
            // a page showing the files as the reference's did, sorted anew
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
            ours.set_sort_type(sort.by.clone());
            ours.set_sort_order(if sort.ascending {
                SortOrder::Ascending
            } else {
                SortOrder::Descending
            });
            assert_eq!(
                ours.results(),
                ids(&case["files"]).as_slice(),
                "{} {:?}",
                page["page"],
                case["sort"]
            );
            checked += 1;
        }
    }
    // every system sort but random, three namespace sorts and three
    // rating services, both ways, on two pages
    assert_eq!(checked, 136);
}

#[test]
fn the_sort_control_offers_namespace_and_rating_sorts() {
    use hydrus_gui::{MainWindow, Pages, bind, headless};
    use slint::Model as _;

    let fixture = fixture();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(fixture.store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let names: Vec<String> = {
        let names = ui.get_sort_names();
        (0..names.row_count())
            .map(|i| names.row_data(i).unwrap().to_string())
            .collect()
    };
    // after the system sorts, the options' namespace sorts, then each
    // rating service, by name
    let at = |name: &str| {
        names
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("{name} in {names:?}"))
    };
    let namespaces = at("tags: series-creator-title-volume-chapter-page");
    assert_eq!(
        names[namespaces..],
        [
            "tags: series-creator-title-volume-chapter-page",
            "tags: creator-series-title-volume-chapter-page",
            "rating: counter",
            "rating: favourites",
            "rating: stars",
        ]
    );
    let orders = |ui: &MainWindow| -> Vec<String> {
        let orders = ui.get_order_names();
        (0..orders.row_count())
            .map(|i| orders.row_data(i).unwrap().to_string())
            .collect()
    };
    // a namespace sort: a-z first
    ui.invoke_sort_chosen(i32::try_from(namespaces).unwrap());
    let page = bound.current.borrow().clone();
    assert_eq!(
        page.borrow().sort().by,
        PageSortBy::Namespaces {
            namespaces: ["series", "creator", "title", "volume", "chapter", "page"]
                .map(String::from)
                .to_vec(),
            tag_display_type: 1,
        }
    );
    assert!(page.borrow().sort().ascending);
    assert_eq!(orders(&ui), ["a-z", "z-a"]);
    assert_eq!(ui.get_order_index(), 0);
    // a rating: descending first
    ui.invoke_sort_chosen(i32::try_from(at("rating: stars")).unwrap());
    assert!(matches!(page.borrow().sort().by, PageSortBy::Rating(_)));
    assert!(!page.borrow().sort().ascending);
    assert_eq!(orders(&ui), ["ascending", "descending"]);
    assert_eq!(ui.get_order_index(), 1);
    ui.invoke_order_chosen(0);
    assert!(page.borrow().sort().ascending);
    // and a sort the options don't offer (a session's custom one) is listed
    // for the page sorted by it
    page.borrow_mut().set_sort_type(PageSortBy::Namespaces {
        namespaces: vec!["page".into()],
        tag_display_type: 1,
    });
    ui.invoke_order_chosen(0);
    let names = ui.get_sort_names();
    assert_eq!(
        names
            .row_data(usize::try_from(ui.get_sort_index()).unwrap())
            .unwrap(),
        "tags: page"
    );
}
