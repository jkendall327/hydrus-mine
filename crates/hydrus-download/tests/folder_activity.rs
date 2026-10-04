//! Deterministically request a manager pause during real import-folder work.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use hydrus_download::Downloader;
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_store::folder_activity::{Edit, Kind};
use hydrus_store::{Store, import_folders, queues};

#[test]
fn active_import_finishes_its_commit_before_the_manager_reads_and_stops_before_next_seed() {
    let legacy = hydrus_testkit::legacy_fixture("import_folder");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let work = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/fixtures/import_folder/a.png");
    for name in ["lease-a.png", "lease-b.png"] {
        std::fs::copy(&source, work.path().join(name)).unwrap();
    }
    let mut folder = store
        .read(|conn| import_folders::find_import_folder(conn, "drop box"))
        .unwrap()
        .unwrap();
    let id = folder.id();
    folder.settings.path = work.path().to_string_lossy().into_owned();
    folder.settings.check_now = true;
    folder.settings.last_modified_time_skip_period = 0;
    folder.settings.show_working_popup = true;
    folder.settings.actions = hydrus_parse::folders::FolderActions::default();
    let settings = folder.settings;
    store
        .write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))
        .unwrap();
    let net = Arc::new(NetEngine::new(store.clone(), NetOptions::default()).unwrap());
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let downloader = Downloader::new(store.clone(), net, importer).unwrap();

    // Pause the actual worker at its publication transaction. Its lease is
    // already held, and this happens before checking/importing any path. This
    // gate is confined to the test writer connection; production has no hook.
    let (entered, receiving) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    store.write(move |ctx| {
        ctx.conn().create_scalar_function("folder_test_gate", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8, move |_| {
            entered.send(()).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            waiting.recv_timeout(Duration::from_secs(10)).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(0_i32)
        })?;
        ctx.conn().execute_batch("CREATE TRIGGER folder_gate BEFORE INSERT ON popups WHEN json_extract(NEW.job,'$.status_title') LIKE 'import folder - %' BEGIN SELECT folder_test_gate(); END;")?;
        Ok(())
    }).unwrap();
    let worker = std::thread::spawn(move || downloader.work_on_import_folder(id));
    receiving.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut manager = Edit::request(store.dir(), Kind::Import).unwrap().unwrap();
    assert!(
        !manager.try_ready().unwrap(),
        "draft cannot open before worker commits"
    );
    assert!(manager.mark_applied().is_err());
    release.send(()).unwrap();
    let run = worker.join().unwrap().unwrap();
    assert!(run.checked);
    assert_eq!(run.new_files, 2);
    assert_eq!(
        run.imported, 0,
        "pause request must be checked before first seed"
    );
    assert_eq!(run.error, None);
    assert!(manager.try_ready().unwrap());
    let seeds = store.read(|conn| queues::file_seeds(conn, id)).unwrap();
    let pending: Vec<_> = seeds
        .iter()
        .filter(|seed| seed.data.contains("lease-"))
        .collect();
    assert_eq!(pending.len(), 2);
    assert!(
        pending
            .iter()
            .all(|seed| seed.status == queues::SeedStatus::Unknown)
    );
    let committed = store
        .read(|conn| import_folders::import_folder(conn, id))
        .unwrap()
        .unwrap();
    assert!(!committed.settings.check_now);
    assert!(committed.settings.last_checked > 0);
    assert!(
        !committed.paused(),
        "temporary manager pause must not become a folder error pause"
    );
    drop(manager);
    store
        .write(|ctx| {
            ctx.conn().execute_batch("DROP TRIGGER folder_gate;")?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn successful_manager_apply_resets_due_cache_but_cancel_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = store
        .write(|ctx| {
            import_folders::create_import_folder(
                ctx.conn(),
                "schedule notification",
                &hydrus_parse::folders::ImportFolderSettings::default(),
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                false,
                1,
            )
        })
        .unwrap()
        .unwrap();
    let mut schedule = hydrus_download::folders::ImportFolderSchedule::new();
    schedule.refresh(&store, 10).unwrap();
    assert_eq!(schedule.next_due(11), Some(id));
    schedule.worked(id, Some(500), 10);
    assert_eq!(schedule.next_due(11), None);
    let mut editor = Edit::request(dir.path(), Kind::Import).unwrap().unwrap();
    assert!(editor.try_ready().unwrap());
    editor.mark_applied().unwrap();
    drop(editor);
    schedule.refresh(&store, 11).unwrap();
    assert_eq!(schedule.next_due(12), Some(id));
    schedule.worked(id, Some(500), 20);
    let editor = Edit::request(dir.path(), Kind::Import).unwrap().unwrap();
    drop(editor);
    schedule.refresh(&store, 21).unwrap();
    assert_eq!(
        schedule.next_due(22),
        None,
        "Cancel cannot publish an Apply notification"
    );
}
