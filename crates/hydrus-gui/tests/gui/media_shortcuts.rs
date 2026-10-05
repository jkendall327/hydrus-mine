//! The reference's default media shortcuts, on thumbnails and in the media
//! viewer: ctrl+r takes files off the page (not out of the client), and
//! ctrl+e opens the focused file as the OS opens it.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::pages::PageCollect;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::ComponentHandle as _;

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
    ui.show().unwrap();
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
        tag_context: hydrus_core::search::context::TagContext::default(),
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

#[test]
fn alt_and_the_arrows_rearrange_the_selected_thumbnails() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let before = page.borrow().results().to_vec();
    let n = before.len();
    // the third and fourth selected
    page.borrow_mut().hit(Some(2), false, false);
    page.borrow_mut().hit(Some(3), true, false);
    let moved = [before[2], before[3]];
    let at = |page: &SearchPage| {
        let results = page.results();
        [moved[0], moved[1]].map(|f| results.iter().position(|&r| r == f).unwrap())
    };
    // alt+left: back one; alt+home: to the start; alt+end: to the end;
    // alt+right at the end: nowhere
    ui.invoke_rearrange(1);
    assert_eq!(at(&page.borrow()), [1, 2]);
    ui.invoke_rearrange(0);
    assert_eq!(at(&page.borrow()), [0, 1]);
    ui.invoke_rearrange(4);
    assert_eq!(at(&page.borrow()), [n - 2, n - 1]);
    ui.invoke_rearrange(3);
    assert_eq!(at(&page.borrow()), [n - 2, n - 1]);
    // the rest keep their order, and the selection stays
    let rest: Vec<_> = page.borrow().results()[..n - 2].to_vec();
    let mut expected = before.clone();
    expected.retain(|f| !moved.contains(f));
    assert_eq!(rest, expected);
    assert_eq!(page.borrow().selected_files().len(), 2);
    // until the page sorts again
    let ascending = page.borrow().sort().ascending;
    page.borrow_mut().set_sort_order(if ascending {
        hydrus_search::SortOrder::Ascending
    } else {
        hydrus_search::SortOrder::Descending
    });
    assert_eq!(page.borrow().results(), before);
}

#[test]
fn ctrl_c_copies_the_files_themselves() {
    use hydrus_gui::Clip;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (nothing really goes on the clipboard: what would is noted)
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let paths = |of: &[hydrus_core::HashId]| -> Vec<std::path::PathBuf> {
        hydrus_gui::thumbnail_menu::paths(&store, of)
            .into_iter()
            .map(Into::into)
            .collect()
    };
    // nothing selected: nothing copied
    ui.invoke_copy_files();
    assert!(copied.borrow().is_empty());
    // two selected: both, as files, each a real path
    page.borrow_mut().hit(Some(0), false, false);
    page.borrow_mut().hit(Some(1), true, false);
    ui.invoke_copy_files();
    let wanted = paths(&files[..2]);
    assert_eq!(wanted.len(), 2);
    assert!(wanted.iter().all(|p| p.exists()));
    assert_eq!(*copied.borrow(), [Clip::Files(wanted)]);
    // in the viewer, the file shown
    copied.borrow_mut().clear();
    ui.invoke_thumbnail_activated(1);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    viewer.invoke_copy_file();
    assert_eq!(*copied.borrow(), [Clip::Files(paths(&files[1..2]))]);
    viewer.invoke_close_requested();
}
