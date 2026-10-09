//! The thumbnail menu's manage > maintenance and viewing stats > clear
//! (hydrus-gui-model's `thumbnail_maintenance`): the reference's question,
//! then the job run now on a worker thread (or scheduled), or the selected
//! files' viewing records cleared.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui_model::thumbnail_maintenance::{
    Asking, NOW_OR_LATER, clear_viewing_stats_question, regenerate_question,
};
use hydrus_store::Store;
use hydrus_store::file_maintenance::JobType;
use slint::ComponentHandle;

use crate::{ChoiceButtonsWindow, SessionDialog};

thread_local! {
    // (the question shown, kept until answered)
    static QUESTION: RefCell<Option<SessionDialog>> = const { RefCell::new(None) };
    static CHOOSER: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// The yes/no question being asked, if one is open (tests answer it).
pub fn question() -> Option<SessionDialog> {
    QUESTION.with(|q| q.borrow().as_ref().map(ComponentHandle::clone_strong))
}

/// Ask yes/no, then `yes`.
fn ask(message: &str, yes: impl Fn() + 'static) {
    let Ok(window) = crate::app_title::new::<crate::SessionDialog>() else {
        return;
    };
    window.set_window_title("Are you sure?".into());
    window.set_message(message.into());
    window.set_yes_label("yes".into());
    window.set_no_label("no".into());
    let weak = window.as_weak();
    window.on_answered(move |answer| {
        if let Some(window) = weak.upgrade() {
            let _ = window.hide();
        }
        if answer {
            yes();
        }
    });
    let weak = window.as_weak();
    window.on_cancelled(move || {
        if let Some(window) = weak.upgrade() {
            let _ = window.hide();
        }
    });
    if window.show().is_ok() {
        QUESTION.with(|q| *q.borrow_mut() = Some(window));
    }
}

/// Run `job` on `files` now, off the UI thread, then tell `changed`.
fn run_now(store: &Arc<Store>, files: Vec<HashId>, job: JobType, changed: Rc<dyn Fn()>) {
    let store = store.clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = done.clone();
    let spawned = std::thread::Builder::new()
        .name("thumbnail-maintenance".into())
        .spawn(move || {
            let n = files.len() as u64;
            let selected = files.clone();
            let queued = store.write(move |ctx| {
                hydrus_store::file_maintenance::add_jobs(ctx.conn(), &files, job, 0)
            });
            let ran = queued.map_err(|e| e.to_string()).and_then(|()| {
                hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
                    .run_file_maintenance_for_files(&selected, n, u64::MAX, &|j| j == job)
                    .map_err(|e| e.to_string())
            });
            if let Err(error) = ran {
                eprintln!("could not run file maintenance: {error}");
            }
            finished.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    if spawned.is_err() {
        return;
    }
    // (the page is told once the work is done)
    let timer: Rc<RefCell<Option<slint::Timer>>> =
        Rc::new(RefCell::new(Some(slint::Timer::default())));
    let held = timer.clone();
    if let Some(t) = timer.borrow().as_ref() {
        t.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(250),
            move || {
                if done.load(std::sync::atomic::Ordering::SeqCst) {
                    changed();
                    // (stopped, not dropped, inside its own callback)
                    if let Some(t) = held.borrow().as_ref() {
                        t.stop();
                    }
                }
            },
        );
    }
}

/// Ask about, then run or schedule, `job` on the selected `files`.
pub fn regenerate(store: &Arc<Store>, files: Vec<HashId>, job: JobType, changed: Rc<dyn Fn()>) {
    let Some(question) = regenerate_question(job, files.len()) else {
        return;
    };
    let store = store.clone();
    match question {
        Asking::YesNo(message) => ask(&message, move || {
            run_now(&store, files.clone(), job, changed.clone());
        }),
        Asking::NowOrLater(message) => {
            let chooser = crate::choice_buttons::open(
                &crate::choice_buttons::Ask {
                    title: "Are you sure?",
                    message: &message,
                    choices: NOW_OR_LATER.iter().map(|s| (*s).to_owned()).collect(),
                    no_label: "forget it",
                },
                move |choice| match choice {
                    Some(0) => run_now(&store, files, job, changed),
                    Some(_) => {
                        let now = hydrus_core::time::TimestampMs::now().secs();
                        if let Err(error) = store.write(move |ctx| {
                            hydrus_store::file_maintenance::add_jobs(ctx.conn(), &files, job, now)
                        }) {
                            eprintln!("could not schedule file maintenance: {error}");
                        }
                    }
                    None => {}
                },
            );
            match chooser {
                Ok(chooser) => CHOOSER.with(|c| *c.borrow_mut() = chooser),
                Err(error) => eprintln!("could not ask about file maintenance: {error}"),
            }
        }
    }
}

/// Ask, then clear the selected `files`' viewing records.
pub fn clear_viewing_stats(store: &Arc<Store>, files: Vec<HashId>) {
    let Some(message) = clear_viewing_stats_question(files.len()) else {
        return;
    };
    let store = store.clone();
    ask(&message, move || {
        let files = files.clone();
        if let Err(error) = store.write(move |ctx| {
            hydrus_store::viewing_maintenance::clear_files(ctx.conn(), &files).map(|_| ())
        }) {
            eprintln!("could not clear viewing stats: {error}");
        }
    });
}
