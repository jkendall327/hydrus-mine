//! Database > db maintenance > review vacuum data (hydrus-gui-model's
//! `vacuum_review`, the store's `vacuum`): the files' rows, and on "do it"
//! the window closes and the vacuum runs off the UI thread with the
//! reference's popup.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::list_selection::ListSelection;
use hydrus_gui_model::vacuum_review as model;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};
use hydrus_store::vacuum::{self, VacuumData};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::{TableRow, VacuumReviewWindow};

thread_local! {
    static OPEN: RefCell<Option<VacuumReviewWindow>> = const { RefCell::new(None) };
}

struct State {
    rows: Vec<(VacuumData, Option<u64>)>,
    selection: ListSelection<usize>,
}

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

impl State {
    fn chosen(&self) -> Vec<&(VacuumData, Option<u64>)> {
        self.selection
            .selected_order()
            .iter()
            .filter_map(|&i| self.rows.get(i))
            .collect()
    }

    fn can_vacuum(&self) -> bool {
        let chosen = self.chosen();
        !chosen.is_empty()
            && chosen
                .iter()
                .all(|(d, free)| vacuum::check(d, *free).is_ok())
    }
}

fn paint(window: &VacuumReviewWindow, state: &State) {
    let at = now();
    let selected = state.selection.selected_order();
    window.set_rows(ModelRc::new(VecModel::from(
        state
            .rows
            .iter()
            .enumerate()
            .map(|(i, (data, free))| TableRow {
                cells: ModelRc::new(VecModel::from(
                    model::row(data, *free, at)
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                )),
                selected: selected.contains(&i),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_can_vacuum(state.can_vacuum());
}

fn close() {
    if let Some(window) = OPEN.with_borrow_mut(Option::take) {
        let _ = window.hide();
    }
}

/// Vacuum off the UI thread, saying so in a popup (`_Vacuum`).
#[allow(clippy::cast_precision_loss)] // (seconds)
fn run(store: Arc<Store>) {
    std::thread::spawn(move || {
        let mut job = Job::new(false, false, now() as f64);
        job.status_title = Some(model::POPUP_TITLE.into());
        job.status_text_1 = Some(format!("vacuuming {}", vacuum::NAME));
        let key = job.key;
        let at = now();
        let _ = store.write(move |ctx| popups::add(ctx.conn(), &job, at));
        let done = vacuum::vacuum(&store, hydrus_core::TimestampMs::now().millis());
        let at = now();
        let failed = done.as_ref().err().map(ToString::to_string);
        let _ = store.write(move |ctx| {
            popups::update(ctx.conn(), &key, at, |job| {
                job.status_text_1 = Some("done!".into());
                job.finish_and_dismiss(Some(10), at);
            })
            .map(|_| ())
        });
        if let Some(error) = failed {
            eprintln!("vacuum failed: {error}");
            #[allow(clippy::cast_precision_loss)] // (seconds)
            let message = Job::text(format!("{}\n\n{error}", model::FAILED), now() as f64);
            let _ = store.write(move |ctx| popups::add(ctx.conn(), &message, at));
        }
    });
}

/// Open the review, or raise it if it is open.
pub(crate) fn open(store: &Arc<Store>) {
    if let Some(window) = OPEN.with_borrow(|w| w.as_ref().map(slint::ComponentHandle::clone_strong))
    {
        let _ = window.show();
        return;
    }
    let rows = match vacuum::data(store) {
        Ok(rows) => rows
            .into_iter()
            .map(|d| {
                let free = vacuum::free_space(&d.path);
                (d, free)
            })
            .collect(),
        Err(e) => {
            crate::debug_actions::message("Error", &e.to_string());
            return;
        }
    };
    let window = match VacuumReviewWindow::new() {
        Ok(window) => window,
        Err(e) => {
            eprintln!("could not open the vacuum review: {e}");
            return;
        }
    };
    window.set_info(model::INFO.into());
    let state = Rc::new(RefCell::new(State {
        rows,
        selection: ListSelection::default(),
    }));
    paint(&window, &state.borrow());
    window.on_clicked({
        let state = state.clone();
        let weak = window.as_weak();
        move |i, ctrl, shift| {
            let Ok(i) = usize::try_from(i) else { return };
            let mut state = state.borrow_mut();
            let order: Vec<usize> = (0..state.rows.len()).collect();
            state.selection.click(&order, i, ctrl, shift);
            if let Some(window) = weak.upgrade() {
                paint(&window, &state);
            }
        }
    });
    window.on_vacuum_clicked({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let state = state.borrow();
            if !state.can_vacuum() {
                return;
            }
            let total = state.chosen().iter().map(|(d, _)| d.total_size()).sum();
            if let Some(window) = weak.upgrade() {
                window.set_question(model::question(total).into());
            }
        }
    });
    window.on_answered({
        let store = store.clone();
        let weak = window.as_weak();
        move |yes| {
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            if yes {
                close();
                run(store.clone());
            }
        }
    });
    window.on_close_clicked(close);
    window.window().on_close_requested(|| {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = window.show() {
        eprintln!("could not show the vacuum review: {e}");
        return;
    }
    OPEN.set(Some(window));
}
