//! Database > locations (hydrus-gui-model's `database_locations`, the
//! store's `storage_locations`): the list of media locations, changing
//! their weights, limits and the thumbnail override, and "move files now"
//! off the UI thread with a cancellable popup.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hydrus_gui_model::database_locations::{self as model, Disk};
use hydrus_gui_model::list_selection::ListSelection;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};
use hydrus_store::storage_locations::{self as storage, Review};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::{DatabaseLocationsWindow, LocationMaxSizeWindow, TableRow};

/// The window and its child, while open.
#[derive(Default)]
pub(crate) struct Slot {
    window: Option<DatabaseLocationsWindow>,
    max_size: Option<LocationMaxSizeWindow>,
    runtime: Option<crate::ChoiceButtonsWindow>,
    custom: Option<crate::EditValueWindow>,
    timer: Option<slint::Timer>,
    granularity: crate::granularity_window::Slots,
}

pub(crate) type Slots = Rc<RefCell<Slot>>;

struct State {
    store: Arc<Store>,
    ask: crate::menu_bar::Ask,
    review: Review,
    selection: ListSelection<usize>,
    ascending: bool,
    /// A rebalance at work: whether it has finished.
    working: Option<Arc<AtomicBool>>,
}

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

impl State {
    fn reload(&mut self) {
        match self.store.read(storage::review) {
            Ok(review) => self.review = review,
            Err(e) => eprintln!("could not read the locations: {e}"),
        }
    }

    fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.review.locations.len()).collect();
        if !self.ascending {
            order.reverse();
        }
        order
    }

    fn selected(&self) -> Option<&storage::Location> {
        self.selection
            .one()
            .and_then(|i| self.review.locations.get(i))
    }

    fn write(
        &mut self,
        f: impl FnOnce(&rusqlite::Connection) -> hydrus_store::Result<()> + Send + 'static,
    ) {
        if let Err(e) = self.store.write_and_refresh(move |ctx| f(ctx.conn())) {
            crate::debug_actions::message("Warning", &e.to_string());
        }
        self.reload();
    }
}

fn show(window: &DatabaseLocationsWindow, state: &State) {
    let review = &state.review;
    let thumbs = state.store.snapshot().thumbnails.clone();
    let estimates = model::thumbnail_estimates(
        review.total_files,
        thumbs.bounding_width,
        thumbs.bounding_height,
    );
    let ideal = model::ideal(review);
    let override_set = review.locations.iter().any(|l| l.thumbnail_override);
    let db_dir = state.store.dir();
    let rows: Vec<TableRow> = state
        .order()
        .into_iter()
        .map(|i| {
            let location = &review.locations[i];
            let disk = if location.path.is_dir() {
                fs4::available_space(&location.path).map_or(Disk::Unknown, Disk::Free)
            } else {
                Disk::Missing
            };
            let cells = model::row(
                location,
                ideal.get(i).copied().unwrap_or(0.0),
                review,
                estimates.1,
                override_set,
                db_dir,
                disk,
            );
            TableRow {
                cells: ModelRc::new(VecModel::from(
                    cells
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                )),
                selected: state.selection.is_selected(i),
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_ascending(state.ascending);
    window.set_buttons(ModelRc::new(VecModel::from(
        model::buttons(review, state.selected()).to_vec(),
    )));
    let db_bytes =
        std::fs::metadata(db_dir.join(hydrus_store::store::DB_FILE_NAME)).map_or(0, |m| m.len());
    let install = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.display().to_string()))
        .unwrap_or_default();
    window.set_components(ModelRc::new(VecModel::from(
        model::component_lines(
            &install,
            &db_dir.display().to_string(),
            db_bytes,
            review.total_bytes,
            estimates,
        )
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>(),
    )));
    let thumbs_location = review
        .locations
        .iter()
        .find(|l| l.thumbnail_override)
        .map(|l| l.path.display().to_string());
    window.set_can_clear_thumbnails(thumbs_location.is_some());
    window.set_thumbnail_location(thumbs_location.unwrap_or_else(|| "none set".into()).into());
    let granularity = state
        .store
        .read(hydrus_store::storage::FileStorage::load)
        .map_or(2, |s| s.granularity());
    window.set_granularity(model::granularity_label(granularity).into());
    let work = state
        .store
        .read(storage::next_move)
        .ok()
        .flatten()
        .is_some();
    window.set_rebalance_status(model::rebalance_label(work).into());
    window.set_can_rebalance(work);
    window.set_busy(state.working.is_some());
}

/// Open the window (or show it again).
pub(crate) fn open(
    store: &Arc<Store>,
    ask: crate::menu_bar::Ask,
    slots: &Slots,
) -> Result<(), slint::PlatformError> {
    if let Some(window) = slots.borrow().window.as_ref() {
        return window.show();
    }
    let window = DatabaseLocationsWindow::new()?;
    window.set_window_title(model::TITLE.into());
    window.set_warning(model::WARNING.into());
    let mut state = State {
        store: store.clone(),
        ask,
        review: Review::default(),
        selection: ListSelection::default(),
        ascending: true,
        working: None,
    };
    state.reload();
    let state = Rc::new(RefCell::new(state));
    let refresh: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        })
    };
    window.on_clicked({
        let (state, refresh) = (state.clone(), refresh.clone());
        move |row, control, shift| {
            if let Ok(row) = usize::try_from(row) {
                let mut s = state.borrow_mut();
                let order = s.order();
                // (one at a time, as the reference's list selects)
                s.selection.click(&order, row, false, false);
                let _ = (control, shift);
            }
            refresh();
        }
    });
    window.on_sort({
        let (state, refresh) = (state.clone(), refresh.clone());
        move |_, ascending| {
            state.borrow_mut().ascending = ascending;
            refresh();
        }
    });
    window.on_activated({
        let weak = window.as_weak();
        move |row| {
            if let Some(window) = weak.upgrade() {
                window.invoke_clicked(row, false, false);
                window.invoke_action("max".into());
            }
        }
    });
    window.on_action({
        let (state, refresh, slots) = (state.clone(), refresh.clone(), slots.clone());
        move |action| act(&state, &refresh, &slots, action.as_str())
    });
    window.on_close_window({
        let slots = slots.clone();
        move || close(&slots)
    });
    window.window().on_close_requested({
        let slots = slots.clone();
        move || {
            close(&slots);
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show()?;
    slots.borrow_mut().window = Some(window);
    Ok(())
}

fn close(slots: &Slots) {
    let mut slot = slots.borrow_mut();
    if let Some(w) = slot.max_size.take() {
        let _ = w.hide();
    }
    if let Some(w) = slot.runtime.take() {
        let _ = w.hide();
    }
    if let Some(w) = slot.custom.take() {
        let _ = w.hide();
    }
    if let Some(w) = slot.window.take() {
        let _ = w.hide();
    }
    slot.timer = None;
}

#[allow(clippy::too_many_lines)]
fn act(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, slots: &Slots, action: &str) {
    if state.borrow().working.is_some() {
        return;
    }
    match action {
        "add" => {
            let Some(path) = crate::pick(crate::Pick::Folder, model::PICK_LOCATION)
                .into_iter()
                .next()
            else {
                return;
            };
            let mut s = state.borrow_mut();
            if let Some(l) = s.review.locations.iter().find(|l| l.path == path) {
                if l.thumbnail_override {
                    crate::debug_actions::message("Warning", model::IS_THUMBNAIL_LOCATION);
                    return;
                }
                if l.weight > 0 {
                    crate::debug_actions::message("Warning", model::ALREADY_ENTERED);
                    return;
                }
            }
            s.write(move |conn| storage::add_location(conn, &path));
        }
        "increase" | "decrease" => {
            let mut s = state.borrow_mut();
            let Some(l) = s.selected().cloned() else {
                return;
            };
            if action == "increase" {
                let path = l.path.clone();
                if l.weight == 0 {
                    s.write(move |conn| storage::add_location(conn, &path));
                } else {
                    s.write(move |conn| storage::adjust_weight(conn, &path, 1));
                }
            } else if l.weight > 1 {
                let path = l.path.clone();
                s.write(move |conn| storage::adjust_weight(conn, &path, -1));
            } else {
                drop(s);
                return remove(state, refresh, &l);
            }
        }
        "remove" => {
            let l = state.borrow().selected().cloned();
            if let Some(l) = l {
                return remove(state, refresh, &l);
            }
        }
        "max" => {
            let l = {
                let s = state.borrow();
                if !model::buttons(&s.review, s.selected())[2] {
                    return;
                }
                s.selected().cloned()
            };
            let Some(l) = l else {
                return;
            };
            let Ok(window) = LocationMaxSizeWindow::new() else {
                return;
            };
            window.set_window_title(model::MAX_SIZE_TITLE.into());
            window.set_message(model::MAX_SIZE_MESSAGE.into());
            let (amount, unit) = split_bytes(l.max_bytes.unwrap_or(model::MAX_SIZE_DEFAULT));
            window.set_no_limit(l.max_bytes.is_none());
            window.set_amount(amount);
            window.set_unit(unit);
            window.on_apply({
                let weak = window.as_weak();
                let (state, refresh) = (state.clone(), refresh.clone());
                let path = l.path.clone();
                move || {
                    let Some(w) = weak.upgrade() else {
                        return;
                    };
                    let max = (!w.get_no_limit()).then(|| {
                        i64::from(w.get_amount())
                            * 1024_i64.pow(u32::try_from(w.get_unit()).unwrap_or(0))
                    });
                    let _ = w.hide();
                    let path = path.clone();
                    state
                        .borrow_mut()
                        .write(move |conn| storage::set_max_bytes(conn, &path, max));
                    refresh();
                }
            });
            window.on_cancel({
                let weak = window.as_weak();
                move || {
                    if let Some(w) = weak.upgrade() {
                        let _ = w.hide();
                    }
                }
            });
            if window.show().is_ok() {
                slots.borrow_mut().max_size = Some(window);
            }
        }
        "set thumbnails" => {
            let Some(path) = crate::pick(crate::Pick::Folder, model::PICK_THUMBNAILS)
                .into_iter()
                .next()
            else {
                return;
            };
            let mut s = state.borrow_mut();
            if s.review
                .locations
                .iter()
                .any(|l| l.path == path && l.weight > 0)
            {
                crate::debug_actions::message("Warning", model::IS_FILE_LOCATION);
                return;
            }
            s.write(move |conn| storage::set_thumbnail_override(conn, Some(&path)));
        }
        "clear thumbnails" => {
            let ask = state.borrow().ask.clone();
            let (state, refresh) = (state.clone(), refresh.clone());
            ask(
                model::CLEAR_THUMBNAILS.into(),
                Rc::new(move || {
                    state
                        .borrow_mut()
                        .write(|conn| storage::set_thumbnail_override(conn, None));
                    refresh();
                }),
            );
            return;
        }
        "rebalance" => return rebalance(state, refresh, slots),
        "granularity" => {
            let store = state.borrow().store.clone();
            let granularity = slots.borrow().granularity.clone();
            let changed: Rc<dyn Fn()> = {
                let (state, refresh) = (state.clone(), refresh.clone());
                Rc::new(move || {
                    state.borrow_mut().reload();
                    refresh();
                })
            };
            crate::granularity_window::open(&store, &granularity, changed);
            return;
        }
        _ => {}
    }
    refresh();
}

/// A size as the bytes control shows it: an amount and its unit.
fn split_bytes(bytes: i64) -> (i32, i32) {
    let mut unit = 0;
    let mut amount = bytes;
    while unit < 4 && amount >= 1024 && amount % 1024 == 0 {
        amount /= 1024;
        unit += 1;
    }
    (i32::try_from(amount).unwrap_or(i32::MAX), unit)
}

/// "remove location" (`_RemoveBaseLocation`): warned or asked first.
fn remove(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, l: &storage::Location) {
    let (ask, weighted) = {
        let s = state.borrow();
        (
            s.ask.clone(),
            s.review.locations.iter().filter(|l| l.weight > 0).count(),
        )
    };
    if l.weight == 0 {
        crate::debug_actions::message("Warning", model::SELECT_WITH_WEIGHT);
        return;
    }
    if weighted == 1 {
        crate::debug_actions::message("Warning", model::CANNOT_EMPTY_ALL);
        return;
    }
    let question = model::remove_question(l.path.is_dir(), l.files_share > 0.0);
    let (state, refresh, path) = (state.clone(), refresh.clone(), l.path.clone());
    ask(
        question.into(),
        Rc::new(move || {
            let path = path.clone();
            state
                .borrow_mut()
                .write(move |conn| storage::remove_location(conn, &path));
            refresh();
        }),
    );
}

/// "move files now" (`_Rebalance`): every location must be there; then a
/// run time is chosen and the folders move on a worker with a cancellable
/// "rebalancing files" popup.
fn rebalance(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, slots: &Slots) {
    let missing = state
        .borrow()
        .review
        .locations
        .iter()
        .find(|l| !l.path.is_dir())
        .map(|l| l.path.display().to_string());
    if let Some(path) = missing {
        crate::debug_actions::message("Warning", &model::missing_path_warning(&path));
        return;
    }
    let (state_after, refresh_after, slots_after) = (state.clone(), refresh.clone(), slots.clone());
    let chooser = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: "Are you sure?",
            message: model::RUNTIME_QUESTION,
            choices: model::RUNTIMES
                .iter()
                .map(|(l, _)| (*l).to_owned())
                .collect(),
            no_label: "forget it",
        },
        move |choice| {
            let Some(i) = choice else {
                return;
            };
            match model::RUNTIMES.get(i).map(|(_, s)| *s) {
                Some(Some(-1)) => custom_runtime(&state_after, &refresh_after, &slots_after),
                Some(seconds) => start(&state_after, &refresh_after, &slots_after, seconds),
                None => {}
            }
        },
    );
    if let Ok(chooser) = chooser {
        slots.borrow_mut().runtime = chooser;
    }
}

/// "run for custom time": minutes, from two hours.
fn custom_runtime(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, slots: &Slots) {
    let Ok(window) = crate::EditValueWindow::new() else {
        return;
    };
    window.set_window_title("set time to run (minutes)".into());
    window.set_minimum(1);
    window.set_value(120);
    window.on_apply({
        let weak = window.as_weak();
        let (state, refresh, slots) = (state.clone(), refresh.clone(), slots.clone());
        move || {
            if let Some(w) = weak.upgrade() {
                let minutes = i64::from(w.get_value());
                let _ = w.hide();
                start(&state, &refresh, &slots, Some(minutes * 60));
            }
        }
    });
    window.on_cancel({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        }
    });
    if window.show().is_ok() {
        slots.borrow_mut().custom = Some(window);
    }
}

#[allow(clippy::cast_precision_loss)] // (seconds)
fn start(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, slots: &Slots, seconds: Option<i64>) {
    let store = state.borrow().store.clone();
    let done = Arc::new(AtomicBool::new(false));
    state.borrow_mut().working = Some(done.clone());
    let stop_at = seconds.map(|s| now() + s);
    let thread_done = done.clone();
    std::thread::spawn(move || {
        let mut job = Job::new(false, true, now() as f64);
        job.status_title = Some(model::REBALANCE_TITLE.into());
        let key = job.key;
        let at = now();
        let _ = store.write(move |ctx| popups::add(ctx.conn(), &job, at));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut say = |text: String| {
            let at = now();
            let flag = cancelled.clone();
            let _ = store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| {
                    job.status_text_1 = Some(text);
                    if job.cancelled {
                        flag.store(true, Ordering::Relaxed);
                    }
                })
                .map(|_| ())
            });
        };
        let result = storage::rebalance(&store, &mut say, &|| {
            cancelled.load(Ordering::Relaxed) || stop_at.is_some_and(|t| now() >= t)
        });
        let text = match result {
            Ok(_) => model::REBALANCE_DONE.to_owned(),
            Err(e) => format!("could not move the files: {e}"),
        };
        let at = now();
        let failed = text != model::REBALANCE_DONE;
        let _ = store.write(move |ctx| {
            popups::update(ctx.conn(), &key, at, |job| {
                job.status_text_1 = Some(text);
                if failed {
                    job.finish();
                } else {
                    job.finish_and_dismiss(None, at);
                }
            })
            .map(|_| ())
        });
        thread_done.store(true, Ordering::Release);
    });
    // (the list is read again once the work is done)
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        {
            let state = Rc::downgrade(state);
            let refresh = refresh.clone();
            move || {
                let Some(state) = state.upgrade() else {
                    return;
                };
                let finished = state
                    .borrow()
                    .working
                    .as_ref()
                    .is_some_and(|d| d.load(Ordering::Acquire));
                if finished {
                    let mut s = state.borrow_mut();
                    s.working = None;
                    s.reload();
                    drop(s);
                    refresh();
                }
            }
        },
    );
    slots.borrow_mut().timer = Some(timer);
    refresh();
}
