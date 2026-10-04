//! The Options-owned provider queue. Every callback is gated by its parent's
//! lifetime; preferences are staged in Editor and written only by parent Apply.
use crate::{OptionsWindow, TableRow};
use hydrus_gui_model::command_palette::ProviderOrder;
use hydrus_gui_model::options::Editor;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
) -> Rc<dyn Fn()> {
    let queue = Rc::new(RefCell::new(ProviderOrder::new(
        editor.borrow().edited_provider_order(),
    )));
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let queue = queue.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let queue = queue.borrow();
                let rows: Vec<_> = queue
                    .order
                    .iter()
                    .map(|provider| TableRow {
                        cells: ModelRc::new(VecModel::from(vec![provider.name().into()])),
                        selected: queue.selection.is_selected(*provider),
                    })
                    .collect();
                window.set_provider_rows(ModelRc::new(VecModel::from(rows)));
                window.set_provider_missing(ModelRc::new(VecModel::from(
                    queue
                        .missing()
                        .into_iter()
                        .map(|p| p.name().into())
                        .collect::<Vec<_>>(),
                )));
                window.set_provider_has_selection(!queue.selection.is_empty());
            }
        }
    });
    window.on_provider_clicked({
        let weak = window.as_weak();
        let queue = queue.clone();
        let active = active.clone();
        let show = show.clone();
        move |row, ctrl, shift| {
            if active.get()
                && weak.upgrade().is_some_and(|w| w.get_provider_mode() == 0)
                && let Ok(row) = usize::try_from(row)
            {
                queue.borrow_mut().click(row, ctrl, shift);
                show();
            }
        }
    });
    window.on_provider_action({
        let weak = window.as_weak();
        let queue = queue.clone();
        let active = active.clone();
        let show = show.clone();
        let editor = editor.clone();
        move |action| {
            let Some(window) = weak.upgrade().filter(|_| active.get()) else {
                return;
            };
            if window.get_provider_mode() != 0 && action.as_str() != "cancel" {
                return;
            }
            match action.as_str() {
                "up" | "down" => {
                    queue.borrow_mut().move_selected(action == "down");
                    editor
                        .borrow_mut()
                        .set_provider_order(queue.borrow().order.clone());
                }
                "delete" => {
                    if let Some(question) = queue.borrow().removal_question() {
                        window.set_provider_message(question.into());
                        window.set_provider_mode(1);
                    }
                }
                "add" => {
                    if !queue.borrow().missing().is_empty() {
                        window.set_provider_message("Select a provider to add:".into());
                        window.set_provider_mode(2);
                    }
                }
                "cancel" => window.set_provider_mode(0),
                _ => {}
            }
            show();
        }
    });
    window.on_provider_chosen({
        let weak = window.as_weak();
        let queue = queue.clone();
        let active = active.clone();
        let show = show.clone();
        let editor = editor.clone();
        move |index| {
            let Some(window) = weak
                .upgrade()
                .filter(|w| active.get() && w.get_provider_mode() == 2)
            else {
                return;
            };
            let provider = usize::try_from(index)
                .ok()
                .and_then(|index| queue.borrow().missing().get(index).copied());
            if queue.borrow_mut().add(provider) {
                editor
                    .borrow_mut()
                    .set_provider_order(queue.borrow().order.clone());
                window.set_provider_mode(0);
                show();
            }
        }
    });
    window.on_provider_answer({
        let weak = window.as_weak();
        let queue = queue.clone();
        let active = active.clone();
        let show = show.clone();
        let editor = editor.clone();
        move |yes| {
            let Some(window) = weak
                .upgrade()
                .filter(|w| active.get() && w.get_provider_mode() == 1)
            else {
                return;
            };
            if yes {
                queue.borrow_mut().remove_selected();
                editor
                    .borrow_mut()
                    .set_provider_order(queue.borrow().order.clone());
            }
            window.set_provider_mode(0);
            show();
        }
    });
    show
}
