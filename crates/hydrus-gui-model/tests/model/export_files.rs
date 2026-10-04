//! Manual export previews and questions against the real Qt panel; worker
//! failures, cancellation, links and sidecars preserve client source data.
use hydrus_core::HashId;
use hydrus_core::url::strings::{StringConverter, StringProcessor};
use hydrus_gui_model::export_files::{self, Plan};
use hydrus_parse::sidecar::{Exporter, Importer, Router, SidecarNaming, Source};
use hydrus_store::{Store, import::import_legacy};
use serde_json::Value;
use sha2::Digest;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn setup() -> ([tempfile::TempDir; 2], Arc<Store>, Vec<HashId>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let recorded: Value = hydrus_testkit::fixture_json("export_files.json");
    let files = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| HashId(u32::try_from(f["file_id"].as_u64().unwrap()).unwrap()))
        .collect();
    ([legacy, native], store, files)
}
fn naming() -> SidecarNaming {
    SidecarNaming {
        remove_actual_filename_ext: false,
        suffix: String::new(),
        filename_converter: StringConverter::default(),
    }
}
fn plan(store: &Store, files: &[HashId], directory: &std::path::Path, phrase: &str) -> Plan {
    Plan {
        directory: directory.to_owned(),
        rows: export_files::preview(store, files, directory.to_str().unwrap(), phrase).unwrap(),
        routers: Vec::new(),
        trash: false,
        symlinks: false,
    }
}
fn source(store: &Store, file: HashId) -> std::path::PathBuf {
    let snapshot = store.snapshot();
    let media = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap()
        .results
        .remove(0);
    snapshot
        .storage
        .file_path(&media.hash, media.info.unwrap().mime)
        .unwrap()
}
fn current(store: &Store, file: HashId) -> bool {
    let snapshot = store.snapshot();
    let roles = hydrus_store::content::DomainRoles::new(&snapshot.services).unwrap();
    store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap()
        .results[0]
        .is_current_in(roles.combined_local_media)
}

#[test]
fn preview_and_confirmations_match_reference_panel() {
    let (_dirs, store, files) = setup();
    let recorded: Value = hydrus_testkit::fixture_json("export_files.json");
    for case in recorded["previews"].as_array().unwrap() {
        let phrase = case["phrase"].as_str().unwrap();
        if phrase == "../escape" || phrase == "{bad" {
            assert!(
                export_files::preview(&store, &files, "/tmp/hydrus-manual-export-oracle", phrase)
                    .is_err()
            );
            continue;
        }
        let rows =
            export_files::preview(&store, &files, "/tmp/hydrus-manual-export-oracle", phrase)
                .unwrap();
        for (row, theirs) in rows.iter().zip(case["rows"].as_array().unwrap()) {
            assert_eq!(row.number.to_string(), theirs[0]);
            assert_eq!(row.mime, theirs[1]);
            let expected = std::path::absolute(theirs[2].as_str().unwrap()).unwrap();
            assert_eq!(row.destination, expected);
        }
    }
    let work = tempfile::tempdir().unwrap();
    let mut p = plan(&store, &files, work.path(), "same");
    p.trash = true;
    assert_eq!(p.confirmation(false).unwrap(), recorded["questions"][0]);
    assert_eq!(p.confirmation(true).unwrap(), recorded["questions"][1]);
    p.trash = false;
    assert_eq!(p.confirmation(true).unwrap(), recorded["questions"][2]);
    assert_eq!(p.confirmation(false), None);
    assert_eq!(export_files::REMOVE_QUESTION, recorded["remove_question"]);
    let rows = export_files::preview(
        &store,
        &[files[0], files[1]],
        work.path().to_str().unwrap(),
        "nested/same",
    )
    .unwrap();
    assert_eq!(rows[0].number, 1);
    assert_eq!(rows[1].number, 2);
    assert!(
        rows.iter()
            .all(|r| r.destination.parent().unwrap() == work.path().join("nested"))
    );
}

#[test]
fn copies_overwrite_existing_files_and_route_metadata() {
    let (_dirs, store, files) = setup();
    let work = tempfile::tempdir().unwrap();
    for &file in &files {
        store
            .write_content(move |w| w.set_note(file, "exported", "metadata"))
            .unwrap();
    }
    let oracle_plan = plan(&store, &files, work.path(), "same");
    std::fs::write(&oracle_plan.rows[0].destination, b"previous destination").unwrap();
    let oracle_progress = export_files::run(&store, &oracle_plan, &AtomicBool::new(false), |_| {});
    assert_eq!(oracle_progress.error, None);
    let recorded: Value = hydrus_testkit::fixture_json("export_files.json");
    for (name, digest) in recorded["copies"].as_object().unwrap() {
        let bytes = std::fs::read(work.path().join(name)).unwrap();
        assert_eq!(
            hex::encode(sha2::Sha256::digest(bytes)),
            digest.as_str().unwrap()
        );
    }
    let mut p = plan(&store, &files, work.path(), "nested/{#}");
    p.routers.push(Router {
        importers: vec![Importer {
            source: Source::MediaNotes,
            processor: StringProcessor::default(),
        }],
        processor: StringProcessor::default(),
        exporter: Exporter::Json {
            naming: naming(),
            nested_object_names: vec!["notes".into()],
        },
    });
    let first = &p.rows[0].destination;
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    std::fs::write(first, b"old destination").unwrap();
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(progress.completed, files.len());
    assert!(progress.finished);
    assert!(!progress.cancelled);
    for row in &p.rows {
        assert_eq!(
            std::fs::read(&row.destination).unwrap(),
            std::fs::read(source(&store, row.file)).unwrap()
        );
        let sidecar = naming().path(row.destination.to_str().unwrap(), "json");
        let contents: Value = serde_json::from_slice(&std::fs::read(sidecar).unwrap()).unwrap();
        assert!(
            contents["notes"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("exported: metadata"))
        );
        assert!(current(&store, row.file));
    }
}

#[test]
fn cancellation_and_missing_sources_never_trash_partial_exports() {
    let (_dirs, store, files) = setup();
    let work = tempfile::tempdir().unwrap();
    let mut p = plan(&store, &files, work.path(), "{#}");
    p.trash = true;
    let cancel = AtomicBool::new(false);
    let progress = export_files::run(&store, &p, &cancel, |progress| {
        if progress.completed == 1 {
            cancel.store(true, Ordering::Release);
        }
    });
    assert_eq!(progress.completed, 1);
    assert!(progress.cancelled);
    assert!(progress.finished);
    assert_eq!(progress.trashed, 0);
    assert!(files.iter().all(|&f| current(&store, f)));
    let mut last = plan(&store, &files[..1], work.path(), "last");
    last.trash = true;
    cancel.store(false, Ordering::Release);
    let progress = export_files::run(&store, &last, &cancel, |p| {
        if p.completed == p.total {
            cancel.store(true, Ordering::Release);
        }
    });
    assert_eq!(progress.completed, 1);
    assert!(progress.cancelled);
    assert_eq!(progress.trashed, 0);
    assert!(current(&store, files[0]));
    std::fs::remove_file(source(&store, files[1])).unwrap();
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.completed, 1);
    assert!(progress.error.unwrap().contains("actually missing"));
    assert_eq!(progress.trashed, 0);
    assert!(files.iter().all(|&f| current(&store, f)));
}

#[test]
fn sidecar_failure_blocks_trash_even_after_copy_and_success_trashes() {
    let (_dirs, store, files) = setup();
    let work = tempfile::tempdir().unwrap();
    let mut p = plan(&store, &files[..1], work.path(), "{#}");
    p.trash = true;
    p.symlinks = true; // Trashing forces real copies even with stale link state.
    // This file has a note, so an invalid sidecar destination is exercised.
    store
        .write_content({
            let f = files[0];
            move |w| w.set_note(f, "test", "note")
        })
        .unwrap();
    let naming = naming();
    let sidecar = naming.path(p.rows[0].destination.to_str().unwrap(), "txt");
    std::fs::create_dir_all(&sidecar).unwrap();
    p.routers.push(Router {
        importers: vec![Importer {
            source: Source::MediaNotes,
            processor: StringProcessor::default(),
        }],
        processor: StringProcessor::default(),
        exporter: Exporter::Txt {
            naming,
            separator: "\n".into(),
        },
    });
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert!(progress.error.is_some());
    assert_eq!(progress.completed, 0);
    assert_eq!(progress.trashed, 0);
    assert!(current(&store, files[0]));
    assert!(source(&store, files[0]).is_file());
    std::fs::remove_dir(sidecar).unwrap();
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(progress.trashed, 1);
    assert!(
        !p.rows[0]
            .destination
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!current(&store, files[0]));
    assert!(source(&store, files[0]).is_file());
}

#[cfg(unix)]
#[test]
fn symlinks_existing_links_and_hardlinks_preserve_source_content() {
    let (_dirs, store, files) = setup();
    let work = tempfile::tempdir().unwrap();
    let mut p = plan(&store, &files[..1], work.path(), "same");
    p.symlinks = true;
    let original = source(&store, files[0]);
    let bytes = std::fs::read(&original).unwrap();
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(
        std::fs::read_link(&p.rows[0].destination).unwrap(),
        original
    );
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert!(progress.error.is_some());
    std::fs::remove_file(&p.rows[0].destination).unwrap();
    std::fs::hard_link(&original, &p.rows[0].destination).unwrap();
    p.symlinks = false;
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    // A pre-existing destination may alias a different client source.
    let other = source(&store, files[1]);
    let other_bytes = std::fs::read(&other).unwrap();
    std::fs::remove_file(&p.rows[0].destination).unwrap();
    std::fs::hard_link(&other, &p.rows[0].destination).unwrap();
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(std::fs::read(&other).unwrap(), other_bytes);
    assert_eq!(std::fs::read(&p.rows[0].destination).unwrap(), bytes);
    store
        .write_content({
            let f = files[0];
            move |w| w.set_note(f, "test", "metadata")
        })
        .unwrap();
    let sidecar = naming().path(p.rows[0].destination.to_str().unwrap(), "txt");
    std::fs::hard_link(&original, &sidecar).unwrap();
    p.routers.push(Router {
        importers: vec![Importer {
            source: Source::MediaNotes,
            processor: StringProcessor::default(),
        }],
        processor: StringProcessor::default(),
        exporter: Exporter::Txt {
            naming: naming(),
            separator: "\n".into(),
        },
    });
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(progress.error, None);
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(sidecar).unwrap(), "test: metadata");
    assert!(
        std::fs::read_dir(work.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp"))
    );
    let escaping = work.path().join("escape");
    std::os::unix::fs::symlink(original.parent().unwrap(), &escaping).unwrap();
    let p = plan(&store, &files[..1], work.path(), "escape/copy");
    let progress = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert!(progress.error.unwrap().contains("outside"));
}
