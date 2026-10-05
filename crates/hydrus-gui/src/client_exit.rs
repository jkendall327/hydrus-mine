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
    static MAINTENANCE: std::cell::RefCell<Option<crate::SessionDialog>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether the client should start again once it has exited.
pub static RESTART: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Make the next close request exit this way (reset if it is backed out of).
pub fn set_mode(mode: ExitMode) {
    MODE.with(|m| m.set(mode));
}

/// Shutdown maintenance before `then`: run it, ask first, or skip it.
fn shutdown_work(store: &Arc<Store>, mode: ExitMode, then: Rc<dyn Fn()>) {
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
            let answered = Rc::new(Cell::new(false));
            let answer = Rc::new({
                let weak = dialog.as_weak();
                let store = store.clone();
                let answered = answered.clone();
                move |yes: bool| {
                    if answered.replace(true) {
                        return;
                    }
                    if let Some(dialog) = weak.upgrade() {
                        let _ = dialog.hide();
                    }
                    if yes {
                        run();
                    } else if let Err(error) = shutdown_work::register(&store, now) {
                        // (if they said no, don't keep asking)
                        eprintln!("could not register shutdown maintenance: {error}");
                    }
                    then();
                }
            });
            dialog.on_answered({
                let answer = answer.clone();
                move |yes| answer(yes)
            });
            dialog.on_cancelled({
                let answer = answer.clone();
                move || answer(false)
            });
            let timer = slint::Timer::default();
            timer.start(slint::TimerMode::SingleShot, Duration::from_secs(15), {
                let answer = answer.clone();
                move || answer(false)
            });
            if dialog.show().is_ok() {
                // (the timer lives as long as the question)
                std::mem::forget(timer);
                MAINTENANCE.with(|m| *m.borrow_mut() = Some(dialog));
            } else {
                answer(false);
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
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let store = store.clone();
        let active = active.clone();
        move || {
            let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && window.window().is_visible())
            else {
                return;
            };
            let mode = MODE.with(|m| m.replace(ExitMode::Exit));
            let weak = window.as_weak();
            let finished = finished.clone();
            let store_after = store.clone();
            shutdown_work(
                &store,
                mode,
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
            let question = MODE.with(Cell::get).question();
            if confirm {
                ask(question.into(), close.clone());
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
                close();
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
                hydrus_store::settings::set(ctx.conn(), &settings)
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
}
