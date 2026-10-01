//! Favourite searches: the fixture's comes across from hydrus, shows in
//! the favourites menu, and loading it sets the page's search and sort.

use slint::Model as _;

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

#[test]
fn a_favourite_search_loads_into_the_page() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    let favourite = favourites.0[0].clone();

    // loaded directly: the same files as searching for its predicates
    let mut page = SearchPage::new(store.clone());
    page.load_favourite(&favourite);
    let loaded = page.predicates();
    assert_eq!(loaded.len(), 3, "{loaded:?}");
    assert!(loaded.contains(&"system:inbox".to_owned()), "{loaded:?}");
    let found = page.results().to_vec();
    assert!(!found.is_empty());
    let mut by_hand = SearchPage::new(store.clone());
    for p in &loaded {
        by_hand.add_predicate(p);
    }
    by_hand.set_sort_by(page.sort().by);
    by_hand.set_sort_order(page.sort().order);
    assert_eq!(by_hand.results(), found);
    // its sort: largest file first
    assert_eq!(page.sort().by, hydrus_search::SortBy::FileSize);
    assert_eq!(page.sort().order, hydrus_search::SortOrder::Descending);

    // in the window: the menu, then loading it
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store)));
    let rows: Vec<(String, i32, i32)> = ui
        .get_favourites()
        .iter()
        .map(|r| (r.label.to_string(), r.depth, r.search))
        .collect();
    assert_eq!(
        rows,
        [
            ("example search".to_owned(), 0, -1),
            ("inbox filter".to_owned(), 1, 0)
        ]
    );
    ui.set_favourites_open(true);
    {
        use slint::ComponentHandle as _;
        ui.show().unwrap();
        let window = windows.get(0).unwrap();
        headless::render(&window, 900, 500);
        let pixels = headless::render(&window, 900, 500);
        let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
        headless::save_png(&shots.join("favourites.png"), &pixels, 900, 500).unwrap();
    }
    ui.invoke_favourite_chosen(0);
    let shown: Vec<String> = (0..ui.get_predicates().row_count())
        .map(|i| ui.get_predicates().row_data(i).unwrap().text.to_string())
        .collect();
    assert_eq!(shown, loaded);
    assert_eq!(ui.get_status(), format!("{} files", found.len()));
}
