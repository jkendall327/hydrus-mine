//! File > import/export folders' "check import folder now" and "run export
//! folder now": a named folder alone, then "check all" / "run all", reach
//! the flags the scheduler reads.
use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::search::context::FileSearchContext;
use hydrus_gui_model::folder_runs::{
    EXPORT_FOLDERS_PAUSED, IMPORT_FOLDERS_PAUSED, check_import_folders, run_export_folders,
};
use hydrus_gui_model::folders::new_export_folder;
use hydrus_parse::folders::ImportFolderSettings;
use hydrus_store::settings::ExportFolders;

#[test]
fn named_then_all_folders_are_flagged_to_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    store
        .write(|ctx| {
            for name in ["a", "b"] {
                hydrus_store::import_folders::create_import_folder(
                    ctx.conn(),
                    name,
                    &ImportFolderSettings::default(),
                    &ImportOptionsSlice::default(),
                    false,
                    0,
                )?;
            }
            let folders = ["a", "b"]
                .map(|name| hydrus_parse::folders::ExportFolder {
                    name: name.into(),
                    ..new_export_folder(String::new(), FileSearchContext::default())
                })
                .to_vec();
            hydrus_store::settings::set(ctx.conn(), &ExportFolders(folders))
        })
        .unwrap();
    let checked = || -> Vec<bool> {
        store
            .read(hydrus_store::import_folders::import_folders)
            .unwrap()
            .iter()
            .map(|f| f.settings.check_now)
            .collect()
    };
    let running = || -> Vec<bool> {
        let folders: ExportFolders = store.read(hydrus_store::settings::get).unwrap();
        folders.0.iter().map(|f| f.run_now).collect()
    };
    assert_eq!(checked(), [false, false]);
    assert_eq!(
        check_import_folders(&store, Some("b".into())).unwrap(),
        None
    );
    assert_eq!(checked(), [false, true]);
    assert_eq!(check_import_folders(&store, None).unwrap(), None);
    assert_eq!(checked(), [true, true]);
    assert_eq!(running(), [false, false]);
    assert_eq!(run_export_folders(&store, Some("a".into())).unwrap(), None);
    assert_eq!(running(), [true, false]);
    assert_eq!(run_export_folders(&store, None).unwrap(), None);
    assert_eq!(running(), [true, true]);
}

#[test]
fn checking_a_folder_unpauses_it_and_paused_folders_say_so_but_are_still_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    store
        .write(|ctx| {
            let conn = ctx.conn();
            for (name, paused) in [("a", true), ("b", false)] {
                hydrus_store::import_folders::create_import_folder(
                    conn,
                    name,
                    &ImportFolderSettings::default(),
                    &ImportOptionsSlice::default(),
                    paused,
                    0,
                )?;
            }
            let mut pauses: hydrus_store::settings::FolderSettings =
                hydrus_store::settings::get(conn)?;
            pauses.pause_import_folders = true;
            pauses.pause_export_folders = true;
            hydrus_store::settings::set(conn, &pauses)
        })
        .unwrap();
    let state = || -> Vec<(String, bool, bool)> {
        store
            .read(hydrus_store::import_folders::import_folders)
            .unwrap()
            .iter()
            .map(|f| (f.name().to_owned(), f.paused(), f.settings.check_now))
            .collect()
    };
    assert_eq!(
        state(),
        [("a".into(), true, false), ("b".into(), false, false)]
    );
    // checking "a" unpauses it, flags it only, and says the folders are paused
    assert_eq!(
        check_import_folders(&store, Some("a".into())).unwrap(),
        Some(IMPORT_FOLDERS_PAUSED)
    );
    assert_eq!(
        state(),
        [("a".into(), false, true), ("b".into(), false, false)]
    );
    assert_eq!(
        run_export_folders(&store, None).unwrap(),
        Some(EXPORT_FOLDERS_PAUSED)
    );
}
