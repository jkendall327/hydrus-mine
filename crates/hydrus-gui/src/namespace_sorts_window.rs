//! Independent staged namespace queue; only child Apply returns an Options draft.
use crate::{NamespaceSortsWindow, TableRow};
use hydrus_core::pages::PageSort;
use hydrus_gui_model::namespace_sorts::{Editor, Prompt, TEXT_MESSAGE, VIEW_MESSAGE, VIEWS};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Slot = Rc<RefCell<Option<NamespaceSortsWindow>>>;
pub type Applied = Rc<dyn Fn(Vec<PageSort>) -> Result<(), String>>;
thread_local! {static LAST:RefCell<Option<slint::Weak<NamespaceSortsWindow>>>=const {RefCell::new(None)};}
pub fn last_opened() -> Option<NamespaceSortsWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
}
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}
fn busy(window: &NamespaceSortsWindow) -> bool {
    window.get_asking_text() || window.get_choosing_view() || window.get_confirming()
}
fn show(window: &NamespaceSortsWindow, editor: &Editor) {
    let rows = editor.rows();
    window.set_has_selection(rows.iter().any(|row| row.selected));
    window.set_rows(ModelRc::new(VecModel::from(
        rows.into_iter()
            .map(|row| TableRow {
                cells: ModelRc::new(VecModel::from(vec![SharedString::from(row.label())])),
                selected: row.selected,
            })
            .collect::<Vec<_>>(),
    )));
}
fn prompt(window: &NamespaceSortsWindow, next: Prompt) {
    window.set_asking_text(false);
    window.set_choosing_view(false);
    match next {
        Prompt::Text(default) => {
            window.set_window_title("Enter text".into());
            window.set_message(TEXT_MESSAGE.into());
            window.set_text(default.into());
            window.set_asking_text(true);
        }
        Prompt::View => {
            window.set_window_title(hydrus_gui_model::namespace_sorts::VIEW_TITLE.into());
            window.set_message(VIEW_MESSAGE.into());
            window.set_choosing_view(true);
        }
        Prompt::Done => {
            window.set_window_title("namespace file sorting".into());
        }
    }
}
pub(crate) fn open(
    sorts: &[PageSort],
    advanced: bool,
    slot: &Slot,
    applied: Applied,
) -> Result<(), String> {
    if slot.borrow().is_some() {
        return Ok(());
    }
    let window = NamespaceSortsWindow::new().map_err(|error| error.to_string())?;
    let editor = Rc::new(RefCell::new(Editor::new(sorts)));
    let active = Rc::new(Cell::new(true));
    let removal: Rc<RefCell<Option<Vec<u64>>>> = Rc::default();
    window.set_views(ModelRc::new(VecModel::from(
        VIEWS
            .iter()
            .map(|(label, _)| SharedString::from(*label))
            .collect::<Vec<_>>(),
    )));
    let close = {
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let active = active.clone();
        let editor = editor.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            editor.borrow_mut().cancel();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    };
    window.on_clicked({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move |index, ctrl, shift| {
            if let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && !busy(window))
                && let Ok(index) = usize::try_from(index)
            {
                editor.borrow_mut().click(index, ctrl, shift);
                show(&window, &editor.borrow());
            }
        }
    });
    window.on_action({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        let removal = removal.clone();
        move |action| {
            let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && !busy(window))
            else {
                return;
            };
            match action.as_str() {
                "add" | "edit" => {
                    let next = editor.borrow_mut().begin(action == "edit", advanced);
                    prompt(&window, next);
                }
                "up" | "down" => editor.borrow_mut().move_selected(action == "down"),
                "delete" => {
                    if let Some((ids, message)) = editor.borrow().delete_request() {
                        *removal.borrow_mut() = Some(ids);
                        window.set_message(message.into());
                        window.set_window_title("Are you sure?".into());
                        window.set_confirming(true);
                    }
                }
                _ => {}
            }
            show(&window, &editor.borrow());
        }
    });
    window.on_text_entered({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && window.get_asking_text())
            {
                let text = window.get_text();
                if text.is_empty() {
                    return;
                }
                let next = editor.borrow_mut().text(Some(&text));
                prompt(&window, next);
                show(&window, &editor.borrow());
            }
        }
    });
    window.on_view_chosen({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && window.get_choosing_view())
            {
                let next = editor.borrow_mut().view(usize::try_from(index).ok());
                prompt(&window, next);
                show(&window, &editor.borrow());
            }
        }
    });
    window.on_cancel_prompt({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade().filter(|_| active.get()) {
                editor.borrow_mut().cancel();
                prompt(&window, Prompt::Done);
            }
        }
    });
    window.on_answer({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move |accepted| {
            if let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && window.get_confirming())
            {
                if let Some(ids) = removal.borrow_mut().take()
                    && accepted
                {
                    editor.borrow_mut().delete(&ids);
                }
                window.set_confirming(false);
                window.set_window_title("namespace file sorting".into());
                show(&window, &editor.borrow());
            }
        }
    });
    window.on_apply({
        let editor = editor.clone();
        let active = active.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            let Some(window) = weak
                .upgrade()
                .filter(|window| active.get() && !busy(window))
            else {
                return;
            };
            let value = editor.borrow().value();
            match applied(value) {
                Ok(()) => close(),
                Err(error) => window.set_error(error.into()),
            }
        }
    });
    window.on_cancel(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    show(&window, &editor.borrow());
    window.show().map_err(|error| error.to_string())?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window);
    Ok(())
}
