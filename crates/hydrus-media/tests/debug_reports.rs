//! Help > debug > report modes > similar files metadata generation report
//! mode: generating a file's perceptual hashes says what it is doing, in the
//! reference's words (`ClientImagePerceptualHashes`), compared with what the
//! running reference said for the same file
//! (`oracle/record_debug_report_modes.py`, the import of
//! `oracle/fixtures/debug_report_modes.png`). The switches are process-wide,
//! so this is tested alone here.
use std::sync::{Arc, Mutex};

use hydrus_core::Mime;
use hydrus_core::debug_flags::{self, Flag};
use hydrus_media::MediaTools;

// leaf: audit-options-help-debug-action-similar-files-metadata-generation-report-mode
#[test]
fn phash_generation_says_what_the_reference_says_for_the_same_file() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned());
    })));
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let recorded: Vec<String> = recording["import"]["on"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_owned())
        .filter(|t| t.starts_with("phash generation: "))
        .collect();
    assert_eq!(recorded.len(), 9);
    assert!(
        recording["import"]["off"]["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let path = hydrus_testkit::fixture_path("debug_report_modes.png");
    let tools = MediaTools::new();

    let silent = tools.perceptual_hashes(&path, Mime::ImagePng);
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );

    Flag::SimilarFilesMetadataGenerationReport.set(true);
    let hashes = tools.perceptual_hashes(&path, Mime::ImagePng);
    Flag::SimilarFilesMetadataGenerationReport.set(false);
    debug_flags::set_sink(None);
    assert_eq!(hashes, silent, "reporting changes nothing");
    // every line the same, but for the median, a float from the DCT whose
    // last digits are the reference's numpy arithmetic's own
    let said = seen.lock().unwrap().clone();
    assert_eq!(said.len(), recorded.len());
    for (mine, theirs) in said.iter().zip(&recorded) {
        if let Some(theirs) = theirs.strip_prefix("phash generation: median: ") {
            let mine: f64 = mine
                .strip_prefix("phash generation: median: ")
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                (mine - theirs.parse::<f64>().unwrap()).abs() < 1e-9,
                "{mine}"
            );
        } else {
            assert_eq!(mine, theirs);
        }
    }
    // and the hash it ends on is the one the reference reported
    let last = recorded.last().unwrap();
    assert_eq!(
        last.strip_prefix("phash generation: perceptual_hash: "),
        Some(hex::encode(hashes[0].0).as_str())
    );
}
