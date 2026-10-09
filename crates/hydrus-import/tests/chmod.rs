//! "ADVANCED: Do not do chmod when copying files" (Options > files and
//! trash) against the reference's own import, replaying
//! `oracle/fixtures/file_paths_options.json` (`oracle/record_file_paths_options.py`):
//! files of each recorded permission mode are imported by a real importer
//! (which applies its store's setting to the process, as the client does)
//! with the option off and on, and the stored files' modes are the
//! reference's. The setting holds for the whole process, so this is tested
//! alone here.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use hydrus_core::Mime;
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::settings::FileHandlingSettings;

// leaf: audit-options-files-and-trash-advanced-do-not-do-chmod-when-copying-files
#[test]
fn imported_files_get_the_permissions_the_reference_gives_them() {
    let recorded = hydrus_testkit::fixture_json("file_paths_options.json");
    // the modes below are for the umask the reference ran under
    let umask = recorded["umask"].as_u64().unwrap() as u32;
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("umask");
    std::fs::File::create(&probe).unwrap();
    let current = !std::fs::metadata(&probe).unwrap().permissions().mode() & 0o666;
    assert_eq!(current, umask, "the recording's umask");
    let store = Store::open(dir.path()).unwrap();
    let sources = tempfile::tempdir().unwrap();
    let cases = recorded["chmod"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let off = case["do_not_chmod"].as_bool().unwrap();
        // the option as Options saves it; the importer applies it
        store
            .write(move |ctx| {
                let mut settings: FileHandlingSettings = hydrus_store::settings::get(ctx.conn())?;
                settings.do_not_chmod = off;
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
        let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
        assert_eq!(hydrus_store::paths::do_not_chmod(), off);
        let seed = case["seed"].as_u64().unwrap() as u32;
        let source = sources.path().join(format!("chmod_{seed}.bmp"));
        std::fs::write(&source, hydrus_testkit::bmp(seed, 20000)).unwrap();
        let mode = case["source_mode"].as_u64().unwrap() as u32;
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(mode)).unwrap();
        let imported = importer
            .import_path(&source, &FileImportOptions::default())
            .unwrap();
        let hash = imported.hash.unwrap();
        assert_eq!(imported.mime, Some(Mime::ImageBmp));
        let stored = store
            .snapshot()
            .storage
            .file_path(&hash, Mime::ImageBmp)
            .unwrap();
        let stored_mode = std::fs::metadata(&stored).unwrap().permissions().mode() & 0o7777;
        assert_eq!(
            stored_mode,
            case["stored_mode"].as_u64().unwrap() as u32,
            "do not chmod {off}, source {mode:o}: stored {stored_mode:o}"
        );
    }
}
