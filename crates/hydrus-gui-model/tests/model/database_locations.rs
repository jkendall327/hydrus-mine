//! Database > locations: its rows, buttons and words, as
//! `MoveMediaFilesPanel` shows them.
use std::path::PathBuf;

use hydrus_gui_model::database_locations::{
    Disk, buttons, granularity_label, ideal, percent, remove_question, row, thumbnail_estimates,
};
use hydrus_store::storage_locations::{Location, Review};

fn location(path: &str, weight: i64, max: Option<i64>, share: f64) -> Location {
    Location {
        path: PathBuf::from(path),
        weight,
        max_bytes: max,
        files_share: share,
        thumbnails_share: share,
        thumbnail_override: false,
    }
}

#[test]
fn rows_and_buttons_follow_the_reference() {
    let review = Review {
        locations: vec![
            location("/db/client_files", 1, None, 1.0),
            location("/other", 1, None, 0.0),
        ],
        total_bytes: 1024 * 1024,
        total_files: 10,
    };
    let shares = ideal(&review);
    assert_eq!(shares, [0.5, 0.5]);
    let first = row(
        &review.locations[0],
        shares[0],
        &review,
        0,
        false,
        std::path::Path::new("/db"),
        Disk::Free(2048),
    );
    assert_eq!(first[0], "/db/client_files");
    assert_eq!(first[1], "yes");
    assert_eq!(first[2], "2 KB");
    assert_eq!(first[3], "1 MB - 100% everything");
    assert_eq!(first[4], "1");
    assert_eq!(first[5], "n/a");
    assert_eq!(first[6], "512 KB - 50% everything");
    let second = row(
        &review.locations[1],
        shares[1],
        &review,
        0,
        false,
        std::path::Path::new("/db"),
        Disk::Missing,
    );
    assert_eq!(second[0], "DOES NOT EXIST: /other");
    assert_eq!(second[3], "nothing");
    // increase, decrease, set max size, remove
    assert_eq!(
        buttons(&review, Some(&review.locations[0])),
        [true, true, true, true]
    );
    let alone = Review {
        locations: vec![location("/db/client_files", 1, None, 1.0)],
        ..review.clone()
    };
    assert_eq!(
        buttons(&alone, Some(&alone.locations[0])),
        [false, false, false, false]
    );
    assert_eq!(percent(0.5), "50%");
    assert_eq!(percent(1.0 / 3.0), "33.33%");
    assert_eq!(thumbnail_estimates(0, 150, 125), (0, 0));
    assert!(granularity_label(2).ends_with("512 total subfolders."));
    assert_eq!(
        remove_question(true, false),
        "Are you sure you want to remove this location?"
    );
}
