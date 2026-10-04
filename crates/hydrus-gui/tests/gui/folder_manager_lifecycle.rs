//! Real folder-manager leases, timer waits, cancellation and store failures.

use std::time::{Duration, Instant};

use hydrus_gui::{Bound, FoldersWindow, MainWindow, Pages, bind, headless};
use hydrus_store::folder_activity::{self, Activity, Kind};
use hydrus_store::settings::{self, ExportFolders, FolderSettings};
use hydrus_store::{Store, import_folders};
use slint::{ComponentHandle as _, Model as _};

fn slot(bound: &Bound, kind: Kind) -> Option<FoldersWindow> {
    let slot = match kind {
        Kind::Import => &bound.folders.import_list,
        Kind::Export => &bound.folders.export_list,
    };
    slot.borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
}

fn open(ui: &MainWindow, kind: Kind) {
    crate::folders::open(
        ui,
        match kind {
            Kind::Import => "manage import folders\u{2026}",
            Kind::Export => "manage export folders\u{2026}",
        },
    );
}

fn settle(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "folder-manager timer did not settle"
        );
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
    }
}

fn set_pause(store: &Store, value: bool) {
    store
        .write(move |ctx| {
            let mut settings: FolderSettings = settings::get(ctx.conn())?;
            settings.pause_import_folders = value;
            settings.pause_export_folders = value;
            settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

#[test]
fn replay_reference_pause_wait_apply_cancel_and_original_pause_restoration() {
    let fixture = hydrus_testkit::fixture_json("folder_manager_lifecycle.json");
    let windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // Reference exceptions are exercised separately with genuine native read
    // and write failures, rather than pretending the native editor throws Qt.
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["answer"] != "exception")
    {
        let kind = if case["folder_kind"] == "import" {
            Kind::Import
        } else {
            Kind::Export
        };
        let paused = case["initially_paused"].as_bool().unwrap();
        set_pause(&store, paused);
        let worker = case["worker_initially_running"]
            .as_bool()
            .unwrap()
            .then(|| Activity::acquire(store.dir(), kind).unwrap().unwrap());
        open(&ui, kind);
        let list = slot(&bound, kind).unwrap();
        assert_eq!(list.get_waiting(), worker.is_some());
        assert!(folder_activity::paused(&store, kind).unwrap());
        assert!(Activity::acquire(store.dir(), kind).unwrap().is_none());
        assert_eq!(list.get_rows().row_count(), 0);
        if worker.is_some() {
            let wait = case["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["kind"] == "wait")
                .unwrap();
            assert_eq!(list.get_wait_message(), wait["text"].as_str().unwrap());
            list.invoke_apply();
            list.invoke_add();
            assert!(slot(&bound, kind).is_some());
            assert!(bound.folders.import_edit.borrow().is_none());
            assert!(bound.folders.export_edit.borrow().is_none());
            let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 860, 420);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("folder_manager_wait.png"),
                &pixels,
                860,
                420,
            )
            .unwrap();
        }
        drop(worker);
        settle(|| !list.get_waiting());
        if case["answer"] == "apply" {
            list.invoke_apply();
        } else {
            list.invoke_cancel();
        }
        assert!(slot(&bound, kind).is_none());
        assert!(!folder_activity::edit_requested(store.dir(), kind).unwrap());
        assert_eq!(
            folder_activity::paused(&store, kind).unwrap(),
            case["finally_paused"].as_bool().unwrap()
        );
        assert!(Activity::acquire(store.dir(), kind).unwrap().is_some());
        list.invoke_add();
        list.invoke_apply();
        assert!(
            slot(&bound, kind).is_none(),
            "retained callbacks cannot reopen a closed draft"
        );
        assert!(bound.folders.import_edit.borrow().is_none());
        assert!(bound.folders.export_edit.borrow().is_none());
    }
}

#[test]
fn wait_cancel_window_close_and_live_pause_changes_release_only_the_transient_request() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for kind in [Kind::Import, Kind::Export] {
        for window_close in [false, true] {
            set_pause(&store, false);
            let worker = Activity::acquire(store.dir(), kind).unwrap().unwrap();
            open(&ui, kind);
            let wait = slot(&bound, kind).unwrap();
            assert!(wait.get_waiting());
            set_pause(&store, true); // A separate live consumer changes its own preference.
            if window_close {
                wait.window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            } else {
                wait.invoke_cancel();
            }
            assert!(slot(&bound, kind).is_none());
            assert!(!folder_activity::edit_requested(store.dir(), kind).unwrap());
            assert!(folder_activity::paused(&store, kind).unwrap());
            drop(worker);
            std::thread::sleep(Duration::from_millis(120));
            slint::platform::update_timers_and_animations();
            assert!(
                slot(&bound, kind).is_none(),
                "cancelled wait must not open later"
            );
            wait.invoke_apply();
            assert!(slot(&bound, kind).is_none());
        }
        open(&ui, kind);
        let list = slot(&bound, kind).unwrap();
        set_pause(&store, false);
        assert!(folder_activity::paused(&store, kind).unwrap());
        list.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(
            !folder_activity::paused(&store, kind).unwrap(),
            "later user unpause survives manager closure"
        );
    }
}

#[test]
fn draft_is_read_after_worker_commit_and_read_failure_releases_the_request() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let worker = Activity::acquire(store.dir(), Kind::Import)
        .unwrap()
        .unwrap();
    open(&ui, Kind::Import);
    let wait = slot(&bound, Kind::Import).unwrap();
    assert!(wait.get_waiting());
    assert_eq!(wait.get_rows().row_count(), 0);
    store
        .write(|ctx| {
            import_folders::create_import_folder(
                ctx.conn(),
                "worker committed this folder",
                &hydrus_parse::folders::ImportFolderSettings::default(),
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                false,
                1,
            )?;
            Ok(())
        })
        .unwrap();
    drop(worker);
    settle(|| !wait.get_waiting());
    assert_eq!(wait.get_rows().row_count(), 1);
    assert_eq!(
        wait.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "worker committed this folder"
    );
    wait.invoke_cancel();
    let worker = Activity::acquire(store.dir(), Kind::Export)
        .unwrap()
        .unwrap();
    open(&ui, Kind::Export);
    let wait = slot(&bound, Kind::Export).unwrap();
    assert!(wait.get_waiting());
    store.write(|ctx| {
        ctx.conn().execute("INSERT OR REPLACE INTO settings(key,value) VALUES('export_folders','invalid json')", [])?;
        Ok(())
    }).unwrap();
    drop(worker);
    settle(|| slot(&bound, Kind::Export).is_none());
    assert!(!folder_activity::edit_requested(store.dir(), Kind::Export).unwrap());
    assert!(
        Activity::acquire(store.dir(), Kind::Export)
            .unwrap()
            .is_some()
    );
    store
        .write(|ctx| settings::set(ctx.conn(), &ExportFolders::default()))
        .unwrap();
    open(&ui, Kind::Export);
    slot(&bound, Kind::Export).unwrap().invoke_cancel();
}

#[test]
fn failed_folder_commit_closes_owners_and_releases_the_lease_without_partial_writes() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            import_folders::create_import_folder(
                ctx.conn(),
                "save failure",
                &hydrus_parse::folders::ImportFolderSettings::default(),
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                false,
                1,
            )?;
            Ok(())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for kind in [Kind::Import, Kind::Export] {
        let before = store.read(import_folders::import_folders).unwrap();
        open(&ui, kind);
        let list = slot(&bound, kind).unwrap();
        store.write(move |ctx| {
            let sql = match kind {
                Kind::Import => "CREATE TRIGGER reject_folders BEFORE UPDATE ON import_queues BEGIN SELECT RAISE(ABORT,'scripted folder save failure'); END;",
                Kind::Export => "CREATE TRIGGER reject_folders BEFORE INSERT ON settings WHEN NEW.key='export_folders' BEGIN SELECT RAISE(ABORT,'scripted folder save failure'); END;",
            };
            ctx.conn().execute_batch(sql)?;
            Ok(())
        }).unwrap();
        list.invoke_apply();
        assert!(slot(&bound, kind).is_none());
        assert!(!folder_activity::edit_requested(store.dir(), kind).unwrap());
        let after = store.read(import_folders::import_folders).unwrap();
        assert_eq!(after[0].settings, before[0].settings);
        list.invoke_add();
        list.invoke_apply();
        assert!(bound.folders.import_edit.borrow().is_none());
        assert!(bound.folders.export_edit.borrow().is_none());
        store
            .write(|ctx| {
                ctx.conn().execute_batch("DROP TRIGGER reject_folders;")?;
                Ok(())
            })
            .unwrap();
    }
}
