//! Client-exit confirmation shared by the window close button and File > exit.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle as _;

use hydrus_store::Store;

use crate::MainWindow;

const QUESTION: &str = "Are you sure you want to exit the client? (Will auto-yes in 15 seconds)";
type Ask = Rc<dyn Fn(String, Rc<dyn Fn()>)>;

pub(crate) fn bind(window: &MainWindow, store: Arc<Store>, timer: &Rc<slint::Timer>, ask: Ask) {
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            crate::windows::save_named(window.window(), &store, "main_gui");
            let _ = window.hide();
        }
    });
    window.window().on_close_requested({
        let timer = timer.clone();
        let weak = window.as_weak();
        move || {
            let confirm = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::GuiSettings>)
                .is_ok_and(|settings| settings.confirm_exit);
            if confirm {
                ask(QUESTION.into(), close.clone());
                timer.start(slint::TimerMode::SingleShot, Duration::from_secs(15), {
                    let weak = weak.clone();
                    move || {
                        if let Some(window) = weak.upgrade()
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
