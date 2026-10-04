//! Simple downloader formula lists, edited in isolation and saved on Apply.
use crate::{SimpleFormulaeWindow, TableRow, list_selection::ListSelection};
use hydrus_parse::simple::SimpleFormula;
use hydrus_store::{
    Store,
    settings::{self, SimpleDownloaderFormulae},
};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc, sync::Arc};
/// List and child formula windows while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub list: Rc<RefCell<Option<SimpleFormulaeWindow>>>,
    pub formula: crate::formula_window::Slots,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("list", &self.list.borrow().is_some())
            .field("formula", &self.formula)
            .finish()
    }
}
struct State {
    values: SimpleDownloaderFormulae,
    selection: ListSelection<usize>,
    editing: Option<(Option<usize>, SimpleFormula)>,
    deleting: bool,
    child_open: bool,
    pending_edits: Vec<usize>,
}
fn show(w: &SimpleFormulaeWindow, s: &State) {
    w.set_rows(ModelRc::new(VecModel::from(
        s.values
            .formulae
            .iter()
            .enumerate()
            .map(|(i, f)| TableRow {
                cells: ModelRc::new(VecModel::from(vec![SharedString::from(f.name.as_str())])),
                selected: s.selection.is_selected(i),
            })
            .collect::<Vec<_>>(),
    )));
    w.set_selected(
        !s.selection
            .in_order(&(0..s.values.formulae.len()).collect::<Vec<_>>())
            .is_empty(),
    );
    w.set_naming(s.editing.is_some());
    w.set_deleting(s.deleting);
    w.set_editing_formula(s.child_open);
}
fn put(s: &mut State, at: Option<usize>, f: SimpleFormula) {
    crate::formula_editors::put_simple_formula(&mut s.values.formulae, at, f);
}
/// Open the saved simple downloader formulae. Apply persists them, preserving
/// the favourite name, and refreshes the caller's chooser.
pub fn open(
    store: &Arc<Store>,
    slots: &Slots,
    applied: Rc<dyn Fn()>,
) -> Result<SimpleFormulaeWindow, slint::PlatformError> {
    let w = SimpleFormulaeWindow::new()?;
    let mut values = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap_or_default();
    values.formulae.sort_by(|a, b| a.name.cmp(&b.name));
    let state = Rc::new(RefCell::new(State {
        values,
        selection: ListSelection::default(),
        editing: None,
        deleting: false,
        child_open: false,
        pending_edits: Vec::new(),
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show(&w, &state.borrow());
            }
        }
    });
    w.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, c, h| {
            if let Ok(r) = usize::try_from(r) {
                let mut s = state.borrow_mut();
                let order = (0..s.values.formulae.len()).collect::<Vec<_>>();
                s.selection.click(&order, r, c, h);
            }
            refresh();
        }
    });
    let name: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move |at| {
            let f = at
                .and_then(|i| state.borrow().values.formulae.get(i).cloned())
                .unwrap_or_else(|| SimpleFormula {
                    name: "new parsing formula".into(),
                    formula: crate::formula_editors::new_formula(false),
                });
            if let Some(w) = weak.upgrade() {
                w.set_name(f.name.as_str().into());
            }
            state.borrow_mut().editing = Some((at, f));
            refresh();
        }
    });
    w.on_add({
        let name = name.clone();
        let state = state.clone();
        move || {
            state.borrow_mut().pending_edits.clear();
            name(None);
        }
    });
    w.on_edit({
        let state = state.clone();
        let name = name.clone();
        move || {
            let picked = {
                let mut s = state.borrow_mut();
                let selected = s
                    .selection
                    .in_order(&(0..s.values.formulae.len()).collect::<Vec<_>>());
                s.pending_edits = selected.iter().skip(1).copied().collect();
                selected.first().copied()
            };
            if let Some(i) = picked {
                name(Some(i));
            }
        }
    });
    w.on_row_activated({
        let name = name.clone();
        let state = state.clone();
        move |r| {
            state.borrow_mut().pending_edits.clear();
            if let Ok(i) = usize::try_from(r) {
                name(Some(i));
            }
        }
    });
    w.on_named({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let name = name.clone();
        move || {
            let Some((at, mut f)) = state.borrow_mut().editing.take() else {
                return;
            };
            if let Some(w) = weak.upgrade() {
                f.name = w.get_name().to_string();
            }
            state.borrow_mut().child_open = true;
            refresh();
            let formula = f.formula.clone();
            let put = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let name = name.clone();
                move |formula| {
                    let next = {
                        let mut s = state.borrow_mut();
                        let mut f = f.clone();
                        f.formula = formula;
                        put(&mut s, at, f);
                        if s.pending_edits.is_empty() {
                            None
                        } else {
                            Some(s.pending_edits.remove(0))
                        }
                    };
                    refresh();
                    if let Some(i) = next {
                        name(Some(i));
                    }
                }
            });
            match crate::formula_window::open(
                &store,
                &formula,
                crate::formula_window::FormulaTestData::default(),
                &slots.formula,
                put,
            ) {
                Ok(w) => {
                    w.on_closed({
                        let state = state.clone();
                        let refresh = refresh.clone();
                        move |accepted| {
                            let mut s = state.borrow_mut();
                            s.child_open = false;
                            if !accepted {
                                s.pending_edits.clear();
                            }
                            drop(s);
                            refresh();
                        }
                    });
                    *slots.formula.formula.borrow_mut() = Some(w);
                }
                Err(e) => {
                    let mut s = state.borrow_mut();
                    s.child_open = false;
                    s.pending_edits.clear();
                    drop(s);
                    refresh();
                    eprintln!("could not open formula: {e}");
                }
            }
        }
    });
    w.on_name_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            {
                let mut s = state.borrow_mut();
                s.editing = None;
                s.pending_edits.clear();
            }
            refresh();
        }
    });
    w.on_delete({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut s = state.borrow_mut();
            let selected = s
                .selection
                .in_order(&(0..s.values.formulae.len()).collect::<Vec<_>>());
            s.deleting = !selected.is_empty();
            if let Some(w) = weak.upgrade() {
                w.set_question(format!("Remove {} selected?", selected.len()).into());
            }
            drop(s);
            refresh();
        }
    });
    w.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            let mut s = state.borrow_mut();
            if s.deleting && i == 0 {
                let selected = s
                    .selection
                    .in_order(&(0..s.values.formulae.len()).collect::<Vec<_>>());
                for i in selected.into_iter().rev() {
                    s.values.formulae.remove(i);
                }
                s.selection = ListSelection::default();
            }
            s.deleting = false;
            drop(s);
            refresh();
        }
    });
    w.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().deleting = false;
            refresh();
        }
    });
    w.on_defaults({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            for f in SimpleDownloaderFormulae::default().formulae {
                put(&mut state.borrow_mut(), None, f);
            }
            refresh();
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slots = slots.clone();
        move || {
            slots.formula.cancel();
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.list.borrow_mut().take();
        }
    });
    w.on_apply({
        let weak = w.as_weak();
        let store = store.clone();
        let state = state.clone();
        let close = close.clone();
        move || {
            let formulae = state.borrow().values.formulae.clone();
            match store.write_and_refresh(move |ctx| {
                let mut saved = settings::get::<SimpleDownloaderFormulae>(ctx.conn())?;
                saved.formulae = formulae;
                settings::set(ctx.conn(), &saved)
            }) {
                Ok(()) => {
                    close();
                    applied();
                }
                Err(e) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_error(e.to_string().into());
                    }
                }
            }
        }
    });
    w.on_cancel({
        let close = close.clone();
        move || close()
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    w.show()?;
    Ok(w)
}
