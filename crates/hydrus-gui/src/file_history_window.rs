//! Owned global history queries and native four-series chart controls.
use crate::FileHistoryWindow;
use hydrus_core::{ServiceKey, search::context::FileSearchContext};
use hydrus_gui_model::file_history::{self as model, Chart};
use hydrus_search::{TextContext, parse_api_search, predicate_text};
use hydrus_store::{Store, content::DomainRoles, file_history::History};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
pub type Slot = Rc<RefCell<Option<FileHistoryWindow>>>;
struct State {
    active: Cell<bool>,
    chart: RefCell<Chart>,
    context: RefCell<FileSearchContext>,
    cancel: RefCell<Option<Arc<AtomicBool>>>,
    receiver: RefCell<Option<mpsc::Receiver<Result<History, String>>>>,
}
fn paint(window: &FileHistoryWindow, state: &State) {
    let chart = state.chart.borrow();
    window.set_paths(ModelRc::new(VecModel::from(
        chart.paths().map(SharedString::from).to_vec(),
    )));
    window.set_visible_series(ModelRc::new(VecModel::from(chart.visible.to_vec())));
    window.set_min_count(i32::try_from(chart.y.0).unwrap_or(0));
    window.set_max_count(i32::try_from(chart.y.1.min(1_000_000_000)).unwrap_or(1));
    window.set_start_date(model::date(chart.x.0).into());
    window.set_end_date(model::date(chart.x.1).into());
    window.set_predicates(ModelRc::new(VecModel::from(
        state
            .context
            .borrow()
            .predicates
            .iter()
            .map(|p| SharedString::from(predicate_text(p, &TextContext::default())))
            .collect::<Vec<_>>(),
    )));
}
pub fn open(
    store: &Arc<Store>,
    slot: &Slot,
    owner: Rc<dyn Fn() -> bool>,
) -> Result<FileHistoryWindow, String> {
    let predecessor = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(predecessor) = predecessor {
        predecessor.invoke_close_clicked();
    }
    let window = FileHistoryWindow::new().map_err(|e| e.to_string())?;
    let roles = DomainRoles::new(&store.snapshot().services).map_err(|e| e.to_string())?;
    let domains = store
        .snapshot()
        .services
        .all()
        .filter(|s| {
            roles.local.contains(&s.id)
                || [
                    roles.combined_local_media,
                    roles.local_file_storage,
                    roles.trash,
                ]
                .contains(&s.id)
        })
        .map(|s| (s.key.clone(), s.name.clone(), s.id))
        .collect::<Vec<_>>();
    let initial = domains
        .iter()
        .position(|s| s.2 == roles.combined_local_media)
        .ok_or_else(|| "no combined local media service".to_string())?;
    window.set_domains(ModelRc::new(VecModel::from(
        domains
            .iter()
            .map(|s| SharedString::from(s.1.clone()))
            .collect::<Vec<_>>(),
    )));
    window.set_domain_index(i32::try_from(initial).map_err(|e| e.to_string())?);
    let state = Rc::new(State {
        active: Cell::new(true),
        chart: RefCell::new(Chart::default()),
        context: RefCell::new(FileSearchContext::default()),
        cancel: RefCell::new(None),
        receiver: RefCell::new(None),
    });
    let timer = Rc::new(Timer::default());
    let close = Rc::new({
        let state = state.clone();
        let slot = slot.clone();
        let weak = window.as_weak();
        let timer = timer.clone();
        move || {
            if !state.active.replace(false) {
                return;
            }
            if let Some(cancel) = state.cancel.borrow_mut().take() {
                cancel.store(true, Ordering::Release);
            }
            state.receiver.borrow_mut().take();
            timer.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    let valid = Rc::new({
        let state = state.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !state.active.get() {
                return false;
            }
            if !owner() || !weak.upgrade().is_some_and(|w| w.window().is_visible()) {
                close();
                return false;
            }
            true
        }
    });
    let refresh = Rc::new({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some(cancel) = state.cancel.borrow_mut().take() {
                cancel.store(true, Ordering::Release);
            }
            let cancel = Arc::new(AtomicBool::new(false));
            *state.cancel.borrow_mut() = Some(cancel.clone());
            window.set_loading(true);
            window.set_chart_visible(false);
            window.set_status("loading…".into());
            let (send, receive) = mpsc::channel();
            *state.receiver.borrow_mut() = Some(receive);
            let context = state.context.borrow().clone();
            let store = store.clone();
            std::thread::spawn(move || {
                let result =
                    model::load(&store, &context, 7680, &cancel).map_err(|e| e.to_string());
                let _ = send.send(result);
            });
        }
    });
    window.on_refresh({
        let refresh = refresh.clone();
        move || refresh()
    });
    window.on_domain_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let valid = valid.clone();
        move |index| {
            if !valid() {
                return;
            }
            let Some((key, _, _)) = usize::try_from(index).ok().and_then(|i| domains.get(i)) else {
                return;
            };
            state.context.borrow_mut().location =
                hydrus_core::search::context::LocationContext::single(ServiceKey::clone(key));
            refresh();
        }
    });
    window.on_enter({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move || {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let input = window.get_input();
            if input.trim().is_empty() {
                return;
            }
            match parse_api_search(&serde_json::json!([input.as_str()])) {
                Ok(predicates) => {
                    let mut context = state.context.borrow_mut();
                    for predicate in predicates {
                        if !context.predicates.contains(&predicate) {
                            context.predicates.push(predicate);
                        }
                    }
                    drop(context);
                    window.set_input(SharedString::new());
                    paint(&window, &state);
                    refresh();
                }
                Err(error) => window.set_status(error.to_string().into()),
            }
        }
    });
    window.on_remove({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |index| {
            if !valid() {
                return;
            }
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if index < state.context.borrow().predicates.len() {
                state.context.borrow_mut().predicates.remove(index);
                if let Some(w) = weak.upgrade() {
                    paint(&w, &state);
                }
                refresh();
            }
        }
    });
    window.on_toggle({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |index| {
            if !valid() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                state.chart.borrow_mut().toggle(index);
                if let Some(w) = weak.upgrade() {
                    paint(&w, &state);
                }
            }
        }
    });
    window.on_count_edited({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |min, max| {
            if !valid() {
                return;
            }
            let result = state
                .chart
                .borrow_mut()
                .range_y(i64::from(min), i64::from(max));
            if let Some(w) = weak.upgrade() {
                if let Err(error) = result {
                    w.set_status(error.into());
                }
                paint(&w, &state);
            }
        }
    });
    window.on_date_edited({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |start: SharedString, end: SharedString| {
            if !valid() {
                return;
            }
            let result = model::parse_date(&start).and_then(|start| {
                model::parse_date(&end).and_then(|end| state.chart.borrow_mut().range_x(start, end))
            });
            if let Some(w) = weak.upgrade() {
                if let Err(error) = result {
                    w.set_status(error.into());
                }
                paint(&w, &state);
            }
        }
    });
    window.on_refit({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |x| {
            if !valid() {
                return;
            }
            if x {
                state.chart.borrow_mut().refit_x();
            } else {
                state.chart.borrow_mut().refit_y();
            }
            if let Some(w) = weak.upgrade() {
                paint(&w, &state);
            }
        }
    });
    window.on_cancel_work({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move || {
            if !valid() {
                return;
            }
            if let Some(cancel) = state.cancel.borrow_mut().take() {
                cancel.store(true, Ordering::Release);
            }
            state.receiver.borrow_mut().take();
            if let Some(w) = weak.upgrade() {
                w.set_loading(false);
                w.set_chart_visible(false);
                w.set_status("Cancelled!".into());
            }
        }
    });
    timer.start(TimerMode::Repeated, Duration::from_millis(30), {
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move || {
            if !valid() {
                return;
            }
            let result = state
                .receiver
                .borrow()
                .as_ref()
                .and_then(|r| r.try_recv().ok());
            let Some(result) = result else {
                return;
            };
            state.receiver.borrow_mut().take();
            state.cancel.borrow_mut().take();
            let Some(w) = weak.upgrade() else {
                return;
            };
            w.set_loading(false);
            match result {
                Ok(history) => {
                    state.chart.borrow_mut().publish(history);
                    paint(&w, &state);
                    w.set_status(SharedString::new());
                    w.set_chart_visible(true);
                }
                Err(error) => {
                    w.set_status(error.into());
                    w.set_chart_visible(false);
                }
            }
        }
    });
    window.on_close_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(window.clone_strong());
    paint(&window, &state);
    refresh();
    Ok(window)
}
