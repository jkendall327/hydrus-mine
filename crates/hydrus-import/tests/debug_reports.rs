//! Help > debug > report modes > file import report mode: an import says what
//! it is doing, in the reference's words (`FileImportJob`). The switches are
//! process-wide, so this is tested alone here.
use std::sync::{Arc, Mutex};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;

// leaf: audit-options-help-debug-action-file-import-report-mode
#[test]
fn file_import_report_mode_reports_the_import_job_as_it_goes() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned())
    })));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(store, MediaTools::new());
    let path = hydrus_testkit::fixture_path("media/cbz_flat.cbz");

    importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );

    Flag::FileImportReport.set(true);
    let result = importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    Flag::FileImportReport.set(false);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4, "{seen:?}");
    assert!(seen[0].starts_with(&format!(
        "File import job created:\nSource: {}\nRaw import path: ",
        path.display()
    )));
    assert_eq!(seen[1], "File import job starting work.");
    assert_eq!(
        seen[2],
        format!("File import job hash: {}", result.hash.unwrap())
    );
    assert_eq!(
        seen[3],
        "File import job is done, now publishing content updates"
    );
}
