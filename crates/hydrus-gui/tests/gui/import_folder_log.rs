//! Real modal cache ownership: child/folder/manager cancellation and full seed restore.
use hydrus_gui::{FileLogWindow, MainWindow, Pages, bind, headless};
use hydrus_store::{
    import_folders,
    queues::{self, FileSeed, FileSeedMeta, SeedStatus, SeedType},
};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn initial(fixture: &Value) -> Vec<FileSeed> {
    fixture["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| FileSeed {
            id: 0,
            queue_id: 0,
            seed_type: SeedType::Path,
            data: row["data"].as_str().unwrap().into(),
            data_for_comparison: row["data"].as_str().unwrap().into(),
            created: row["created"].as_i64().unwrap(),
            modified: row["modified"].as_i64().unwrap(),
            source_time: row["source_time"].as_i64(),
            status: SeedStatus::from_code(row["status"].as_i64().unwrap()).unwrap(),
            note: row["note"].as_str().unwrap().into(),
            referral_url: Some("https://folder-log.example/source".into()),
            meta: FileSeedMeta {
                request_headers: vec![("X-File-Log".into(), "private header".into())],
                tags: std::collections::BTreeSet::from(["cached:tag".into()]),
                notes: vec![("note".into(), "complete cached note".into())],
                hashes: vec![("sha256".into(), "12".repeat(32))],
                ..FileSeedMeta::default()
            },
        })
        .collect()
}
fn cell(log: &FileLogWindow, index: usize, column: usize) -> String {
    log.get_rows()
        .row_data(index)
        .unwrap()
        .cells
        .row_data(column)
        .unwrap()
        .to_string()
}
fn assert_rows(log: &FileLogWindow, expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(log.get_rows().row_count(), expected.len());
    for (i, row) in expected.iter().enumerate() {
        assert_eq!(cell(log, i, 1), row["data"].as_str().unwrap());
        assert_eq!(cell(log, i, 6), row["note"].as_str().unwrap());
        let status = match row["status"].as_i64().unwrap() {
            0 => "",
            2 => "already in db",
            4 => "error",
            other => panic!("unrecorded status {other}"),
        };
        assert_eq!(cell(log, i, 2), status);
    }
}
fn choose(log: &FileLogWindow, path: &[&str]) {
    for (pane, label) in path.iter().enumerate() {
        let lines = log.get_menu_panes().row_data(pane).unwrap().lines;
        let at = lines.iter().position(|line| line.label == *label).unwrap();
        let pane = i32::try_from(pane).unwrap();
        let at = i32::try_from(at).unwrap();
        log.invoke_menu_line_hovered(pane, at, 300.0, 100.0, 10.0);
        log.invoke_menu_line_clicked(pane, at, 300.0, 100.0, 10.0);
    }
}

#[test]
fn copied_cache_replays_reference_child_apply_cancel_and_folder_cancel_then_manager_commit() {
    let fixture = hydrus_testkit::fixture_json("import_folder_log.json");
    for case in fixture["cases"].as_array().unwrap() {
        let windows = headless::init();
        let (_dirs, store) = crate::subscriptions::store();
        let work = tempfile::tempdir().unwrap();
        let path = work.path().to_string_lossy().into_owned();
        let seeds = initial(&fixture);
        let queue = store
            .write(move |ctx| {
                let settings = hydrus_parse::folders::ImportFolderSettings {
                    path,
                    ..hydrus_parse::folders::ImportFolderSettings::default()
                };
                let queue = import_folders::create_import_folder(
                    ctx.conn(),
                    "folder log",
                    &settings,
                    &hydrus_core::import_options::ImportOptionsSlice::default(),
                    true,
                    0,
                )?
                .unwrap();
                queues::restore_file_seeds(ctx.conn(), queue, &seeds)?;
                queues::take_nudges(ctx.conn())?;
                Ok(queue)
            })
            .unwrap();
        let original = store
            .read(move |conn| queues::file_seeds(conn, queue))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        crate::folders::open(&ui, "manage import folders\u{2026}");
        let list = bound
            .folders
            .import_list
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        list.invoke_row_clicked(0, false, false);
        list.invoke_edit();
        let folder = bound
            .folders
            .import_edit
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        folder.invoke_file_log();
        let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
        assert!(log.get_staged());
        assert!(folder.get_file_log_open());
        assert_rows(&log, &case["child_states"][0]["rows"]);
        // Neither parent can save while the modal child owns its cache draft.
        folder.invoke_apply();
        list.invoke_apply();
        assert!(bound.folders.import_list.borrow().is_some());
        assert!(bound.folders.import_edit.borrow().is_some());
        log.invoke_row_clicked(0, false, false);
        log.invoke_row_menu(0, 10.0, 10.0);
        choose(&log, &["try again"]);
        assert_rows(&log, &case["child_states"][1]["rows"]);
        log.invoke_row_clicked(1, false, false);
        log.invoke_delete_pressed();
        assert_eq!(
            log.get_asking_message().as_str(),
            case["calls"][2]["text"].as_str().unwrap()
        );
        log.invoke_apply();
        assert!(
            bound.folders.log.borrow().is_some(),
            "unanswered deletion freezes child Apply"
        );
        log.invoke_chosen(1);
        assert_rows(&log, &case["child_states"][1]["rows"]);
        log.invoke_delete_pressed();
        log.invoke_chosen(0);
        assert_rows(&log, &case["child_states"][2]["rows"]);
        assert_eq!(
            store
                .read(move |conn| queues::file_seeds(conn, queue))
                .unwrap(),
            original
        );
        assert!(
            !store.read(queues::any_nudged).unwrap(),
            "private edits never wake the live folder"
        );
        let pixels = headless::render(&windows.get(3).unwrap(), 1000, 600);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("import_folder_log.png"),
            &pixels,
            1000,
            600,
        )
        .unwrap();
        if case["child_applied"].as_bool().unwrap() {
            log.invoke_apply();
        } else {
            log.invoke_close_window();
        }
        assert!(bound.folders.log.borrow().is_none());
        assert!(!folder.get_file_log_open());
        log.invoke_chosen(0);
        log.invoke_apply();
        log.invoke_log_menu(0.0, 0.0);
        assert_eq!(
            store
                .read(move |conn| queues::file_seeds(conn, queue))
                .unwrap(),
            original
        );
        if case["folder_applied"].as_bool().unwrap() {
            folder.invoke_apply();
        } else {
            folder.invoke_cancel();
        }
        assert!(bound.folders.import_edit.borrow().is_none());
        folder.invoke_apply(); // Closed field editor cannot resurrect/save its clone.
        list.invoke_row_clicked(0, false, false);
        list.invoke_edit();
        let reopened = bound
            .folders
            .import_edit
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        reopened.invoke_file_log();
        let reopened_log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
        assert_rows(&reopened_log, &case["manager_rows"]);
        reopened_log.invoke_close_window();
        reopened.invoke_cancel();
        list.invoke_apply();
        let saved = store
            .read(move |conn| queues::file_seeds(conn, queue))
            .unwrap();
        let expected = case["reopened_rows"].as_array().unwrap();
        assert_eq!(saved.len(), expected.len());
        for (seed, row) in saved.iter().zip(expected) {
            assert_eq!(seed.data, row["data"].as_str().unwrap());
            assert_eq!(seed.status.code(), row["status"].as_i64().unwrap());
            assert_eq!(seed.note, row["note"].as_str().unwrap());
            assert_eq!(seed.created, row["created"].as_i64().unwrap());
            assert_eq!(seed.source_time, row["source_time"].as_i64());
            let before = original.iter().find(|old| old.data == seed.data).unwrap();
            assert_eq!(seed.meta, before.meta);
            assert_eq!(seed.referral_url, before.referral_url);
            assert_eq!(seed.data_for_comparison, before.data_for_comparison);
            assert_eq!(seed.queue_id, queue);
            assert_eq!(seed.seed_type, before.seed_type);
            if seed.status == before.status {
                assert_eq!(seed.modified, before.modified);
            } else {
                assert_ne!(
                    seed.modified, before.modified,
                    "retry changes only its modified time/status/note"
                );
            }
        }
        let next = store
            .read(move |conn| queues::next_file_seed(conn, queue))
            .unwrap()
            .unwrap();
        let expected_next = expected.iter().find(|row| row["status"] == 0).unwrap();
        assert_eq!(
            next.data,
            expected_next["data"].as_str().unwrap(),
            "accepted order/status reaches the real folder worker's next seed selection"
        );
        // Reopen the persisted manager, then cancel another accepted cache edit.
        crate::folders::open(&ui, "manage import folders\u{2026}");
        let list = bound
            .folders
            .import_list
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        list.invoke_row_clicked(0, false, false);
        list.invoke_edit();
        let folder = bound
            .folders
            .import_edit
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        folder.invoke_file_log();
        let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
        log.invoke_row_clicked(0, false, false);
        log.invoke_delete_pressed();
        log.invoke_chosen(0);
        log.invoke_apply();
        folder.invoke_cancel();
        list.invoke_cancel();
        log.invoke_apply();
        folder.invoke_apply();
        list.invoke_apply();
        assert_eq!(
            store
                .read(move |conn| queues::file_seeds(conn, queue))
                .unwrap(),
            saved,
            "manager Cancel rejects accepted cache edits and retained callbacks"
        );
    }
}

#[test]
fn unsaved_folder_has_owned_empty_log_and_owner_closure_discards_pending_cache() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(folder.get_has_file_log());
    folder.invoke_file_log();
    let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
    assert!(log.get_staged());
    assert_eq!(log.get_rows().row_count(), 0);
    hydrus_gui::set_clipboard_reader(|| Err("synthetic clipboard failure".into()));
    log.invoke_log_menu(0.0, 0.0);
    choose(&log, &["ADVANCED: import new sources", "from clipboard"]);
    assert!(log.get_asking());
    assert_eq!(log.get_asking_title(), "Problem pasting!");
    assert_eq!(log.get_asking_message(), "synthetic clipboard failure");
    list.invoke_cancel();
    assert!(!log.window().is_visible());
    assert!(!folder.window().is_visible());
    assert!(bound.folders.log.borrow().is_none());
    log.invoke_chosen(0);
    log.invoke_apply();
    folder.invoke_apply();
    list.invoke_apply();
    assert!(
        store
            .read(import_folders::import_folders)
            .unwrap()
            .is_empty()
    );
}
