//! Database > file maintenance > manage scheduled jobs…, "add new work" tab
//! (`ReviewFileMaintenance`), opened from the real menu: its words, the job
//! choices and their descriptions, choosing files by a typed search or by the
//! easy-select buttons, and "add job" queueing the chosen job on them.

use std::time::{Duration, Instant};

use super::database_menu_jobs::{basic, from_menu_path};
use hydrus_gui::{FileMaintenanceWindow, MainWindow, Pages, bind, headless};
use hydrus_gui_model::file_maintenance_new as model;
use hydrus_gui_model::file_maintenance_new::Pick;
use hydrus_store::Store;
use hydrus_store::file_maintenance::{self, JobType};
use slint::{ComponentHandle as _, Model as _};

fn until(mut ready: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "never: {what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn counts(store: &Store) -> std::collections::BTreeMap<JobType, (u64, u64)> {
    store
        .read(|conn| file_maintenance::job_counts(conn, i64::MAX / 2))
        .unwrap()
}

struct Rig {
    _dir: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    window: FileMaintenanceWindow,
    _bound: hydrus_gui::Bound,
    _windows: headless::Windows,
}

fn rig() -> Rig {
    let (dir, store) = basic();
    store
        .write(|ctx| file_maintenance::cancel_jobs(ctx.conn(), &JobType::ALL))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    from_menu_path(&ui, &["file maintenance"], "manage scheduled jobs\u{2026}");
    let window = bound
        .file_maintenance
        .as_ref()
        .unwrap()
        .slot()
        .borrow()
        .as_ref()
        .expect("the window opened")
        .clone_strong();
    assert!(window.window().is_visible());
    Rig {
        _dir: dir,
        store,
        ui,
        window,
        _bound: bound,
        _windows: windows,
    }
}

// leaf: audit-media-database-maintenance-search
#[test]
fn new_work_selects_files_by_search_or_the_easy_select_buttons() {
    let rig = rig();
    let w = &rig.window;
    assert_eq!(w.get_new_explanation(), model::EXPLANATION);
    assert_eq!(w.get_search_status(), model::NO_RESULTS);
    assert_eq!(w.get_files_label(), model::NONE_SELECTED);
    assert!(!w.get_can_add());
    let everything = model::find(&rig.store, Pick::AllMedia, &[]).unwrap().len();
    assert!(everything > 1);

    // "run this search" with nothing typed is system:everything, in the
    // default local file domain
    let searched = model::find(&rig.store, Pick::Search, &[]).unwrap().len();
    assert!(searched > 1);
    w.invoke_run_search();
    assert_eq!(w.get_search_status(), model::LOADING);
    until(|| !w.get_searching(), "the search");
    assert_eq!(w.get_search_status(), model::found(searched));
    assert_eq!(w.get_files_label(), model::selected(searched));
    assert!(w.get_can_add());

    // a typed predicate narrows it, and can be taken away again
    w.set_search_input("system:archive".into());
    w.invoke_search_entered();
    assert_eq!(w.get_search_input(), "");
    assert_eq!(w.get_predicates().row_count(), 1);
    w.invoke_run_search();
    until(|| !w.get_searching(), "the narrowed search");
    let archived = model::find(
        &rig.store,
        Pick::Search,
        &hydrus_search::parse_api_search(&serde_json::json!(["system:archive"])).unwrap(),
    )
    .unwrap()
    .len();
    assert_eq!(w.get_search_status(), model::found(archived));
    assert_eq!(w.get_files_label(), model::selected(archived));
    w.invoke_predicate_removed(0);
    assert_eq!(w.get_predicates().row_count(), 0);

    // an unreadable search says why and keeps nothing
    w.set_search_input("system:not a real predicate".into());
    w.invoke_search_entered();
    assert_eq!(w.get_predicates().row_count(), 0);
    assert_ne!(w.get_search_status(), model::found(archived));

    // the easy-select buttons: all media files, all repository update files
    w.invoke_easy_select(false);
    until(
        || w.get_files_label() == model::selected(everything),
        "all media files",
    );
    assert!(w.get_can_add());
    w.invoke_easy_select(true);
    until(
        || w.get_files_label() == model::selected(0),
        "all repository update files",
    );
    assert!(!w.get_can_add(), "nothing to add a job to");
    let _ = &rig.ui;
}

// leaf: audit-media-database-maintenance-schedule
#[test]
fn new_work_lists_jobs_describes_them_and_queues_the_chosen_one_on_the_selected_files() {
    let rig = rig();
    let w = &rig.window;
    // the job types, in the reference's order, with their descriptions
    let labels: Vec<String> = w.get_job_labels().iter().map(|l| l.to_string()).collect();
    assert_eq!(labels, model::job_labels());
    assert_eq!(labels.len(), 27);
    let job = model::JOBS[3];
    w.set_job_index(3);
    w.invoke_see_description();
    assert_eq!(w.get_information(), model::description(job));
    w.invoke_information_closed();
    assert_eq!(w.get_information(), "");

    // no files, no job
    w.invoke_add_job();
    assert!(counts(&rig.store).is_empty());

    // files chosen: the job is queued on every one of them
    w.invoke_easy_select(false);
    until(|| w.get_can_add(), "the files");
    let everything = model::find(&rig.store, Pick::AllMedia, &[]).unwrap().len();
    w.invoke_add_job();
    assert_eq!(w.get_new_question(), "", "few files need no confirmation");
    until(|| w.get_information() == model::ADDED, "jobs added");
    assert_eq!(counts(&rig.store)[&job].0, everything as u64);
    assert_eq!(counts(&rig.store).len(), 1);
    // the current-work list now shows it
    until(
        || {
            w.get_rows()
                .iter()
                .any(|row| row.cells.row_data(0).unwrap() == job.description())
        },
        "the scheduled-work list",
    );
}
