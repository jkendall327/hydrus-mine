//! Pages sort their files as the reference's pages do, against
//! `oracle/fixtures/media_sort.json` (made by `oracle/record_media_sort.py`):
//! the options' fallback sort first, then the sort chosen, each stably,
//! with the page's defaults for files with no value; and the options'
//! default, fallback and namespace sorts come across from hydrus.

use std::sync::Arc;

use serde_json::Value;

use hydrus_core::HashId;
use hydrus_core::pages::{PageSort, PageSortBy, SortSettings};
use hydrus_gui::SearchPage;
use hydrus_search::{FileSearchContext, LocationContext, SortBy, SortOrder};
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
    assert_eq!(page.page_sort(), sorts.default_sort);
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
            let PageSortBy::System(code) = sort.by else {
                continue;
            };
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
            ours.set_sort_by(SortBy::from_code(code).unwrap());
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
    assert_eq!(checked, 108);
}
