//! `hydrus bench` on a small synthetic store: it reports every measurement,
//! and it changes nothing in the store (the database file keeps its bytes,
//! size and modification time, and no file other than SQLite's own
//! write-ahead-log companions appears).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use hydrus_store::Store;
use hydrus_store::synth::{SynthSpec, populate};

const HYDRUS: &str = env!("CARGO_BIN_EXE_hydrus");
const STUB_THUMBNAIL: &[u8] = include_bytes!("../../../static/hydrus.png");

/// A synthetic library with a thumbnail for every file, closed.
fn synthetic_store() -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("store");
    let store = Store::open(&dir).unwrap();
    let library = populate(
        &store,
        SynthSpec {
            files: 400,
            tags: 120,
            tags_per_file: 6,
            siblings: 10,
            parents: 10,
            batch: 200,
            ..SynthSpec::default()
        },
    )
    .unwrap();
    let snapshot = store.snapshot();
    for (_, hash) in &library.hashes {
        let path = snapshot.storage.thumbnail_path(hash).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, STUB_THUMBNAIL).unwrap();
    }
    drop(snapshot);
    drop(store);
    (parent, dir)
}

/// Every database file under `dir`: its size and bytes. (Modification times
/// are not compared: SQLite may touch the `-wal` and `-shm` files.)
fn state(dir: &Path) -> BTreeMap<PathBuf, (u64, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (u64, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "db") {
                out.insert(path.clone(), (meta.len(), std::fs::read(&path).unwrap()));
            } else if !is_wal_companion(&path) {
                out.insert(path, (meta.len(), Vec::new()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    out
}

fn is_wal_companion(path: &Path) -> bool {
    path.to_string_lossy().ends_with("-wal") || path.to_string_lossy().ends_with("-shm")
}

#[test]
fn bench_reports_every_measurement_and_does_not_change_the_store() {
    let (_parent, dir) = synthetic_store();
    let before = state(&dir);
    let out = Command::new(HYDRUS)
        .arg("bench")
        .arg(&dir)
        .args(["--runs", "3", "--thumbnails", "20", "--json"])
        .args(["--search", "system:everything; system:limit=5"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["store"]["files"], 400);
    assert_eq!(report["runs"], 3);
    let timings = report["timings"].as_array().unwrap();
    let names: Vec<&str> = timings
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for wanted in [
        "store open (read-only)",
        "startup queries",
        "a single tag",
        "two tags",
        "a negated tag",
        "system:inbox",
        "system:limit",
        "your search: system:everything; system:limit=5",
        "autocomplete for",
        "load basic metadata of 400 files",
        "load full metadata of 400 files",
        "read and decode 20 thumbnails from disk",
        "load the sibling and parent graphs",
        "look up siblings and parents of",
        "count potential duplicate pairs",
    ] {
        assert!(
            names.iter().any(|n| n.contains(wanted)),
            "no timing for {wanted:?} in {names:#?}"
        );
    }
    for t in timings {
        assert_eq!(t["runs_ms"].as_array().unwrap().len(), 3);
        assert!(t["median_ms"].as_f64().unwrap() <= t["p90_ms"].as_f64().unwrap());
    }
    let find = |name: &str| {
        timings
            .iter()
            .find(|t| t["name"].as_str().unwrap().contains(name))
            .unwrap()
    };
    // (the stub thumbnails really are decoded)
    assert!(
        find("read and decode 20")["note"]
            .as_str()
            .unwrap()
            .starts_with("20 decoded, 0 missing")
    );
    assert_eq!(find("your search")["note"], "5 files");
    // (a limit above the file count finds them all)
    assert_eq!(find("system:limit [")["note"], "400 files");

    // the plain-text report is the same measurements
    let text = Command::new(HYDRUS)
        .arg("bench")
        .arg(&dir)
        .args(["--runs", "2"])
        .output()
        .unwrap();
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("hydrus bench\nversion: "), "{text}");
    assert!(text.contains("store: 400 files"), "{text}");
    assert!(
        text.contains("[searches]") && text.contains("[duplicates]"),
        "{text}"
    );

    // and the store is as it was: the same files, the database byte for byte
    assert_eq!(before, state(&dir), "the store changed");
}

#[test]
fn bench_refuses_a_directory_that_is_not_a_store_and_makes_nothing() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("nowhere");
    let out = Command::new(HYDRUS)
        .arg("bench")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a hydrus-rs store"));
    assert!(!dir.exists());
}
