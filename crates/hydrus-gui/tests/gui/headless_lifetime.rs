//! Completed native owners must release adapters, callbacks and background stores.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use slint::ComponentHandle as _;
use std::sync::Arc;

#[test]
fn last_collector_hides_visible_components_and_releases_callbacks_and_store() {
    let (_directories, store) = crate::subscriptions::store();
    let weak_store = Arc::downgrade(&store);
    let windows = headless::init();
    let successor = windows.clone();
    let ui = MainWindow::new().unwrap();
    let weak_component = ui.as_weak();
    ui.on_refresh_page({
        let store = store.clone();
        move || {
            let _ = store.snapshot();
        }
    });
    ui.show().unwrap();
    let adapter = windows.get(0).unwrap();
    let weak_adapter = std::rc::Rc::downgrade(&adapter);
    drop(adapter);
    drop(ui);
    drop(store);
    assert!(weak_component.upgrade().is_some());
    assert!(weak_store.upgrade().is_some());
    drop(windows);
    assert!(
        weak_adapter.upgrade().is_some(),
        "the successor still owns the collector"
    );
    drop(successor);
    assert!(
        weak_component.upgrade().is_none(),
        "hiding releases Slint's visible component"
    );
    assert!(
        weak_adapter.upgrade().is_none(),
        "the platform keeps only a weak adapter"
    );
    assert!(
        weak_store.upgrade().is_none(),
        "the callback and database writer are released"
    );
}

#[test]
fn thread_exit_releases_bound_workers_when_the_collector_is_discarded() {
    let weak_store = std::thread::spawn(|| {
        let (_directories, store) = crate::subscriptions::store();
        let weak_store = Arc::downgrade(&store);
        headless::init();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.show().unwrap();
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        drop(bound);
        drop(ui);
        drop(store);
        weak_store
    })
    .join()
    .unwrap();
    // Owned workers may finish a current read after cancellation, but must not
    // retain their Store indefinitely after the GUI thread/context has ended.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while weak_store.strong_count() != 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        weak_store.strong_count(),
        0,
        "completed native worker ownership releases its Store"
    );
}
