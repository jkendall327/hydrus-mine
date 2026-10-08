//! The manage-favourites list's selection, Add and Edit, and the edit
//! dialog's "searching immediately" switch, as the reference's list and
//! `EditFavouriteSearchPanel` behave (`oracle/record_favourite_searches.py`
//! records the menu, rows, overwrite questions and values; these drive the
//! rest through the real windows and store).

use slint::Model as _;

use hydrus_core::pages::FavouriteSearch;
use hydrus_gui::{
    FavouriteEditWindow, FavouritesWindow, MainWindow, Pages, SearchPage, bind, headless,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::settings::FavouriteSearches;

fn store_with_four() -> ([tempfile::TempDir; 2], std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut rows: FavouriteSearches = store.read(hydrus_store::settings::get).unwrap();
    let first = rows.0[0].clone();
    rows.0.clear();
    for (folder, name, sync) in [
        (None, "delta", true),
        (None, "alpha", false),
        (Some("art"), "charlie", true),
        (Some("art"), "bravo", true),
    ] {
        rows.0.push(FavouriteSearch {
            folder: folder.map(str::to_owned),
            name: name.to_owned(),
            synchronised: sync,
            ..first.clone()
        });
    }
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &rows))
        .unwrap();
    ([legacy, native], store)
}

fn kept(store: &Store) -> Vec<(Option<String>, String, bool)> {
    let rows: FavouriteSearches = store.read(hydrus_store::settings::get).unwrap();
    rows.0
        .into_iter()
        .map(|f| (f.folder, f.name, f.synchronised))
        .collect()
}

fn rows(window: &FavouritesWindow) -> Vec<(String, bool)> {
    window
        .get_rows()
        .iter()
        .map(|r| (r.cells.row_data(1).unwrap().to_string(), r.selected))
        .collect()
}

fn selected(window: &FavouritesWindow) -> Vec<String> {
    rows(window)
        .into_iter()
        .filter(|r| r.1)
        .map(|r| r.0)
        .collect()
}

// leaf: audit-options-favourites-list-selection
// leaf: audit-options-favourites-list-add-edit
#[test]
fn the_list_selects_by_click_ctrl_and_shift_and_adds_and_edits_through_its_child() {
    let (_dirs, store) = store_with_four();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let list = || {
        bound
            .favourites
            .list
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the list open")
    };
    let edit = || -> FavouriteEditWindow {
        bound
            .favourites
            .edit
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the edit dialog open")
    };

    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    // sorted by folder (blank folders first), then as stored
    let names: Vec<String> = rows(&manage).into_iter().map(|r| r.0).collect();
    assert_eq!(names.len(), 4);
    assert!(!manage.get_can_edit() && !manage.get_can_delete());

    // a plain click selects one, ctrl toggles, shift takes the range
    manage.invoke_row_clicked(1, false, false);
    assert_eq!(selected(&manage), [names[1].clone()]);
    assert!(manage.get_can_edit() && manage.get_can_delete());
    manage.invoke_row_clicked(3, true, false);
    assert_eq!(selected(&manage), [names[1].clone(), names[3].clone()]);
    // several cannot be edited, but can be deleted together
    assert!(manage.get_can_delete());
    manage.invoke_edit();
    assert!(
        bound.favourites.edit.borrow().is_none(),
        "edit needs one row"
    );
    manage.invoke_row_clicked(3, true, false);
    assert_eq!(selected(&manage), [names[1].clone()]);
    manage.invoke_row_clicked(0, false, false);
    manage.invoke_row_clicked(2, false, true);
    assert_eq!(
        selected(&manage),
        [names[0].clone(), names[1].clone(), names[2].clone()]
    );
    // the range follows the displayed order after a header sort
    manage.invoke_header_clicked(1);
    let sorted: Vec<String> = rows(&manage).into_iter().map(|r| r.0).collect();
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(sorted, expected);
    manage.invoke_row_clicked(0, false, false);
    manage.invoke_row_clicked(1, false, true);
    assert_eq!(selected(&manage), sorted[..2].to_vec());
    manage.invoke_delete();
    manage.invoke_answered(true);
    assert_eq!(rows(&manage).len(), 2);
    assert!(selected(&manage).is_empty());
    manage.invoke_cancel();
    assert_eq!(kept(&store).len(), 4, "cancel keeps the store");

    // Add opens a new search named so nothing clashes, and returns it to the
    // draft; a second Add is named differently again
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    manage.invoke_add();
    let window = edit();
    let first_name = window.get_name().to_string();
    assert_eq!(first_name, "new favourite search");
    assert_eq!(window.get_folder(), "");
    window.invoke_apply();
    assert!(bound.favourites.edit.borrow().is_none());
    assert_eq!(rows(&manage).len(), 5);
    manage.invoke_add();
    let second = edit().get_name().to_string();
    assert_ne!(second, first_name);
    edit().invoke_cancel();
    assert_eq!(rows(&manage).len(), 5, "cancelled child adds nothing");

    // Edit and a double-click open the selected row; the changed values return
    // to the draft only
    manage.invoke_row_clicked(0, false, false);
    let before = selected(&manage);
    manage.invoke_edit();
    let window = edit();
    assert_eq!(window.get_name().to_string(), before[0]);
    window.set_name("renamed once".into());
    window.invoke_apply();
    assert!(rows(&manage).iter().any(|r| r.0 == "renamed once"));
    assert!(kept(&store).iter().all(|r| r.1 != "renamed once"));
    let double = rows(&manage)
        .iter()
        .position(|r| r.0 == "renamed once")
        .unwrap();
    manage.invoke_row_activated(i32::try_from(double).unwrap());
    assert_eq!(edit().get_name(), "renamed once");
    assert_eq!(selected(&manage), ["renamed once"]);
    edit().invoke_cancel();
    manage.invoke_apply();
    assert_eq!(kept(&store).len(), 5);
    assert!(kept(&store).iter().any(|r| r.1 == "renamed once"));
}

// leaf: audit-options-favourites-edit-sync
#[test]
fn the_immediate_search_switch_is_kept_and_decides_whether_a_loaded_search_runs() {
    let (_dirs, store) = store_with_four();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let list = || {
        bound
            .favourites
            .list
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the list open")
    };
    let edit = || -> FavouriteEditWindow {
        bound
            .favourites
            .edit
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the edit dialog open")
    };
    // "alpha" is stored as not searching immediately
    assert!(!kept(&store).iter().find(|r| r.1 == "alpha").unwrap().2);
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    let at = rows(&manage).iter().position(|r| r.0 == "alpha").unwrap();
    manage.invoke_row_activated(i32::try_from(at).unwrap());
    let window = edit();
    assert!(!window.get_synchronised());
    window.invoke_flip_synchronised();
    assert!(window.get_synchronised());
    window.invoke_apply();
    manage.invoke_apply();
    assert!(kept(&store).iter().find(|r| r.1 == "alpha").unwrap().2);

    // and flipped back the other way, it is kept off
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    let at = rows(&manage).iter().position(|r| r.0 == "delta").unwrap();
    manage.invoke_row_activated(i32::try_from(at).unwrap());
    let window = edit();
    assert!(window.get_synchronised());
    window.invoke_flip_synchronised();
    window.invoke_apply();
    manage.invoke_apply();
    let delta = kept(&store).into_iter().find(|r| r.1 == "delta").unwrap();
    assert!(!delta.2);

    // loading a paused favourite sets the search but does not run it
    let favourites: FavouriteSearches = store.read(hydrus_store::settings::get).unwrap();
    let paused = favourites.0.iter().find(|f| f.name == "delta").unwrap();
    let mut page = SearchPage::new(store.clone());
    page.load_favourite(paused);
    assert!(!page.synchronised());
    assert!(!page.predicates().is_empty());
    let live = favourites.0.iter().find(|f| f.name == "alpha").unwrap();
    let mut page = SearchPage::new(store);
    page.load_favourite(live);
    assert!(page.synchronised());
    assert!(!page.results().is_empty());
}
