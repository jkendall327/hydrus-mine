//! A detached, tri-state downloader/media-viewer display editor.
use crate::{DownloaderDisplayWindow, TableColumn, TableRow};
use hydrus_gui_model::{
    downloader_display::{self as model, Draft},
    list_selection::ListSelection,
};
use hydrus_store::Store;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Default)]
struct Owner {
    window: RefCell<Option<DownloaderDisplayWindow>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(w) = self.window.get_mut().take() {
            w.invoke_cancel_clicked();
        }
    }
}
/// Windows live with the main-window hook; their callbacks hold weak owners.
#[derive(Clone, Default)]
pub struct Slots(Rc<Owner>);
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloaderDisplaySlots")
            .finish_non_exhaustive()
    }
}
thread_local! { static LAST: RefCell<Option<slint::Weak<DownloaderDisplayWindow>>> = const { RefCell::new(None) }; }
pub fn last_opened() -> Option<DownloaderDisplayWindow> {
    LAST.with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
struct State {
    draft: Draft,
    selections: [ListSelection<usize>; 2],
    sorts: [(usize, bool); 2],
    order: Vec<usize>,
    question: Option<(bool, Vec<usize>)>,
}
fn refresh(w: &DownloaderDisplayWindow, s: &mut State) {
    let tab = usize::from(w.get_tab() == 1);
    let classes = tab == 1;
    let rows = s.draft.rows(classes);
    let headings = if classes {
        vec!["name", "type", "display in media viewer?"]
    } else {
        vec!["name", "display in main selector?"]
    };
    w.set_columns(ModelRc::new(VecModel::from(
        headings
            .iter()
            .enumerate()
            .map(|(i, title)| TableColumn {
                title: (*title).into(),
                width: if i == 0 { 220.0 } else { 200.0 },
                stretch: i == 0,
            })
            .collect::<Vec<_>>(),
    )));
    let (column, ascending) = s.sorts[tab];
    s.order = (0..rows.len()).collect();
    s.order.sort_by(|&a, &b| {
        let cmp = rows[a]
            .cells(classes)
            .get(column)
            .cmp(&rows[b].cells(classes).get(column));
        if ascending { cmp } else { cmp.reverse() }
    });
    w.set_sort_column(i32::try_from(column).unwrap_or(0));
    w.set_ascending(ascending);
    w.set_rows(ModelRc::new(VecModel::from(
        s.order
            .iter()
            .map(|&i| TableRow {
                cells: ModelRc::new(VecModel::from(
                    rows[i]
                        .cells(classes)
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<slint::SharedString>>(),
                )),
                selected: s.selections[tab].is_selected(i),
            })
            .collect::<Vec<_>>(),
    )));
    w.set_can_edit(!s.selections[tab].is_empty());
}
/// Open once, bringing an existing detached draft forward.
pub fn open(store: &Arc<Store>, slots: &Slots) -> Result<DownloaderDisplayWindow, String> {
    if let Some(w) = slots.0.window.borrow().as_ref() {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w.clone_strong());
    }
    let draft = Draft::load(store).map_err(|e| e.to_string())?;
    let w = crate::app_title::new::<crate::DownloaderDisplayWindow>().map_err(|e| e.to_string())?;
    w.set_show_unmatched(draft.show_unmatched);
    let state = Rc::new(RefCell::new(State {
        draft,
        selections: std::array::from_fn(|_| ListSelection::default()),
        sorts: [(0, true); 2],
        order: Vec::new(),
        question: None,
    }));
    refresh(&w, &mut state.borrow_mut());
    let active = Rc::new(Cell::new(true));
    w.on_tab_changed({
        let weak = w.as_weak();
        let s = state.clone();
        move || {
            if let Some(w) = weak.upgrade()
                && s.borrow().question.is_none()
            {
                refresh(&w, &mut s.borrow_mut());
            }
        }
    });
    w.on_row_clicked({
        let weak = w.as_weak();
        let s = state.clone();
        move |row, ctrl, shift| {
            if let (Some(w), Ok(row)) = (weak.upgrade(), usize::try_from(row)) {
                let mut s = s.borrow_mut();
                if s.question.is_some() {
                    return;
                }
                let tab = usize::from(w.get_tab() == 1);
                let order = s.order.clone();
                s.selections[tab].click(&order, row, ctrl, shift);
                refresh(&w, &mut s);
            }
        }
    });
    w.on_sort({
        let weak = w.as_weak();
        let s = state.clone();
        move |column, ascending| {
            if let (Some(w), Ok(column)) = (weak.upgrade(), usize::try_from(column)) {
                let mut s = s.borrow_mut();
                if s.question.is_some() {
                    return;
                }
                let tab = usize::from(w.get_tab() == 1);
                if column
                    < s.draft
                        .rows(tab == 1)
                        .first()
                        .map_or(0, |r| r.cells(tab == 1).len())
                {
                    s.sorts[tab] = (column, ascending);
                    refresh(&w, &mut s);
                }
            }
        }
    });
    w.on_edit_clicked({
        let weak = w.as_weak();
        let s = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                let mut s = s.borrow_mut();
                if s.question.is_some() {
                    return;
                }
                let classes = w.get_tab() == 1;
                let indices = s.selections[usize::from(classes)].in_order(&s.order);
                if indices.is_empty() {
                    return;
                }
                let (title, text) = model::question(s.draft.rows(classes), &indices, classes);
                s.question = Some((classes, indices));
                w.set_question_title(title.into());
                w.set_question(text.into());
            }
        }
    });
    w.on_answer({
        let weak = w.as_weak();
        let s = state.clone();
        move |answer| {
            if let Some(w) = weak.upgrade() {
                let mut s = s.borrow_mut();
                if let Some((classes, indices)) = s.question.take() {
                    s.draft.answer(
                        classes,
                        &indices,
                        match answer {
                            1 => Some(true),
                            0 => Some(false),
                            _ => None,
                        },
                    );
                }
                w.set_question("".into());
                w.set_question_title("".into());
                refresh(&w, &mut s);
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let owner = Rc::downgrade(&slots.0);
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(owner) = owner.upgrade() {
                owner.window.borrow_mut().take();
            }
        }
    });
    w.on_cancel_clicked({
        let close = close.clone();
        move || close()
    });
    w.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    w.on_apply_clicked({
        let store = store.clone();
        let weak = w.as_weak();
        let s = state;
        move || {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let mut s = s.borrow_mut();
            if s.question.is_some() {
                return;
            }
            s.draft.show_unmatched = w.get_show_unmatched();
            match s.draft.save(&store) {
                Ok(()) => close(),
                Err(e) => w.set_error(e.to_string().into()),
            }
        }
    });
    *slots.0.window.borrow_mut() = Some(w.clone_strong());
    LAST.with(|s| *s.borrow_mut() = Some(w.as_weak()));
    w.show().map_err(|e| e.to_string())?;
    Ok(w)
}

/// Read the displayed URL links for a file using current durable preferences.
/// The desktop calls this on navigation and while an existing viewer is open.
pub fn file_links(
    store: &Store,
    file: hydrus_core::HashId,
) -> hydrus_store::Result<Vec<(String, String)>> {
    let snapshot = store.snapshot();
    store.read(|conn| {
        let viewer = hydrus_store::settings::get(conn)?;
        let urls = hydrus_store::media::load(conn, &snapshot.services, None, &[file])?
            .results
            .into_iter()
            .next()
            .map_or_else(Vec::new, |media| media.urls);
        Ok(model::viewer_links(&urls, &snapshot.url_classes, &viewer))
    })
}
