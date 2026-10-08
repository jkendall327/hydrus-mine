//! The popup message manager's cards driven through the real main window
//! and store: content and gauges, pause, cancel, dismissal, attached files,
//! the traceback copy, and the ten-card window (hydrus
//! `ClientGUIPopupMessages.PopupMessage`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Clip, MainWindow, Pages, bind, headless};
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

fn add(store: &Store, job: &Job) {
    let job = job.clone();
    store
        .write(move |ctx| popups::add(ctx.conn(), &job, now()))
        .unwrap();
}

fn stored(store: &Store, key: [u8; 32]) -> Option<Job> {
    store
        .read(|conn| popups::all(conn, now()))
        .unwrap()
        .into_iter()
        .find(|j| j.key == key)
}

fn window(store: &Arc<Store>) -> (MainWindow, hydrus_gui::Bound) {
    // (the headless windows stay for the test thread's life)
    std::mem::forget(headless::init());
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (ui, bound)
}

// leaf: audit-options-popups-content
// leaf: audit-options-popups-pause
#[test]
fn cards_show_title_texts_and_gauges_and_pausing_hides_them() {
    let (_dirs, store) = store();
    let mut job = Job::new(true, true, 0.0);
    job.status_title = Some("a title".into());
    job.status_text_1 = Some("first".into());
    job.status_text_2 = Some("second".into());
    job.popup_gauge_1 = Some((1, 4));
    job.popup_gauge_2 = Some((0, 0));
    add(&store, &job);
    let (ui, _bound) = window(&store);
    let row = ui.get_popups().row_data(0).unwrap();
    assert_eq!(
        (row.title.as_str(), row.text_1.as_str(), row.text_2.as_str()),
        ("a title", "first", "second")
    );
    assert!(row.has_gauge_1 && !row.gauge_1_going);
    assert!((row.gauge_1 - 0.25).abs() < 1e-6);
    assert!(row.has_gauge_2 && row.gauge_2_going, "no range: pulsing");

    // paused: "paused" for the first text, no gauges and no second text
    ui.invoke_popup_pause_play(0);
    let row = ui.get_popups().row_data(0).unwrap();
    assert!(row.paused);
    assert_eq!(row.text_1, "paused");
    assert!(!row.has_gauge_1 && !row.has_gauge_2);
    assert_eq!(row.text_2, "");
    assert!(stored(&store, job.key).unwrap().paused, "and in the store");

    // played again: everything is back
    ui.invoke_popup_pause_play(0);
    let row = ui.get_popups().row_data(0).unwrap();
    assert!(!row.paused);
    assert_eq!(row.text_1, "first");
    assert_eq!(row.text_2, "second");
    assert!(row.has_gauge_1 && row.has_gauge_2);
    assert!(!stored(&store, job.key).unwrap().paused);

    // a job that is not pausable ignores it
    let plain = Job::text("plain", 0.0);
    add(&store, &plain);
    std::thread::sleep(std::time::Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    ui.invoke_popup_pause_play(1);
    assert!(!stored(&store, plain.key).unwrap().paused);

    // text past the reference's 1024-character cutoff shows its start
    let mut long = Job::text("long", 0.0);
    long.status_text_2 = Some("y".repeat(1500));
    add(&store, &long);
    std::thread::sleep(std::time::Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    let row = ui.get_popups().row_data(2).unwrap();
    let (head, body) = row.text_2.split_once('\n').unwrap();
    assert_eq!(
        head,
        "The text is too long to display here. Here is the start of it (the rest is printed to the log):"
    );
    assert_eq!(body, "y".repeat(1024));
}

// leaf: audit-options-popups-cancel
#[test]
fn cancel_marks_a_cancellable_job_cancelled_and_leaves_others_alone() {
    let (_dirs, store) = store();
    let cancellable = Job::new(true, true, 0.0);
    let mut fixed = Job::new(true, false, 0.0);
    fixed.status_text_1 = Some("cannot stop".into());
    add(&store, &cancellable);
    add(&store, &fixed);
    let (ui, _bound) = window(&store);
    assert!(ui.get_popups().row_data(0).unwrap().cancellable);
    assert!(!ui.get_popups().row_data(1).unwrap().cancellable);

    ui.invoke_popup_cancel(1);
    let job = stored(&store, fixed.key).unwrap();
    assert!(
        !job.cancelled && !job.done,
        "not cancellable: nothing happens"
    );

    ui.invoke_popup_cancel(0);
    let job = stored(&store, cancellable.key).unwrap();
    assert!(job.cancelled && job.done, "the producer sees the flag");
    assert!(!job.cancellable && !job.pausable);
}

#[test]
fn copy_traceback_puts_version_system_and_the_whole_job_on_the_clipboard() {
    let (_dirs, store) = store();
    let mut job = Job::text("it broke", 0.0);
    job.status_title = Some("oh no".into());
    job.traceback = Some("trace line 1\ntrace line 2\n".to_owned() + &"z".repeat(2000));
    add(&store, &job);
    let (ui, _bound) = window(&store);
    let row = ui.get_popups().row_data(0).unwrap();
    assert!(
        row.traceback
            .starts_with("The text is too long to display here"),
        "the card shows only the start of a long traceback"
    );
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    ui.invoke_popup_copy_traceback(0);
    let copied = copied.borrow();
    assert_eq!(copied.len(), 1);
    let (info, trace) = copied[0].split_once('\n').unwrap();
    // (the reference's first line is "v<version>, <platform>, <source|frozen>")
    assert!(info.contains(env!("CARGO_PKG_VERSION")), "{info}");
    assert_eq!(
        trace,
        job.nice_string(),
        "the complete text, not the cut one"
    );
    assert!(trace.starts_with("oh no\nit broke\ntrace line 1\n"));
    assert!(trace.ends_with(&"z".repeat(2000)));
}

// leaf: audit-options-popups-dismiss
#[test]
fn dismiss_all_keeps_active_jobs_and_a_done_one_dismisses_alone() {
    let (_dirs, store) = store();
    let active = Job::new(true, true, 0.0);
    let mut done = Job::text("done", 0.0);
    done.finish();
    let mut other = Job::text("also done", 0.0);
    other.finish();
    add(&store, &active);
    add(&store, &done);
    add(&store, &other);
    let (ui, _bound) = window(&store);
    assert_eq!(ui.get_popup_summary(), "3 messages");
    // dismissing the active one does nothing
    ui.invoke_popup_dismiss(0);
    assert_eq!(ui.get_popups().row_count(), 3);
    assert!(!stored(&store, active.key).unwrap().dismissed);
    // a done one goes, and only it
    ui.invoke_popup_dismiss(1);
    assert_eq!(ui.get_popups().row_count(), 2);
    assert!(stored(&store, done.key).is_none());
    assert!(stored(&store, other.key).is_some());
    // dismiss all: those done go, the active job stays
    ui.invoke_popups_dismiss_all();
    assert_eq!(ui.get_popups().row_count(), 1);
    assert!(stored(&store, active.key).is_some());
    assert!(stored(&store, other.key).is_none());
    assert_eq!(ui.get_popup_summary(), "1 message");
}

// leaf: audit-options-popups-files
#[test]
fn show_files_opens_a_named_page_and_vanished_files_clear_the_attachment() {
    let (_dirs, store) = store();
    let hashes: Vec<hydrus_core::Sha256> = store
        .read(|conn| {
            let ids: Vec<hydrus_core::HashId> = conn
                .prepare("SELECT hash_id FROM hashes ORDER BY hash_id LIMIT 2")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(hydrus_store::master::hashes(conn, &ids)?
                .into_values()
                .collect())
        })
        .unwrap();
    let mut job = Job::text("found", 0.0);
    job.set_files(hashes, Some("my files".into()));
    add(&store, &job);
    // a done job whose only files are not in the client at all
    let mut gone = Job::text("gone", 0.0);
    gone.set_files(vec![hydrus_core::Sha256([7; 32])], Some("ghosts".into()));
    gone.finish();
    add(&store, &gone);
    let (ui, bound) = window(&store);
    assert_eq!(
        ui.get_popups().row_data(0).unwrap().files,
        "my files - show 2 files"
    );
    ui.invoke_popup_show_files(0);
    assert_eq!(bound.pages.borrow().shown().name, "my files");
    assert_eq!(bound.current.borrow().borrow().files().len(), 2);

    ui.invoke_popup_show_files(1);
    assert!(
        stored(&store, gone.key).is_none(),
        "no files left: the done job is dismissed"
    );
}

#[test]
fn only_the_ten_oldest_cards_show_and_the_line_counts_them_all() {
    let (_dirs, store) = store();
    for i in 0..12 {
        add(&store, &Job::text(format!("m{i}"), f64::from(i)));
    }
    let (ui, _bound) = window(&store);
    let shown: Vec<String> = ui
        .get_popups()
        .iter()
        .map(|p| p.text_1.to_string())
        .collect();
    assert_eq!(shown, (0..10).map(|i| format!("m{i}")).collect::<Vec<_>>());
    assert_eq!(ui.get_popup_summary(), "12 messages");
    // nothing was cancelled or deleted by hiding the other two
    assert_eq!(store.read(|c| popups::all(c, now())).unwrap().len(), 12);
}
