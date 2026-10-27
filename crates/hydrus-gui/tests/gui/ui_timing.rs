//! (spike) How long the UI takes to make and draw, for comparing the
//! compiled UI with the interpreted one. Prints; asserts nothing.
//! `cargo test -p hydrus-gui --test gui ui_timing -- --nocapture`

use std::time::Instant;

use slint::ComponentHandle as _;
use slint::platform::WindowAdapter as _;

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

/// (spike) What repeated compiles leave behind, with the interpreter.
#[cfg(feature = "interpreter-cached-ui")]
#[test]
fn ui_timing_compile_leak() {
    let rss = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|l| l.starts_with("VmRSS"))
            .unwrap()
            .to_owned()
    };
    for i in 0..8 {
        hydrus_gui::interp_compile_and_drop("ManageTagsWindow");
        eprintln!("ui_timing leak: after compile {i}: {}", rss());
    }
    let windows = headless::init();
    for i in 0..8 {
        let w = hydrus_gui::ManageTagsWindow::new().unwrap();
        drop(w);
        hydrus_gui::interp_clear_cache();
        eprintln!("ui_timing leak: after instance+clear {i}: {}", rss());
    }
    drop(windows);
}

/// (spike) Whether a thread's compiled definitions are freed when it ends.
#[cfg(feature = "interpreter-cached-ui")]
#[test]
fn ui_timing_thread_leak() {
    let rss = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|l| l.starts_with("VmRSS"))
            .unwrap()
            .to_owned()
    };
    for (i, mode) in ["compile only", "compile only", "compile only", "instance", "instance", "instance", "instance+clear", "instance+clear", "instance+clear"].iter().enumerate() {
        let mode = *mode;
        std::thread::spawn(move || {
            if mode == "compile only" {
                hydrus_gui::interp_compile_and_drop("ManageTagsWindow");
                return;
            }
            let windows = headless::init();
            let w = hydrus_gui::ManageTagsWindow::new().unwrap();
            drop(w);
            if mode == "instance+clear" {
                hydrus_gui::interp_clear_cache();
            }
            drop(windows);
        })
        .join()
        .unwrap();
        eprintln!("ui_timing thread: after thread {i} ({mode}): {}", rss());
    }
}

/// (spike) Whether a typical test's main window (bound, shown) is freed
/// with its thread.
#[test]
fn ui_timing_bound_main_window_leak() {
    let rss = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|l| l.starts_with("VmRSS"))
            .unwrap()
            .to_owned()
    };
    for i in 0..6 {
        let freed = std::thread::spawn(move || {
            let (_dirs, store) = store();
            let windows = headless::init();
            let ui = MainWindow::new().unwrap();
            let weak = ui.as_weak();
            let bound = bind(&ui, Pages::open(store.clone()).unwrap());
            if i % 2 == 1 {
                ui.show().unwrap();
                draw(&windows);
            }
            drop(bound);
            drop(ui);
            drop(store);
            drop(windows);
            weak.upgrade().is_none()
        })
        .join()
        .unwrap();
        eprintln!("ui_timing bound: thread {i} (shown: {}): freed {freed}, {}", i % 2 == 1, rss());
    }
}

/// (spike) Whether a manage tags dialog and a media viewer the user closes
/// are freed while the main window stays open, as in the app.
#[test]
fn ui_timing_closed_dialogs_are_freed() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let weak_manage = manage.as_weak();
    manage.invoke_cancel();
    drop(manage);
    for _ in 0..10 {
        slint::platform::update_timers_and_animations();
    }
    eprintln!(
        "ui_timing closed: manage tags slot empty {}, freed after cancel {}",
        bound.manage_tags.borrow().is_none(),
        weak_manage.upgrade().is_none()
    );
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let weak_viewer = viewer.as_weak();
    viewer.invoke_close_requested();
    drop(viewer);
    for _ in 0..10 {
        slint::platform::update_timers_and_animations();
    }
    eprintln!(
        "ui_timing closed: viewer slot empty {}, freed after close {}",
        bound.viewer.borrow().is_none(),
        weak_viewer.upgrade().is_none()
    );
    drop((bound, ui, windows));
}
