//! Real menu → owned deadlines → durable Store jobs → actual native toaster.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{Store, popups, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc, time::Duration};

fn jobs(store: &Store) -> Vec<popups::Job> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
}
fn clock(control: &hydrus_gui::debug_long_popup::Control) -> Rc<Cell<Duration>> {
    let now = Rc::new(Cell::new(Duration::ZERO));
    control.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    now
}
fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let rows = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = rows.iter().position(|row| row.label == label).unwrap();
    assert!(rows.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}
fn launch(ui: &MainWindow) {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(ui, 0, "debug");
    choose(ui, 1, "gui actions");
    let rows = ui.get_menu_panes().row_data(2).unwrap().lines;
    assert!(
        rows.iter()
            .position(|row| row.label == "make a long text popup")
            .unwrap()
            < rows
                .iter()
                .position(|row| row.label == "make a popup in five seconds")
                .unwrap()
    );
    choose(ui, 2, "make a popup in five seconds");
}

// leaf: audit-options-help-debug-action-make-a-popup-in-five-seconds
#[test]
fn actual_menu_overlapping_deadlines_hidden_progress_store_reopen_and_rendered_cards() {
    let fixture = hydrus_testkit::fixture_json("debug_delayed_popup.json");
    assert_eq!(
        fixture["menu_path"],
        serde_json::json!([
            "help",
            "debug",
            "gui actions",
            "make a popup in five seconds"
        ])
    );
    for entry in fixture["schedule"].as_array().unwrap() {
        assert_eq!(entry["delay_seconds"], 5);
    }
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    launch(&ui);
    assert!(
        jobs(&store).is_empty(),
        "scheduling is not early JobStatus publication"
    );
    assert_eq!(ui.get_popups().row_count(), 0);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 1);
    assert!(bound.debug_long_popup.timer_running());
    now.set(Duration::from_secs(1));
    launch(&ui);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 2);
    ui.hide().unwrap();
    bound.debug_long_popup.start_delayed_popup();
    assert_eq!(
        bound.debug_long_popup.pending_delayed_popups(),
        2,
        "hidden main refuses a new launch"
    );
    for (millis, count) in [(4999, 0), (5000, 1), (5999, 1), (6000, 2)] {
        now.set(Duration::from_millis(millis));
        bound.debug_long_popup.tick();
        let saved = jobs(&store);
        assert_eq!(saved.len(), count, "owned deadline at {millis}");
        assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 2 - count);
        assert_eq!(
            ui.get_popups().row_count(),
            0,
            "hidden Main freezes card projection"
        );
        for (job, expected) in saved.iter().zip(fixture["published"].as_array().unwrap()) {
            assert_eq!(job.status_text_1.as_deref(), expected["text"].as_str());
            assert_eq!(job.status_title.as_deref(), expected["title"].as_str());
            assert_eq!(Some(job.done), expected["done"].as_bool());
            assert_eq!(Some(job.pausable), expected["pausable"].as_bool());
            assert_eq!(Some(job.cancellable), expected["cancellable"].as_bool());
        }
    }
    assert!(!bound.debug_long_popup.timer_running());
    let saved = jobs(&store);
    assert_ne!(
        saved[0].key, saved[1].key,
        "overlapping calls create independent jobs"
    );
    bound.debug_long_popup.tick();
    assert_eq!(jobs(&store), saved, "due jobs publish exactly once");
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(jobs(&reopened), saved);
    ui.show().unwrap();
    // Show resumes the real 250ms refresh; it does not synchronously project jobs.
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    while ui.get_popups().row_count() != 2 {
        slint::platform::update_timers_and_animations();
        assert!(
            std::time::Instant::now() < deadline,
            "visible delayed popup cards did not refresh"
        );
        std::thread::sleep(Duration::from_millis(8));
    }
    assert_eq!(ui.get_popups().row_count(), 2);
    assert_eq!(
        jobs(&store),
        saved,
        "projecting saved cards cannot republish jobs"
    );
    for (index, expected) in fixture["cards"].as_array().unwrap().iter().enumerate() {
        let row = ui.get_popups().row_data(index).unwrap();
        assert_eq!(row.text_1, expected["text"].as_str().unwrap());
        assert!(row.title.is_empty() && !row.cancellable && !row.pausable);
    }
    let pixels = super::popup_width::render_settled(&ui, &windows.get(0).unwrap(), 1000, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("debug_delayed_popup_native.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
    ui.invoke_popup_dismiss(0);
    ui.invoke_popup_dismiss(0);
    assert!(jobs(&store).is_empty());
    now.set(Duration::from_secs(30));
    bound.debug_long_popup.tick();
    assert!(
        jobs(&store).is_empty(),
        "dismissed publication cannot reappear"
    );
}

#[test]
fn long_text_updates_and_dismissal_do_not_starve_a_pending_delayed_popup() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    bound.debug_long_popup.start();
    let before = jobs(&store);
    assert_eq!(before.len(), 2);
    launch(&ui);
    assert_eq!(bound.debug_long_popup.pending_updates(), 124);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 1);
    now.set(Duration::from_millis(200));
    bound.debug_long_popup.tick();
    let after = jobs(&store);
    assert_ne!(before[0].status_text_1, after[0].status_text_1);
    assert_eq!(before[1], after[1]);
    assert_eq!(bound.debug_long_popup.pending_updates(), 123);
    ui.invoke_popup_dismiss(0);
    ui.invoke_popup_dismiss(0);
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_updates(), 0);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 1);
    assert!(bound.debug_long_popup.timer_running());
    now.set(Duration::from_millis(4999));
    bound.debug_long_popup.tick();
    assert!(jobs(&store).is_empty());
    now.set(Duration::from_millis(5000));
    bound.debug_long_popup.tick();
    assert_eq!(
        jobs(&store)[0].status_text_1.as_deref(),
        Some("This is a delayed popup message.")
    );
    assert_eq!(ui.get_popups().row_count(), 1);
    assert!(!bound.debug_long_popup.timer_running());
}

#[test]
fn pending_delayed_jobs_retire_permanently_on_accepted_exit_rebind_and_owner_destruction() {
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    launch(&ui);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        ui.get_question()
            .starts_with("Are you sure you want to exit the client?")
    );
    ui.invoke_answer(false);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 1);
    assert!(bound.debug_long_popup.timer_running());
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(
        jobs(&store).len(),
        1,
        "Exit Cancel preserves actual pending work"
    );
    ui.invoke_popup_dismiss(0);
    launch(&ui);
    let old = bound.debug_long_popup.clone();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(old.pending_delayed_popups(), 0);
    assert!(!old.timer_running());
    now.set(Duration::from_secs(30));
    old.tick();
    old.start_delayed_popup();
    assert!(jobs(&store).is_empty());
    let next_clock = clock(&successor.debug_long_popup);
    launch(&ui);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!(successor.debug_long_popup.pending_delayed_popups(), 0);
    assert!(!successor.debug_long_popup.timer_running());
    ui.show().unwrap();
    next_clock.set(Duration::from_secs(30));
    successor.debug_long_popup.tick();
    successor.debug_long_popup.start_delayed_popup();
    assert!(
        jobs(&store).is_empty(),
        "retained re-shown main cannot resurrect accepted-exit work"
    );
    let fresh = bind(&ui, Pages::open(store.clone()).unwrap());
    let fresh_clock = clock(&fresh.debug_long_popup);
    launch(&ui);
    let retained = fresh.debug_long_popup.clone();
    drop(fresh);
    assert_eq!(retained.pending_delayed_popups(), 0);
    assert!(!retained.timer_running());
    fresh_clock.set(Duration::from_secs(30));
    retained.tick();
    retained.start_delayed_popup();
    assert!(jobs(&store).is_empty());
    let final_owner = bind(&ui, Pages::open(store.clone()).unwrap());
    let final_clock = clock(&final_owner.debug_long_popup);
    launch(&ui);
    ui.hide().unwrap();
    let weak = ui.as_weak();
    drop(ui);
    assert!(
        weak.upgrade().is_none(),
        "the deadline producer owns only Weak MainWindow"
    );
    final_clock.set(Duration::from_secs(30));
    final_owner.debug_long_popup.tick();
    assert_eq!(final_owner.debug_long_popup.pending_delayed_popups(), 0);
    assert!(!final_owner.debug_long_popup.timer_running());
    assert!(jobs(&store).is_empty());
}
