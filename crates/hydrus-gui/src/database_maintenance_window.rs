//! Owner-held Database maintenance confirmations and service choices.
//! Admission ends on Cancel/No/X, hidden input, replacement or binding retirement.
//! Already admitted jobs retain their existing off-UI completion policy.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::database_maintenance::{self as model, Answer, Asking, Job, WHICH_SERVICE};
use hydrus_store::Store;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{ChoiceButtonsWindow, MainWindow, SessionDialog};

enum Dialog {
    Question(SessionDialog),
    Choices(ChoiceButtonsWindow),
}
impl Dialog {
    fn visible(&self) -> bool {
        match self {
            Self::Question(window) => window.window().is_visible(),
            Self::Choices(window) => window.window().is_visible(),
        }
    }
    fn hide(&self) {
        match self {
            Self::Question(window) => {
                let _ = window.hide();
            }
            Self::Choices(window) => {
                let _ = window.hide();
            }
        }
    }
}
struct State {
    parent: slint::Weak<MainWindow>,
    binding_active: Rc<Cell<bool>>,
    live: Cell<bool>,
    serial: Cell<u64>,
    dialog: RefCell<Option<Dialog>>,
}
impl State {
    fn permits(&self) -> bool {
        self.live.get()
            && self.binding_active.get()
            && self.parent.upgrade().is_some_and(|window| {
                window.window().is_visible() && window.get_question().is_empty()
            })
    }
    fn clear(&self) {
        self.serial.set(self.serial.get().wrapping_add(1));
        let dialog = self.dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            dialog.hide();
        }
    }
    fn cancel(&self, serial: u64) {
        if self.serial.get() == serial {
            self.clear();
        }
    }
    fn finish(&self, serial: u64) -> bool {
        if self.serial.get() != serial {
            return false;
        }
        let valid = self.permits() && self.dialog.borrow().as_ref().is_some_and(Dialog::visible);
        self.clear();
        valid
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(dialog) = self.dialog.get_mut().take() {
            dialog.hide();
        }
    }
}

/// Current confirmation owner. Callbacks hold Weak state and Weak Store only.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct Slot(Rc<State>);
impl Slot {
    pub(crate) fn new(parent: &MainWindow, binding_active: Rc<Cell<bool>>) -> Self {
        Self(Rc::new(State {
            parent: parent.as_weak(),
            binding_active,
            live: Cell::new(true),
            serial: Cell::new(0),
            dialog: RefCell::default(),
        }))
    }
    /// The current confirmation, for native owner and cancellation regressions.
    pub fn question(&self) -> Option<SessionDialog> {
        match self.0.dialog.borrow().as_ref() {
            Some(Dialog::Question(window)) => Some(window.clone_strong()),
            _ => None,
        }
    }
    /// The current initial/service chooser, for native decision regressions.
    pub fn chooser(&self) -> Option<ChoiceButtonsWindow> {
        match self.0.dialog.borrow().as_ref() {
            Some(Dialog::Choices(window)) => Some(window.clone_strong()),
            _ => None,
        }
    }
    /// Retire pending decisions permanently; admitted background jobs continue.
    pub fn retire(&self) {
        self.0.live.set(false);
        self.0.clear();
    }
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
}
/// Last Bound clone retires pending decisions even if the Slot is retained.
pub(crate) struct Owner(Slot);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}

/// Ask about `job` and admit it at most once from its current visible owner.
pub fn open(store: &Arc<Store>, slot: &Slot, job: Job) -> Result<(), String> {
    if !slot.0.permits() {
        slot.0.clear();
        return Ok(());
    }
    if slot.0.dialog.borrow().as_ref().is_some_and(Dialog::visible) {
        return Ok(());
    }
    // A hidden predecessor is terminal, including after a later manual re-show.
    slot.0.clear();
    match job.asking() {
        Asking::YesNo { yes, no } => {
            let window = crate::app_title::new::<crate::SessionDialog>().map_err(|e| e.to_string())?;
            window.set_window_title("Are you sure?".into());
            window.set_message(job.question().into());
            window.set_yes_label(yes.into());
            window.set_no_label(no.into());
            let serial = slot.0.serial.get();
            window.on_answered({
                let state = Rc::downgrade(&slot.0);
                let store = Arc::downgrade(store);
                move |yes| {
                    let Some(state) = state.upgrade() else { return };
                    if state.finish(serial)
                        && yes
                        && let Some(store) = store.upgrade()
                    {
                        after_yes(&store, &Slot(state), job, Answer::default());
                    }
                }
            });
            let cancel: Rc<dyn Fn()> = Rc::new({
                let state = Rc::downgrade(&slot.0);
                move || {
                    if let Some(state) = state.upgrade() {
                        state.cancel(serial);
                    }
                }
            });
            window.on_cancelled({
                let cancel = cancel.clone();
                move || cancel()
            });
            window.on_force_close({
                let cancel = cancel.clone();
                move || cancel()
            });
            window.window().on_close_requested(move || {
                cancel();
                slint::CloseRequestResponse::HideWindow
            });
            window.show().map_err(|e| e.to_string())?;
            *slot.0.dialog.borrow_mut() = Some(Dialog::Question(window));
        }
        Asking::YesYesNo { yes, no } => {
            let store = Arc::downgrade(store);
            ask_buttons(
                slot,
                &crate::choice_buttons::Ask {
                    title: "Are you sure?",
                    message: job.question(),
                    choices: yes.iter().map(|s| (*s).to_owned()).collect(),
                    no_label: no,
                },
                move |choice| {
                    if let Some(choice) = choice
                        && let Some(store) = store.upgrade()
                    {
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
        }
        Asking::Buttons { title, choices } => {
            let store = Arc::downgrade(store);
            ask_buttons(
                slot,
                &crate::choice_buttons::Ask {
                    title,
                    message: job.question(),
                    choices: choices.into_iter().map(|c| c.0).collect(),
                    no_label: "",
                },
                move |choice| {
                    if let Some(choice) = choice
                        && let Some(store) = store.upgrade()
                    {
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
        }
    }
    Ok(())
}

// The shared chooser hides before its callback, so this owned chooser validates
// the current visible decision before hiding. It preserves one-choice selection.
fn ask_buttons(
    slot: &Slot,
    ask: &crate::choice_buttons::Ask<'_>,
    answer: impl FnOnce(Option<usize>) + 'static,
) -> Result<(), String> {
    if !slot.0.permits() {
        slot.0.clear();
        return Ok(());
    }
    if ask.choices.len() == 1 && ask.no_label.is_empty() {
        answer(Some(0));
        return Ok(());
    }
    let window = crate::app_title::new::<crate::ChoiceButtonsWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(ask.title.into());
    window.set_message(ask.message.into());
    window.set_no_label(ask.no_label.into());
    window.set_choices(ModelRc::new(VecModel::from(
        ask.choices
            .iter()
            .map(|choice| SharedString::from(choice.as_str()))
            .collect::<Vec<_>>(),
    )));
    let serial = slot.0.serial.get();
    let count = ask.choices.len();
    let answer = Rc::new(RefCell::new(Some(answer)));
    let finish = Rc::new({
        let state = Rc::downgrade(&slot.0);
        move |choice: Option<usize>| {
            let Some(state) = state.upgrade() else { return };
            if !state.finish(serial) {
                answer.borrow_mut().take();
                return;
            }
            let taken = answer.borrow_mut().take();
            if let Some(answer) = taken {
                answer(choice.filter(|i| *i < count));
            }
        }
    });
    window.on_chosen({
        let finish = finish.clone();
        move |i| finish(usize::try_from(i).ok())
    });
    window.on_cancelled({
        let finish = finish.clone();
        move || finish(None)
    });
    window.window().on_close_requested(move || {
        finish(None);
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slot.0.dialog.borrow_mut() = Some(Dialog::Choices(window));
    Ok(())
}

fn after_yes(store: &Arc<Store>, slot: &Slot, job: Job, answer: Answer) {
    if !job.chooses_service() {
        start(store, job, answer);
        return;
    }
    let choices = model::service_choices(store, job);
    let names = choices.iter().map(|c| c.0.clone()).collect();
    let keys: Vec<_> = choices.into_iter().map(|c| c.1).collect();
    let store = Arc::downgrade(store);
    if let Err(error) = ask_buttons(
        slot,
        &crate::choice_buttons::Ask {
            title: WHICH_SERVICE,
            message: "",
            choices: names,
            no_label: "",
        },
        move |choice| {
            if let Some(service) = choice.and_then(|i| keys.get(i).cloned())
                && let Some(store) = store.upgrade()
            {
                start(&store, job, Answer { service, ..answer });
            }
        },
    ) {
        eprintln!("could not ask which service: {error}");
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
