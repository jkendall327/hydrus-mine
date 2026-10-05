//! Client-exit confirmation shared by the window close button and File > exit.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle as _;

use hydrus_store::Store;

use crate::MainWindow;

const QUESTION: &str = "Are you sure you want to exit the client? (Will auto-yes in 15 seconds)";
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
            crate::windows::save_named(window.window(), &store, "main_gui");
            finished();
            let _ = window.hide();
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
            if confirm {
                ask(QUESTION.into(), close.clone());
                timer.start(slint::TimerMode::SingleShot, timeout, {
                    let weak = weak.clone();
                    let active = active.clone();
                    move || {
                        if active.get()
                            && let Some(window) = weak.upgrade()
                            && window.window().is_visible()
                            && window.get_question() == QUESTION
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
