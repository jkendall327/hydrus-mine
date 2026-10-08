//! Database > file maintenance > "clear orphan files" (hydrus-gui-model's
//! `orphan_files`, the store's `orphan_files`): the choice of moving or
//! deleting, then the scan and clear on a worker with a cancellable
//! "clearing orphans" popup.
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hydrus_gui_model::orphan_files as model;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

thread_local! {
    static CHOOSER: RefCell<Option<crate::ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

/// The question now shown, if any (for tests of the entry).
pub fn chooser() -> Option<crate::ChoiceButtonsWindow> {
    use slint::ComponentHandle as _;
    CHOOSER.with(|c| {
        c.borrow()
            .as_ref()
            .map(crate::ChoiceButtonsWindow::clone_strong)
    })
}

/// Ask, then (moving somewhere chosen, or deleting) start.
pub(crate) fn open(store: &Arc<Store>) {
    let store = store.clone();
    let asked = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: "Are you sure?",
            message: model::QUESTION,
            choices: model::CHOICES.iter().map(|c| (*c).to_owned()).collect(),
            no_label: model::NO,
        },
        move |choice| match choice {
            Some(0) => {
                if let Some(to) = crate::pick(crate::Pick::Folder, model::PICK_TITLE)
                    .into_iter()
                    .next()
                {
                    start(&store, Some(to));
                }
            }
            Some(1) => start(&store, None),
            _ => {}
        },
    );
    if let Ok(window) = asked {
        CHOOSER.with(|c| *c.borrow_mut() = window);
    }
}

#[allow(clippy::cast_precision_loss)] // (seconds)
fn start(store: &Arc<Store>, to: Option<std::path::PathBuf>) {
    let store = store.clone();
    std::thread::spawn(move || {
        let mut job = Job::new(false, true, now() as f64);
        job.status_title = Some(model::POPUP_TITLE.into());
        job.status_text_1 = Some("preparing".into());
        let key = job.key;
        let at = now();
        if store
            .write(move |ctx| popups::add(ctx.conn(), &job, at))
            .is_err()
        {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let update = |one: Option<String>, two: Option<Option<String>>| {
            let at = now();
            let flag = cancelled.clone();
            let _ = store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| {
                    if let Some(one) = one {
                        job.status_text_1 = Some(one);
                    }
                    if let Some(two) = two {
                        job.status_text_2 = two;
                    }
                    if job.cancelled {
                        flag.store(true, Ordering::Relaxed);
                    }
                })
                .map(|_| ())
            });
        };
        let is_cancelled = || cancelled.load(Ordering::Relaxed);
        let scanned = hydrus_store::orphan_files::scan(
            &store,
            &mut |one, two| update(Some(one), two.map(Some)),
            &is_cancelled,
        );
        let text = match scanned {
            Err(_) if is_cancelled() => return,
            Err(e) => format!("could not clear the orphans: {e}"),
            Ok(found) => {
                update(Some("finished checking".into()), Some(None));
                if to.is_none() && !found.orphan_files.is_empty() {
                    update(Some(model::found(found.orphan_files.len(), "files")), None);
                }
                match hydrus_store::orphan_files::clear(
                    &store,
                    &found,
                    to.as_deref(),
                    &mut |text| update(Some(text), None),
                    &is_cancelled,
                ) {
                    Ok(()) => {
                        model::final_text(found.orphan_files.len(), found.orphan_thumbnails.len())
                    }
                    Err(e) => e.to_string(),
                }
            }
        };
        let at = now();
        let _ = store.write(move |ctx| {
            popups::update(ctx.conn(), &key, at, |job| {
                job.status_text_1 = Some(text);
                job.status_text_2 = None;
                job.finish();
            })
            .map(|_| ())
        });
    });
}
