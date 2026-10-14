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

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("file_maintenance_new.json")
}

fn step<'a>(recording: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    recording["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == name)
        .unwrap()
}

/// The reference's own "add new work" tab (oracle/record_file_maintenance_new.py, on the same
/// basic client), replayed through the real window: its words, every job's label and
/// description, the searches and easy selects with their counts, adding a job and the
/// scheduled list afterwards.
// leaf: audit-media-database-maintenance-search
// leaf: audit-media-database-maintenance-schedule
#[test]
fn the_new_work_tab_replays_the_reference_s_steps() {
    let recording = recorded();
    let rig = rig();
    let w = &rig.window;
    let opened = step(&recording, "opened");
    let text = |start: &str| {
        opened["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .find(|t| t.starts_with(start))
            .unwrap_or_else(|| panic!("{start}"))
            .to_owned()
    };
    assert_eq!(w.get_new_explanation(), text("Here you can queue up"));
    assert_eq!(w.get_search_status(), text("no results yet"));
    assert_eq!(w.get_files_label(), text("no files selected yet"));
    // every job: its label, and the description it shows
    let labels: Vec<String> = w.get_job_labels().iter().map(|l| l.to_string()).collect();
    let wanted: Vec<String> = opened["job_labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(labels, wanted);
    for (i, description) in opened["descriptions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        w.set_job_index(i32::try_from(i).unwrap());
        w.invoke_see_description();
        assert_eq!(
            w.get_information(),
            description["dialogs"][0][1].as_str().unwrap(),
            "{}",
            description["job"]
        );
        w.invoke_information_closed();
    }
    // no files, no job
    assert!(!w.get_can_add());
    // the default search, then the easy selects
    let selected = |n: &str| step(&recording, n)["selected"].as_str().unwrap().to_owned();
    w.invoke_run_search();
    until(|| !w.get_searching(), "the search");
    assert_eq!(
        w.get_search_status(),
        step(&recording, "run the default search")["search"]
            .as_str()
            .unwrap()
    );
    assert_eq!(w.get_files_label(), selected("run the default search"));
    assert!(w.get_can_add());
    w.invoke_easy_select(false);
    until(
        || w.get_files_label() == selected("all media files"),
        "all media files",
    );
    assert!(w.get_can_add());
    w.invoke_easy_select(true);
    until(
        || w.get_files_label() == selected("all repository update files"),
        "all repository update files",
    );
    assert!(!w.get_can_add());
    // the job the reference added, on all media files
    w.invoke_easy_select(false);
    until(|| w.get_can_add(), "the files");
    let added = step(&recording, "add the file metadata job on all media");
    let job = added["job"].as_i64().unwrap();
    assert_eq!(job, 0, "regenerate file metadata");
    let label = wanted[9].clone();
    w.set_job_index(9);
    assert_eq!(w.get_files_label(), added["selected"].as_str().unwrap());
    w.invoke_add_job();
    assert_eq!(w.get_new_question(), "", "few files need no confirmation");
    until(
        || w.get_information() == added["dialogs"][0][1].as_str().unwrap(),
        "jobs added",
    );
    w.invoke_information_closed();
    let after = step(&recording, "add a job on 1001 files, forget it");
    let rows = after["rows"].as_array().unwrap();
    until(
        || {
            w.get_rows()
                .iter()
                .map(|row| row.cells.iter().map(|c| c.to_string()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
                == rows
                    .iter()
                    .map(|r| {
                        r.as_array()
                            .unwrap()
                            .iter()
                            .map(|c| c.as_str().unwrap().to_owned())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
        },
        "the scheduled-work list",
    );
    assert_eq!(rows[0][0], label.as_str());
}

/// Past a thousand files the reference asks before it schedules.
// leaf: audit-media-database-maintenance-schedule
#[test]
fn a_job_on_over_a_thousand_files_asks_first() {
    let recording = recorded();
    let rig = rig();
    let w = &rig.window;
    // a thousand more files in the local domain
    rig.store
        .write_and_refresh(|ctx| {
            let conn = ctx.conn();
            // (in every domain that holds the existing files: the local ones, the combined
            // one, and the local storage)
            let domains: Vec<i64> = conn
                .prepare("SELECT DISTINCT service_id FROM file_domain_current")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for i in 0..1000_i64 {
                let id = 1_000_000 + i;
                conn.execute("INSERT INTO hashes (hash_id, sha256) VALUES (?, randomblob(32))", [id])?;
                conn.execute("INSERT INTO files (hash_id, size, mime) VALUES (?, 1, 1)", [id])?;
                for domain in &domains {
                    conn.execute("INSERT INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?, ?, 0)", [domain, &id])?;
                }
            }
            Ok(())
        })
        .unwrap();
    let everything = model::find(&rig.store, Pick::AllMedia, &[]).unwrap().len();
    let in_domain: i64 = rig
        .store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM file_domain_current", [], |r| r.get(0))?))
        .unwrap();
    assert!(everything > 1000, "{everything} of {in_domain}");
    w.invoke_easy_select(false);
    until(|| w.get_can_add(), "the files");
    until(
        || w.get_files_label() == model::selected(everything),
        "all media",
    );
    w.set_job_index(9);
    w.invoke_add_job();
    let asked = step(&recording, "add a job on 1001 files, forget it")["dialogs"][0].clone();
    let pretty = hydrus_core::numbers::human_int(everything as u64);
    assert_eq!(
        w.get_new_question(),
        asked[1].as_str().unwrap().replace("1,001", &pretty)
    );
    // declining schedules nothing
    w.invoke_new_answered(false);
    std::thread::sleep(Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    assert!(counts(&rig.store).is_empty());
    assert_eq!(w.get_new_question(), "");
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
