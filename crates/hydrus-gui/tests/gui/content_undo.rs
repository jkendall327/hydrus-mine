//! The Undo menu's undo and redo of content writes, replayed step by step
//! against `oracle/fixtures/content_undo.json` (`record_content_undo.py`):
//! the reference writes archive, inbox and tag changes through
//! `controller.Write( 'content_updates', ... )`, then chooses the menu's undo
//! and redo entries; its `GetUndoRedoStrings` and the files' inbox and tag
//! state are recorded after every step.

use std::collections::BTreeSet;
use std::sync::Arc;

use slint::Model as _;

use hydrus_core::HashId;
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::import::import_legacy;
use hydrus_store::schema::MappingTables;
use hydrus_store::undo::{Change, MappingChange, Package};
use hydrus_store::{Store, sessions};

fn client() -> ([tempfile::TempDir; 2], Arc<Store>, MainWindow) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "one".into(),
            content: PageContent::Search {
                search: FileSearchContext::default(),
                synchronised: false,
                sort: None,
                lock: None,
                collect: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    // (kept alive by the window's callbacks)
    std::mem::forget(bind(&ui, Pages::open(store.clone()).unwrap()));
    ([legacy, native], store, ui)
}

/// The Undo menu's lines: "undo …" and "redo …" first, as `(label, line)`.
fn undo_redo_lines(ui: &MainWindow) -> Vec<(String, i32)> {
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "undo")
        .unwrap() as i32;
    ui.invoke_menu_title_pressed(at, 80.0, 22.0);
    let panes = ui.get_menu_panes();
    let mut out = Vec::new();
    if panes.row_count() > 0 {
        let lines = panes.row_data(0).unwrap().lines;
        for i in 0..lines.row_count() {
            let label = lines.row_data(i).unwrap().label.to_string();
            if label.starts_with("undo ") || label.starts_with("redo ") {
                out.push((label, i as i32));
            }
        }
    }
    out
}

/// What the menu offers: the undo and the redo text, where there are any.
fn menu_strings(ui: &MainWindow) -> [Option<String>; 2] {
    let lines = undo_redo_lines(ui);
    ui.invoke_menu_dismissed();
    let pick = |prefix: &str| {
        lines
            .iter()
            .find(|(l, _)| l.starts_with(prefix))
            .map(|(l, _)| l.clone())
    };
    [pick("undo "), pick("redo ")]
}

fn choose_in_undo_menu(ui: &MainWindow, label: &str) {
    let lines = undo_redo_lines(ui);
    let (_, line) = lines
        .iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("{label} in {lines:?}"));
    ui.invoke_menu_line_clicked(0, *line, 0.0, 0.0, 0.0);
}

/// A label with its actions in a fixed order: the reference joins them from
/// a Python set, in no fixed order ("archive, add" and "add, archive" are
/// both its text), so only their set is compared.
fn unordered(label: Option<&String>) -> Option<String> {
    let label = label?;
    let (verb, rest) = label.split_once(' ')?;
    let (place, rest) = rest.split_once("->").map_or(("", rest), |(p, r)| (p, r));
    let (rest, files) = rest.strip_suffix(" files")?.rsplit_once(' ')?;
    let (words, tags) = rest
        .strip_suffix(" tags for")
        .map_or((rest, ""), |w| (w, " tags for"));
    let mut words: Vec<_> = words.split(", ").collect();
    words.sort_unstable();
    Some(format!(
        "{verb} {place}->{}{tags} {files} files",
        words.join(", ")
    ))
}

fn recorded_strings(value: &serde_json::Value) -> [Option<String>; 2] {
    let strings = value["menu"]["strings"].as_array().unwrap();
    [0, 1].map(|i| strings[i].as_str().map(str::to_owned))
}

// leaf: audit-options-undo-manager-undo-last-content-operation
// leaf: audit-options-undo-manager-redo-last-content-operation
#[test]
fn the_undo_menu_undoes_and_redoes_content_writes_as_the_references_manager_does() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("content_undo.json");
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let (_dirs, store, ui) = client();

    let hashes: Vec<HashId> = fixture["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let hex = manifest["files"][i.as_u64().unwrap() as usize]["hash"]
                .as_str()
                .unwrap();
            let hash = hydrus_core::Sha256::from_slice(&hex::decode(hex).unwrap()).unwrap();
            store
                .read(move |c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect();
    let service = store
        .snapshot()
        .services
        .tag_services()
        .find(|s| s.name == fixture["tag"].as_str().unwrap())
        .unwrap()
        .id;
    let tables = MappingTables::new(service);
    let tag_id = |text: &str| {
        let tag = hydrus_core::Tag::new(text).unwrap();
        store
            .write(move |ctx| hydrus_store::master::intern_tag(ctx.conn(), &tag))
            .unwrap()
    };
    let pick = |indices: &serde_json::Value| -> Vec<HashId> {
        indices
            .as_array()
            .unwrap()
            .iter()
            .map(|i| hashes[i.as_u64().unwrap() as usize])
            .collect()
    };
    let file_state = |store: &Store| -> Vec<(bool, BTreeSet<String>)> {
        hashes
            .iter()
            .map(|&hash| {
                store
                    .read(|conn| {
                        let inbox: i64 = conn.query_row(
                            "SELECT count(*) FROM file_inbox WHERE hash_id = ?",
                            [hash],
                            |r| r.get(0),
                        )?;
                        let mut stmt = conn.prepare(&format!(
                            "SELECT n.namespace || ':' || s.subtag FROM {} m
                             JOIN tags t USING (tag_id)
                             JOIN namespaces n USING (namespace_id)
                             JOIN subtags s USING (subtag_id)
                             WHERE m.hash_id = ? AND n.namespace = 'oracle'",
                            tables.current
                        ))?;
                        let tags = stmt
                            .query_map([hash], |r| r.get::<_, String>(0))?
                            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
                        Ok((inbox == 1, tags))
                    })
                    .unwrap()
            })
            .collect()
    };

    let mut before: [Option<String>; 2] = [None, None];
    for (n, step) in fixture["steps"].as_array().unwrap().iter().enumerate() {
        let what = step["step"].as_array().unwrap();
        let kind = what[0].as_str().unwrap();
        match kind {
            "undo" | "redo" => {
                // (the reference's Undo and Redo do nothing when there is nothing
                // to do, and its menu offers no such item)
                let index = usize::from(kind == "redo");
                if let Some(label) = &before[index] {
                    choose_in_undo_menu(&ui, label);
                } else {
                    assert!(
                        !undo_redo_lines(&ui)
                            .iter()
                            .any(|(l, _)| l.starts_with(kind)),
                        "step {n}: no {kind} offered"
                    );
                    ui.invoke_menu_dismissed();
                }
            }
            "archive" => store
                .write_undoable(Package(vec![Change::Archive(pick(&what[1]))]))
                .unwrap(),
            "inbox" => store
                .write_undoable(Package(vec![Change::Inbox(pick(&what[1]))]))
                .unwrap(),
            "add_tag" | "delete_tag" => {
                let change = if kind == "add_tag" {
                    MappingChange::Add
                } else {
                    MappingChange::Delete
                };
                store
                    .write_undoable(Package(vec![Change::Mappings {
                        service,
                        change,
                        tag: tag_id(what[1].as_str().unwrap()),
                        files: pick(&what[2]),
                    }]))
                    .unwrap();
            }
            "archive_and_tag" | "tag_and_archive" => {
                // (the reference's package keeps a service's changes together,
                // in the order the services were added)
                let files = pick(&what[2]);
                let archive = Change::Archive(files.clone());
                let tags = Change::Mappings {
                    service,
                    change: MappingChange::Add,
                    tag: tag_id(what[1].as_str().unwrap()),
                    files,
                };
                let changes = if kind == "archive_and_tag" {
                    vec![archive, tags]
                } else {
                    vec![tags, archive]
                };
                store.write_undoable(Package(changes)).unwrap();
            }
            other => panic!("unknown recorded step {other}"),
        }

        let shown = menu_strings(&ui);
        assert_eq!(
            [unordered(shown[0].as_ref()), unordered(shown[1].as_ref())],
            recorded_strings(step).map(|l| unordered(l.as_ref())),
            "step {n} {what:?}: the menu's undo and redo entries"
        );
        let want: Vec<(bool, BTreeSet<String>)> = step["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["inbox"].as_bool().unwrap(),
                    f["tags"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| t.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect();
        assert_eq!(file_state(&store), want, "step {n} {what:?}: the files");
        before = shown;
    }
}
