//! The reference's default media shortcuts, on thumbnails and in the media
//! viewer: ctrl+r takes files off the page (not out of the client), and
//! ctrl+e opens the focused file as the OS opens it.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::pages::PageCollect;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

#[test]
fn ctrl_r_and_ctrl_e() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (nothing is really opened: what would be is noted)
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let before = page.borrow().results().to_vec();
    let path_of = |file| {
        hydrus_gui::thumbnail_menu::paths(&store, &[file])
            .pop()
            .unwrap()
    };

    // ctrl+e: the focused file opens
    page.borrow_mut().hit(Some(2), false, false);
    ui.invoke_open_externally();
    assert_eq!(*launched.borrow(), [path_of(before[2])]);

    // ctrl+r: the selected files leave the page, but not the client
    page.borrow_mut().hit(Some(4), true, false);
    ui.invoke_remove_selected();
    let after = page.borrow().results().to_vec();
    assert_eq!(after.len(), before.len() - 2);
    assert!(!after.contains(&before[2]) && !after.contains(&before[4]));
    assert!(page.borrow().selected_files().is_empty());
    let still_there = store
        .read(|c| hydrus_store::media::load_basic(c, &[before[2]]))
        .unwrap();
    assert_eq!(still_there.len(), 1);
    // (with nothing selected, nothing)
    ui.invoke_remove_selected();
    assert_eq!(page.borrow().results().len(), after.len());

    // on a collection, ctrl+e opens nothing (one file must be focused)
    page.borrow_mut().set_collect(PageCollect {
        namespaces: vec!["series".into()],
        ratings: Vec::new(),
        collect_unmatched: true,
    });
    page.borrow_mut().hit(Some(0), false, false);
    launched.borrow_mut().clear();
    ui.invoke_open_externally();
    assert!(launched.borrow().is_empty());
    // and ctrl+r takes a collection's files off together
    let files = page.borrow().files().len();
    let in_it = page.borrow().selected_files().len();
    assert!(in_it > 1);
    ui.invoke_remove_selected();
    assert_eq!(page.borrow().files().len(), files - in_it);
    page.borrow_mut().set_collect(PageCollect::default());

    // in the viewer: ctrl+e opens the file shown, and ctrl+r takes it off
    // the viewer and the page, showing the next
    let shown_before = page.borrow().results().to_vec();
    ui.invoke_thumbnail_activated(1);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let count = shown_before.len();
    assert_eq!(viewer.get_caption(), format!("2/{count}"));
    viewer.invoke_open_externally();
    assert_eq!(*launched.borrow(), [path_of(shown_before[1])]);
    viewer.invoke_remove_from_view();
    assert_eq!(viewer.get_caption(), format!("2/{}", count - 1));
    assert!(!page.borrow().results().contains(&shown_before[1]));
    viewer.invoke_close_requested();
}
