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
