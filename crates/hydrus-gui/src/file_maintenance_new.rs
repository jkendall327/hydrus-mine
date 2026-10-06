//! The file maintenance window's "add new work" tab: a typed search or an
//! easy-select button picks files off the GUI thread, and "add job" queues
//! the chosen job on them. The window's callbacks own the tab's state; its
//! poll holds it weakly, so both end with the window.
use crate::FileMaintenanceWindow;
use hydrus_core::HashId;
use hydrus_gui_model::file_maintenance_new::{self as model, Pick};
use hydrus_search::{Predicate, TextContext, parse_api_search, predicate_text};
use hydrus_store::{Store, file_maintenance::JobType};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

enum Done {
    Found(Pick, Result<Vec<HashId>, String>),
    Added(Result<(), String>),
}

struct State {
    predicates: RefCell<Vec<Predicate>>,
    files: RefCell<Vec<HashId>>,
    asking: RefCell<Option<(JobType, Vec<HashId>)>>,
    adding: std::cell::Cell<bool>,
    done: (mpsc::Sender<Done>, mpsc::Receiver<Done>),
    timer: Timer,
}

fn paint(window: &FileMaintenanceWindow, state: &State) {
    window.set_predicates(ModelRc::new(VecModel::from(
        state
            .predicates
            .borrow()
            .iter()
            .map(|p| SharedString::from(predicate_text(p, &TextContext::default())))
            .collect::<Vec<_>>(),
    )));
    let files = state.files.borrow().len();
    window.set_can_add(files > 0 && !state.adding.get());
}

fn job(window: &FileMaintenanceWindow) -> JobType {
    usize::try_from(window.get_job_index())
        .ok()
        .and_then(|i| model::JOBS.get(i))
        .copied()
        .unwrap_or(model::JOBS[0])
}

/// Wire the tab on `window`; `refresh` reloads the scheduled-work list.
pub fn bind(window: &FileMaintenanceWindow, store: &Arc<Store>, refresh: Rc<dyn Fn()>) {
    window.set_new_explanation(model::EXPLANATION.into());
    window.set_search_status(model::NO_RESULTS.into());
    window.set_files_label(model::NONE_SELECTED.into());
    window.set_job_labels(ModelRc::new(VecModel::from(
        model::job_labels()
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    window.set_job_index(0);
    let state = Rc::new(State {
        predicates: RefCell::default(),
        files: RefCell::default(),
        asking: RefCell::default(),
        adding: std::cell::Cell::new(false),
        done: mpsc::channel(),
        timer: Timer::default(),
    });
    paint(window, &state);
    let start = {
        let state = state.clone();
        let store = store.clone();
        let weak = window.as_weak();
        Rc::new(move |pick: Pick| {
            let Some(window) = weak.upgrade() else { return };
            window.set_searching(true);
            if pick == Pick::Search {
                window.set_search_status(model::LOADING.into());
            }
            let predicates = state.predicates.borrow().clone();
            let send = state.done.0.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                let found = model::find(&store, pick, &predicates).map_err(|e| e.to_string());
                let _ = send.send(Done::Found(pick, found));
            });
        })
    };
    let add = {
        let state = state.clone();
        let store = store.clone();
        let weak = window.as_weak();
        Rc::new(move |job: JobType, files: Vec<HashId>| {
            state.adding.set(true);
            if let Some(window) = weak.upgrade() {
                paint(&window, &state);
            }
            let send = state.done.0.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                let added = model::schedule(&store, files, job).map_err(|e| e.to_string());
                let _ = send.send(Done::Added(added));
            });
        })
    };
    window.on_search_entered({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let input = window.get_search_input();
            if input.trim().is_empty() {
                return;
            }
            match parse_api_search(&serde_json::json!([input.as_str()])) {
                Ok(predicates) => {
                    let mut current = state.predicates.borrow_mut();
                    for predicate in predicates {
                        if !current.contains(&predicate) {
                            current.push(predicate);
                        }
                    }
                    drop(current);
                    window.set_search_input(SharedString::new());
                    paint(&window, &state);
                }
                Err(error) => window.set_search_status(error.to_string().into()),
            }
        }
    });
    window.on_predicate_removed({
        let state = state.clone();
        let weak = window.as_weak();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if index < state.predicates.borrow().len() {
                state.predicates.borrow_mut().remove(index);
                if let Some(window) = weak.upgrade() {
                    paint(&window, &state);
                }
            }
        }
    });
    window.on_run_search({
        let start = start.clone();
        move || start(Pick::Search)
    });
    window.on_easy_select(move |updates| {
        start(if updates {
            Pick::AllUpdates
        } else {
            Pick::AllMedia
        });
    });
    window.on_see_description({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_information(model::description(job(&window)).into());
            }
        }
    });
    window.on_information_closed({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_information(SharedString::new());
            }
        }
    });
    window.on_add_job({
        let state = state.clone();
        let weak = window.as_weak();
        let add = add.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let files = state.files.borrow().clone();
            if files.is_empty() || state.adding.get() {
                return;
            }
            let job = job(&window);
            if let Some(question) = model::schedule_question(job, files.len()) {
                *state.asking.borrow_mut() = Some((job, files));
                window.set_new_question(question.into());
            } else {
                add(job, files);
            }
        }
    });
    window.on_new_answered({
        let state = state.clone();
        let weak = window.as_weak();
        move |yes| {
            if let Some(window) = weak.upgrade() {
                window.set_new_question(SharedString::new());
            }
            if let Some((job, files)) = state.asking.borrow_mut().take()
                && yes
            {
                add(job, files);
            }
        }
    });
    let weak = window.as_weak();
    let polled = Rc::downgrade(&state);
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let (Some(window), Some(state)) = (weak.upgrade(), polled.upgrade()) else {
                return;
            };
            while let Ok(done) = state.done.1.try_recv() {
                match done {
                    Done::Found(pick, Ok(files)) => {
                        window.set_searching(false);
                        if pick == Pick::Search {
                            window.set_search_status(model::found(files.len()).into());
                        }
                        window.set_files_label(model::selected(files.len()).into());
                        *state.files.borrow_mut() = files;
                    }
                    Done::Found(_, Err(error)) => {
                        window.set_searching(false);
                        window.set_search_status(error.into());
                    }
                    Done::Added(result) => {
                        state.adding.set(false);
                        match result {
                            Ok(()) => {
                                window.set_information(model::ADDED.into());
                                refresh();
                            }
                            Err(error) => window.set_error(error.into()),
                        }
                    }
                }
                paint(&window, &state);
            }
        });
}
