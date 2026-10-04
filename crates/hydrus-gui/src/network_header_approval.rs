//! Automatic questions for custom headers blocking live daemon requests.
//! One worker reads live context snapshots; weak callbacks cannot outlive the desktop.

use crate::{HeaderApprovalWindow, MainWindow};
use crossbeam_channel::{Receiver, Sender};
use hydrus_gui_model::network_sessions::{self as model, HeaderQuestion};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

enum Operation {
    Answer(HeaderQuestion, bool),
    Refresh,
    Stop,
}
enum Event {
    Questions(Result<Vec<HeaderQuestion>, String>),
    Answered(HeaderQuestion, Result<(), String>),
}
struct Worker {
    send: Sender<Operation>,
    receive: Receiver<Event>,
}
impl Worker {
    fn start(store: Arc<Store>) -> Self {
        let (send, operations) = crossbeam_channel::unbounded();
        let (events, receive) = crossbeam_channel::bounded(8);
        std::thread::spawn(move || {
            loop {
                match operations.recv_timeout(Duration::from_millis(500)) {
                    Ok(Operation::Stop)
                    | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Ok(Operation::Answer(question, approved)) => {
                        let result =
                            model::answer_header_question(&store, question.clone(), approved)
                                .map_err(|e| e.to_string());
                        if events.send(Event::Answered(question, result)).is_err() {
                            break;
                        }
                    }
                    Ok(Operation::Refresh) | Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                }
                let questions =
                    model::pending_header_questions(&store, jiff::Timestamp::now().as_second())
                        .map_err(|e| e.to_string());
                if events.send(Event::Questions(questions)).is_err() {
                    break;
                }
            }
        });
        let _ = send.send(Operation::Refresh);
        Self { send, receive }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Operation::Stop);
    }
}

struct Prompt {
    window: HeaderApprovalWindow,
    question: HeaderQuestion,
    active: Rc<Cell<bool>>,
}
impl Drop for Prompt {
    fn drop(&mut self) {
        self.active.set(false);
        let _ = self.window.hide();
    }
}
struct Inner {
    parent: slint::Weak<MainWindow>,
    worker: Worker,
    timer: Timer,
    prompt: RefCell<Option<Prompt>>,
    dismissed: RefCell<Vec<HeaderQuestion>>,
    active: Cell<bool>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.timer.stop();
        self.prompt.borrow_mut().take();
    }
}

/// Retains automatic pending-header questions for the bound desktop window's lifetime.
#[derive(Clone)]
pub struct Monitor(Rc<Inner>);
impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderApprovalMonitor")
            .finish_non_exhaustive()
    }
}
thread_local! {
    static LAST: RefCell<Option<slint::Weak<HeaderApprovalWindow>>> = const { RefCell::new(None) };
}
/// Most recent visible question, for interaction tests.
pub fn last_question() -> Option<HeaderApprovalWindow> {
    LAST.with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
impl Monitor {
    /// Start independent of whether the network review windows are open.
    pub fn bind(parent: &MainWindow, store: Arc<Store>) -> Self {
        let inner = Rc::new(Inner {
            parent: parent.as_weak(),
            worker: Worker::start(store),
            timer: Timer::default(),
            prompt: RefCell::default(),
            dismissed: RefCell::default(),
            active: Cell::new(true),
        });
        inner
            .timer
            .start(TimerMode::Repeated, Duration::from_millis(100), {
                let weak = Rc::downgrade(&inner);
                move || {
                    if let Some(inner) = weak.upgrade() {
                        Self(inner).poll();
                    }
                }
            });
        Self(inner)
    }
    /// Stop polling and invalidate every outstanding question without answering it.
    pub fn close(&self) {
        self.0.active.set(false);
        self.0.timer.stop();
        self.0.prompt.borrow_mut().take();
        let _ = self.0.worker.send.send(Operation::Stop);
    }
    /// Drain worker results and show at most one question at a time.
    pub fn poll(&self) {
        if !self.0.active.get() {
            return;
        }
        if self.0.parent.upgrade().is_none() {
            self.close();
            return;
        }
        let mut questions = None;
        for event in self.0.worker.receive.try_iter() {
            match event {
                Event::Questions(Ok(rows)) => questions = Some(rows),
                Event::Questions(Err(e)) => eprintln!("Could not review pending headers: {e}"),
                Event::Answered(question, result) => {
                    let matches = self
                        .0
                        .prompt
                        .borrow()
                        .as_ref()
                        .is_some_and(|p| p.question == question);
                    if matches {
                        match result {
                            Ok(()) => {
                                self.0.prompt.borrow_mut().take();
                            }
                            Err(e) => {
                                if let Some(prompt) = self.0.prompt.borrow().as_ref() {
                                    prompt.window.set_busy(false);
                                    prompt.window.set_error(e.into());
                                }
                            }
                        }
                    }
                }
            }
        }
        let Some(questions) = questions else {
            return;
        };
        self.0
            .dismissed
            .borrow_mut()
            .retain(|q| questions.contains(q));
        let stale = self
            .0
            .prompt
            .borrow()
            .as_ref()
            .is_some_and(|p| !questions.contains(&p.question) && !p.window.get_busy());
        if stale {
            self.0.prompt.borrow_mut().take();
        }
        if self.0.prompt.borrow().is_some() {
            return;
        }
        let next = questions
            .into_iter()
            .find(|q| !self.0.dismissed.borrow().contains(q));
        if let Some(question) = next {
            self.show(question);
        }
    }
    fn show(&self, question: HeaderQuestion) {
        let window = match HeaderApprovalWindow::new() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Could not open header approval: {e}");
                return;
            }
        };
        window.set_question(question.text().into());
        let active = Rc::new(Cell::new(true));
        window.on_answer({
            let owner = Rc::downgrade(&self.0);
            let active = active.clone();
            let weak = window.as_weak();
            let question = question.clone();
            move |approved| {
                let (Some(owner), Some(w)) = (owner.upgrade(), weak.upgrade()) else {
                    return;
                };
                if !active.get() || !owner.active.get() || w.get_busy() {
                    return;
                }
                w.set_busy(true);
                let _ = owner
                    .worker
                    .send
                    .send(Operation::Answer(question.clone(), approved));
            }
        });
        let dismiss: Rc<dyn Fn()> = Rc::new({
            let owner = Rc::downgrade(&self.0);
            let active = active.clone();
            let question = question.clone();
            move || {
                let Some(owner) = owner.upgrade() else {
                    return;
                };
                if !active.replace(false) {
                    return;
                }
                owner.dismissed.borrow_mut().push(question.clone());
                owner.prompt.borrow_mut().take();
            }
        });
        window.on_later({
            let dismiss = dismiss.clone();
            move || dismiss()
        });
        window.window().on_close_requested(move || {
            dismiss();
            slint::CloseRequestResponse::HideWindow
        });
        LAST.with(|s| *s.borrow_mut() = Some(window.as_weak()));
        *self.0.prompt.borrow_mut() = Some(Prompt {
            window: window.clone_strong(),
            question,
            active,
        });
        if let Err(e) = window.show() {
            eprintln!("Could not show header approval: {e}");
            self.0.prompt.borrow_mut().take();
        }
    }
}
