//! Database > locations > "manage granularity" (hydrus-gui-model's
//! `database_granularity`, the store's `granularity`): migrating this
//! client's file storage between 2 and 3, or an offline folder's, on a
//! worker with a pausable, cancellable popup, then saying how it went.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hydrus_gui_model::database_granularity as model;
use hydrus_store::Store;
use hydrus_store::granularity::{self, Outcome, Progress};
use hydrus_store::popups;
use slint::ComponentHandle as _;

use crate::GranularityWindow;

/// The window and what it is waiting on.
#[derive(Default)]
pub(crate) struct Slot {
    window: Option<GranularityWindow>,
    question: Option<crate::ChoiceButtonsWindow>,
    timer: Option<slint::Timer>,
}

pub(crate) type Slots = Rc<RefCell<Slot>>;

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

fn granularity_of(store: &Store) -> usize {
    store
        .read(hydrus_store::storage::FileStorage::load)
        .map_or(2, |s| s.granularity())
}

fn show(window: &GranularityWindow, store: &Store) {
    let granularity = granularity_of(store);
    window.set_granularity(i32::try_from(granularity).unwrap_or(2));
    window.set_client_label(model::granularity_label(granularity).into());
}

/// Open the window; `changed` runs after a client migration.
pub(crate) fn open(store: &Arc<Store>, slots: &Slots, changed: Rc<dyn Fn()>) {
    if let Some(window) = slots.borrow().window.as_ref() {
        let _ = window.show();
        return;
    }
    let Ok(window) = GranularityWindow::new() else {
        return;
    };
    window.set_window_title(model::TITLE.into());
    window.set_warning(model::WARNING.into());
    window.set_intro(model::INTRO.into());
    window.set_offline_label(model::OFFLINE.into());
    show(&window, store);
    window.on_action({
        let (store, slots, changed) = (store.clone(), slots.clone(), changed.clone());
        move |action| act(&store, &slots, &changed, action.as_str())
    });
    window.window().on_close_requested({
        let slots = slots.clone();
        move || {
            let mut slot = slots.borrow_mut();
            slot.window = None;
            if let Some(q) = slot.question.take() {
                let _ = q.hide();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    if window.show().is_ok() {
        slots.borrow_mut().window = Some(window);
    }
}

/// Ask with the reference's own button labels; `yes` runs on its yes.
fn ask(slots: &Slots, title: &str, message: &str, yes: &str, no: &str, then: Box<dyn FnOnce()>) {
    let asked = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title,
            message,
            choices: vec![yes.to_owned()],
            no_label: no,
        },
        move |choice| {
            if choice == Some(0) {
                then();
            }
        },
    );
    if let Ok(window) = asked {
        slots.borrow_mut().question = window;
    }
}

fn act(store: &Arc<Store>, slots: &Slots, changed: &Rc<dyn Fn()>, action: &str) {
    let (folder, from, to) = match action {
        "2to3" => (false, 2, 3),
        "3to2" => (false, 3, 2),
        "folder 2to3" => (true, 2, 3),
        "folder 3to2" => (true, 3, 2),
        _ => return,
    };
    if folder {
        let (title, message, yes, no) = model::folder_ready();
        let (store, slots_after) = (store.clone(), slots.clone());
        ask(
            slots,
            title,
            &message,
            yes,
            no,
            Box::new(move || {
                let Some(path) = crate::pick(crate::Pick::Folder, model::PICK_FOLDER)
                    .into_iter()
                    .next()
                else {
                    return;
                };
                let (title, question) = model::folder_check(granularity::estimate(&path), from, to);
                let slots_go = slots_after.clone();
                ask(
                    &slots_after,
                    title,
                    &question,
                    "yes",
                    "no",
                    Box::new(move || {
                        run(
                            &store,
                            &slots_go,
                            Rc::new(|| {}),
                            Job::Folder(path),
                            from,
                            to,
                        );
                    }),
                );
            }),
        );
    } else {
        let (title, message, yes, no) = model::client_question(from);
        let (store, slots_after, changed) = (store.clone(), slots.clone(), changed.clone());
        ask(
            slots,
            title,
            &message,
            yes,
            no,
            Box::new(move || run(&store, &slots_after, changed, Job::Client, from, to)),
        );
    }
}

enum Job {
    Client,
    Folder(std::path::PathBuf),
}

/// Do the migration on a worker; when it ends, say how it went.
#[allow(clippy::cast_precision_loss)] // (seconds)
fn run(store: &Arc<Store>, slots: &Slots, changed: Rc<dyn Fn()>, job: Job, from: usize, to: usize) {
    let report: Arc<Mutex<Option<String>>> = Arc::default();
    let thread_report = report.clone();
    let thread_store = store.clone();
    let title = match &job {
        Job::Client => model::client_title(from, to),
        Job::Folder(_) => model::FOLDER_TITLE.to_owned(),
    };
    std::thread::spawn(move || {
        let store = thread_store;
        let mut popup = popups::Job::new(true, true, now() as f64);
        popup.status_title = Some(title);
        let key = popup.key;
        let at = now();
        let _ = store.write(move |ctx| popups::add(ctx.conn(), &popup, at));
        let (paused, cancelled) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let mut say = |one: Option<String>, two: Option<String>| {
            let at = now();
            let (p, c) = (paused.clone(), cancelled.clone());
            let _ = store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| {
                    if one.is_some() {
                        job.status_text_1 = one;
                    }
                    if two.is_some() {
                        job.status_text_2 = two;
                    }
                    p.store(job.paused, Ordering::Relaxed);
                    c.store(job.cancelled, Ordering::Relaxed);
                })
                .map(|_| ())
            });
        };
        let (is_paused, is_cancelled) = (
            || paused.load(Ordering::Relaxed),
            || cancelled.load(Ordering::Relaxed),
        );
        let started = std::time::Instant::now();
        let mut progress = Progress {
            say: &mut say,
            paused: &is_paused,
            cancelled: &is_cancelled,
        };
        let result: hydrus_store::Result<Outcome> = match &job {
            Job::Client => granularity::granularise_store(&store, from, to, &mut progress),
            Job::Folder(path) => {
                granularity::regranularise(&[path.clone()], &['f', 't'], from, to, &mut progress)
            }
        };
        let seconds = i64::try_from(started.elapsed().as_secs()).unwrap_or(i64::MAX);
        let was_cancelled = cancelled.load(Ordering::Relaxed);
        let text = match (&job, result) {
            (Job::Client, Ok(o)) => {
                model::client_done(from, to, o.moved, seconds, o.weird_dirs, o.weird_files)
            }
            (Job::Folder(_), Ok(o)) => {
                model::folder_done(to, o.moved, seconds, o.weird_dirs, o.weird_files)
            }
            (Job::Client, Err(_)) if was_cancelled => model::CANCELLED.to_owned(),
            (Job::Folder(_), Err(_)) if was_cancelled => model::FOLDER_CANCELLED.to_owned(),
            (Job::Client, Err(e)) => model::client_error(&e.to_string()),
            (Job::Folder(_), Err(e)) => model::folder_error(&e.to_string()),
        };
        let at = now();
        let _ = store.write(move |ctx| {
            popups::update(ctx.conn(), &key, at, |job| {
                job.status_text_1 = Some("done!".into());
                job.status_text_2 = None;
                job.finish_and_dismiss(None, at);
            })
            .map(|_| ())
        });
        *thread_report
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(text);
    });
    if let Some(window) = slots.borrow().window.as_ref() {
        window.set_busy(true);
    }
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        {
            let (slots, store) = (Rc::downgrade(slots), store.clone());
            move || {
                let Some(slots) = slots.upgrade() else {
                    return;
                };
                let Some(text) = report
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                else {
                    return;
                };
                if let Some(window) = slots.borrow().window.as_ref() {
                    window.set_busy(false);
                    show(window, &store);
                }
                crate::debug_actions::message("Information", &text);
                changed();
            }
        },
    );
    slots.borrow_mut().timer = Some(timer);
}
