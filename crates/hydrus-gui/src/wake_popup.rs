//! The popup the reference shows when the computer seems to have woken
//! (`ClientController._ShowJustWokeToUser`): "just woke up from sleep",
//! cancellable, saying when input and output are enabled again; cancelling
//! enables them now.
use std::cell::RefCell;
use std::time::Duration;

use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

thread_local! {
    /// The repeating check on the popup while a wake delay runs.
    static WATCH: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `enabling I/O {when}`: the seconds until `ends_ms`, as
/// `TimestampToPrettyTimeDelta( ..., just_now_threshold = 0 )` words them.
fn text(ends_ms: i64, now_ms: i64) -> String {
    let (ends, now) = (ends_ms / 1000, now_ms / 1000);
    if ends <= now {
        "enabling I/O now".to_owned()
    } else {
        format!(
            "enabling I/O in {}",
            hydrus_core::time::pretty_time_delta(ends - now, false)
        )
    }
}

/// Show the popup, and keep it up to date until the delay is over or it is
/// cancelled.
pub(crate) fn begin(store: &Store) {
    if WATCH.with(|w| w.borrow().is_some()) {
        return;
    }
    let Some(ends) = store.wake_ends_at() else {
        return;
    };
    let mut job = Job::new(false, true, now());
    job.status_title = Some("just woke up from sleep".into());
    job.status_text_1 = Some(text(ends, hydrus_core::TimestampMs::now().0));
    let key = job.key;
    #[allow(clippy::cast_possible_truncation)] // (seconds)
    let at = now() as i64;
    if let Err(e) = store.write(move |ctx| popups::add(ctx.conn(), &job, at)) {
        eprintln!("could not show the wake popup: {e}");
        return;
    }
    let weak = store.downgrade();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Some(store) = weak.upgrade() else {
                WATCH.with(|w| *w.borrow_mut() = None);
                return;
            };
            let now_ms = hydrus_core::TimestampMs::now().0;
            let ends = store.wake_ends_at();
            #[allow(clippy::cast_possible_truncation)] // (seconds)
            let at = now() as i64;
            let outcome = store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| {
                    if job.cancelled {
                        job.status_text_1 = Some("enabling I/O now".into());
                        job.finish_and_dismiss(None, at);
                        Some(true)
                    } else if let Some(ends) = ends.filter(|e| *e > now_ms) {
                        // (under a second to go: the words stay as they were)
                        if ends / 1000 > now_ms / 1000 {
                            job.status_text_1 = Some(text(ends, now_ms));
                        }
                        None
                    } else {
                        job.finish_and_dismiss(None, at);
                        Some(false)
                    }
                })
            });
            match outcome {
                Ok(Some(Some(cancelled))) => {
                    if cancelled {
                        store.end_wake_delay();
                    }
                    WATCH.with(|w| *w.borrow_mut() = None);
                }
                Ok(Some(None)) => {}
                // gone (dismissed by hand): the delay runs its course
                _ => WATCH.with(|w| *w.borrow_mut() = None),
            }
        },
    );
    WATCH.with(|w| *w.borrow_mut() = Some(timer));
}
