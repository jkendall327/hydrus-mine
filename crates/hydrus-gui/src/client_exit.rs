//! Client-exit confirmation shared by the window close button and File > exit.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle as _;

use hydrus_store::Store;

use crate::MainWindow;

use hydrus_gui_model::shutdown_work::{self, Decision, ExitMode};

#[cfg(test)]
const QUESTION: &str = ExitMode::Exit.question();

thread_local! {
    /// How the next close exits (File > restart, exit/force maintenance).
    static MODE: Cell<ExitMode> = const { Cell::new(ExitMode::Exit) };
    /// The "Maintenance is due" question, kept while shown.
    static MAINTENANCE: std::cell::RefCell<Option<Rc<MaintenanceQuestion>>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether the client should start again once it has exited.
pub static RESTART: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Make the next close request exit this way (reset if it is backed out of).
pub fn set_mode(mode: ExitMode) {
    MODE.with(|m| m.set(mode));
}

/// The maintenance child owns its timer; callbacks hold only weak backedges.
struct MaintenanceQuestion {
    dialog: crate::SessionDialog,
    timer: slint::Timer,
    answered: Cell<bool>,
}

impl MaintenanceQuestion {
    fn retire(&self) {
        self.answered.set(true);
        self.timer.stop();
        let _ = self.dialog.hide();
    }

    fn current(self: &Rc<Self>) -> bool {
        MAINTENANCE.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, self))
        })
    }

    fn close(self: &Rc<Self>) {
        self.retire();
        if self.current() {
            MAINTENANCE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }
}

fn retire_maintenance_question() {
    let previous = MAINTENANCE.with(|slot| slot.borrow_mut().take());
    if let Some(previous) = previous {
        previous.retire();
    }
}

type ValidExit = Rc<dyn Fn() -> bool>;

/// Shutdown maintenance before `then`: explicit Cancel abandons the exit,
/// whereas No and the reference's 15-second auto-no register and continue.
fn shutdown_work(store: &Arc<Store>, mode: ExitMode, valid: ValidExit, then: Rc<dyn Fn()>) {
    shutdown_work_with_timeout(store, mode, valid, then, Duration::from_secs(15));
}

fn shutdown_work_with_timeout(
    store: &Arc<Store>,
    mode: ExitMode,
    valid: ValidExit,
    then: Rc<dyn Fn()>,
    timeout: Duration,
) {
    if !valid() {
        return;
    }
    retire_maintenance_question();
    let now = hydrus_core::time::TimestampMs::now().secs();
    let settings: hydrus_store::settings::ShutdownWork =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let work = shutdown_work::work_due(store);
    let run = {
        let store = store.clone();
        move || {
            if let Err(error) = shutdown_work::run(&store, now) {
                eprintln!("shutdown maintenance failed: {error}");
            }
        }
    };
    match shutdown_work::decide(&settings, mode, now, &work) {
        Decision::Skip => then(),
        Decision::Run => {
            run();
            then();
        }
        Decision::Ask(text) => {
            let Ok(dialog) = crate::SessionDialog::new() else {
                then();
                return;
            };
            dialog.set_window_title(shutdown_work::ASK_TITLE.into());
            dialog.set_message(text.into());
            let question = Rc::new(MaintenanceQuestion {
                dialog,
                timer: slint::Timer::default(),
                answered: Cell::new(false),
            });
            let answer = Rc::new({
                let weak = Rc::downgrade(&question);
                let store = store.clone();
                move |yes: bool| {
                    let Some(question) = weak.upgrade() else {
                        return;
                    };
                    if question.answered.get() || !question.current() {
                        return;
                    }
                    if !valid() || !question.dialog.window().is_visible() {
                        question.close();
                        return;
                    }
                    question.close();
                    if yes {
                        run();
                    } else if let Err(error) = shutdown_work::register(&store, now) {
                        // Explicit No or auto-no: do not keep asking.
                        eprintln!("could not register shutdown maintenance: {error}");
                    }
                    then();
                }
            });
            question.dialog.on_answered({
                let answer = answer.clone();
                move |yes| answer(yes)
            });
            let cancel: Rc<dyn Fn()> = Rc::new({
                let weak = Rc::downgrade(&question);
                move || {
                    if let Some(question) = weak.upgrade() {
                        question.close();
                    }
                }
            });
            question.dialog.on_cancelled({
                let cancel = cancel.clone();
                move || cancel()
            });
            question.dialog.window().on_close_requested(move || {
                cancel();
                slint::CloseRequestResponse::HideWindow
            });
            MAINTENANCE.with(|slot| *slot.borrow_mut() = Some(question.clone()));
            if question.dialog.show().is_ok() {
                question
                    .timer
                    .start(slint::TimerMode::SingleShot, timeout, move || {
                        answer(false);
                    });
            } else {
                // Failed presentation cannot provide an affirmative answer.
                question.close();
            }
        }
    }
}
type Ask = Rc<dyn Fn(String, Rc<dyn Fn()>)>;

pub(crate) fn bind(
    window: &MainWindow,
    store: Arc<Store>,
    timer: &Rc<slint::Timer>,
    active: Rc<Cell<bool>>,
    ask: Ask,
    finished: Rc<dyn Fn()>,
) {
    bind_with_timeout(
        window,
        store,
        timer,
        active,
        ask,
        finished,
        Duration::from_secs(15),
    );
}

fn bind_with_timeout(
    window: &MainWindow,
    store: Arc<Store>,
    timer: &Rc<slint::Timer>,
    active: Rc<Cell<bool>>,
    ask: Ask,
    finished: Rc<dyn Fn()>,
    timeout: Duration,
) {
    // Rebinding permanently retires the former maintenance child and its timer.
    retire_maintenance_question();
    let close: Rc<dyn Fn(ExitMode)> = Rc::new({
        let weak = window.as_weak();
        let store = store.clone();
        let active = active.clone();
        move |mode: ExitMode| {
            let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && window.window().is_visible())
            else {
                return;
            };
            let weak = window.as_weak();
            let finished = finished.clone();
            let store_after = store.clone();
            shutdown_work(
                &store,
                mode,
                Rc::new({
                    let weak = weak.clone();
                    let active = active.clone();
                    move || {
                        active.get()
                            && weak
                                .upgrade()
                                .is_some_and(|window| window.window().is_visible())
                    }
                }),
                Rc::new(move || {
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    crate::windows::save_named(window.window(), &store_after, "main_gui");
                    if mode == ExitMode::Restart {
                        RESTART.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    finished();
                    let _ = window.hide();
                }),
            );
        }
    });
    window.window().on_close_requested({
        let timer = timer.clone();
        let weak = window.as_weak();
        move || {
            if !active.get()
                || !weak
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
            {
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            let confirm = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::GuiSettings>)
                .is_ok_and(|settings| settings.confirm_exit);
            // (this request's way of exiting, spent now: backing out of a
            // restart leaves the next close a plain exit)
            let mode = MODE.with(|m| m.replace(ExitMode::Exit));
            let question = mode.question();
            if confirm {
                let close = close.clone();
                ask(question.into(), Rc::new(move || close(mode)));
                timer.start(slint::TimerMode::SingleShot, timeout, {
                    let weak = weak.clone();
                    let active = active.clone();
                    move || {
                        if active.get()
                            && let Some(window) = weak.upgrade()
                            && window.window().is_visible()
                            && window.get_question() == question
                        {
                            window.invoke_answer(true);
                        }
                    }
                });
            } else {
                close(mode);
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    type Close = Rc<dyn Fn()>;

    #[test]
    fn a_retained_old_timer_and_close_callback_cannot_finish_the_new_question() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store
            .write(|ctx| {
                let mut settings: hydrus_store::settings::GuiSettings =
                    hydrus_store::settings::get(ctx.conn())?;
                settings.confirm_exit = true;
                hydrus_store::settings::set(ctx.conn(), &settings)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ShutdownWork {
                        action: 0,
                        ..hydrus_store::settings::ShutdownWork::default()
                    },
                )
            })
            .unwrap();
        let _windows = crate::headless::init();
        let window = MainWindow::new().unwrap();
        window.show().unwrap();
        let pending: Rc<RefCell<Option<Close>>> = Rc::default();
        let ask: Ask = Rc::new({
            let pending = pending.clone();
            let weak = window.as_weak();
            move |question, answer| {
                weak.upgrade().unwrap().set_question(question.into());
                *pending.borrow_mut() = Some(answer);
            }
        });
        window.on_answer({
            let pending = pending.clone();
            let weak = window.as_weak();
            move |yes| {
                weak.upgrade().unwrap().set_question("".into());
                let answer = pending.borrow_mut().take();
                if yes && let Some(answer) = answer {
                    answer();
                }
            }
        });
        let old_timer = Rc::new(slint::Timer::default());
        let old_active = Rc::new(Cell::new(true));
        let old_finished = Rc::new(Cell::new(false));
        bind_with_timeout(
            &window,
            store.clone(),
            &old_timer,
            old_active.clone(),
            ask.clone(),
            Rc::new({
                let finished = old_finished.clone();
                move || finished.set(true)
            }),
            Duration::from_millis(1),
        );
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        let old_close = pending.borrow_mut().take().unwrap();
        old_active.set(false);
        let current_timer = Rc::new(slint::Timer::default());
        let current_finished = Rc::new(Cell::new(false));
        bind_with_timeout(
            &window,
            store,
            &current_timer,
            Rc::new(Cell::new(true)),
            ask,
            Rc::new({
                let finished = current_finished.clone();
                move || finished.set(true)
            }),
            Duration::from_secs(3600),
        );
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
        assert_eq!(window.get_question(), QUESTION);
        assert!(pending.borrow().is_some());
        assert!(window.window().is_visible());
        old_close();
        assert!(!old_finished.get() && !current_finished.get());
        assert!(window.window().is_visible());
        window.invoke_answer(true);
        assert!(current_finished.get());
        assert!(!window.window().is_visible());
    }

    fn maintenance_test_store() -> (tempfile::TempDir, Arc<Store>) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        assert!(!shutdown_work::work_due(&store).is_empty());
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ShutdownWork {
                        action: 2,
                        period_seconds: 0,
                        max_minutes: 0,
                        last_done: 0,
                    },
                )
            })
            .unwrap();
        (directory, store)
    }

    fn maintenance_child() -> Rc<MaintenanceQuestion> {
        MAINTENANCE.with(|slot| slot.borrow().as_ref().unwrap().clone())
    }

    fn start_test_maintenance(
        main: &MainWindow,
        store: &Arc<Store>,
        active: Rc<Cell<bool>>,
        finished: Rc<Cell<usize>>,
        timeout: Duration,
    ) {
        let weak = main.as_weak();
        shutdown_work_with_timeout(
            store,
            ExitMode::Exit,
            Rc::new({
                let weak = weak.clone();
                move || active.get() && weak.upgrade().is_some_and(|w| w.window().is_visible())
            }),
            Rc::new(move || {
                finished.set(finished.get() + 1);
                weak.upgrade().unwrap().hide().unwrap();
            }),
            timeout,
        );
    }

    #[test]
    fn maintenance_cancel_and_native_close_abort_exit_but_no_and_timeout_continue_once() {
        let _windows = crate::headless::init();
        let (_directory, store) = maintenance_test_store();
        let main = MainWindow::new().unwrap();
        main.show().unwrap();
        let active = Rc::new(Cell::new(true));
        let finished = Rc::new(Cell::new(0));
        let last_done = || {
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::ShutdownWork>)
                .unwrap()
                .last_done
        };
        for native_close in [false, true] {
            start_test_maintenance(
                &main,
                &store,
                active.clone(),
                finished.clone(),
                Duration::from_millis(1),
            );
            let question = maintenance_child();
            assert!(question.timer.running());
            if native_close {
                question
                    .dialog
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            } else {
                question.dialog.invoke_cancelled();
            }
            assert!(main.window().is_visible());
            assert!(!question.dialog.window().is_visible());
            assert!(!question.timer.running());
            assert!(MAINTENANCE.with(|slot| slot.borrow().is_none()));
            question.dialog.invoke_answered(true);
            std::thread::sleep(Duration::from_millis(20));
            slint::platform::update_timers_and_animations();
            assert_eq!(last_done(), 0, "Cancel must not register maintenance");
            assert_eq!(finished.get(), 0, "Cancel must abandon the exit");
        }
        start_test_maintenance(
            &main,
            &store,
            active.clone(),
            finished.clone(),
            Duration::from_secs(3600),
        );
        let question = maintenance_child();
        question.dialog.invoke_answered(false);
        assert!(!main.window().is_visible());
        assert!(!question.timer.running());
        assert!(last_done() > 0, "No registers the skipped work");
        assert_eq!(finished.get(), 1);
        question.dialog.invoke_answered(true);
        question.dialog.invoke_cancelled();
        assert_eq!(finished.get(), 1);
        store
            .write(|ctx| {
                let mut settings: hydrus_store::settings::ShutdownWork =
                    hydrus_store::settings::get(ctx.conn())?;
                settings.last_done = 0;
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
        main.show().unwrap();
        start_test_maintenance(
            &main,
            &store,
            active,
            finished.clone(),
            Duration::from_millis(1),
        );
        let question = maintenance_child();
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
        assert!(!main.window().is_visible());
        assert!(!question.timer.running());
        assert!(last_done() > 0, "Auto-no registers the skipped work");
        assert_eq!(finished.get(), 2);
        question.dialog.invoke_answered(true);
        assert_eq!(finished.get(), 2);
    }

    #[test]
    fn retired_maintenance_children_cannot_register_or_finish_successor_exit() {
        let _windows = crate::headless::init();
        let (_directory, store) = maintenance_test_store();
        let main = MainWindow::new().unwrap();
        main.show().unwrap();
        let old_active = Rc::new(Cell::new(true));
        let old_finished = Rc::new(Cell::new(0));
        start_test_maintenance(
            &main,
            &store,
            old_active.clone(),
            old_finished.clone(),
            Duration::from_millis(1),
        );
        let old = maintenance_child();
        old_active.set(false);
        old.dialog.invoke_answered(true);
        assert!(main.window().is_visible());
        assert!(!old.timer.running());
        assert_eq!(old_finished.get(), 0);
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::ShutdownWork>)
                .unwrap()
                .last_done,
            0
        );
        let finished = Rc::new(Cell::new(0));
        start_test_maintenance(
            &main,
            &store,
            Rc::new(Cell::new(true)),
            finished.clone(),
            Duration::from_secs(3600),
        );
        let current = maintenance_child();
        old.dialog.invoke_answered(false);
        old.dialog.invoke_cancelled();
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
        assert!(current.dialog.window().is_visible());
        assert!(current.timer.running());
        assert!(current.current());
        assert_eq!(finished.get(), 0);
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::ShutdownWork>)
                .unwrap()
                .last_done,
            0
        );
        // Creating another current exit retires a still-live pending child.
        start_test_maintenance(
            &main,
            &store,
            Rc::new(Cell::new(true)),
            finished.clone(),
            Duration::from_secs(3600),
        );
        let successor = maintenance_child();
        assert!(!current.dialog.window().is_visible());
        assert!(!current.timer.running());
        current.dialog.invoke_answered(true);
        current.dialog.invoke_cancelled();
        assert!(successor.current());
        assert_eq!(finished.get(), 0);
        successor.dialog.invoke_cancelled();
        assert!(main.window().is_visible());
    }
}
