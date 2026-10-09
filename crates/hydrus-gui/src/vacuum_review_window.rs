//! Database > db maintenance > review vacuum data (hydrus-gui-model's
//! `vacuum_review`, the store's `vacuum`): the files' rows, and on "do it"
//! the window closes and the vacuum runs off the UI thread with the
//! reference's popup.
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use hydrus_gui_model::list_selection::ListSelection;
use hydrus_gui_model::vacuum_review as model;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};
use hydrus_store::vacuum::{self, VacuumData};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::{TableRow, VacuumReviewWindow};

struct Open {
    // Keep the child alive while callbacks and Confirmation hold weak handles.
    _window: VacuumReviewWindow,
    state: Rc<Confirmation>,
}
struct Family {
    live: Cell<bool>,
    current: RefCell<Option<Open>>,
}
/// A Main binding's vacuum review; retired bindings cannot reopen it.
#[derive(Clone)]
pub(crate) struct Slot(Rc<Family>);
impl Default for Slot {
    fn default() -> Self {
        Self(Rc::new(Family {
            live: Cell::new(true),
            current: RefCell::new(None),
        }))
    }
}
impl Slot {
    /// Close this exact child and refuse every later launch from its owner.
    pub fn retire(&self) {
        self.0.live.set(false);
        let state = self
            .0
            .current
            .borrow()
            .as_ref()
            .map(|open| open.state.clone());
        if let Some(state) = state {
            state.close();
        }
    }
    #[cfg(test)]
    fn window(&self) -> Option<VacuumReviewWindow> {
        self.0
            .current
            .borrow()
            .as_ref()
            .and_then(|open| open.state.window.upgrade())
    }
}
/// Only the final shared Bound owner retires the slot; callback clones do not own it.
pub(crate) struct Owner(Slot);
impl Owner {
    pub fn new(slot: &Slot) -> Self {
        Self(slot.clone())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}

struct State {
    rows: Vec<(VacuumData, Option<u64>)>,
    selection: ListSelection<usize>,
}
struct Confirmation {
    window: slint::Weak<VacuumReviewWindow>,
    family: Weak<Family>,
    active: Cell<bool>,
    pending: Cell<bool>,
    owner_valid: Rc<dyn Fn() -> bool>,
    can_accept: Rc<dyn Fn() -> bool>,
    review: RefCell<State>,
    accepted: Rc<dyn Fn()>,
    timer: Timer,
}
impl Confirmation {
    fn close(self: &Rc<Self>) {
        if !self.active.replace(false) {
            return;
        }
        self.pending.set(false);
        self.timer.stop();
        if let Some(window) = self.window.upgrade() {
            window.set_question(SharedString::new());
            let _ = window.hide();
        }
        if let Some(family) = self.family.upgrade() {
            let current = family
                .current
                .borrow()
                .as_ref()
                .is_some_and(|open| Rc::ptr_eq(&open.state, self));
            if current {
                family.current.borrow_mut().take();
            }
        }
    }
    fn input(self: &Rc<Self>) -> Option<VacuumReviewWindow> {
        if !self.active.get() {
            return None;
        }
        let owned = self.family.upgrade().is_some_and(|family| {
            family.live.get()
                && family
                    .current
                    .borrow()
                    .as_ref()
                    .is_some_and(|open| Rc::ptr_eq(&open.state, self))
        });
        if !owned || !(self.owner_valid)() {
            self.close();
            return None;
        }
        let window = self.window.upgrade();
        if !window
            .as_ref()
            .is_some_and(|window| window.window().is_visible())
        {
            self.close();
            return None;
        }
        window
    }
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

/// Open this binding's review, or raise its existing child.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Slot,
    valid: Rc<dyn Fn() -> bool>,
    can_accept: Rc<dyn Fn() -> bool>,
) -> Result<(), String> {
    let accepted = Rc::new({
        let store = store.clone();
        move || run(store.clone())
    });
    open_using_with_admission(store, slot, valid, can_accept, accepted)
}

#[cfg(test)]
fn open_using(
    store: &Arc<Store>,
    slot: &Slot,
    valid: Rc<dyn Fn() -> bool>,
    accepted: Rc<dyn Fn()>,
) -> Result<(), String> {
    open_using_with_admission(store, slot, valid, Rc::new(|| true), accepted)
}

fn open_using_with_admission(
    store: &Arc<Store>,
    slot: &Slot,
    valid: Rc<dyn Fn() -> bool>,
    can_accept: Rc<dyn Fn() -> bool>,
    accepted: Rc<dyn Fn()>,
) -> Result<(), String> {
    if !slot.0.live.get() || !valid() {
        return Err("The owning window is no longer active.".into());
    }
    let existing = slot
        .0
        .current
        .borrow()
        .as_ref()
        .map(|open| open.state.clone());
    if let Some(state) = existing
        && let Some(window) = state.input()
    {
        return window.show().map_err(|error| error.to_string());
    }
    let rows = vacuum::data(store)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|data| {
            let free = vacuum::free_space(&data.path);
            (data, free)
        })
        .collect();
    let window = crate::app_title::new::<crate::VacuumReviewWindow>().map_err(|error| error.to_string())?;
    window.set_info(model::INFO.into());
    let state = Rc::new(Confirmation {
        window: window.as_weak(),
        family: Rc::downgrade(&slot.0),
        active: Cell::new(true),
        pending: Cell::new(false),
        owner_valid: valid,
        can_accept,
        review: RefCell::new(State {
            rows,
            selection: ListSelection::default(),
        }),
        accepted,
        timer: Timer::default(),
    });
    paint(&window, &state.review.borrow());
    window.on_clicked({
        let state = state.clone();
        move |index, ctrl, shift| {
            let Some(window) = state.input() else { return };
            if state.pending.get() {
                return;
            }
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let mut review = state.review.borrow_mut();
            let order: Vec<usize> = (0..review.rows.len()).collect();
            review.selection.click(&order, index, ctrl, shift);
            paint(&window, &review);
        }
    });
    window.on_vacuum_clicked({
        let state = state.clone();
        move || {
            let Some(window) = state.input() else { return };
            let review = state.review.borrow();
            if !(state.can_accept)() || state.pending.get() || !review.can_vacuum() {
                return;
            }
            let total = review
                .chosen()
                .iter()
                .map(|(data, _)| data.total_size())
                .sum();
            state.pending.set(true);
            window.set_question(model::question(total).into());
        }
    });
    window.on_answered({
        let state = state.clone();
        move |yes| {
            let Some(window) = state.input() else { return };
            // A main-window question blocks admission without retiring the review
            // or consuming its pending decision. Declining exit can resume it.
            if !(state.can_accept)() || !state.pending.replace(false) {
                return;
            }
            window.set_question(SharedString::new());
            if yes && state.review.borrow().can_vacuum() {
                // Retire admission before publishing work, so reentrant and
                // retained answers cannot start a second worker.
                state.close();
                (state.accepted)();
            }
        }
    });
    window.on_close_clicked({
        let state = state.clone();
        move || state.close()
    });
    window.window().on_close_requested({
        let state = state.clone();
        move || {
            state.close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(250), {
            let weak = Rc::downgrade(&state);
            move || {
                if let Some(state) = weak.upgrade()
                    && (!(state.owner_valid)()
                        || state
                            .window
                            .upgrade()
                            .is_none_or(|window| !window.window().is_visible()))
                {
                    state.close();
                }
            }
        });
    *slot.0.current.borrow_mut() = Some(Open {
        _window: window.clone_strong(),
        state: state.clone(),
    });
    if let Err(error) = window.show() {
        state.close();
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model as _;

    #[expect(
        clippy::used_underscore_binding,
        reason = "tests inspect the lifetime-only Bound owner"
    )]
    fn bound_window(bound: &crate::Bound) -> Option<VacuumReviewWindow> {
        bound._vacuum_review_owner.0.window()
    }

    fn store() -> (tempfile::TempDir, Arc<Store>) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        (directory, store)
    }
    fn start_question(window: &VacuumReviewWindow) {
        window.invoke_clicked(0, false, false);
        assert!(window.get_can_vacuum());
        window.invoke_vacuum_clicked();
        assert!(!window.get_question().is_empty());
    }
    fn recorded_time(store: &Store) -> Option<i64> {
        vacuum::data(store).unwrap()[0].last_vacuumed_ms
    }

    // leaf: audit-media-database-maintenance-vacuum-run
    #[test]
    fn visible_pending_eligible_confirmation_declines_and_runs_real_vacuum_once() {
        let windows = crate::headless::init();
        let (_directory, store) = store();
        let slot = Slot::default();
        let owner = Owner::new(&slot);
        let calls = Rc::new(Cell::new(0_i64));
        open_using(
            &store,
            &slot,
            Rc::new(|| true),
            Rc::new({
                let calls = calls.clone();
                let store = store.clone();
                move || {
                    calls.set(calls.get() + 1);
                    // An accepted native callback must reach the persisted consumer.
                    vacuum::vacuum(&store, 1234).unwrap();
                }
            }),
        )
        .unwrap();
        let window = slot.window().unwrap();
        window.invoke_answered(true);
        window.invoke_vacuum_clicked();
        assert_eq!(calls.get(), 0);
        assert!(window.get_question().is_empty());
        window.invoke_clicked(0, false, false);
        window.invoke_answered(true);
        assert_eq!(
            calls.get(),
            0,
            "selection alone does not create a confirmation"
        );
        // A real admission failure must not create a pending question.
        let state = slot.0.current.borrow().as_ref().unwrap().state.clone();
        let free = state.review.borrow().rows[0].1;
        state.review.borrow_mut().rows[0].1 = Some(0);
        window.invoke_vacuum_clicked();
        window.invoke_answered(true);
        assert!(window.get_question().is_empty());
        assert_eq!(calls.get(), 0);
        state.review.borrow_mut().rows[0].1 = free;
        window.invoke_vacuum_clicked();
        let expected = model::question(vacuum::data(&store).unwrap()[0].total_size());
        assert_eq!(window.get_question(), expected);
        let selected = window.get_rows().row_data(0).unwrap().selected;
        window.invoke_clicked(0, true, false);
        assert_eq!(
            window.get_rows().row_data(0).unwrap().selected,
            selected,
            "the modal decision retains the selected database"
        );
        let pixels = crate::headless::render(&windows.get(0).unwrap(), 980, 650);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
        window.invoke_answered(false);
        assert!(window.get_question().is_empty());
        assert!(slot.window().is_some());
        assert_eq!(recorded_time(&store), None);
        window.invoke_answered(true);
        assert_eq!(calls.get(), 0, "decline consumes its question");
        window.invoke_vacuum_clicked();
        window.hide().unwrap();
        window.invoke_answered(true);
        assert_eq!(calls.get(), 0);
        assert_eq!(recorded_time(&store), None);
        assert!(slot.window().is_none());
        assert!(window.get_question().is_empty());
        window.show().unwrap();
        window.invoke_answered(true);
        assert_eq!(
            calls.get(),
            0,
            "a hidden callback permanently consumes the old admission"
        );
        window.hide().unwrap();
        open_using(&store, &slot, Rc::new(|| true), state.accepted.clone()).unwrap();
        let window = slot.window().unwrap();
        start_question(&window);
        window.invoke_answered(true);
        assert_eq!(calls.get(), 1);
        assert_eq!(recorded_time(&store), Some(1234));
        assert_eq!(
            vacuum::data(&Store::open(store.dir()).unwrap()).unwrap()[0].last_vacuumed_ms,
            Some(1234)
        );
        assert!(slot.window().is_none());
        assert!(!window.window().is_visible());
        window.show().unwrap();
        window.invoke_vacuum_clicked();
        window.invoke_answered(true);
        window.invoke_answered(true);
        assert_eq!(
            calls.get(),
            1,
            "retained close/show and repeated answers stay retired"
        );
        drop(owner);
        window.hide().unwrap();
    }

    #[test]
    fn closed_predecessor_cannot_close_or_vacuum_successor_or_another_store() {
        let _windows = crate::headless::init();
        let (_first_directory, first_store) = store();
        let (_next_directory, next_store) = store();
        let slot = Slot::default();
        let _owner = Owner::new(&slot);
        let calls = Rc::new(Cell::new(0));
        let accept: Rc<dyn Fn()> = Rc::new({
            let calls = calls.clone();
            move || calls.set(calls.get() + 1)
        });
        open_using(&first_store, &slot, Rc::new(|| true), accept.clone()).unwrap();
        let previous = slot.window().unwrap();
        start_question(&previous);
        previous
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(slot.window().is_none());
        let live = Rc::new(Cell::new(true));
        open_using(
            &next_store,
            &slot,
            Rc::new({
                let live = live.clone();
                move || live.get()
            }),
            accept,
        )
        .unwrap();
        let next = slot.window().unwrap();
        start_question(&next);
        let question = next.get_question();
        previous.show().unwrap();
        previous.invoke_answered(true);
        previous.invoke_close_clicked();
        previous.invoke_clicked(0, false, false);
        previous.invoke_vacuum_clicked();
        assert!(slot.window().is_some());
        assert!(next.window().is_visible());
        assert_eq!(next.get_question(), question);
        assert_eq!(calls.get(), 0);
        assert_eq!(recorded_time(&first_store), None);
        assert_eq!(recorded_time(&next_store), None);
        live.set(false);
        next.invoke_answered(true);
        assert!(slot.window().is_none());
        assert!(!next.window().is_visible());
        live.set(true);
        next.show().unwrap();
        next.invoke_answered(true);
        assert_eq!(calls.get(), 0, "an invalidated owner never revives");
        previous.hide().unwrap();
        next.hide().unwrap();
    }

    fn menu(window: &crate::MainWindow) {
        let database = window
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "database")
            .unwrap();
        window.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
        let maintenance = window
            .get_menu_panes()
            .row_data(0)
            .unwrap()
            .lines
            .iter()
            .position(|row| row.label == "db maintenance")
            .unwrap();
        window.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
        let pane = window.get_menu_panes().row_data(1).unwrap();
        let review = pane
            .lines
            .iter()
            .position(|row| row.label == "review vacuum data\u{2026}")
            .unwrap();
        assert!(pane.lines.row_data(review).unwrap().usable);
        window.invoke_menu_line_clicked(1, i32::try_from(review).unwrap(), 0.0, 0.0, 0.0);
    }

    #[test]
    fn pending_main_exit_blocks_vacuum_then_decline_admits_the_same_question() {
        let _windows = crate::headless::init();
        let (_directory, store) = store();
        store
            .write(|ctx| {
                let mut settings: hydrus_store::settings::GuiSettings =
                    hydrus_store::settings::get(ctx.conn())?;
                settings.confirm_exit = true;
                hydrus_store::settings::set(ctx.conn(), &settings)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ShutdownWork {
                        action: 0,
                        ..hydrus_store::settings::ShutdownWork::default()
                    },
                )
            })
            .unwrap();
        let ui = crate::MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = crate::bind(
            &ui,
            crate::Pages::single(crate::SearchPage::new(store.clone())),
        );
        menu(&ui);
        let review = bound_window(&bound).unwrap();
        start_question(&review);
        let question = review.get_question();
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(!ui.get_question().is_empty());
        review.invoke_answered(true);
        // Admission closes the review synchronously, before spawning its worker.
        // These assertions detect wrong admission without a sleep-based absence check.
        assert!(bound_window(&bound).is_some());
        assert!(review.window().is_visible());
        assert_eq!(review.get_question(), question);
        assert_eq!(recorded_time(&store), None);
        // The separate lifetime timer must also preserve the pending review.
        std::thread::sleep(Duration::from_millis(300));
        slint::platform::update_timers_and_animations();
        assert!(bound_window(&bound).is_some());
        assert!(review.window().is_visible());
        assert_eq!(review.get_question(), question);
        ui.invoke_answer(false);
        assert!(ui.get_question().is_empty());
        assert!(ui.window().is_visible());
        assert_eq!(review.get_question(), question);
        review.invoke_answered(true);
        assert!(bound_window(&bound).is_none());
        assert!(!review.window().is_visible());
        // This is the production menu hook and real off-thread SQLite consumer.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let completed = store
                .read(|conn| popups::all(conn, now()))
                .unwrap()
                .iter()
                .any(|job| {
                    job.status_title.as_deref() == Some(model::POPUP_TITLE)
                        && job.status_text_1.as_deref() == Some("done!")
                });
            if recorded_time(&store).is_some() && completed {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "declining exit must allow the original vacuum confirmation to run"
            );
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(5));
        }
        let saved = recorded_time(&store);
        assert_eq!(recorded_time(&Store::open(store.dir()).unwrap()), saved);
        ui.hide().unwrap();
    }

    #[test]
    fn real_menu_main_hide_rebind_exit_and_last_bound_clone_retire_only_their_child() {
        let _windows = crate::headless::init();
        let (_directory, store) = store();
        let ui = crate::MainWindow::new().unwrap();
        ui.show().unwrap();
        let pages = || crate::Pages::single(crate::SearchPage::new(store.clone()));
        let first = crate::bind(&ui, pages());
        menu(&ui);
        let old = bound_window(&first).unwrap();
        start_question(&old);
        ui.hide().unwrap();
        old.invoke_answered(true);
        assert!(bound_window(&first).is_none());
        assert!(!old.window().is_visible());
        assert_eq!(recorded_time(&store), None);
        ui.show().unwrap();
        menu(&ui);
        let rebound_child = bound_window(&first).unwrap();
        start_question(&rebound_child);
        let second = crate::bind(&ui, pages());
        assert!(!rebound_child.window().is_visible());
        assert!(bound_window(&first).is_none());
        menu(&ui);
        let current = bound_window(&second).unwrap();
        start_question(&current);
        rebound_child.show().unwrap();
        rebound_child.invoke_answered(true);
        rebound_child.invoke_close_clicked();
        assert!(bound_window(&second).is_some());
        assert!(!current.get_question().is_empty());
        store
            .write(|ctx| {
                let mut settings: hydrus_store::settings::GuiSettings =
                    hydrus_store::settings::get(ctx.conn())?;
                settings.confirm_exit = true;
                hydrus_store::settings::set(ctx.conn(), &settings)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ShutdownWork {
                        action: 0,
                        ..hydrus_store::settings::ShutdownWork::default()
                    },
                )
            })
            .unwrap();
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        ui.invoke_answer(false);
        assert!(ui.window().is_visible());
        assert!(current.window().is_visible());
        assert!(!current.get_question().is_empty());
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        ui.invoke_answer(true);
        assert!(!current.window().is_visible());
        assert!(bound_window(&second).is_none());
        ui.show().unwrap();
        current.show().unwrap();
        current.invoke_answered(true);
        assert_eq!(recorded_time(&store), None);
        current.hide().unwrap();
        rebound_child.hide().unwrap();
        let third = crate::bind(&ui, pages());
        menu(&ui);
        let kept_child = bound_window(&third).unwrap();
        start_question(&kept_child);
        let kept = third.clone();
        drop(third);
        assert!(kept_child.window().is_visible());
        drop(kept);
        assert!(ui.window().is_visible());
        assert!(!kept_child.window().is_visible());
        kept_child.show().unwrap();
        kept_child.invoke_answered(true);
        assert_eq!(recorded_time(&store), None);
        kept_child.hide().unwrap();
        ui.hide().unwrap();
    }
}
