//! The Database menu's maintenance entries (hydrus-gui-model's
//! `database_maintenance`): the reference's question, then for some the
//! "Which service?" chooser, then the job on a worker thread, its popups
//! going through the store.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::database_maintenance::{self as model, Answer, Asking, Job, WHICH_SERVICE};
use hydrus_store::Store;
use slint::ComponentHandle;

use crate::{ChoiceButtonsWindow, SessionDialog};

/// The open question or chooser, so one job is asked at a time.
#[derive(Default, Clone)]
#[allow(missing_debug_implementations)]
pub struct Slot {
    question: Rc<RefCell<Option<SessionDialog>>>,
    chooser: Rc<RefCell<Option<ChoiceButtonsWindow>>>,
}

impl Slot {
    fn busy(&self) -> bool {
        let visible = |w: Option<bool>| w.unwrap_or(false);
        visible(
            self.question
                .borrow()
                .as_ref()
                .map(|w| w.window().is_visible()),
        ) || visible(
            self.chooser
                .borrow()
                .as_ref()
                .map(|w| w.window().is_visible()),
        )
    }
}

/// Ask about `job` and run it if accepted.
pub fn open(store: &Arc<Store>, slot: &Slot, job: Job) -> Result<(), String> {
    if slot.busy() {
        return Ok(());
    }
    match job.asking() {
        Asking::YesNo { yes, no } => {
            let window = SessionDialog::new().map_err(|e| e.to_string())?;
            window.set_window_title("Are you sure?".into());
            window.set_message(job.question().into());
            window.set_yes_label(yes.into());
            window.set_no_label(no.into());
            window.on_answered({
                let weak = window.as_weak();
                let store = store.clone();
                let slot = slot.clone();
                move |yes| {
                    if let Some(window) = weak.upgrade() {
                        let _ = window.hide();
                    }
                    if yes {
                        after_yes(&store, &slot, job, Answer::default());
                    }
                }
            });
            window.on_cancelled({
                let weak = window.as_weak();
                move || {
                    if let Some(window) = weak.upgrade() {
                        let _ = window.hide();
                    }
                }
            });
            window.show().map_err(|e| e.to_string())?;
            *slot.question.borrow_mut() = Some(window);
        }
        Asking::YesYesNo { yes, no } => {
            let store = store.clone();
            let chooser = crate::choice_buttons::open(
                &crate::choice_buttons::Ask {
                    title: "Are you sure?",
                    message: job.question(),
                    choices: yes.iter().map(|s| (*s).to_owned()).collect(),
                    no_label: no,
                },
                move |choice| {
                    if let Some(choice) = choice {
                        start(
                            &store,
                            job,
                            Answer {
                                full: choice == 1,
                                ..Answer::default()
                            },
                        );
                    }
                },
            )?;
            *slot.chooser.borrow_mut() = chooser;
        }
        Asking::Buttons { title, choices } => {
            let store = store.clone();
            let chooser = crate::choice_buttons::open(
                &crate::choice_buttons::Ask {
                    title,
                    message: job.question(),
                    choices: choices.into_iter().map(|c| c.0).collect(),
                    no_label: "",
                },
                move |choice| {
                    if let Some(choice) = choice {
                        start(
                            &store,
                            job,
                            Answer {
                                tag_definitions: choice == 1,
                                ..Answer::default()
                            },
                        );
                    }
                },
            )?;
            *slot.chooser.borrow_mut() = chooser;
        }
    }
    Ok(())
}

/// After yes: the service chooser where the job asks one, then the job.
fn after_yes(store: &Arc<Store>, slot: &Slot, job: Job, answer: Answer) {
    if !job.chooses_tag_service() {
        start(store, job, answer);
        return;
    }
    let choices = model::service_choices(store);
    let names = choices.iter().map(|c| c.0.clone()).collect();
    let keys: Vec<_> = choices.into_iter().map(|c| c.1).collect();
    let store = store.clone();
    match crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: WHICH_SERVICE,
            message: "",
            choices: names,
            no_label: "",
        },
        move |choice| {
            if let Some(service) = choice.and_then(|i| keys.get(i).cloned()) {
                start(&store, job, Answer { service, ..answer });
            }
        },
    ) {
        Ok(chooser) => *slot.chooser.borrow_mut() = chooser,
        Err(error) => eprintln!("could not ask which service: {error}"),
    }
}

/// Run the job off the UI thread; a clipboard result is copied back on it.
fn start(store: &Arc<Store>, job: Job, answer: Answer) {
    let store = store.clone();
    std::thread::spawn(move || {
        let now = hydrus_core::time::TimestampMs::now().secs();
        match model::run(&store, job, &answer, now) {
            Ok(outcome) => {
                if let Some(text) = outcome.clipboard {
                    let _ = slint::invoke_from_event_loop(move || crate::copy_to_clipboard(&text));
                }
            }
            Err(error) => {
                #[allow(clippy::cast_precision_loss)] // (seconds)
                let popup = hydrus_store::popups::Job::text(error.to_string(), now as f64);
                let _ = store.write(move |ctx| hydrus_store::popups::add(ctx.conn(), &popup, now));
            }
        }
    });
}
