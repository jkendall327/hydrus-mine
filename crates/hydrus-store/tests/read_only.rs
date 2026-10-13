//! A store opened read-only reads, and cannot write.
use hydrus_store::Store;
use hydrus_store::synth::{SynthSpec, populate};

#[test]
fn a_read_only_store_reads_and_refuses_writes() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("store");
    let store = Store::open(&dir).unwrap();
    populate(
        &store,
        SynthSpec {
            files: 50,
            tags: 20,
            tags_per_file: 3,
            siblings: 2,
            parents: 2,
            batch: 50,
            ..SynthSpec::default()
        },
    )
    .unwrap();
    drop(store);

    let store = Store::open_read_only(&dir).unwrap();
    let files: i64 = store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(files, 50);
    assert!(store.write(|_| Ok(())).is_err(), "no writer exists");
    // (and SQLite itself refuses, past any code of ours)
    assert!(
        store
            .read(|c| Ok(c.execute("DELETE FROM files", [])?))
            .is_err()
    );
    let files: i64 = store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(files, 50);
}

#[test]
fn a_read_only_open_creates_nothing_and_a_missing_store_is_an_error() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("nowhere");
    assert!(Store::open_read_only(&dir).is_err());
    assert!(!dir.exists());
}
