//! Modal popups driven through the real main window and store: a held job
//! is thrown up in a dialog (not the popup stack), waits while the main
//! window is minimised or not active or another dialog is up, asks before
//! cancelling, warns if it can't be, closes itself when the job finishes
//! and then lets the popup stack have the job (hydrus `AddModalMessage` and
//! `PopupMessageDialogPanel`, recorded in `popup_modal.json`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_gui_model::popup_modal::{CANCEL_QUESTION, CANNOT_CANCEL, DEFAULT_TITLE};
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

struct Modal {
    _dirs: [tempfile::TempDir; 2],
    store: Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    _windows: headless::Windows,
}

fn start() -> Modal {
    let (dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    Modal {
        _dirs: dirs,
        store,
        ui,
        bound,
        _windows: windows,
    }
}

fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pump() {
    let end = Instant::now() + Duration::from_millis(600);
    while Instant::now() < end {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl Modal {
    /// Publish a job as modal: held for its dialog.
    fn publish(&self, mut job: Job) -> [u8; 32] {
        job.held_by_modal = true;
        let key = job.key;
        self.store
            .write(move |ctx| popups::add(ctx.conn(), &job, now()))
            .unwrap();
        key
    }

    fn change(&self, key: [u8; 32], f: impl FnOnce(&mut Job) + Send + 'static) {
        self.store
            .write(move |ctx| popups::update(ctx.conn(), &key, now(), f).map(|_| ()))
            .unwrap();
    }

    fn job(&self, key: [u8; 32]) -> Job {
        self.store
            .read(move |c| popups::get(c, &key, now()))
            .unwrap()
            .unwrap()
    }

    /// The popups the stack has (released jobs).
    fn stacked(&self) -> Vec<[u8; 32]> {
        self.store
            .read(|c| popups::all(c, now()))
            .unwrap()
            .iter()
            .map(|j| j.key)
            .collect()
    }

    fn running(title: Option<&str>, cancellable: bool) -> Job {
        // (a job is ongoing only if it can be paused or cancelled)
        let mut job = Job::new(!cancellable, cancellable, 0.0);
        job.status_title = title.map(str::to_owned);
        job
    }
}

// leaf: audit-options-popups-modal
#[test]
fn a_modal_job_has_a_dialog_asks_before_cancelling_and_is_released_to_the_popups() {
    let m = start();
    let controller = m.bound.popup_modal.clone();
    let mut job = Modal::running(Some("a long job"), true);
    job.status_text_1 = Some("working".into());
    job.popup_gauge_1 = Some((1, 4));
    let key = m.publish(job);
    pump_until("the dialog opens", || controller.dialog().is_some());
    let dialog = controller.dialog().unwrap();
    assert_eq!(dialog.get_window_title(), "a long job");
    assert!(!dialog.get_hide_close_button(), "it can be cancelled");
    let popup = dialog.get_popup();
    assert_eq!(popup.text_1, "working");
    assert!(popup.has_gauge_1 && (popup.gauge_1 - 0.25).abs() < 1e-6);
    assert!(popup.cancellable);
    assert!(
        m.stacked().is_empty() && m.ui.get_popups().row_count() == 0,
        "not in the popups while its dialog is up"
    );

    // the dialog follows the job
    m.change(key, |j| j.status_text_1 = Some("still working".into()));
    pump_until("the text follows", || {
        dialog.get_popup().text_1 == "still working"
    });

    // closing a running, cancellable job asks; no leaves it running
    dialog.invoke_close_clicked();
    let question = controller.question().expect("asked");
    assert_eq!(question.get_message(), CANCEL_QUESTION);
    assert_eq!(question.get_window_title(), "Are you sure?");
    question.invoke_cancelled();
    assert!(controller.question().is_none());
    assert!(controller.dialog().is_some(), "answered no: still up");
    assert!(!m.job(key).cancelled);
    assert!(m.stacked().is_empty());

    // yes cancels it, releases it to the popups, and closes the dialog
    dialog.invoke_close_clicked();
    controller.question().unwrap().invoke_chosen(0);
    assert!(m.job(key).cancelled);
    assert!(controller.dialog().is_none());
    assert!(!m.job(key).held_by_modal);
    assert_eq!(m.stacked(), [key]);
    pump_until("the stack has it", || m.ui.get_popups().row_count() == 1);
}

// leaf: audit-options-popups-modal
#[test]
fn a_modal_job_that_cannot_be_cancelled_warns_and_the_dialog_closes_itself_when_it_finishes() {
    let m = start();
    let controller = m.bound.popup_modal.clone();
    // no title: "important job"; pausable only, so no close button
    let key = m.publish(Modal::running(None, false));
    pump_until("the dialog opens", || controller.dialog().is_some());
    let dialog = controller.dialog().unwrap();
    assert_eq!(dialog.get_window_title(), DEFAULT_TITLE);
    assert!(dialog.get_hide_close_button());

    // (the window's own close still goes through the same question)
    dialog.invoke_close_clicked();
    let warning = hydrus_gui::message_window().expect("warned");
    assert_eq!(warning.get_window_title(), "Warning");
    assert_eq!(warning.get_message(), CANNOT_CANCEL);
    warning.invoke_cancelled();
    assert!(controller.dialog().is_some() && controller.question().is_none());
    assert!(m.job(key).held_by_modal);

    // pausing from the dialog pauses the job
    dialog.invoke_pause_play();
    assert!(m.job(key).paused);
    dialog.invoke_pause_play();

    // it finishes: the dialog closes itself, and the popups have it
    m.change(key, Job::finish);
    pump_until("the dialog closes", || controller.dialog().is_none());
    assert!(!m.job(key).held_by_modal);
    assert_eq!(m.stacked(), [key]);
}

// leaf: audit-options-popups-modal
#[test]
fn a_modal_job_waits_while_the_main_window_is_minimised_or_inactive_or_a_dialog_is_up() {
    let m = start();
    let controller = m.bound.popup_modal.clone();

    // a job already done goes straight to the popups, with no dialog
    let mut done = Job::new(false, false, 0.0);
    done.status_title = Some("done already".into());
    let done_key = m.publish(done);
    pump_until("released", || m.stacked() == [done_key]);
    assert!(controller.dialog().is_none());

    // not the active window: it waits
    controller.set_active(Some(false));
    let first = m.publish(Modal::running(Some("first"), true));
    pump();
    assert!(controller.dialog().is_none());
    assert_eq!(controller.pending(), 1);
    assert!(m.job(first).held_by_modal);
    controller.set_active(Some(true));
    pump_until("it opens once active", || controller.dialog().is_some());
    assert_eq!(controller.pending(), 0);
    assert_eq!(controller.dialog().unwrap().get_window_title(), "first");

    // another dialog is up: the second waits for the first to close
    let second = m.publish(Modal::running(Some("second"), true));
    pump();
    assert_eq!(controller.pending(), 1);
    assert_eq!(controller.dialog().unwrap().get_window_title(), "first");
    m.change(first, Job::finish);
    pump_until("the second opens", || {
        controller
            .dialog()
            .is_some_and(|d| d.get_window_title() == "second")
    });
    assert!(!m.job(first).held_by_modal, "the first was released");
    m.change(second, Job::finish);
    pump_until("closed", || controller.dialog().is_none());

    // minimised: it waits, and opens when the window is back
    m.ui.window().set_minimized(true);
    let third = m.publish(Modal::running(Some("third"), true));
    pump();
    assert!(controller.dialog().is_none());
    assert_eq!(controller.pending(), 1);
    m.ui.window().set_minimized(false);
    pump_until("it opens when restored", || controller.dialog().is_some());
    m.change(third, Job::cancel);
    pump_until("closed", || controller.dialog().is_none());

    // dismissed while it waits: dropped, never shown
    controller.set_active(Some(false));
    let gone = m.publish(Modal::running(Some("gone"), true));
    pump();
    assert_eq!(controller.pending(), 1);
    m.change(gone, |j| j.finish_and_dismiss(None, 0));
    controller.set_active(Some(true));
    pump();
    assert!(controller.dialog().is_none());
    assert_eq!(controller.pending(), 0);
}
