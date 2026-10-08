//! Help > debug > report modes > file report mode: asking the file storage
//! for a path says so (`ClientFilesManager`). The switch is process-wide, so
//! this is tested alone here.
use std::sync::{Arc, Mutex};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::{Mime, Sha256};
use hydrus_store::Store;

// leaf: audit-options-help-debug-action-file-report-mode
#[test]
fn file_report_mode_reports_file_and_thumbnail_path_requests() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned());
    })));
    let dir = tempfile::tempdir().unwrap();
    let storage = Store::open(dir.path()).unwrap().snapshot().storage.clone();
    let hash = Sha256([7; 32]);
    storage.file_path(&hash, Mime::ImagePng);
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );
    Flag::FileReport.set(true);
    storage.file_path(&hash, Mime::ImagePng);
    storage.thumbnail_path(&hash);
    Flag::FileReport.set(false);
    let hex = hash.to_hex();
    assert_eq!(
        *seen.lock().unwrap(),
        [
            format!("File path request: ('{hex}', ImagePng)"),
            format!("Thumbnail path request: ('{hex}')"),
        ]
    );
    debug_flags::set_sink(None);
}
