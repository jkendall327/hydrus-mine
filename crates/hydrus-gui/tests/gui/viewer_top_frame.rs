//! The media viewer's top hover frame, as the reference's
//! `CanvasHoverFrameTop`: shown near the top middle, its buttons following
//! the file shown (archive or re-inbox, send to the trash or delete
//! completely, undelete), its zoom the viewer's, and, where the reference
//! has a button to drag the file out, one showing it in the file browser.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_gui::thumbnail_menu::{Entry, open_menu};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::ComponentHandle;
use slint::platform::WindowEvent;

#[test]
fn the_viewer_s_top_hover_frame() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (nothing is really shown: what would be is noted)
    let shown_in: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_file_browser({
        let shown_in = shown_in.clone();
        move |path| shown_in.borrow_mut().push(path.to_owned())
    });
    let windows = headless::init();
    // a page on all local files, so a file sent to the trash stays on it
    let all_local = store
        .snapshot()
        .services
        .all()
        .find(|s| s.service_type() == hydrus_core::ServiceType::HydrusLocalFileStorage)
        .unwrap()
        .key
        .clone();
    let page = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location: LocationContext::single(all_local),
            ..FileSearchContext::default()
        },
        true,
        None,
        Vec::new(),
    );
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    // one never deleted from anywhere (one deleted from a local file
    // domain once shows the undelete button, as in the reference)
    let snapshot = store.snapshot();
    let media = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &files))
        .unwrap()
        .results;
    let index = files
        .iter()
        .position(|f| {
            media
                .iter()
                .any(|m| m.hash_id == *f && m.deleted.is_empty())
        })
        .unwrap();
    let file = files[index];
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .unwrap();

    // it shows with the pointer near the top middle, and on it
    headless::render(&windows.get(windows.count() - 1).unwrap(), 800, 600);
    let at = |x: f32, y: f32| WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    };
    viewer.window().dispatch_event(at(400.0, 300.0));
    assert!(!viewer.get_info_showing());
    viewer.window().dispatch_event(at(400.0, 10.0));
    assert!(viewer.get_info_showing());
    viewer.window().dispatch_event(at(100.0, 10.0));
    assert!(!viewer.get_info_showing(), "not over the left fifth");

    // an inbox file here: archive, and send to the trash
    let buttons = |v: &hydrus_gui::MediaViewerWindow| {
        (
            v.get_file_inbox(),
            v.get_file_local(),
            v.get_file_trashed(),
            v.get_file_undeletable(),
        )
    };
    assert_eq!(buttons(&viewer), (true, true, false, false));
    viewer.invoke_archive();
    assert_eq!(buttons(&viewer), (false, true, false, false));
    // in the trash: delete completely, and undelete
    viewer.invoke_delete();
    viewer.invoke_answer(true);
    assert!(bound.current.borrow().borrow().results().contains(&file));
    assert_eq!(buttons(&viewer), (false, false, true, true));
    viewer.invoke_undelete();
    // (asked first, while "Confirm sending files to trash" is on)
    assert_eq!(
        viewer.get_question(),
        "Undelete this file back to my files?"
    );
    viewer.invoke_answer(true);
    assert_eq!(buttons(&viewer), (false, true, false, false));

    // its zoom is the viewer's: the zoom switch changes it, to or from 100%
    let before = viewer.get_zoom_text().to_string();
    assert!(before.ends_with('%'), "{before}");
    viewer.invoke_zoom(0, false, 0.0, 0.0);
    let after = viewer.get_zoom_text().to_string();
    assert_ne!(before, after);
    assert!(before == "100%" || after == "100%", "{before} {after}");

    // the file shown in the file browser, in the drag button's place
    viewer.invoke_show_in_file_browser();
    let path = hydrus_gui::thumbnail_menu::paths(&store, &[file])
        .pop()
        .unwrap();
    assert_eq!(*shown_in.borrow(), [path]);

    // and the open menu has it too, after "in web browser", as the
    // reference's has it in advanced mode
    let labels: Vec<String> = open_menu(&store, Some(file), 1)
        .into_iter()
        .filter_map(|e| match e {
            Entry::Item(label, _) => Some(label),
            _ => None,
        })
        .collect();
    let at = labels.iter().position(|l| l == "in file browser").unwrap();
    assert_eq!(labels[at - 1], "in web browser");
}
