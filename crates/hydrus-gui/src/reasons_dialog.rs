//! The question asked before closing pages when some of them object
//! (`ShowReasonsAndPagesConfirmationDialog`): the message with each
//! objecting page's reason, a "yes" that can only be pressed after a
//! moment, "no", and "no, but show me the pages", which opens a list with a
//! button for each objecting page to go to.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use hydrus_core::pages::PageKey;
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::ChoiceButtonsWindow;
use crate::pages::Objection;

/// How long "yes" stays disabled (`disable_yes_initially`).
pub const YES_DELAY: Duration = Duration::from_millis(1200);
/// The dialog's title.
pub const TITLE: &str = "Are you sure?";
/// Its second "no".
pub const SHOW_PAGES: &str = "no, but show me the pages";
/// The list of pages' title.
pub const LIST_TITLE: &str = "Go To Page";

thread_local! {
    static DELAY: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
    static DIALOG: RefCell<slint::Weak<ChoiceButtonsWindow>> = RefCell::new(slint::Weak::default());
    static LIST: RefCell<slint::Weak<ChoiceButtonsWindow>> = RefCell::new(slint::Weak::default());
}

/// How long "yes" waits now: `YES_DELAY`, unless a test on this thread has
/// set another (a test cannot sit out a second and a bit, nor trust that
/// it will not).
pub fn yes_delay() -> Duration {
    DELAY.with(std::cell::Cell::get).unwrap_or(YES_DELAY)
}

/// Make the dialogs asked from this thread wait this long for "yes" (a
/// test's).
pub fn set_yes_delay(delay: Option<Duration>) {
    DELAY.with(|d| d.set(delay));
}

/// The dialog while it is up (for tests).
pub fn dialog() -> Option<ChoiceButtonsWindow> {
    DIALOG.with(|d| d.borrow().upgrade())
}

/// The list of objecting pages while it is up (for tests).
pub fn list() -> Option<ChoiceButtonsWindow> {
    LIST.with(|d| d.borrow().upgrade())
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

/// Ask. `yes` closes the pages; `show` goes to one of the listed pages.
pub fn open(
    message: &str,
    objections: Vec<Objection>,
    yes_delay: Duration,
    yes: Rc<dyn Fn()>,
    show: Rc<dyn Fn(PageKey)>,
) -> Result<ChoiceButtonsWindow, String> {
    open_with_auto_yes(message, objections, yes_delay, None, yes, show)
}

/// [`open`], answering yes by itself after `auto_yes` (`auto_yes_time`),
/// even if "yes" is still disabled.
pub fn open_with_auto_yes(
    message: &str,
    objections: Vec<Objection>,
    yes_delay: Duration,
    auto_yes: Option<Duration>,
    yes: Rc<dyn Fn()>,
    show: Rc<dyn Fn(PageKey)>,
) -> Result<ChoiceButtonsWindow, String> {
    let window =
        crate::app_title::new::<crate::ChoiceButtonsWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(TITLE.into());
    window.set_message(message.into());
    window.set_choices(strings(["yes", "no", SHOW_PAGES].map(str::to_owned)));
    // (kept alive by the window's handlers)
    let timer = Rc::new(Timer::default());
    if yes_delay.is_zero() {
        window.set_choices_enabled(ModelRc::new(VecModel::from(vec![true, true, true])));
    } else {
        window.set_choices_enabled(ModelRc::new(VecModel::from(vec![false, true, true])));
        timer.start(TimerMode::SingleShot, yes_delay, {
            let weak = window.as_weak();
            move || {
                if let Some(window) = weak.upgrade() {
                    window
                        .set_choices_enabled(ModelRc::new(VecModel::from(vec![true, true, true])));
                }
            }
        });
    }
    let auto_timer = Rc::new(Timer::default());
    let done = Rc::new({
        let weak = window.as_weak();
        let timer = timer.clone();
        let auto_timer = auto_timer.clone();
        move || {
            timer.stop();
            auto_timer.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    if let Some(auto_yes) = auto_yes {
        auto_timer.start(TimerMode::SingleShot, auto_yes, {
            // (weakly: the handlers own `done`, which owns this timer)
            let done = Rc::downgrade(&done);
            let yes = yes.clone();
            let weak = window.as_weak();
            move || {
                if let Some(done) = done.upgrade()
                    && weak.upgrade().is_some_and(|w| w.window().is_visible())
                {
                    done();
                    yes();
                }
            }
        });
    }
    window.on_chosen({
        let done = done.clone();
        let weak = window.as_weak();
        move |choice| {
            let enabled = weak.upgrade().is_some_and(|w| {
                use slint::Model as _;
                w.get_choices_enabled()
                    .row_data(usize::try_from(choice).unwrap_or(0))
                    != Some(false)
            });
            if !enabled {
                return;
            }
            done();
            match choice {
                0 => yes(),
                2 => {
                    let _ = list_of(&objections, &show);
                }
                _ => {}
            }
        }
    });
    window.on_cancelled({
        let done = done.clone();
        move || done()
    });
    window.window().on_close_requested({
        let done = done.clone();
        move || {
            done();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    DIALOG.with(|d| *d.borrow_mut() = window.as_weak());
    Ok(window)
}

/// A button for each objecting page (`reason: page`), which goes to it; the
/// list stays up for the next.
fn list_of(
    objections: &[Objection],
    show: &Rc<dyn Fn(PageKey)>,
) -> Result<ChoiceButtonsWindow, String> {
    let window =
        crate::app_title::new::<crate::ChoiceButtonsWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(LIST_TITLE.into());
    window.set_choices(strings(
        objections
            .iter()
            .map(|o| format!("{}: {}", o.reason, o.page)),
    ));
    let keys: Vec<PageKey> = objections.iter().map(|o| o.key).collect();
    window.on_chosen({
        let show = show.clone();
        move |i| {
            if let Some(key) = usize::try_from(i).ok().and_then(|i| keys.get(i)) {
                show(*key);
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
    LIST.with(|d| *d.borrow_mut() = window.as_weak());
    Ok(window)
}
