//! The "review files to import" window's list against the reference's
//! (`oracle/record_local_import.py`): the same folder (with sidecars, a
//! subfolder, a duplicate, an empty file, a `Thumbs.db` and a file it
//! can't import), one of its files again and a missing path, parsed with
//! and without the folder's subfolders, must give the reference's rows
//! (#, path, filetype, size, in order), progress text, buttons and files
//! to import.

use std::path::{MAIN_SEPARATOR_STR, Path, PathBuf};
use std::time::Duration;

use serde_json::Value as Json;

use hydrus_gui_model::local_import::{Parsed, Review};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The folder the reference parsed, made again.
fn folder(work: &Path) -> PathBuf {
    let folder = work.join("in");
    copy_dir(&hydrus_testkit::fixture_path("import_folder"), &folder);
    std::fs::File::create(folder.join("empty.png")).unwrap();
    // (a picture, but by its name not one to import)
    std::fs::copy(folder.join("a.png"), folder.join("Thumbs.db")).unwrap();
    folder
}

fn parse_all(review: &mut Review) {
    for _ in 0..1000 {
        if !review.working() {
            return;
        }
        review.work(Duration::from_millis(100));
    }
    panic!("parsing never finished");
}

/// The buttons as the reference's enables them.
fn enabled(review: &Review) -> Json {
    serde_json::json!({
        "import now": review.can_import(),
        "add tags/urls with the import >>": review.can_import(),
        "pause": review.working(),
        "stop": review.working(),
    })
}

// leaf: audit-network-import-parsing
#[test]
fn the_list_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("local_import_dialog.json");
    let work = tempfile::tempdir().unwrap();
    let folder = folder(work.path());
    let missing = work.path().join("gone.png");
    let folder_text = folder.to_string_lossy().into_owned();
    let missing_text = missing.to_string_lossy().into_owned();
    // (a recorded path as ours: the folder's, with this system's separator)
    let path = |recorded: &str| -> String {
        if recorded == "<missing>" {
            return missing_text.clone();
        }
        recorded
            .replacen("<folder>", &folder_text, 1)
            .replace('/', MAIN_SEPARATOR_STR)
    };

    // with nothing given yet
    let review = Review::new();
    let (text, _, _) = review.progress();
    assert_eq!(text, recorded["empty"]["progress"]);
    assert_eq!(enabled(&review), recorded["empty"]["enabled"]);

    for case in recorded["cases"].as_array().unwrap() {
        let subdirectories = case["search_subdirectories"].as_bool().unwrap();
        let mut review = Review::new();
        review.search_subdirectories = subdirectories;
        review.add_paths(
            case["given"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| path(p.as_str().unwrap())),
        );
        parse_all(&mut review);
        let theirs: Vec<Vec<String>> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let cells: Vec<&str> = row
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap())
                    .collect();
                vec![
                    cells[0].to_owned(),
                    path(cells[1]),
                    cells[2].to_owned(),
                    cells[3].to_owned(),
                ]
            })
            .collect();
        let ours: Vec<Vec<String>> = review.parsed().iter().map(|p| p.row().to_vec()).collect();
        assert_eq!(ours, theirs, "subdirectories {subdirectories}");
        let (text, done, total) = review.progress();
        assert_eq!(text, case["progress"], "subdirectories {subdirectories}");
        assert_eq!(done, total);
        assert_eq!(enabled(&review), case["enabled"]);
        let imported: Vec<String> = case["imported"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| path(p.as_str().unwrap()))
            .collect();
        assert_eq!(review.good_paths(), imported);
    }
}

/// While parsing: "x/y files parsed", the pause and stop buttons, and no
/// importing until it is done or paused; stopped, what is left goes.
#[test]
fn parsing_shows_its_progress_and_can_be_paused_and_stopped() {
    let work = tempfile::tempdir().unwrap();
    let folder = folder(work.path());
    let mut review = Review::new();
    review.add_paths([
        folder.join("a.png").to_string_lossy().into_owned(),
        folder.join("b p12.jpg").to_string_lossy().into_owned(),
        folder.join("dupe.gif").to_string_lossy().into_owned(),
    ]);
    assert_eq!(review.progress(), ("0/3 files parsed.".to_owned(), 0, 3));
    // (one at a time: no time to spare)
    review.work(Duration::ZERO);
    assert_eq!(review.progress(), ("1/3 files parsed.".to_owned(), 1, 3));
    assert!(review.working());
    assert!(!review.can_import(), "still parsing");
    review.pause_play();
    assert!(review.can_import(), "paused");
    assert!(!review.work(Duration::ZERO), "paused, nothing is parsed");
    review.pause_play();
    review.cancel();
    assert!(!review.working());
    assert_eq!(review.progress(), ("1 files parsed.".to_owned(), 1, 1));
    assert_eq!(review.good_paths().len(), 1);

    // removing rows numbers the rest again
    review.add_paths([
        folder.join("b p12.jpg").to_string_lossy().into_owned(),
        folder.join("dupe.gif").to_string_lossy().into_owned(),
    ]);
    parse_all(&mut review);
    review.remove(&std::collections::HashSet::from([0]));
    let rows: Vec<[String; 4]> = review.parsed().iter().map(Parsed::row).collect();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0][0].as_str(), rows[1][0].as_str()), ("1", "2"));
    assert!(rows[0][1].ends_with("b p12.jpg"));
}
