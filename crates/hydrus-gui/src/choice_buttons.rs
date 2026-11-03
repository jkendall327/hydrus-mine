//! A choice among buttons (`SelectFromListButtons`, `GetYesYesNo`): shown
//! until a choice, a "no" or a close, then answering once.
use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::ChoiceButtonsWindow;

/// The answer callback, taken once.
type Answer = Box<dyn FnOnce(Option<usize>)>;

/// What the chooser asks.
#[derive(Debug)]
pub struct Ask<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub choices: Vec<String>,
    /// No "no" button when empty.
    pub no_label: &'a str,
}

/// Show the chooser; `answer` gets the chosen index, or none on "no" or close.
/// With one choice and no "no" button it answers at once, as
/// `allow_insta_one_item_select` does.
pub fn open(
    ask: &Ask<'_>,
    answer: impl FnOnce(Option<usize>) + 'static,
) -> Result<Option<ChoiceButtonsWindow>, String> {
    if ask.choices.len() == 1 && ask.no_label.is_empty() {
        answer(Some(0));
        return Ok(None);
    }
    let window =
        crate::app_title::new::<crate::ChoiceButtonsWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(ask.title.into());
    window.set_message(ask.message.into());
    window.set_no_label(ask.no_label.into());
    window.set_choices(ModelRc::new(VecModel::from(
        ask.choices
            .iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    // (`SelectFromListButtons`'s `quick_select_dialog` frame with buttons and no
    // "no", or `GetYesNo`/`GetYesYesNo`'s `quick_yesno_dialog` with a "no"
    // too; a message with no choices is not one of those dialogs and has no
    // frame of its own. Kept when a choice is made, as the reference's
    // dialog does when accepted.)
    let frame = match (ask.choices.is_empty(), ask.no_label.is_empty()) {
        (true, _) => None,
        (false, true) => Some("quick_select_dialog"),
        (false, false) => Some("quick_yesno_dialog"),
    };
    let answer: Rc<RefCell<Option<Answer>>> = Rc::new(RefCell::new(Some(Box::new(answer))));
    // (the answer, once: whatever ends the dialog first gives it)
    let give = Rc::new({
        let answer = answer.clone();
        move |choice: Option<usize>| {
            let taken = answer.borrow_mut().take();
            if let Some(answer) = taken {
                answer(choice);
            }
        }
    });
    if let Some((name, store)) = frame.zip(crate::windows::store()) {
        // (an ordinary dialog by its frame: a choice is its apply)
        let ended = crate::dialog::Dialog::named(&window, &store, name);
        ended.on_closed({
            let give = give.clone();
            move || give(None)
        });
        window.on_chosen({
            let ended = ended.clone();
            move |i| {
                let Ok(choice) = usize::try_from(i) else {
                    ended.cancel();
                    return;
                };
                // (taken before it closes, which would answer "none"; then
                // closed, then given, as `exec` returns)
                let taken = answer.borrow_mut().take();
                ended.accept();
                if let Some(answer) = taken {
                    answer(Some(choice));
                }
            }
        });
        window.on_cancelled(move || {
            ended.cancel();
        });
    } else {
        // (a message, or no client running: closed by any of its ends)
        let finish = Rc::new({
            let weak = window.as_weak();
            move |choice: Option<usize>| {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                give(choice);
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
        window.on_escape({
            let finish = finish.clone();
            move || finish(None)
        });
        window.window().on_close_requested(move || {
            finish(None);
            slint::CloseRequestResponse::HideWindow
        });
    }
    window.show().map_err(|e| e.to_string())?;
    Ok(Some(window))
}
