//! The file maintenance window's "add new work" tab: a typed search or an
//! easy-select button picks files off the GUI thread, and "add job" queues
//! the chosen job on them. The window's callbacks own the tab's state; its
//! poll holds it weakly. The current-work owner retires this tab on close,
//! and its exact-window admission gate also guards new work.
use crate::FileMaintenanceWindow;
use hydrus_core::HashId;
use hydrus_gui_model::file_maintenance_new::{self as model, Pick};
use hydrus_search::{Predicate, TextContext, parse_api_search, predicate_text};
use hydrus_store::{Store, file_maintenance::JobType};
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

enum Done {
    Found(Pick, Result<Vec<HashId>, String>),
    Added(Result<(), String>),
}

struct State {
    active: Cell<bool>,
    shutdown: Arc<AtomicBool>,
    window: slint::Weak<FileMaintenanceWindow>,
    valid: Rc<dyn Fn() -> bool>,
    predicates: RefCell<Vec<Predicate>>,
    files: RefCell<Vec<HashId>>,
    asking: RefCell<Option<(JobType, Vec<HashId>)>>,
    adding: Cell<bool>,
    done: (mpsc::Sender<Done>, mpsc::Receiver<Done>),
    timer: Timer,
}

/// The owning current-work window explicitly retires its new-work tab.
#[derive(Clone)]
pub(crate) struct Control(Rc<State>);
impl Control {
    pub fn retire(&self) {
        if !self.0.active.replace(false) {
            return;
        }
        self.0.shutdown.store(true, Ordering::Release);
        self.0.timer.stop();
        self.0.asking.borrow_mut().take();
        self.0.files.borrow_mut().clear();
        if let Some(window) = self.0.window.upgrade() {
            window.set_new_question(SharedString::new());
            window.set_can_add(false);
            window.set_searching(false);
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}
impl State {
    fn input(&self) -> Option<FileMaintenanceWindow> {
        if !self.active.get() {
            return None;
        }
        let window = self.window.upgrade()?;
        if !(self.valid)() || !window.window().is_visible() {
            // A hidden or superseded question cannot be accepted after showing
            // a retained handle again. Keep selected files for a fresh decision.
            self.asking.borrow_mut().take();
            window.set_new_question(SharedString::new());
            return None;
        }
        Some(window)
    }
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
pub(crate) fn bind(
    window: &FileMaintenanceWindow,
    store: &Arc<Store>,
    valid: Rc<dyn Fn() -> bool>,
    refresh: Rc<dyn Fn()>,
) -> Control {
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
        active: Cell::new(true),
        shutdown: Arc::new(AtomicBool::new(false)),
        window: window.as_weak(),
        valid,
        predicates: RefCell::default(),
        files: RefCell::default(),
        asking: RefCell::default(),
        adding: Cell::new(false),
        done: mpsc::channel(),
        timer: Timer::default(),
    });
    paint(window, &state);
    let start = {
        let state = state.clone();
        let store = store.clone();
        Rc::new(move |pick: Pick| {
            let Some(window) = state.input() else { return };
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
        Rc::new(move |job: JobType, files: Vec<HashId>| {
            let Some(window) = state.input() else { return };
            if state.adding.replace(true) {
                return;
            }
            paint(&window, &state);
            let send = state.done.0.clone();
            let store = store.clone();
            let shutdown = state.shutdown.clone();
            std::thread::spawn(move || {
                // Check retirement inside the queued writer transaction too.
                // Work already admitted by that transaction may complete.
                let added = store
                    .write(move |ctx| {
                        if shutdown.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        hydrus_store::file_maintenance::add_jobs(ctx.conn(), &files, job, 0)
                    })
                    .map_err(|e| e.to_string());
                let _ = send.send(Done::Added(added));
            });
        })
    };
    window.on_search_entered({
        let state = state.clone();
        move || {
            let Some(window) = state.input() else { return };
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
        move |index| {
            let Some(window) = state.input() else { return };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if index < state.predicates.borrow().len() {
                state.predicates.borrow_mut().remove(index);
                paint(&window, &state);
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
        let state = state.clone();
        move || {
            if let Some(window) = state.input() {
                window.set_information(model::description(job(&window)).into());
            }
        }
    });
    window.on_information_closed({
        let state = state.clone();
        move || {
            if let Some(window) = state.input() {
                window.set_information(SharedString::new());
            }
        }
    });
    window.on_add_job({
        let state = state.clone();
        let add = add.clone();
        move || {
            let Some(window) = state.input() else { return };
            let files = state.files.borrow().clone();
            if files.is_empty() || state.adding.get() || state.asking.borrow().is_some() {
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
        move |yes| {
            let Some(window) = state.input() else { return };
            let pending = state.asking.borrow_mut().take();
            window.set_new_question(SharedString::new());
            if let Some((job, files)) = pending
                && yes
            {
                add(job, files);
            }
        }
    });
    let polled = Rc::downgrade(&state);
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(state) = polled.upgrade() else {
                return;
            };
            let Some(window) = state.input() else { return };
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
    Control(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::Sha256;
    use hydrus_store::file_maintenance;
    use slint::Model as _;
    use std::time::Instant;

    fn pump(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() && Instant::now() < deadline {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ready(), "new maintenance work did not settle");
    }
    fn settle() {
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn store(files: u64) -> (tempfile::TempDir, Arc<Store>) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store.write_content(move |writer| {
            let roles = writer.roles().clone();
            for number in 0..files {
                let mut bytes = [0; 32];
                bytes[..8].copy_from_slice(&number.to_le_bytes());
                let hash = hydrus_store::master::intern_hash(writer.conn(), &Sha256(bytes))?;
                writer.conn().execute("INSERT INTO files(hash_id,size,mime,width,height) VALUES(?1,1,2,1,1)", [hash])?;
                for service in [roles.local_file_storage, roles.combined_local_media] {
                    writer.conn().execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,0)", rusqlite::params![service, hash])?;
                }
            }
            hydrus_store::domains::changed(writer.conn())?;
            hydrus_store::settings::set(writer.conn(), &file_maintenance::FileMaintenanceSettings {
                during_idle: false,
                during_active: false,
                ..file_maintenance::FileMaintenanceSettings::default()
            })?;
            Ok(())
        }).unwrap();
        (directory, store)
    }
    fn counts(store: &Store) -> std::collections::BTreeMap<JobType, (u64, u64)> {
        store
            .read(|conn| file_maintenance::job_counts(conn, i64::MAX / 2))
            .unwrap()
    }
    fn choose(window: &FileMaintenanceWindow, job: JobType) {
        window.set_job_index(
            i32::try_from(model::JOBS.iter().position(|item| *item == job).unwrap()).unwrap(),
        );
    }
    fn select(window: &FileMaintenanceWindow, files: usize) {
        window.invoke_easy_select(false);
        pump(|| !window.get_searching() && window.get_files_label() == model::selected(files));
        assert!(window.get_can_add());
    }
    fn open(ui: &crate::MainWindow, bound: &crate::Bound) -> FileMaintenanceWindow {
        let database = ui
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "database")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
        let pane = ui.get_menu_panes().row_data(0).unwrap();
        let maintenance = pane
            .lines
            .iter()
            .position(|row| row.label == "file maintenance")
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
        let pane = ui.get_menu_panes().row_data(1).unwrap();
        let manage = pane
            .lines
            .iter()
            .position(|row| row.label.starts_with("manage scheduled jobs"))
            .unwrap();
        assert!(pane.lines.row_data(manage).unwrap().usable);
        ui.invoke_menu_line_clicked(1, i32::try_from(manage).unwrap(), 0.0, 0.0, 0.0);
        bound
            .file_maintenance
            .as_ref()
            .unwrap()
            .slot()
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }
    fn ask(window: &FileMaintenanceWindow) {
        choose(window, JobType::Blurhash);
        window.invoke_add_job();
        assert_eq!(
            window.get_new_question(),
            model::schedule_question(JobType::Blurhash, 1001).unwrap()
        );
    }

    #[test]
    fn native_new_confirmation_close_hidden_replacement_and_reopen_reach_only_current_queue() {
        let _windows = crate::headless::init();
        let (_directory, store) = store(1001);
        let ui = crate::MainWindow::new().unwrap();
        ui.show().unwrap();
        let pages = || crate::Pages::single(crate::SearchPage::new(store.clone()));
        let first = crate::bind(&ui, pages());
        let old = open(&ui, &first);
        select(&old, 1001);
        old.invoke_new_answered(true);
        assert!(counts(&store).is_empty());
        ask(&old);
        old.invoke_new_answered(false);
        old.invoke_new_answered(true);
        assert!(old.get_new_question().is_empty());
        assert!(counts(&store).is_empty());
        ask(&old);
        // Repeated Add must not replace the captured job during its question.
        choose(&old, JobType::HasExif);
        old.invoke_add_job();
        assert_eq!(
            old.get_new_question(),
            model::schedule_question(JobType::Blurhash, 1001).unwrap()
        );
        old.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(
            first
                .file_maintenance
                .as_ref()
                .unwrap()
                .slot()
                .borrow()
                .is_none()
        );
        assert!(old.get_new_question().is_empty());
        assert!(!old.get_can_add());
        old.show().unwrap();
        old.invoke_new_answered(true);
        old.invoke_add_job();
        old.invoke_easy_select(false);
        old.invoke_run_search();
        settle();
        assert!(counts(&store).is_empty());
        assert!(!old.get_can_add());
        let next = open(&ui, &first);
        select(&next, 1001);
        ask(&next);
        let pending = next.get_new_question();
        old.invoke_close_clicked();
        old.invoke_new_answered(true);
        assert!(next.window().is_visible());
        assert_eq!(next.get_new_question(), pending);
        ui.hide().unwrap();
        next.invoke_new_answered(true);
        next.invoke_add_job();
        assert!(next.get_new_question().is_empty());
        assert!(counts(&store).is_empty());
        ui.show().unwrap();
        next.invoke_new_answered(true);
        assert!(counts(&store).is_empty());
        ask(&next); // hidden rejection keeps the selected real files.
        let second = crate::bind(&ui, pages());
        assert!(!next.window().is_visible());
        assert!(next.get_new_question().is_empty());
        let current = open(&ui, &second);
        select(&current, 1001);
        ask(&current);
        next.show().unwrap();
        next.invoke_new_answered(true);
        next.invoke_add_job();
        next.invoke_close_clicked();
        old.invoke_new_answered(true);
        settle();
        assert!(counts(&store).is_empty());
        assert!(current.window().is_visible());
        assert!(!current.get_new_question().is_empty());
        current.hide().unwrap();
        current.invoke_new_answered(true);
        assert!(current.get_new_question().is_empty());
        current.show().unwrap();
        current.invoke_new_answered(true);
        assert!(counts(&store).is_empty());
        ask(&current);
        choose(&current, JobType::HasExif);
        current.invoke_new_answered(true);
        current.invoke_new_answered(true);
        pump(|| current.get_information() == model::ADDED);
        assert_eq!(
            counts(&store),
            std::collections::BTreeMap::from([(JobType::Blurhash, (1001, 0))])
        );
        pump(|| {
            current.get_rows().iter().any(|row| {
                row.cells
                    .row_data(0)
                    .is_some_and(|label| label == JobType::Blurhash.description())
                    && row.cells.row_data(1).is_some_and(|count| count == "1,001")
            })
        });
        current.invoke_information_closed();
        current.invoke_close_clicked();
        let reopened = open(&ui, &second);
        pump(|| reopened.get_rows().row_count() == 1);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(0)
                .unwrap()
                .cells
                .row_data(1)
                .unwrap(),
            "1,001"
        );
        assert_eq!(counts(&Store::open(store.dir()).unwrap()), counts(&store));
        reopened.invoke_close_clicked();
        old.hide().unwrap();
        next.hide().unwrap();
        ui.hide().unwrap();
    }

    #[test]
    fn native_small_add_after_close_or_final_bound_drop_cannot_write_but_fresh_add_can() {
        let _windows = crate::headless::init();
        let (_directory, store) = store(1);
        let ui = crate::MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = crate::bind(
            &ui,
            crate::Pages::single(crate::SearchPage::new(store.clone())),
        );
        let closed = open(&ui, &bound);
        select(&closed, 1);
        choose(&closed, JobType::HasExif);
        closed.invoke_close_clicked();
        closed.show().unwrap();
        closed.invoke_add_job();
        settle();
        assert!(counts(&store).is_empty());
        let kept = open(&ui, &bound);
        select(&kept, 1);
        choose(&kept, JobType::Blurhash);
        let owner = bound.clone();
        drop(bound);
        assert!(kept.window().is_visible());
        kept.invoke_add_job();
        assert!(
            kept.get_new_question().is_empty(),
            "small selections schedule directly"
        );
        pump(|| kept.get_information() == model::ADDED);
        assert_eq!(
            counts(&store),
            std::collections::BTreeMap::from([(JobType::Blurhash, (1, 0))])
        );
        store
            .write(|ctx| file_maintenance::cancel_jobs(ctx.conn(), &JobType::ALL))
            .unwrap();
        choose(&kept, JobType::HasExif);
        drop(owner);
        assert!(!kept.window().is_visible());
        kept.show().unwrap();
        kept.invoke_add_job();
        kept.invoke_new_answered(true);
        settle();
        assert!(counts(&store).is_empty());
        closed.hide().unwrap();
        kept.hide().unwrap();
        ui.hide().unwrap();
    }

    #[test]
    fn native_accepted_add_queued_behind_writer_is_cancelled_by_tab_retirement() {
        let _windows = crate::headless::init();
        let (_directory, store) = store(1);
        let window = crate::app_title::new::<crate::FileMaintenanceWindow>().unwrap();
        window.show().unwrap();
        let tab = bind(&window, &store, Rc::new(|| true), Rc::new(|| {}));
        select(&window, 1);
        choose(&window, JobType::Blurhash);
        let (entered, writer_entered) = mpsc::channel();
        let (release, writer_release) = mpsc::channel();
        let blocker = std::thread::spawn({
            let store = store.clone();
            move || {
                store
                    .write(move |_| {
                        entered.send(()).unwrap();
                        writer_release
                            .recv_timeout(Duration::from_secs(10))
                            .unwrap();
                        Ok(())
                    })
                    .unwrap();
            }
        });
        writer_entered
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        window.invoke_add_job();
        assert!(tab.0.adding.get());
        tab.retire();
        release.send(()).unwrap();
        blocker.join().unwrap();
        // Observe the real worker's completion before asserting absence of writes.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(Done::Added(result)) = tab.0.done.1.try_recv() {
                assert!(result.is_ok());
                break;
            }
            assert!(Instant::now() < deadline, "retired worker did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(counts(&store).is_empty());
        assert!(!window.get_can_add());
        assert!(window.get_information().is_empty());
        window.hide().unwrap();
    }
}
