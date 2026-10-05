//! Help > debug's actions (hydrus-gui-model's `debug_actions`): messages,
//! popups, the delayed "modal" popup (an ordinary popup here, counting
//! down), saving the last session, the database checkpoint, the
//! environment dump, clearing the rendering caches and leaving the event
//! loop at once.
use std::cell::RefCell;
use std::io::Write as _;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hydrus_core::Sha256;
use hydrus_gui_model::debug_actions::{self as model, Action};
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

use crate::ChoiceButtonsWindow;

thread_local! {
    /// The message shown, kept open until answered.
    static MESSAGE: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// What the actions work with.
pub(crate) struct Context {
    pub pages: Rc<RefCell<crate::Pages>>,
    pub ask: crate::menu_bar::Ask,
    /// Clear the image, tile and thumbnail caches.
    pub clear_caches: Rc<dyn Fn()>,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

#[allow(clippy::cast_possible_truncation)] // (seconds)
fn post(store: &Store, jobs: Vec<Job>) {
    let at = now() as i64;
    let done = store.write(move |ctx| {
        for job in &jobs {
            popups::add(ctx.conn(), job, at)?;
        }
        Ok(())
    });
    if let Err(e) = done {
        eprintln!("could not show the debug popups: {e}");
    }
}

/// An information or warning message with an "ok" button.
pub(crate) fn message(title: &str, text: &str) {
    let ask = crate::choice_buttons::Ask {
        title,
        message: text,
        choices: Vec::new(),
        no_label: "ok",
    };
    match crate::choice_buttons::open(&ask, |_| {}) {
        Ok(window) => MESSAGE.with(|m| *m.borrow_mut() = window),
        Err(e) => eprintln!("could not show the message: {e}"),
    }
}

/// `_DebugMakeDelayedModalPopup`'s thread: after five seconds, ten seconds
/// counting down (stopping if cancelled), then gone.
#[allow(clippy::cast_possible_truncation)] // (seconds)
fn modal(store: &Store, cancellable: bool) {
    std::thread::sleep(Duration::from_secs(5));
    let mut job = Job::new(false, cancellable, now());
    job.status_title = Some(model::MODAL_TITLE.into());
    let key = job.key;
    post(store, vec![job]);
    for i in 0..10 {
        let at = now() as i64;
        let cancelled = store
            .write(move |ctx| {
                // (gone, as dismissed, counts as cancelled)
                Ok(popups::update(ctx.conn(), &key, at, |job| {
                    job.status_text_1 = Some(model::modal_text(i));
                    job.popup_gauge_1 = Some((i, 10));
                    job.cancelled
                })?
                .unwrap_or(true))
            })
            .unwrap_or(true);
        if cancelled {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let at = now() as i64;
    let _ = store.write(move |ctx| {
        popups::update(ctx.conn(), &key, at, |job| job.finish_and_dismiss(None, at)).map(|_| ())
    });
}

pub(crate) fn run(context: &Context, action: Action) {
    let store = context.pages.borrow().store().clone();
    match action {
        Action::ProfileInfo => message("Information", model::PROFILE_MESSAGE),
        Action::MessageBox => message("Warning", model::MESSAGE_BOX_TEXT),
        Action::SomePopups => {
            post(
                &store,
                model::some_popups(now(), || Sha256(rand::random::<[u8; 32]>())),
            );
            for i in 1..4 {
                let store = store.clone();
                slint::Timer::single_shot(Duration::from_millis(500 * i as u64), move || {
                    post(&store, vec![Job::text(model::delayed_popup_text(i), now())]);
                });
            }
        }
        Action::ModalPopup { cancellable } => {
            std::thread::spawn(move || modal(&store, cancellable));
        }
        // (hydrus-rs keeps no column widths to reset)
        Action::ResetColumns => (context.ask)(model::RESET_COLUMNS_QUESTION.into(), Rc::new(|| {})),
        Action::SaveLastSession => {
            #[allow(clippy::cast_possible_truncation)] // (seconds)
            let at = now() as i64;
            if let Err(e) = context.pages.borrow_mut().save(at) {
                eprintln!("could not save the last session: {e}");
            }
        }
        Action::FlushLog => {
            eprintln!("{}", model::FLUSH_LOG);
            let _ = std::io::stderr().flush();
        }
        Action::ForceCommit => {
            let done = store.write(|ctx| {
                ctx.conn()
                    .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
                    .map_err(Into::into)
            });
            if let Err(e) = done {
                eprintln!("could not commit the database: {e}");
            }
        }
        Action::ShowEnv => {
            let separator = if cfg!(windows) { ';' } else { ':' };
            let text = model::env_text(std::env::vars(), separator);
            eprintln!("{text}");
            post(&store, vec![Job::text(text, now())]);
        }
        Action::Exit => {
            let _ = slint::quit_event_loop();
        }
        Action::ClearRenderingCaches => (context.clear_caches)(),
    }
}
