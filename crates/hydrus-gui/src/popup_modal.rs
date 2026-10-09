//! Modal popups (the reference's `AddModalMessage` and
//! `PopupMessageDialogPanel`): a job published as modal is kept out of the
//! popups (`held_by_modal`) while its dialog is up, and released to them
//! when the dialog closes. Which of that happens, and when, is
//! hydrus-gui-model's [`popup_modal`](hydrus_gui_model::popup_modal); this
//! runs it four times a second against the store's held jobs.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle as _;

use hydrus_gui_model::popup_modal::{self as model, Conditions, Effect, JobFacts, Key, Modals};
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

use crate::{ChoiceButtonsWindow, MainWindow, PopupModalWindow};

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn facts(job: &Job) -> JobFacts {
    JobFacts {
        key: job.key,
        title: job.status_title.clone(),
        done: job.done,
        dismissed: false,
        cancellable: job.cancellable,
    }
}

#[derive(Default)]
struct State {
    modals: Modals,
    /// The held jobs this has seen, so each is added once.
    known: HashSet<Key>,
    dialog: Option<PopupModalWindow>,
    question: Option<ChoiceButtonsWindow>,
}

/// Runs the held jobs as modals for one main window.
pub struct Controller {
    store: Arc<Store>,
    main: slint::Weak<MainWindow>,
    state: RefCell<State>,
    timer: slint::Timer,
    active: Cell<Option<bool>>,
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("pending", &self.pending())
            .finish_non_exhaustive()
    }
}

impl Controller {
    /// Watch the store for held jobs, for `window`.
    pub(crate) fn bind(window: &MainWindow, store: Arc<Store>) -> Rc<Self> {
        let controller = Rc::new(Self {
            store,
            main: window.as_weak(),
            state: RefCell::default(),
            timer: slint::Timer::default(),
            active: Cell::new(None),
        });
        let weak = Rc::downgrade(&controller);
        controller.timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(250),
            move || {
                if let Some(controller) = weak.upgrade() {
                    controller.tick();
                }
            },
        );
        controller
    }

    /// Say whether the main window is the active one, instead of asking the
    /// window system (for a platform that can't be asked, and for tests).
    pub fn set_active(&self, active: Option<bool>) {
        self.active.set(active);
    }

    /// The dialog up, if one is.
    pub fn dialog(&self) -> Option<PopupModalWindow> {
        self.state
            .borrow()
            .dialog
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }

    /// The question open on the dialog, if one is.
    pub fn question(&self) -> Option<ChoiceButtonsWindow> {
        self.state
            .borrow()
            .question
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }

    /// How many jobs wait for the dialog.
    pub fn pending(&self) -> usize {
        self.state.borrow().modals.pending()
    }

    fn conditions(&self) -> Conditions {
        let main = self.main.upgrade();
        Conditions {
            minimised_or_hidden: main.as_ref().is_none_or(|main| {
                !main.window().is_visible()
                    || crate::popup_freeze::minimized(main.window()) == Some(true)
            }),
            // (this dialog's own is the model's to know)
            dialog_open: false,
            active: self.active.get().unwrap_or_else(|| {
                main.as_ref()
                    .is_some_and(|main| crate::popup_freeze::active(main.window()))
            }),
        }
    }

    fn job(&self, key: &Key) -> Option<Job> {
        let key = *key;
        self.store
            .read(move |conn| popups::get(conn, &key, now()))
            .ok()
            .flatten()
    }

    /// `REPEATINGPageUpdate`'s retry, the new jobs, and `REPEATINGUpdate`.
    pub fn tick(self: &Rc<Self>) {
        if self.main.upgrade().is_none() {
            self.timer.stop();
            return;
        }
        let held = self
            .store
            .read(|conn| popups::held(conn, now()))
            .unwrap_or_default();
        let around = self.conditions();
        let mut effects = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            for job in &held {
                if state.known.insert(job.key) {
                    effects.extend(state.modals.add(&facts(job), &around));
                }
            }
            let open = state.modals.open_key();
            state
                .known
                .retain(|key| held.iter().any(|job| job.key == *key) || Some(*key) == open);
        }
        let retried = self.state.borrow_mut().modals.page_update();
        if let Some(key) = retried
            && let Some(job) = self.job(&key)
        {
            effects.extend(self.state.borrow_mut().modals.add(&facts(&job), &around));
        }
        self.run(effects);
        // the dialog follows its job, and closes itself when it is done
        let open = self.state.borrow().modals.open_key();
        if let Some(key) = open {
            let job = self.job(&key);
            if let Some(job) = &job {
                self.show_job(job);
            }
            // (a job that has gone is as good as done)
            let job_facts = job.as_ref().map_or_else(
                || JobFacts {
                    key,
                    title: None,
                    done: true,
                    dismissed: false,
                    cancellable: false,
                },
                facts,
            );
            let effects = self.state.borrow_mut().modals.tick(&job_facts);
            self.run(effects);
        }
    }

    fn show_job(&self, job: &Job) {
        let Some(dialog) = self.dialog() else { return };
        let figures = hydrus_gui_model::gui_format::preferences(&self.store).figures;
        let view = crate::popups::view_with_figures(job, figures);
        let data = crate::popups::data(
            &view,
            &hydrus_store::popup_width::PopupWidth::default(),
            &[0; 32],
        );
        dialog.set_popup(data);
        dialog.set_hide_close_button(!job.cancellable);
    }

    /// The close button, and the window's close.
    fn close_requested(self: &Rc<Self>) {
        let Some(key) = self.state.borrow().modals.open_key() else {
            return;
        };
        if self.state.borrow().question.is_some() {
            return;
        }
        let job = self.job(&key).as_ref().map_or_else(
            || JobFacts {
                key,
                title: None,
                done: true,
                dismissed: false,
                cancellable: false,
            },
            facts,
        );
        let effects = self.state.borrow_mut().modals.close_requested(&job);
        self.run(effects);
    }

    fn answered(self: &Rc<Self>, yes: bool) {
        self.state.borrow_mut().question = None;
        let effects = self.state.borrow_mut().modals.answered(yes);
        self.run(effects);
    }

    fn change(&self, key: Key, f: impl FnOnce(&mut Job) + Send + 'static) {
        let done = self
            .store
            .write(move |ctx| popups::update(ctx.conn(), &key, now(), f).map(|_| ()));
        if let Err(error) = done {
            eprintln!("could not change the modal popup: {error}");
        }
    }

    fn run(self: &Rc<Self>, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                // (the viewer's playback pauses itself when it is not shown)
                Effect::PauseAllMedia => {}
                Effect::Show {
                    key,
                    title,
                    hide_close_button,
                } => {
                    let Ok(dialog) = crate::app_title::new::<crate::PopupModalWindow>() else {
                        eprintln!("could not show the modal popup");
                        continue;
                    };
                    dialog.set_window_title(title.into());
                    dialog.set_hide_close_button(hide_close_button);
                    let weak = Rc::downgrade(self);
                    dialog.on_close_clicked({
                        let weak = weak.clone();
                        move || {
                            if let Some(me) = weak.upgrade() {
                                me.close_requested();
                            }
                        }
                    });
                    dialog.on_pause_play({
                        let weak = weak.clone();
                        move || {
                            if let Some(me) = weak.upgrade() {
                                me.change(key, Job::pause_play);
                            }
                        }
                    });
                    dialog.on_cancel({
                        let weak = weak.clone();
                        move || {
                            if let Some(me) = weak.upgrade() {
                                me.change(key, Job::cancel);
                            }
                        }
                    });
                    dialog.window().on_close_requested({
                        let weak = weak.clone();
                        move || {
                            if let Some(me) = weak.upgrade() {
                                me.close_requested();
                            }
                            slint::CloseRequestResponse::KeepWindowShown
                        }
                    });
                    if let Some(job) = self.job(&key) {
                        let figures =
                            hydrus_gui_model::gui_format::preferences(&self.store).figures;
                        let view = crate::popups::view_with_figures(&job, figures);
                        dialog.set_popup(crate::popups::data(
                            &view,
                            &hydrus_store::popup_width::PopupWidth::default(),
                            &[0; 32],
                        ));
                    }
                    if let Err(error) = dialog.show() {
                        eprintln!("could not show the modal popup: {error}");
                    }
                    self.state.borrow_mut().dialog = Some(dialog);
                }
                Effect::Release(key) => self.change(key, |job| job.held_by_modal = false),
                Effect::Cancel(key) => self.change(key, Job::cancel),
                Effect::Ask => {
                    let weak = Rc::downgrade(self);
                    let ask = crate::choice_buttons::Ask {
                        title: "Are you sure?",
                        message: model::CANCEL_QUESTION,
                        choices: vec!["yes".to_owned()],
                        no_label: "no",
                    };
                    match crate::choice_buttons::open(&ask, move |choice| {
                        if let Some(me) = weak.upgrade() {
                            me.answered(choice == Some(0));
                        }
                    }) {
                        Ok(window) => self.state.borrow_mut().question = window,
                        Err(error) => eprintln!("could not ask: {error}"),
                    }
                }
                Effect::Warn => {
                    crate::debug_actions::message_then(
                        "Warning",
                        model::CANNOT_CANCEL,
                        Box::new(|| {}),
                    );
                }
                Effect::Close => {
                    let dialog = self.state.borrow_mut().dialog.take();
                    if let Some(dialog) = dialog {
                        let _ = dialog.hide();
                    }
                }
            }
        }
    }
}
