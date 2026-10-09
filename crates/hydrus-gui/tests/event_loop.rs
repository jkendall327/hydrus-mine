//! What stops the running event loop. Alone in a test binary of its own:
//! Slint keeps one event loop proxy for the whole process, and only one UI
//! thread can have it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

// leaf: audit-options-help-debug-action-simulate-program-exit-signal
#[test]
fn simulate_program_exit_signal_stops_the_event_loop() {
    let _windows = headless::init_with_event_loop();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(store).unwrap());

    // once the loop runs: help > debug > data actions > simulate program
    // exit signal (the reference's `QApplication.instance().exit`)
    let clicked = Rc::new(Cell::new(false));
    let click = slint::Timer::default();
    click.start(slint::TimerMode::SingleShot, Duration::ZERO, {
        let ui = ui.as_weak();
        let clicked = clicked.clone();
        move || {
            let ui = ui.unwrap();
            let help = ui
                .get_menu_titles()
                .iter()
                .position(|row| row.label == "help")
                .unwrap();
            ui.invoke_menu_title_pressed(i32::try_from(help).unwrap(), 20.0, 22.0);
            for (pane, label) in ["debug", "data actions", "simulate program exit signal"]
                .into_iter()
                .enumerate()
            {
                let rows = ui.get_menu_panes().row_data(pane).unwrap().lines;
                let row = rows
                    .iter()
                    .position(|row| row.label == label)
                    .unwrap_or_else(|| panic!("no menu entry {label:?}"));
                ui.invoke_menu_line_clicked(
                    i32::try_from(pane).unwrap(),
                    i32::try_from(row).unwrap(),
                    0.0,
                    0.0,
                    0.0,
                );
            }
            clicked.set(true);
        }
    });
    // (a loop the action failed to stop is stopped later, and the test fails)
    let gave_up = Rc::new(Cell::new(false));
    let give_up = slint::Timer::default();
    give_up.start(slint::TimerMode::SingleShot, Duration::from_secs(60), {
        let gave_up = gave_up.clone();
        move || {
            gave_up.set(true);
            slint::quit_event_loop().unwrap();
        }
    });

    slint::run_event_loop().unwrap();
    assert!(clicked.get(), "the action was clicked");
    assert!(!gave_up.get(), "the action stopped the event loop");
    // the main window is still there: the loop stopped, the program did not
    // close it first
    assert!(ui.window().is_visible());
}
