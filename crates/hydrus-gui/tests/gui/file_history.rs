//! Real Database frame, independent query, controls and asynchronous owner guards.
use hydrus_gui::{FileHistoryWindow, MainWindow, Pages, bind, file_history_window, headless};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
mod seeding {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hydrus-gui-model/tests/support/file_history_seed.rs"
    ));
}
fn wait(window: &FileHistoryWindow) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while window.get_loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_loading());
    assert!(window.get_chart_visible(), "{}", window.get_status());
}
#[test]
fn history_frame_consumes_times_filters_and_ranges_without_touching_main_query_or_retired_owner() {
    let recorded = hydrus_testkit::fixture_json("file_history.json");
    let (_dirs, store) = super::subscriptions::store();
    seeding::seed(&store, &recorded);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let tree = bound.pages.borrow().session().pages.clone();
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|t| t.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let history = lines
        .iter()
        .position(|r| r.label == "view file history")
        .unwrap();
    assert!(lines.row_data(history).unwrap().usable);
    ui.invoke_menu_line_clicked(0, i32::try_from(history).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.file_history.borrow().as_ref().unwrap().clone_strong();
    wait(&window);
    assert_eq!(window.get_min_count(), 0);
    assert_eq!(window.get_max_count(), 4);
    assert_eq!(window.get_start_date(), "2024-01-01");
    assert_eq!(window.get_end_date(), "2024-01-05");
    assert!(window.get_paths().iter().all(|p| !p.is_empty()));
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 700);
    assert!(pixels.chunks_exact(4).any(|p| p[0] != p[1] || p[1] != p[2]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("file_history.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    window.invoke_toggle(0);
    assert_eq!(window.get_max_count(), 2);
    window.invoke_count_edited(2, 12);
    window.invoke_date_edited("2024-01-02".into(), "2024-01-05".into());
    window.invoke_toggle(1);
    window.invoke_refresh();
    wait(&window);
    assert_eq!((window.get_min_count(), window.get_max_count()), (2, 12));
    assert_eq!(window.get_start_date(), "2024-01-02");
    window.invoke_refit(true);
    window.invoke_refit(false);
    assert_eq!(window.get_start_date(), "2024-01-03");
    assert_eq!(window.get_max_count(), 2);
    window.invoke_refresh();
    window.invoke_cancel_work();
    assert_eq!(window.get_status(), "Cancelled!");
    assert!(!window.get_chart_visible());
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(window.get_status(), "Cancelled!");
    window.set_input("parity:history-selected".into());
    window.invoke_enter();
    wait(&window);
    assert_eq!(window.get_predicates().row_count(), 1);
    assert_eq!(window.get_max_count(), 1);
    window.invoke_toggle(0);
    assert!(!window.get_paths().row_data(0).unwrap().is_empty());
    assert_eq!(bound.pages.borrow().session().pages, tree);
    window.invoke_close_clicked();
    assert!(bound.file_history.borrow().is_none());
    window.invoke_refresh();
    assert!(!window.get_loading());
    let slot = Rc::new(RefCell::new(None));
    let open = || {
        file_history_window::open(&store, &slot, {
            let weak = ui.as_weak();
            Rc::new(move || weak.upgrade().is_some_and(|w| w.window().is_visible()))
        })
        .unwrap()
    };
    let old = open();
    let current = open();
    old.invoke_close_clicked();
    old.invoke_cancel_work();
    assert!(slot.borrow().is_some());
    wait(&current);
    assert_eq!(current.get_predicates().row_count(), 0);
    assert_eq!(current.get_max_count(), 4);
    ui.hide().unwrap();
    current.set_input("system:inbox".into());
    current.invoke_enter();
    assert!(slot.borrow().is_none());
    assert_eq!(current.get_predicates().row_count(), 0);
    ui.show().unwrap();
    current.invoke_refresh();
    assert!(!current.window().is_visible());
    assert!(slot.borrow().is_none());
    ui.hide().unwrap();
}
