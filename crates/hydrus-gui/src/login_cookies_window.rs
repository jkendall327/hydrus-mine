//! Cookie name/value matcher lists staged beneath scripts and steps.
use crate::{LoginCookiesWindow, TableRow};
use hydrus_core::url::strings::StringMatch;
use hydrus_gui_model::login_workflows::CookiesEditor;
use hydrus_parse::login::CookieRequirement;
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
/// List and matcher descendants, invalidated together on owner cancellation.
#[derive(Clone, Default)]
pub struct Slots {
    pub window: Rc<RefCell<Option<LoginCookiesWindow>>>,
    pub strings: crate::string_processor_window::Slots,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginCookieSlots")
            .field("open", &self.window.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    pub fn cancel(&self) {
        let window = self
            .window
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_action("cancel".into());
        }
        self.strings.cancel_all();
    }
}
/// Accepted list; the script/step owner remains responsible for persistence.
pub type Applied = Rc<dyn Fn(Vec<CookieRequirement>) -> Result<(), String>>;
fn show(window: &LoginCookiesWindow, editor: &CookiesEditor) {
    let selected = editor.selection.in_order(&editor.order());
    window.set_rows(ModelRc::new(VecModel::from(
        editor
            .order()
            .into_iter()
            .map(|i| {
                let cookie = &editor.rows[i];
                TableRow {
                    cells: ModelRc::new(VecModel::from(vec![
                        cookie.name.describe(false, false).into(),
                        cookie.value.describe(false, false).into(),
                    ])),
                    selected: selected.contains(&i),
                }
            })
            .collect::<Vec<_>>(),
    )));
    window.set_one_selected(selected.len() == 1);
    window.set_any_selected(!selected.is_empty());
}
fn edit_pair(
    window: &LoginCookiesWindow,
    store: &Arc<Store>,
    editor: &Rc<RefCell<CookiesEditor>>,
    active: &Rc<Cell<bool>>,
    strings: &crate::string_processor_window::Slots,
    index: Option<usize>,
) {
    let cookie = index
        .and_then(|i| editor.borrow().rows.get(i).cloned())
        .unwrap_or_else(|| CookieRequirement {
            name: StringMatch::any(),
            value: StringMatch::any(),
            reference_auxiliary: None,
        });
    window.set_child_open(true);
    crate::string_processor_window::open_match(
        store,
        &cookie.name,
        strings,
        Rc::new({
            let weak = window.as_weak();
            let active = active.clone();
            let editor = editor.clone();
            let store = store.clone();
            let strings = strings.clone();
            let value = cookie.value;
            move |name| {
                if !active.get() {
                    return;
                }
                let Some(window) = weak.upgrade() else {
                    return;
                };
                window.set_child_open(true);
                crate::string_processor_window::open_match(
                    &store,
                    &value,
                    &strings,
                    Rc::new({
                        let weak = weak.clone();
                        let active = active.clone();
                        let editor = editor.clone();
                        move |value| {
                            if !active.get() {
                                return;
                            }
                            editor.borrow_mut().put(
                                index,
                                CookieRequirement {
                                    name: name.clone(),
                                    value,
                                    reference_auxiliary: None,
                                },
                            );
                            if let Some(window) = weak.upgrade() {
                                show(&window, &editor.borrow());
                            }
                        }
                    }),
                );
                own_matcher(&window, &strings, "edit match");
            }
        }),
    );
    own_matcher(window, strings, "edit cookie name");
}
fn own_matcher(
    window: &LoginCookiesWindow,
    strings: &crate::string_processor_window::Slots,
    title: &str,
) {
    let child = strings
        .step
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(child) = child {
        child.set_window_title(title.into());
        let weak = window.as_weak();
        child.on_closed(move || {
            if let Some(window) = weak.upgrade() {
                window.set_child_open(false);
            }
        });
    } else {
        window.set_child_open(false);
    }
}
/// Edit complete cookie requirements with the shared permitted-input matcher.
pub fn open(
    store: &Arc<Store>,
    rows: &[CookieRequirement],
    slots: &Slots,
    applied: Applied,
) -> Result<LoginCookiesWindow, slint::PlatformError> {
    if let Some(window) = slots.window.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginCookiesWindow::new()?;
    let editor = Rc::new(RefCell::new(CookiesEditor::new(rows)));
    let active = Rc::new(Cell::new(true));
    show(&window, &editor.borrow());
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let active = active.clone();
        let slot = Rc::downgrade(&slots.window);
        let strings = slots.strings.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            strings.cancel_all();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let active = active.clone();
        let editor = editor.clone();
        move |index, ctrl, shift| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_deleting() || window.get_child_open() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                let mut editor = editor.borrow_mut();
                let order = editor.order();
                editor.selection.click(&order, index, ctrl, shift);
                show(&window, &editor);
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let active = active.clone();
        let close = close.clone();
        let editor = editor.clone();
        let strings = slots.strings.clone();
        let store = store.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if action == "cancel" {
                close();
                return;
            }
            if window.get_child_open() {
                return;
            }
            if window.get_deleting() && !matches!(action.as_str(), "confirm-delete" | "back") {
                return;
            }
            match action.as_str() {
                "add" | "edit" => {
                    let index = if action == "edit" {
                        editor.borrow().selection.one()
                    } else {
                        None
                    };
                    if action == "edit" && index.is_none() {
                        return;
                    }
                    edit_pair(&window, &store, &editor, &active, &strings, index);
                }
                "delete" => {
                    if !editor.borrow().selection.is_empty() {
                        window.set_deleting(true);
                    }
                }
                "confirm-delete" => {
                    editor.borrow_mut().delete();
                    window.set_deleting(false);
                    show(&window, &editor.borrow());
                }
                "back" => window.set_deleting(false),
                "apply" => match applied(editor.borrow().value()) {
                    Ok(()) => close(),
                    Err(error) => window.set_error(error.into()),
                },
                _ => {}
            }
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slots.window.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
