//! A choice among buttons (`SelectFromListButtons`, `GetYesYesNo`): shown
//! until a choice, a "no" or a close, then answering once.
use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::ChoiceButtonsWindow;

/// What the chooser asks.
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
    ask: &Ask,
    answer: impl FnOnce(Option<usize>) + 'static,
) -> Result<Option<ChoiceButtonsWindow>, String> {
    if ask.choices.len() == 1 && ask.no_label.is_empty() {
        answer(Some(0));
        return Ok(None);
    }
    let window = ChoiceButtonsWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(ask.title.into());
    window.set_message(ask.message.into());
    window.set_no_label(ask.no_label.into());
    window.set_choices(ModelRc::new(VecModel::from(
        ask.choices
            .iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    let answer: Rc<RefCell<Option<Box<dyn FnOnce(Option<usize>)>>>> =
        Rc::new(RefCell::new(Some(Box::new(answer))));
    let finish = Rc::new({
        let weak = window.as_weak();
        move |choice: Option<usize>| {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            let taken = answer.borrow_mut().take();
            if let Some(answer) = taken {
                answer(choice);
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
    Ok(Some(window))
}
