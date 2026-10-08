//! The popup stack's summary bar: its collapse button hides the cards (the
//! count line stays) and expanding shows the same oldest ten again, without
//! cancelling or deleting any job.
use std::sync::Arc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::popups::{self, Job};

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

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

// leaf: audit-options-popups-summary
#[test]
fn the_summary_bar_collapses_and_expands_the_cards_without_touching_jobs() {
    let (_dirs, store) = store();
    let windows = headless::init();
    for i in 0..12 {
        store
            .write(move |ctx| {
                popups::add(ctx.conn(), &Job::text(format!("m{i}"), f64::from(i)), now())
            })
            .unwrap();
    }
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let (w, h) = (1100_u32, 760_u32);
    let window = windows.get(0).unwrap();
    let expanded = headless::render(&window, w, h);
    assert_eq!(ui.get_popup_summary(), "12 messages");
    assert_eq!(ui.get_popups().row_count(), 10);
    let count = || store.read(|c| popups::all(c, now())).unwrap().len();
    // (the bar sits at the stack's bottom, 25px above the window's, its
    // collapse button the rightmost 20px in from the right)
    let click = |x: f32, y: f32| {
        let position = slint::LogicalPosition::new(x, y);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerExited);
    };
    click(w as f32 - 20.0 - 22.0, h as f32 - 25.0 - 16.0);
    let collapsed = headless::render(&window, w, h);
    assert!(collapsed != expanded, "the cards are hidden");
    assert_eq!(ui.get_popup_summary(), "12 messages", "the line stays");
    assert_eq!(count(), 12, "nothing was cancelled or deleted");
    click(w as f32 - 20.0 - 22.0, h as f32 - 25.0 - 16.0);
    let again = headless::render(&window, w, h);
    assert!(again == expanded, "the same ten cards are back");
    assert_eq!(count(), 12);
}

// leaf: audit-options-popups-traceback
#[test]
fn a_card_shows_and_hides_its_traceback_and_copies_all_of_it() {
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = store();
    let windows = headless::init();
    let mut job = Job::text("it broke", 0.0);
    job.status_title = Some("oh no".into());
    job.traceback = Some("trace line 1\ntrace line 2\n".to_owned() + &"z".repeat(2000));
    store
        .write({
            let job = job.clone();
            move |ctx| popups::add(ctx.conn(), &job, now())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let (w, h) = (1100_u32, 760_u32);
    let window = windows.get(0).unwrap();
    let hidden = headless::render(&window, w, h);
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let click = |x: f32, y: f32| {
        let position = slint::LogicalPosition::new(x, y);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerExited);
    };
    // Find the button that changes the card: from the summary bar upward.
    let x = w as f32 - 20.0 - 60.0;
    let mut shown = None;
    let mut y = h as f32 - 25.0 - 40.0;
    while y > h as f32 - 25.0 - 200.0 {
        click(x, y);
        let now = headless::render(&window, w, h);
        if now != hidden {
            shown = Some(now);
            break;
        }
        y -= 3.0;
    }
    let shown = shown.expect("a button shows the traceback");
    let toggled_at = y;
    copied.borrow_mut().clear();
    // The same card, taller: hide it again with the button now showing "hide".
    let mut y = toggled_at;
    let mut back = false;
    while y > h as f32 - 25.0 - 400.0 {
        click(x, y);
        if headless::render(&window, w, h) == hidden {
            back = true;
            break;
        }
        y -= 3.0;
    }
    assert!(back, "the traceback hides again");
    let _ = shown;
    copied.borrow_mut().clear();
    // Copy: the version line, then the whole job (not the cut display).
    ui.invoke_popup_copy_traceback(0);
    let copied = copied.borrow();
    let text = copied.last().unwrap();
    let (info, trace) = text.split_once('\n').unwrap();
    assert!(info.contains(env!("CARGO_PKG_VERSION")), "{info}");
    assert_eq!(trace, job.nice_string());
}
