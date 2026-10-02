//! Each page keeps how far its thumbnails are scrolled, as each of the
//! reference's pages is its own widget: shown again, a page is where it was
//! left, and a page never scrolled is at the top.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

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

#[test]
#[allow(clippy::float_cmp)] // (offsets set, not computed)
fn a_page_shown_again_is_scrolled_as_it_was_left() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (two pages, the second shown)
    let mut pages = Pages::open(store).unwrap();
    pages.new_search_page();
    let _bound = bind(&ui, pages);
    ui.window().set_size(slint::LogicalSize::new(900.0, 650.0));
    let tabs = ui.get_tab_rows().row_data(0).unwrap().names.row_count();
    assert!(tabs >= 2, "{tabs} tabs");
    let first = 0;
    let second = i32::try_from(tabs - 1).unwrap();

    // the first page scrolled down, then the second shown: it starts at
    // the top
    ui.invoke_tab_chosen(0, first);
    ui.set_grid_scroll(-300.0);
    ui.invoke_tab_chosen(0, second);
    assert_eq!(ui.get_grid_scroll(), 0.0);
    // scrolled a little, and the first shown again: as it was left
    ui.set_grid_scroll(-120.0);
    ui.invoke_tab_chosen(0, first);
    assert_eq!(ui.get_grid_scroll(), -300.0);
    // and the second
    ui.invoke_tab_chosen(0, second);
    assert_eq!(ui.get_grid_scroll(), -120.0);
    // (showing the page already shown leaves it where it is)
    ui.set_grid_scroll(-40.0);
    ui.invoke_tab_chosen(0, second);
    assert_eq!(ui.get_grid_scroll(), -40.0);
}
