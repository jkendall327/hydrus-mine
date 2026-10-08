//! Help > debug > report modes > similar files metadata generation report
//! mode: generating a perceptual hash says what it is doing, in the
//! reference's words (`GenerateShapeHash`). The switches are process-wide, so
//! this is tested alone here.
use std::sync::{Arc, Mutex};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_media::{Raster, perceptual_hash};

// leaf: audit-options-help-debug-action-similar-files-metadata-generation-report-mode
#[test]
fn phash_generation_report_mode_reports_each_stage() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned())
    })));
    let image = Raster::new(40, 30, 3, vec![120; 40 * 30 * 3]).unwrap();
    let silent = perceptual_hash(&image);
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );
    Flag::SimilarFilesMetadataGenerationReport.set(true);
    let hash = perceptual_hash(&image);
    Flag::SimilarFilesMetadataGenerationReport.set(false);
    assert_eq!(hash, silent, "reporting changes nothing");
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[..5],
        [
            "phash generation: image shape: (30, 40, 3)",
            "phash generation: grey image shape: (30, 40)",
            "phash generation: tiny image shape: (32, 32)",
            "phash generation: tiny float image shape: (32, 32)",
            "phash generation: generating dct",
        ]
    );
    assert!(seen[5].starts_with("phash generation: median: "));
    assert_eq!(seen[6], "phash generation: collapsing bytes");
    assert!(seen[7].starts_with("phash generation: perceptual_hash: "));
    assert_eq!(seen.len(), 8);
    debug_flags::set_sink(None);
}
