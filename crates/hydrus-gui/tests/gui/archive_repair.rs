//! Native Database dispatch, exact questions, real timestamp consumers and owners.
use hydrus_core::{HashId, Sha256};
use hydrus_gui::{ArchiveRepairWindow, MainWindow, Pages, archive_repair_window, bind, headless};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

fn seed(store: &Store) -> Vec<HashId> {
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    store.write_content(move |writer| {
        let roles=writer.roles().clone();
        let mut ids=Vec::new();
        for i in 0..2 {
            let hash:Sha256=recorded["corpus"]["hashes"][i].as_str().unwrap().parse().unwrap();
            let id=hydrus_store::master::intern_hash(writer.conn(),&hash)?;
            ids.push(id);
            for sid in [roles.local_file_storage,roles.combined_local_media] {
                writer.conn().execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![sid,id,recorded["corpus"]["imports"][i].as_i64().unwrap()])?;
            }
        }
        Ok(ids)
    }).unwrap()
}
fn times(store: &Store, ids: &[HashId]) -> Vec<Option<i64>> {
    store
        .read(|conn| {
            Ok(
                hydrus_store::media::load(conn, &store.snapshot().services, None, ids)?
                    .results
                    .iter()
                    .map(|m| m.archived.map(|t| t.millis()))
                    .collect(),
            )
        })
        .unwrap()
}
fn wait_phase(window: &ArchiveRepairWindow, phase: i32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while window.get_phase() != phase && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(window.get_phase(), phase, "{}", window.get_status());
}
#[test]
fn database_repair_has_owned_scan_population_questions_and_updates_real_media_on_reopen() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ids = seed(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let maintenance = pane
        .lines
        .iter()
        .position(|r| r.label == "file maintenance")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
    let pane = ui.get_menu_panes().row_data(1).unwrap();
    let repair = pane
        .lines
        .iter()
        .position(|r| r.label.starts_with("fix missing file archived times"))
        .unwrap();
    assert!(pane.lines.row_data(repair).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(repair).unwrap(), 0.0, 0.0, 0.0);
    let first = bound
        .archive_repair
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        first.get_question(),
        hydrus_gui_model::archive_repair::SCAN_QUESTION
    );
    first.invoke_scan_answered(false);
    assert!(bound.archive_repair.borrow().is_none());
    assert_eq!(times(&store, &ids), vec![None, None]);

    let changed = Rc::new(Cell::new(0));
    let slot = Rc::new(RefCell::new(None));
    let open = || {
        archive_repair_window::open(
            &store,
            &slot,
            {
                let changed = changed.clone();
                Rc::new(move || changed.set(changed.get() + 1))
            },
            {
                let weak = ui.as_weak();
                Rc::new(move || weak.upgrade().is_some_and(|w| w.window().is_visible()))
            },
        )
        .unwrap()
    };
    let cancelled = open();
    cancelled.invoke_scan_answered(true);
    cancelled.invoke_cancel_work();
    assert_eq!(cancelled.get_status(), "Cancelled!");
    cancelled.invoke_close_clicked();
    let declined = open();
    declined.invoke_scan_answered(true);
    wait_phase(&declined, 2);
    assert_eq!(
        declined
            .get_choices()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["do legacy times", "do import times", "do both"]
    );
    assert!(
        declined
            .get_question()
            .contains("--1 Missing Legacy Times--")
    );
    assert!(
        declined
            .get_question()
            .contains("--1 Missing Import Times--")
    );
    declined.invoke_close_clicked();
    declined.invoke_chosen(2);
    assert_eq!(times(&store, &ids), vec![None, None]);
    let retired = open();
    let accepted = open();
    retired.invoke_close_clicked();
    retired.invoke_scan_answered(true);
    assert!(slot.borrow().is_some());
    assert_eq!(accepted.get_phase(), 0);
    accepted.invoke_scan_answered(true);
    wait_phase(&accepted, 2);
    let index = windows.count() - 1;
    let pixels = headless::render(&windows.get(index).unwrap(), 680, 480);
    assert!(pixels.chunks_exact(4).any(|p| p[0] != p[1] || p[1] != p[2]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_time_repair.png"),
        &pixels,
        680,
        480,
    )
    .unwrap();
    ui.hide().unwrap();
    accepted.invoke_chosen(2); // immediate owner gate, without waiting for timer
    assert_eq!(times(&store, &ids), vec![None, None]);
    assert!(slot.borrow().is_none());
    ui.show().unwrap();
    accepted.invoke_chosen(2);
    assert_eq!(times(&store, &ids), vec![None, None]);
    let accepted = open();
    accepted.invoke_scan_answered(true);
    wait_phase(&accepted, 2);
    accepted.invoke_chosen(2);
    wait_phase(&accepted, 3);
    assert_eq!(changed.get(), 1);
    assert!(accepted.get_status().starts_with("Done!"));
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    assert_eq!(
        times(&store, &ids),
        vec![
            recorded["reopened_archived"][0].as_i64(),
            recorded["reopened_archived"][1].as_i64()
        ]
    );
    accepted.invoke_close_clicked();
    let reopened = open();
    reopened.invoke_scan_answered(true);
    wait_phase(&reopened, 3);
    assert_eq!(reopened.get_status(), "No missing archive times found!");
    reopened.invoke_close_clicked();
    assert_eq!(
        store
            .read(
                |conn| Ok(conn.query_row("SELECT count(*) FROM file_inbox", [], |r| r
                    .get::<_, i64>(0))?)
            )
            .unwrap(),
        0
    );
    ui.hide().unwrap();
}
