//! Export folders against the reference (`oracle/record_export_folder.py`):
//! the reference's database with three folders set up is migrated, the
//! folders are made as they were, and each does its work once. Every
//! folder's contents afterwards, the files the client still has and each
//! folder's saved state must be the reference's.

use std::collections::BTreeMap;
use std::path::Path;

use hydrus_core::ServiceType;
use hydrus_parse::folders::ExportFolder;
use hydrus_store::Store;
use hydrus_store::settings::ExportFolders;

/// The folders as the reference's run found them.
fn place(work: &Path) {
    for name in ["regular", "sync", "delete"] {
        std::fs::create_dir_all(work.join(name)).unwrap();
    }
    std::fs::create_dir_all(work.join("regular").join("metroid")).unwrap();
    std::fs::write(
        work.join("regular").join("metroid").join("EXISTING.json"),
        "{}",
    )
    .unwrap();
    std::fs::create_dir_all(work.join("sync").join("empty").join("emptier")).unwrap();
    for name in ["stale.png", "stale.png.txt", "other.bin"] {
        std::fs::write(work.join("sync").join(name), "leftover").unwrap();
    }
}

/// A folder's contents: sizes, sidecar text, which file a link points at,
/// and empty directories.
fn listing(folder: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk(folder, folder, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let kind = entry.file_type().unwrap();
        let ext = path.extension().and_then(|e| e.to_str());
        if kind.is_dir() {
            if std::fs::read_dir(&path).unwrap().next().is_none() {
                out.insert(
                    format!("{rel}{}", std::path::MAIN_SEPARATOR),
                    "\"empty dir\"".into(),
                );
            }
            walk(root, &path, out);
        } else if kind.is_symlink() {
            let target = std::fs::read_link(&path).unwrap();
            let name = target.file_name().unwrap().to_string_lossy();
            out.insert(rel, format!("\"link to {name}\""));
        } else if ext == Some("txt") || ext == Some("json") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.insert(rel, serde_json::Value::String(text).to_string());
        } else {
            out.insert(rel, entry.metadata().unwrap().len().to_string());
        }
    }
}

#[test]
fn export_folders_do_what_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("export_folder_run.json");
    let legacy = hydrus_testkit::legacy_fixture("export_folder");
    let dir = tempfile::tempdir().unwrap();
    let report = hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    assert!(
        !report.warnings.iter().any(|w| w.contains("xport folder")),
        "{:?}",
        report.warnings
    );
    let store = Store::open(dir.path()).unwrap();

    let work = tempfile::Builder::new()
        .prefix("export_folder")
        .tempdir()
        .unwrap();
    place(work.path());
    let recorded_work = recorded["work"].as_str().unwrap();
    let ours = work.path().to_string_lossy().into_owned();
    let mut folders: ExportFolders = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(folders.0.len(), 3);
    for f in &mut folders.0 {
        assert!(f.path.starts_with(recorded_work), "{}", f.path);
        f.path = f.path.replacen(recorded_work, &ours, 1);
    }
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &folders))
        .unwrap();

    let runs = hydrus_download::export::work_export_folders(&store).unwrap();
    assert_eq!(runs.len(), 3, "{runs:?}");
    for (name, run) in &runs {
        assert_eq!(run.error, None, "{name}: {run:?}");
    }

    let mut problems = Vec::new();
    for (name, theirs) in recorded["folders"].as_object().unwrap() {
        let theirs: BTreeMap<String, String> = theirs
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.replace('/', std::path::MAIN_SEPARATOR_STR), v.to_string()))
            .collect();
        let mine = listing(&work.path().join(name));
        if mine != theirs {
            problems.push(format!("{name}:\n  ours   {mine:#?}\n  theirs {theirs:#?}"));
        }
    }

    // the files the client still has
    let snapshot = store.snapshot();
    let combined = snapshot
        .services
        .of_type(ServiceType::CombinedLocalFileDomains)
        .next()
        .unwrap()
        .id;
    let mut current: Vec<String> = store
        .read(|conn| {
            let ids: Vec<hydrus_core::HashId> = conn
                .prepare("SELECT hash_id FROM file_domain_current WHERE service_id = ?")?
                .query_map([combined], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(hydrus_store::master::hashes(conn, &ids)?
                .values()
                .map(hydrus_core::Sha256::to_hex)
                .collect())
        })
        .unwrap();
    current.sort();
    let theirs: Vec<String> = serde_json::from_value(recorded["current"].clone()).unwrap();
    if current != theirs {
        problems.push(format!("files kept: ours {current:?}, theirs {theirs:?}"));
    }

    // what each folder saved: run regularly, run now, last error, popup,
    // overwrite next run, always overwrite
    let folders: ExportFolders = store.read(hydrus_store::settings::get).unwrap();
    for f in &folders.0 {
        let stored = &recorded["stored"][&f.name][3];
        let theirs = &stored.as_array().unwrap()[10..];
        let mine = serde_json::json!([
            f.run_now,
            f.last_error,
            f.show_working_popup,
            f.overwrite_sidecars_on_next_run,
            f.always_overwrite_sidecars
        ]);
        if mine.as_array().unwrap()[..] != theirs[..] {
            problems.push(format!("{} state: ours {mine}, theirs {theirs:?}", f.name));
        }
        let ExportFolder { run_regularly, .. } = f;
        assert_eq!(Some(*run_regularly), stored[6].as_bool(), "{}", f.name);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    // a second run (made due) changes nothing
    let before: Vec<_> = ["regular", "sync", "delete"]
        .iter()
        .map(|n| listing(&work.path().join(n)))
        .collect();
    let mut folders: ExportFolders = store.read(hydrus_store::settings::get).unwrap();
    for f in &mut folders.0 {
        f.run_now = true;
    }
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &folders))
        .unwrap();
    let again = hydrus_download::export::work_export_folders(&store).unwrap();
    assert!(
        again
            .iter()
            .all(|(_, r)| r.copied == 0 && r.deleted_paths == 0),
        "{again:?}"
    );
    let after: Vec<_> = ["regular", "sync", "delete"]
        .iter()
        .map(|n| listing(&work.path().join(n)))
        .collect();
    assert_eq!(before, after);
}
