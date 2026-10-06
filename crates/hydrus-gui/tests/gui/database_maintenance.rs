//! Ownership regressions for pending Database maintenance decisions.
//! These tests do not establish reference/render parity for every job.
use hydrus_gui::{
    Bound, MainWindow, Pages, SearchPage, bind, database_maintenance_window, headless,
};
use hydrus_gui_model::database_maintenance::Job;
use hydrus_store::{Store, popups, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn open(store: &Arc<Store>, bound: &Bound, job: Job) {
    database_maintenance_window::open(store, &bound.database_maintenance, job).unwrap();
}
fn jobs(store: &Store) -> Vec<popups::Job> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().secs()))
        .unwrap()
}
fn seed_orphan(store: &Store) {
    store
        .write(|ctx| {
            let hash: i64 =
                ctx.conn()
                    .query_row("SELECT hash_id FROM hashes LIMIT 1", [], |r| r.get(0))?;
            ctx.conn()
                .execute("INSERT INTO file_urls VALUES (?1, 999999)", [hash])?;
            Ok(())
        })
        .unwrap();
}
fn orphan_count(store: &Store) -> i64 {
    store
        .read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM file_urls WHERE url_id = 999999",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
}
fn settle() {
    // Give any erroneously admitted worker time to publish its Store result.
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let lines = ui
        .get_menu_panes()
        .row_data(usize::try_from(pane).unwrap())
        .unwrap()
        .lines;
    let index = lines.iter().position(|row| row.label == label).unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
fn menu_orphan(ui: &MainWindow) {
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 20.0, 22.0);
    choose(ui, 0, "db maintenance");
    choose(ui, 1, &format!("{}…", Job::OrphanUrlMappings.label()));
}

#[test]
fn no_cancel_x_and_hidden_questions_cannot_write_or_displace_a_successor() {
    let (_dirs, store) = super::subscriptions::store();
    seed_orphan(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = jobs(&store);
    for boundary in 0..5 {
        menu_orphan(&ui);
        let old = bound.database_maintenance.question().unwrap();
        match boundary {
            0 => old.invoke_answered(false),
            1 => old.invoke_cancelled(),
            2 => old
                .window()
                .dispatch_event(slint::platform::WindowEvent::CloseRequested),
            3 => old.invoke_force_close(),
            _ => {
                old.hide().unwrap();
                old.invoke_answered(true);
            }
        }
        assert!(bound.database_maintenance.question().is_none());
        assert!(!old.window().is_visible());
        menu_orphan(&ui);
        let current = bound.database_maintenance.question().unwrap();
        old.show().unwrap();
        old.invoke_answered(true);
        old.invoke_cancelled();
        assert!(current.window().is_visible(), "A must not hide B");
        assert!(bound.database_maintenance.question().is_some());
        old.hide().unwrap();
        current.invoke_answered(false);
        settle();
        assert_eq!(
            orphan_count(&store),
            1,
            "cancelled A must never repair the orphan"
        );
        assert_eq!(jobs(&store), before, "cancelled A must never publish a job");
    }

    // A live decision admits a real Store operation exactly once.
    menu_orphan(&ui);
    let live = bound.database_maintenance.question().unwrap();
    live.invoke_answered(true);
    live.show().unwrap();
    live.invoke_answered(true);
    live.invoke_answered(true);
    live.hide().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if jobs(&store)
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some("1 orphan url mappings deleted!"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "accepted native job must publish its actual deletion"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    settle();
    assert_eq!(orphan_count(&store), 0);
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(orphan_count(&reopened), 0, "the accepted repair is durable");
    let final_jobs = jobs(&store);
    assert_eq!(
        final_jobs
            .iter()
            .filter(|job| job.status_text_1.as_deref() == Some("1 orphan url mappings deleted!"))
            .count(),
        1
    );
    assert!(
        !final_jobs
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some("No orphan url mappings found!")),
        "repeated Yes must not launch a second job"
    );
}

#[test]
fn cancelled_hidden_and_replaced_initial_or_service_choices_are_terminal() {
    let (_dirs, store) = super::subscriptions::store();
    seed_orphan(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = jobs(&store);
    for job in [
        Job::Analyze,
        Job::TablesUsingDefinitions,
        Job::FixInconsistentMappings,
    ] {
        for boundary in 0..4 {
            open(&store, &bound, job);
            if let Some(question) = bound.database_maintenance.question() {
                question.invoke_answered(true);
            }
            let old = bound
                .database_maintenance
                .chooser()
                .expect("fixture provides multiple service choices");
            match boundary {
                0 => old.invoke_cancelled(),
                1 => old
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested),
                2 => {
                    old.hide().unwrap();
                    old.invoke_chosen(0);
                }
                _ => {
                    old.hide().unwrap();
                }
            }
            // Opening after a hidden predecessor retires it before B is created.
            open(&store, &bound, Job::OrphanUrlMappings);
            let current = bound.database_maintenance.question().unwrap();
            old.show().unwrap();
            old.invoke_chosen(0);
            old.invoke_chosen(1);
            old.invoke_cancelled();
            assert!(current.window().is_visible());
            assert!(bound.database_maintenance.question().is_some());
            assert!(bound.database_maintenance.chooser().is_none());
            old.hide().unwrap();
            current.invoke_cancelled();
            settle();
            assert_eq!(
                jobs(&store),
                before,
                "terminal chooser must never admit any job"
            );
            assert_eq!(orphan_count(&store), 1);
        }
    }
    // Invalid indices are terminal too and do not select a fallback job.
    open(&store, &bound, Job::Analyze);
    let invalid = bound.database_maintenance.chooser().unwrap();
    invalid.invoke_chosen(-1);
    invalid.invoke_chosen(1);
    assert!(bound.database_maintenance.chooser().is_none());
    settle();
    assert_eq!(jobs(&store), before);
}

#[test]
fn current_service_choice_repairs_real_mappings_once_and_releases_previous_question() {
    let (_dirs, store) = super::subscriptions::store();
    let pending = store
        .write(|ctx| {
            let registry = hydrus_store::services::ServiceRegistry::load(ctx.conn())?;
            let service = registry.tag_services().next().unwrap().id;
            let tables = hydrus_store::schema::MappingTables::new(service);
            let (tag, hash): (i64, i64) = ctx.conn().query_row(
                &format!("SELECT tag_id, hash_id FROM {} LIMIT 1", tables.current),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            ctx.conn().execute(
                &format!("INSERT INTO {} VALUES (?1, ?2)", tables.pending),
                [tag, hash],
            )?;
            Ok((tables.pending, tag, hash))
        })
        .unwrap();
    let conflict_count = || {
        store
            .read(|conn| {
                conn.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {} WHERE tag_id = ?1 AND hash_id = ?2",
                        pending.0
                    ),
                    [pending.1, pending.2],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(Into::into)
            })
            .unwrap()
    };
    assert_eq!(conflict_count(), 1);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    open(&store, &bound, Job::FixInconsistentMappings);
    let question = bound.database_maintenance.question().unwrap();
    question.invoke_answered(true);
    assert!(bound.database_maintenance.question().is_none());
    let chooser = bound.database_maintenance.chooser().unwrap();
    assert_eq!(chooser.get_choices().row_data(0).unwrap(), "all services");
    question.show().unwrap();
    question.invoke_answered(true);
    assert!(
        chooser.window().is_visible(),
        "the finished question cannot replace its service chooser"
    );
    question.hide().unwrap();
    chooser.invoke_chosen(0);
    assert!(bound.database_maintenance.chooser().is_none());
    chooser.show().unwrap();
    chooser.invoke_chosen(0);
    chooser.invoke_chosen(0);
    chooser.hide().unwrap();
    let expected = "Found 1 bad mappings! They _should_ be deleted, and your pending counts should be updated.";
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if jobs(&store)
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some(expected))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "live service choice must report the actual repaired conflict"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    settle();
    assert_eq!(conflict_count(), 0);
    let final_jobs = jobs(&store);
    assert_eq!(
        final_jobs
            .iter()
            .filter(|job| job.status_text_1.as_deref() == Some(expected))
            .count(),
        1
    );
    assert!(
        !final_jobs
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some("No inconsistent mappings found!"))
    );
}

#[test]
fn hidden_parent_rebind_last_bound_and_accepted_exit_retire_decisions() {
    let (_dirs, store) = super::subscriptions::store();
    seed_orphan(&store);
    store
        .write(|ctx| {
            let mut preferences: settings::GuiSettings = settings::get(ctx.conn())?;
            preferences.confirm_exit = true;
            settings::set(ctx.conn(), &preferences)?;
            settings::set(
                ctx.conn(),
                &settings::ShutdownWork {
                    action: 0,
                    ..settings::ShutdownWork::default()
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old_bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    open(&store, &old_bound, Job::OrphanUrlMappings);
    let hidden_parent = old_bound.database_maintenance.question().unwrap();
    ui.hide().unwrap();
    hidden_parent.invoke_answered(true);
    assert!(old_bound.database_maintenance.question().is_none());
    ui.show().unwrap();
    hidden_parent.show().unwrap();
    hidden_parent.invoke_answered(true);
    hidden_parent.hide().unwrap();
    open(&store, &old_bound, Job::OrphanUrlMappings);
    let old = old_bound.database_maintenance.question().unwrap();
    let current_bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(
        !old.window().is_visible(),
        "rebinding closes A's pending input"
    );
    open(&store, &current_bound, Job::OrphanUrlMappings);
    let current = current_bound.database_maintenance.question().unwrap();
    old.show().unwrap();
    old.invoke_answered(true);
    assert!(current.window().is_visible());
    old.hide().unwrap();
    let retained_slot = current_bound.database_maintenance.clone();
    let clone = current_bound.clone();
    drop(current_bound);
    assert!(
        current.window().is_visible(),
        "a remaining Bound still owns B"
    );
    drop(clone);
    assert!(
        !current.window().is_visible(),
        "last Bound drop retires B even with retained Slot"
    );
    current.show().unwrap();
    current.invoke_answered(true);
    current.hide().unwrap();
    assert!(retained_slot.question().is_none());

    let exit_bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    open(&store, &exit_bound, Job::OrphanUrlMappings);
    let blocked = exit_bound.database_maintenance.question().unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    blocked.invoke_answered(true);
    assert!(
        exit_bound.database_maintenance.question().is_none(),
        "pending client exit blocks child admission"
    );
    ui.invoke_answer(false);
    open(&store, &exit_bound, Job::OrphanUrlMappings);
    let closing = exit_bound.database_maintenance.question().unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!closing.window().is_visible());
    ui.show().unwrap();
    closing.show().unwrap();
    closing.invoke_answered(true);
    closing.hide().unwrap();
    open(&store, &exit_bound, Job::OrphanUrlMappings);
    assert!(
        exit_bound.database_maintenance.question().is_none(),
        "accepted exit permanently retires the binding"
    );
    settle();
    assert_eq!(orphan_count(&store), 1);
    assert!(jobs(&store).is_empty());
}

#[test]
fn cancelled_dialog_callbacks_release_store_and_owner_before_thread_exit() {
    let weak_store = std::thread::spawn(|| {
        let (_dirs, store) = super::subscriptions::store();
        let weak_store = Arc::downgrade(&store);
        let windows = headless::init();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        open(&store, &bound, Job::FixInconsistentMappings);
        let question = bound.database_maintenance.question().unwrap();
        question.invoke_answered(true);
        let chooser = bound.database_maintenance.chooser().unwrap();
        chooser.invoke_cancelled();
        assert!(bound.database_maintenance.chooser().is_none());
        drop((question, chooser, bound, ui, store));
        drop(windows);
        weak_store
    })
    .join()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while weak_store.strong_count() != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        weak_store.strong_count(),
        0,
        "cancelled native decisions cannot retain Store via an Rc callback cycle"
    );
}
