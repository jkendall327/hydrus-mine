//! "Check now" on an import folder: the schedule must let it come due.

use hydrus_download::folders::ImportFolderSchedule;
use hydrus_parse::folders::ImportFolderSettings;
use hydrus_store::{Store, import_folders};

fn folder(store: &Store, settings: ImportFolderSettings) -> i64 {
    store
        .write(move |ctx| {
            import_folders::create_import_folder(
                ctx.conn(),
                "drop box",
                &settings,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                false,
                0,
            )
        })
        .unwrap()
        .unwrap()
}

fn check_now(store: &Store, id: i64) {
    let mut settings = store
        .read(|conn| import_folders::import_folder(conn, id))
        .unwrap()
        .unwrap()
        .settings;
    settings.check_now = true;
    store
        .write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))
        .unwrap();
}

#[test]
fn a_folder_told_to_check_now_comes_due_although_the_schedule_is_refreshed_every_pass() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = folder(
        &store,
        ImportFolderSettings {
            path: dir.path().to_string_lossy().into_owned(),
            check_now: true,
            ..ImportFolderSettings::default()
        },
    );
    let mut schedule = ImportFolderSchedule::new();
    let mut now = 1_000_000;
    // (the first pass starts the schedule; a pass a second on, the way the
    // daemon looks again, must find the folder due, not set it afresh)
    schedule.refresh(&store, now).unwrap();
    for _ in 0..3 {
        now += 1;
        schedule.refresh(&store, now).unwrap();
    }
    assert_eq!(schedule.next_due(now), Some(id));
}

#[test]
fn a_folder_checked_just_now_and_then_told_to_check_now_comes_due_on_the_next_pass() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = folder(
        &store,
        ImportFolderSettings {
            path: dir.path().to_string_lossy().into_owned(),
            ..ImportFolderSettings::default()
        },
    );
    let mut schedule = ImportFolderSchedule::new();
    let mut now = 1_000_000;
    // the schedule starts (a new folder is due at once), the folder works
    // and is next due in 500 seconds
    schedule.refresh(&store, now).unwrap();
    now += 1;
    assert_eq!(schedule.next_due(now), Some(id));
    schedule.worked(id, Some(now + 500), now);
    assert_eq!(schedule.next_due(now), None);
    // the user chooses "check import folder now"
    check_now(&store, id);
    // the next passes, a second apart as the daemon's are, find it due
    schedule.refresh(&store, now).unwrap();
    now += 1;
    schedule.refresh(&store, now).unwrap();
    assert_eq!(schedule.next_due(now), Some(id));
}
