//! Undo > undo/redo of content changes (the reference's `UndoManager` and
//! `ContentUpdatePackage.ToString`).
use hydrus_core::HashId;
use hydrus_gui_model::main_menu::Facts;
use hydrus_gui_model::media_actions;
use hydrus_store::Store;

fn basic() -> (tempfile::TempDir, tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (legacy, dir, store)
}

fn inbox_files(store: &Store) -> Vec<HashId> {
    store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM file_inbox ORDER BY hash_id LIMIT 2")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<HashId>>>()?)
        })
        .unwrap()
}

fn in_inbox(store: &Store, file: HashId) -> bool {
    store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM file_inbox WHERE hash_id = ?",
                [file],
                |r| r.get::<_, i64>(0),
            )? == 1)
        })
        .unwrap()
}

#[test]
fn archiving_can_be_undone_and_redone_from_the_menu() {
    let (_legacy, _dir, store) = basic();
    let files = inbox_files(&store);
    assert_eq!(files.len(), 2);
    let facts = Facts::from_store(&store).unwrap();
    assert_eq!((facts.undo, facts.redo), (None, None));
    media_actions::archive(&store, &files).unwrap();
    assert!(!in_inbox(&store, files[0]));
    let facts = Facts::from_store(&store).unwrap();
    assert_eq!(facts.undo.as_deref(), Some("undo archive 2 files"));
    assert_eq!(facts.redo, None);
    assert!(store.undo().unwrap());
    assert!(in_inbox(&store, files[0]) && in_inbox(&store, files[1]));
    let facts = Facts::from_store(&store).unwrap();
    assert_eq!(
        (facts.undo, facts.redo.as_deref()),
        (None, Some("redo archive 2 files"))
    );
    assert!(store.redo().unwrap());
    assert!(!in_inbox(&store, files[1]));
    assert!(!store.redo().unwrap());
}

#[test]
fn tag_changes_name_their_service() {
    use hydrus_store::undo::{Change, MappingChange, Package};
    let (_legacy, _dir, store) = basic();
    let files = inbox_files(&store);
    let snapshot = store.snapshot();
    let service = snapshot
        .services
        .tag_services()
        .find(|s| s.name == "my tags")
        .unwrap()
        .id;
    let tag = store
        .write(|ctx| {
            hydrus_store::master::intern_tag(
                ctx.conn(),
                &hydrus_core::Tag::new("undo test").unwrap(),
            )
        })
        .unwrap();
    let package = Package(vec![Change::Mappings {
        service,
        change: MappingChange::Add,
        tag,
        files: files.clone(),
    }]);
    assert_eq!(
        package.describe(&snapshot.services),
        "my tags->add tags for 2 files"
    );
    store.write_undoable(package).unwrap();
    let facts = Facts::from_store(&store).unwrap();
    assert_eq!(
        facts.undo.as_deref(),
        Some("undo my tags->add tags for 2 files")
    );
}
