//! File > import/export folders' "check import folder now" and "run export
//! folder now": a named folder alone, then "check all" / "run all", reach
//! the flags the scheduler reads.
use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::search::context::FileSearchContext;
use hydrus_gui_model::folder_runs::{check_import_folders, run_export_folders};
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
    check_import_folders(&store, Some("b".into())).unwrap();
    assert_eq!(checked(), [false, true]);
    check_import_folders(&store, None).unwrap();
    assert_eq!(checked(), [true, true]);
    assert_eq!(running(), [false, false]);
    run_export_folders(&store, Some("a".into())).unwrap();
    assert_eq!(running(), [true, false]);
    run_export_folders(&store, None).unwrap();
    assert_eq!(running(), [true, true]);
}
