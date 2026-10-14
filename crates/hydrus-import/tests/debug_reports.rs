//! Help > debug > report modes > file import report mode, file report mode:
//! an import says what it is doing, in the reference's words
//! (`FileImportJob`, `ClientDB._ImportFile`, `ClientFilesManager`), compared
//! with what the running reference said for the same file
//! (`oracle/record_debug_report_modes.py`, the Client API's `add_file` of
//! `oracle/fixtures/debug_report_modes.png`: first when it is new, then when
//! it is already in the database).
//!
//! The switches are process-wide, so the tests here take one lock.
use std::path::Path;
use std::sync::{Arc, Mutex};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use serde_json::Value;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// What the reference said, in order, for the first import and the second.
fn recorded() -> (Vec<String>, Vec<String>) {
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let texts = |run: &Value| {
        run["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(recording["import"]["off"]["messages"], serde_json::json!([]));
    (
        texts(&recording["import"]["on"]),
        texts(&recording["import"]["again"]),
    )
}

/// The reference's line as hydrus-rs words it: a Python repr (a bytes literal,
/// a tuple of ints, a list of strings) is shaped as Rust shapes the same
/// values, and the run's own paths and clock are named, not compared.
fn shaped(reference: &str, hash_hex: &str) -> String {
    let unquoted = |s: &str| s.replace('\'', "");
    if let Some(info) = reference.strip_prefix("File import job file info: (") {
        // (size, mime code, width, height, duration, frames, has audio, words)
        let fields: Vec<String> = info
            .trim_end_matches(')')
            .split(", ")
            .enumerate()
            .map(|(i, field)| match (i, field) {
                (1, code) => hydrus_core::Mime::from_code(code.parse().unwrap())
                    .unwrap()
                    .human_name()
                    .to_owned(),
                (_, "False") => "false".to_owned(),
                (_, "True") => "true".to_owned(),
                (_, other) => other.to_owned(),
            })
            .collect();
        return format!("File import job file info: ({})", fields.join(", "));
    }
    if reference.starts_with("File import job generated ") {
        return unquoted(reference);
    }
    if reference.starts_with("File path request: (") {
        // (hash, mime): the reference's bytes literal is the file's hash
        return format!("File path request: ({hash_hex}, png)");
    }
    if reference.starts_with("Adding thumbnail: (") {
        return unquoted(reference);
    }
    reference.to_owned()
}

/// Names what differs from run to run in a line, in either side's words.
fn plain(line: &str, store_dir: &Path, import_path: &Path) -> String {
    let mut line = line
        .replace(&store_dir.display().to_string(), "<db>")
        .replace(&import_path.display().to_string(), "<temp>");
    // (the importer works on a copy of the file in the store's scratch folder)
    line = line
        .split(' ')
        .map(|part| {
            if part.starts_with("<db>/tmp/") {
                "<temp>"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if line.starts_with("File import job created:") {
        line = line
            .lines()
            .map(|l| {
                if l.starts_with("Source: ") {
                    "Source: <source>"
                } else if l.starts_with("Raw import path: ") {
                    "Raw import path: <temp>."
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let (Some(at), Some(end)) = (
        line.find("Imported at "),
        line.find(" before this check."),
    ) {
        line.replace_range(at..end, "Imported at <time>, which was <ago>");
    }
    if let Some(rest) = line.strip_prefix("Adding thumbnail: (")
        && let Some((_, path)) = rest.split_once(", ")
    {
        // (the thumbnail's size in bytes follows from the encoder)
        line = format!("Adding thumbnail: (<bytes>, {path}");
    }
    line
}

struct Run {
    first: Vec<String>,
    again: Vec<String>,
    recorded_first: Vec<String>,
    recorded_again: Vec<String>,
}

/// Imports the recorded file into a fresh store twice with `flags` on, and
/// pairs what was said with what the reference said, both made plain.
fn import_twice(flags: &[Flag], keep: impl Fn(&str) -> bool) -> Run {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned());
    })));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let path = hydrus_testkit::fixture_path("debug_report_modes.png");
    let (first, again) = recorded();
    let hash_hex = first
        .iter()
        .find_map(|l| l.strip_prefix("File import job hash: "))
        .unwrap()
        .to_owned();

    // off: nothing
    let other = tempfile::tempdir().unwrap();
    let quiet = FileImporter::new(Store::open(other.path()).unwrap(), MediaTools::new());
    quiet
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    assert!(seen.lock().unwrap().is_empty(), "silent while the mode is off");

    for flag in flags {
        flag.set(true);
    }
    let result = importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    assert_eq!(result.hash.unwrap().to_hex(), hash_hex);
    let first_said = std::mem::take(&mut *seen.lock().unwrap());
    importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    let again_said = std::mem::take(&mut *seen.lock().unwrap());
    for flag in flags {
        flag.set(false);
    }
    debug_flags::set_sink(None);

    let store_dir = store.dir().to_path_buf();
    let said = |lines: Vec<String>| {
        lines
            .iter()
            .filter(|l| keep(l))
            .map(|l| plain(l, &store_dir, &path))
            .collect::<Vec<_>>()
    };
    let reference = |lines: Vec<String>| {
        lines
            .iter()
            .filter(|l| keep(l))
            .map(|l| plain(&shaped(l, &hash_hex), &store_dir, &path))
            .collect::<Vec<_>>()
    };
    Run {
        first: said(first_said),
        again: said(again_said),
        recorded_first: reference(first),
        recorded_again: reference(again),
    }
}

// leaf: audit-options-help-debug-action-file-import-report-mode
#[test]
fn file_import_report_mode_says_what_the_reference_says_through_an_import() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // (the reference's phash and thumbnail-path lines belong to other modes)
    let run = import_twice(&[Flag::FileImportReport], |line| {
        line.starts_with("File import job ") || line.starts_with("Adding file to client file")
    });
    assert_eq!(run.first, run.recorded_first);
    assert_eq!(run.again, run.recorded_again);
    // (the whole job: created to done, the db job in the middle)
    assert_eq!(run.first.len(), 20, "{:?}", run.first);
    assert_eq!(run.again.len(), 5, "{:?}", run.again);
}

// leaf: audit-options-help-debug-action-file-report-mode
#[test]
fn file_report_mode_says_what_the_reference_says_through_an_import() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let run = import_twice(&[Flag::FileReport], |line| {
        line.starts_with("Adding ")
            || line.starts_with("File path request")
            || line.starts_with("Thumbnail path request")
    });
    assert_eq!(run.first, run.recorded_first);
    assert_eq!(run.again, run.recorded_again);
    assert!(!run.first.is_empty() && !run.again.is_empty());
}
