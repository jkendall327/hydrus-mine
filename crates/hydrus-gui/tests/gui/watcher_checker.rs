//! A watcher page's "checker options" buttons (the reference's
//! `MultipleWatcherImport.SetCheckerOptions` and
//! `WatcherImport.SetCheckerOptions`, recorded by
//! `oracle/record_watcher_checker.py`; their effect on a watcher is
//! hydrus-core's test): the page's, under its URL box, gives its new
//! watchers other checker options (those it has keep theirs, and the page
//! keeps them); the shown watcher's sets its own, timing its next check
//! again.

use std::sync::Arc;

use slint::ComponentHandle as _;

use hydrus_core::subscriptions::{CheckerDefaults, CheckerOptions};
use hydrus_gui::checker_options::PRESETS;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues;
use hydrus_store::watchers::watcher_state;

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

/// The fields of a time's model, as shown.
fn values(fields: &slint::ModelRc<hydrus_gui::DurationField>) -> Vec<i32> {
    use slint::Model as _;
    (0..fields.row_count())
        .map(|f| fields.row_data(f).unwrap().value)
        .collect()
}

#[test]
fn the_watcher_pages_checker_options_buttons() {
    let recorded = hydrus_testkit::fixture_json("watcher_checker.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then watcher
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);
    let key = bound.pages.borrow().shown().key;
    let checker = |queue: i64| -> CheckerOptions {
        watcher_state(
            &store
                .read(move |c| queues::queue(c, queue))
                .unwrap()
                .unwrap(),
        )
        .unwrap()
        .checker
    };
    let made = || {
        store
            .read(move |c| queues::queues_with_page_key(c, &key.0))
            .unwrap()
    };
    let defaults: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    // (the reference's, as the recording has it)
    let slow_thread = PRESETS[1].1.clone();
    let page = &recorded["page"];
    assert_eq!(page["set"][1], slow_thread.never_faster_than);
    assert_eq!(page["default"][1], defaults.watchers.never_faster_than);

    ui.invoke_watcher_urls("https://boards.example/thread/1".into());
    let first = made()[0].id;
    assert_eq!(checker(first), defaults.watchers);

    // the page's: it opens on the page's checker options (the client's
    // default), and what it applies the next watcher gets
    ui.invoke_watcher_page_checker();
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    assert_eq!(values(&editor.get_faster_fields()), [0, 0, 5, 0]);
    editor.invoke_preset(1);
    editor.invoke_ok();
    assert!(bound.checker_options.borrow().is_none());
    ui.invoke_watcher_urls("https://boards.example/thread/2".into());
    let second = made().iter().map(|q| q.id).find(|&id| id != first).unwrap();
    assert_eq!(checker(second), slow_thread);
    assert_eq!(checker(first), defaults.watchers, "the first keeps its own");
    // (and the page keeps them)
    bound.pages.borrow_mut().sync(5).unwrap();
    let saved = store
        .read(|c| hydrus_store::sessions::load(c, hydrus_store::sessions::LAST_SESSION))
        .unwrap()
        .unwrap();
    let kept = saved
        .all_pages()
        .into_iter()
        .find(|p| p.key == key)
        .unwrap()
        .clone();
    let hydrus_core::pages::PageContent::Downloader {
        page: Some(state), ..
    } = kept.content
    else {
        panic!("{kept:?}");
    };
    assert_eq!(state.checker, Some(slow_thread.clone()));

    // the shown watcher's (the first, shown as it was made): it opens on
    // its own, and what it applies is its own, timing its next check again
    // (checked a minute ago, with no files: never slower than from now)
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let mut state = watcher_state(&queues::queue(conn, first)?.unwrap()).unwrap();
            state.last_check_time = now - 60;
            queues::set_queue_extra(conn, first, &serde_json::to_value(&state).unwrap())
        })
        .unwrap();
    assert!(ui.get_watcher_data().highlighted);
    ui.invoke_watcher_checker();
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    assert_eq!(values(&editor.get_slower_fields()), [1, 0, 0, 0]);
    editor.invoke_preset(1);
    editor.invoke_ok();
    let state = watcher_state(
        &store
            .read(move |c| queues::queue(c, first))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state.checker, slow_thread);
    let week = 7 * 86400;
    assert!(
        (now + week..now + week + 60).contains(&state.next_check_time),
        "{}",
        state.next_check_time
    );
}
