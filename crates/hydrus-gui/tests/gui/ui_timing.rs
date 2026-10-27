//! (spike) How long the UI takes to make and draw, for comparing the
//! compiled UI with the interpreted one. Prints; asserts nothing.
//! `cargo test -p hydrus-gui --test gui ui_timing -- --nocapture`

use std::time::Instant;

use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, Pages, bind, headless};

use crate::subscriptions::store;

fn timed<R>(what: &str, f: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let out = f();
    eprintln!("ui_timing: {what}: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    out
}

fn draw(windows: &headless::Windows) {
    for n in 0..windows.count() {
        if let Some(native) = windows.get(n).filter(|w| w.window().is_visible()) {
            headless::render(&native, 1100, 700);
        }
    }
}

#[test]
fn ui_timing_main_window_and_dialogs() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = timed("MainWindow::new (first on this thread)", || MainWindow::new().unwrap());
    let bound = timed("bind the main window to the store", || {
        bind(&ui, Pages::open(store.clone()).unwrap())
    });
    timed("show and draw the main window (first)", || {
        ui.show().unwrap();
        draw(&windows);
    });
    timed("draw the main window again", || draw(&windows));
    let second = timed("MainWindow::new (second)", || MainWindow::new().unwrap());
    drop(second);
    let options = timed("OptionsWindow::new", || hydrus_gui::OptionsWindow::new().unwrap());
    timed("OptionsWindow show and draw", || {
        options.show().unwrap();
        draw(&windows);
    });
    let tags = timed("ManageTagsWindow::new", || hydrus_gui::ManageTagsWindow::new().unwrap());
    let export = timed("ExportFilesWindow::new", || hydrus_gui::ExportFilesWindow::new().unwrap());
    let about = timed("AboutWindow::new", || hydrus_gui::AboutWindow::new().unwrap());
    timed("AboutWindow::new x20", || {
        for _ in 0..20 {
            drop(hydrus_gui::AboutWindow::new().unwrap());
        }
    });
    drop((options, tags, export, about, bound, ui));
}
