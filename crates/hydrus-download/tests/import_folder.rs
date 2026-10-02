//! An import folder against the reference (`oracle/record_import_folder.py`):
//! the reference's database with the folder set up is migrated, the same
//! files are put in a folder, and the folder does its work once. The files
//! left behind and moved, the seeds kept, and each imported file's tags,
//! URLs, notes and modified time must be the reference's.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use hydrus_core::{ContentStatus, ServiceKey, Sha256};
use hydrus_download::Downloader;
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_parse::folders::FolderAction;
use hydrus_store::import_folders;
use hydrus_store::{Store, master, queues};

const OLD_MTIME: i64 = 1_600_000_000;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/fixtures")
}

/// Copy the folder's files in, with settled times but for the recent file.
fn place(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            place(&entry.path(), &target);
            continue;
        }
        std::fs::copy(entry.path(), &target).unwrap();
        let time = if entry.file_name() == "recent.png" {
            std::time::SystemTime::now()
        } else {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(OLD_MTIME as u64)
        };
        std::fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }
}

fn listing(root: &Path) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, entry.metadata().unwrap().len());
            }
        }
    }
    out
}

fn json_map(value: &serde_json::Value) -> BTreeMap<String, u64> {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn an_import_folder_does_what_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("import_folder_run.json");
    let legacy = hydrus_testkit::legacy_fixture("import_folder");
    let dir = tempfile::tempdir().unwrap();
    let report = hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    assert!(
        !report.warnings.iter().any(|w| w.contains("mport folder")),
        "{:?}",
        report.warnings
    );
    let store = Arc::new(Store::open(dir.path()).unwrap());

    // the same files, in a folder of our own
    let work = tempfile::Builder::new()
        .prefix("import_folder")
        .tempdir()
        .unwrap();
    let folder_dir = work.path().join("in");
    let moved_dir = work.path().join("moved");
    place(&fixtures().join("import_folder"), &folder_dir);
    std::fs::create_dir_all(&moved_dir).unwrap();
    let mut folder = store
        .read(|conn| import_folders::find_import_folder(conn, "drop box"))
        .unwrap()
        .expect("the folder came across");
    assert_eq!(folder.settings.path, recorded["folder"].as_str().unwrap());
    assert_eq!(
        folder.settings.actions.successful_and_new,
        FolderAction::Move(recorded["moved"].as_str().unwrap().to_owned())
    );
    folder.settings.path = folder_dir.to_string_lossy().into_owned();
    folder.settings.actions.successful_and_new =
        FolderAction::Move(moved_dir.to_string_lossy().into_owned());
    let (id, settings) = (folder.id(), folder.settings.clone());
    store
        .write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))
        .unwrap();
    let global: hydrus_store::settings::FolderSettings =
        store.read(hydrus_store::settings::get).unwrap();
    assert!(!global.delete_to_recycle_bin, "the option came across");

    let net = Arc::new(
        NetEngine::new(
            Arc::clone(&store),
            NetOptions {
                obey_bandwidth: false,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let downloader = Downloader::new(Arc::clone(&store), net, importer).unwrap();
    let run = downloader.work_on_import_folder(id).unwrap();
    assert!(run.checked, "{run:?}");
    assert_eq!(run.error, None, "{run:?}");
    assert!(run.warnings.is_empty(), "{run:?}");

    let problems = std::cell::RefCell::new(Vec::new());
    let check = |what: &str, ours: String, theirs: String| {
        if ours != theirs {
            problems
                .borrow_mut()
                .push(format!("{what}:\n  ours   {ours}\n  theirs {theirs}"));
        }
    };
    check(
        "folder afterwards",
        format!("{:?}", listing(&folder_dir)),
        format!("{:?}", json_map(&recorded["in_after"])),
    );
    check(
        "moved afterwards",
        format!("{:?}", listing(&moved_dir)),
        format!("{:?}", json_map(&recorded["moved_after"])),
    );
    let seeds: Vec<(String, i64, String, Option<String>)> = store
        .read(|conn| queues::file_seeds(conn, id))
        .unwrap()
        .into_iter()
        .map(|s| {
            let rel = Path::new(&s.data)
                .strip_prefix(&folder_dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (
                rel,
                s.status.code(),
                s.note.clone(),
                s.meta.hash("sha256").map(str::to_owned),
            )
        })
        .collect();
    let theirs: Vec<(String, i64, String, Option<String>)> =
        serde_json::from_value(recorded["seeds"].clone()).unwrap();
    check("seeds", format!("{seeds:?}"), format!("{theirs:?}"));

    // each file's metadata
    let snapshot = store.snapshot();
    for m in recorded["metadata"].as_array().unwrap() {
        let hex = m["hash"].as_str().unwrap();
        let hash = Sha256::from_slice(&hex::decode(hex).unwrap()).unwrap();
        let Some(hash_id) = store.read(|conn| master::hash_id(conn, &hash)).unwrap() else {
            problems
                .borrow_mut()
                .push(format!("{hex} was not imported"));
            continue;
        };
        let batch = store
            .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[hash_id]))
            .unwrap();
        let ours = &batch.results[0];
        for (key, service) in m["tags"].as_object().unwrap() {
            let key = ServiceKey::from_hex(key).unwrap();
            let Ok(service_id) = snapshot.services.by_key(&key).map(|s| s.id) else {
                continue;
            };
            if snapshot.services.get(service_id).unwrap().service_type()
                == hydrus_core::ServiceType::CombinedTag
            {
                continue;
            }
            let theirs: Vec<String> = service["storage_tags"]
                .get("0")
                .map(|v| serde_json::from_value(v.clone()).unwrap())
                .unwrap_or_default();
            let ids = ours
                .tags
                .get(&service_id)
                .and_then(|t| t.by_status.get(&ContentStatus::Current))
                .cloned()
                .unwrap_or_default();
            let names = store.read(|conn| master::tags(conn, &ids)).unwrap();
            let mut mine: Vec<String> = names.values().map(|t| t.as_str().to_owned()).collect();
            mine.sort();
            let mut theirs = theirs;
            theirs.sort();
            check(
                &format!("{hex} tags"),
                format!("{mine:?}"),
                format!("{theirs:?}"),
            );
        }
        let mut urls = ours.urls.clone();
        urls.sort();
        let mut their_urls: Vec<String> = serde_json::from_value(m["known_urls"].clone()).unwrap();
        their_urls.sort();
        check(
            &format!("{hex} urls"),
            format!("{urls:?}"),
            format!("{their_urls:?}"),
        );
        let notes: BTreeMap<String, String> = ours.notes.iter().cloned().collect();
        let their_notes: BTreeMap<String, String> =
            serde_json::from_value(m["notes"].clone()).unwrap_or_default();
        check(
            &format!("{hex} notes"),
            format!("{notes:?}"),
            format!("{their_notes:?}"),
        );
        let modified = ours
            .info
            .as_ref()
            .and_then(|i| i.file_modified)
            .map(|t| t.0 / 1000);
        check(
            &format!("{hex} modified"),
            format!("{modified:?}"),
            format!("{:?}", m["time_modified"].as_i64()),
        );
    }
    let problems = problems.into_inner();
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    // the files it imported are offered in a popup, under its name
    let (their_folder, our_folder) = (
        recorded["folder"].as_str().unwrap(),
        folder_dir.to_string_lossy().into_owned(),
    );
    assert_eq!(
        common::popups_shown(&store),
        common::recorded_shown(&recorded["popups"], their_folder, &our_folder)
    );

    // nothing more to do: a second check finds nothing new
    let mut folder = store
        .read(|conn| import_folders::import_folder(conn, id))
        .unwrap()
        .unwrap();
    folder.settings.check_now = true;
    let settings = folder.settings.clone();
    store
        .write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))
        .unwrap();
    let again = downloader.work_on_import_folder(id).unwrap();
    assert_eq!((again.new_files, again.imported), (0, 0), "{again:?}");

    // its folder gone, checked now: it pauses, and says why
    store
        .write(|ctx| hydrus_store::popups::dismiss_all_done(ctx.conn(), i64::MAX / 2))
        .unwrap();
    let mut folder = store
        .read(|conn| import_folders::import_folder(conn, id))
        .unwrap()
        .unwrap();
    folder.settings.path.push_str(" (gone)");
    folder.settings.check_now = true;
    let settings = folder.settings.clone();
    store
        .write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))
        .unwrap();
    let broken = downloader.work_on_import_folder(id).unwrap();
    assert!(broken.error.is_some(), "{broken:?}");
    assert_eq!(
        common::popups_shown(&store),
        common::recorded_shown(&recorded["broken_popups"], their_folder, &our_folder)
    );
}
