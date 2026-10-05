//! The archive/delete filter, as the reference's: F12 on a page filters
//! its local files (or the one selected), each kept, deleted, skipped or
//! gone back to, by key or click; finishing or stopping asks, and
//! committing archives the kept and deletes the deleted.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::archive_delete::{ArchiveDeleteFilter, DELETE_REASON};
use hydrus_gui::{ArchiveDeleteWindow, MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// (in the inbox, in the trash, its deletion reason)
fn state(store: &Store, id: HashId) -> (bool, bool, Option<String>) {
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
        .unwrap();
    let m = &batch.results[0];
    let trash = snapshot.services.by_name("trash").unwrap().id;
    (
        m.inbox,
        m.current.iter().any(|c| c.service == trash),
        m.deletion_reason.clone(),
    )
}

#[test]
fn the_archive_delete_filter_keeps_and_deletes() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();

    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::FileViewRemoval {
                    trashed: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    // newest first
    page.borrow_mut()
        .set_sort_by(hydrus_search::SortBy::ImportTime);
    let files = page.borrow().results().to_vec();
    let n = files.len();
    assert!(n > 4);
    let filter = || -> ArchiveDeleteWindow {
        bound
            .archive_delete
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the filter opened")
    };

    // (only local files not in the trash are filtered)
    let trashed = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT hash_id FROM file_domain_current WHERE service_id =
                 (SELECT service_id FROM services WHERE name = 'trash') LIMIT 1",
                [],
                |r| r.get::<_, HashId>(0),
            )?)
        })
        .unwrap();
    let mut with_trashed = files.clone();
    with_trashed.push(trashed);
    let model = ArchiveDeleteFilter::new(
        store.clone(),
        with_trashed,
        page.borrow().location().clone(),
    )
    .unwrap();
    assert_eq!(model.caption(), format!("1/{n}"));

    // F12 with nothing selected: every file
    ui.invoke_archive_delete_filter();
    let window = filter();
    assert_eq!(window.get_caption(), format!("1/{n}"));
    window.invoke_keep();
    window.invoke_delete();
    window.invoke_back();
    assert_eq!(window.get_caption(), "2/".to_owned() + &n.to_string());
    window.invoke_delete();
    window.invoke_skip();
    assert_eq!(window.get_caption(), format!("4/{n}"));
    // stopping asks; escape goes back to filtering
    window.invoke_close_requested();
    let question = window.get_question().to_string();
    assert!(
        question.starts_with("keep 1 and delete 1 from ")
            && question.ends_with(", sending directly to trash?"),
        "{question}"
    );
    window.invoke_resume();
    assert_eq!(window.get_question(), "");
    assert_eq!(window.get_caption(), format!("4/{n}"));
    // committing archives the kept and deletes the deleted
    window.invoke_close_requested();
    window.invoke_commit();
    assert!(bound.archive_delete.borrow().is_none(), "it closed");
    assert_eq!(state(&store, files[0]), (false, false, None));
    assert_eq!(
        state(&store, files[1]),
        (true, true, Some(DELETE_REASON.to_owned()))
    );
    assert_eq!(state(&store, files[2]), (true, false, None), "skipped");
    let after = page.borrow().results().to_vec();
    assert_eq!(after.len(), n - 1, "the deleted file leaves the page");
    assert!(!after.contains(&files[1]));

    // forgetting leaves them be
    ui.invoke_archive_delete_filter();
    let window = filter();
    window.invoke_skip();
    window.invoke_keep();
    window.invoke_close_requested();
    window.invoke_forget();
    assert!(bound.archive_delete.borrow().is_none());
    assert_eq!(after[1], files[2]);
    assert!(state(&store, after[1]).0, "still in the inbox");

    // F12 with a file selected: just it; finishing asks, and escape goes
    // back to it
    ui.invoke_thumbnail_clicked(2, false, false);
    let selected = after[2];
    ui.invoke_archive_delete_filter();
    let window = filter();
    assert_eq!(window.get_caption(), "1/1");
    window.invoke_delete();
    assert!(window.get_question().starts_with("delete 1 from "));
    window.invoke_resume();
    assert_eq!(window.get_question(), "");
    // a left click keeps it (a right click would delete it)
    let filter_window = windows.get(windows.count() - 1).unwrap();
    headless::render(&filter_window, 800, 600);
    let at = slint::LogicalPosition::new(400.0, 300.0);
    filter_window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
        position: at,
        button: slint::platform::PointerEventButton::Left,
    });
    filter_window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
        position: at,
        button: slint::platform::PointerEventButton::Left,
    });
    assert_eq!(window.get_question(), "keep 1?");
    window.invoke_commit();
    assert_eq!(state(&store, selected), (false, false, None));
}
