//! "Check now" on an import folder: the schedule must let it come due.

use hydrus_download::folders::ImportFolderSchedule;
use hydrus_parse::folders::ImportFolderSettings;
use hydrus_store::{Store, import_folders};

#[test]
fn a_folder_told_to_check_now_comes_due_although_the_schedule_is_refreshed_every_pass() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let settings = ImportFolderSettings {
        path: dir.path().to_string_lossy().into_owned(),
        check_now: true,
        ..ImportFolderSettings::default()
    };
    let id = store
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
        .unwrap();
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
