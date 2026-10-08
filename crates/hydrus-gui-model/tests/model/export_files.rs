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

// leaf: audit-network-export-preview
#[test]
fn preview_and_confirmations_match_reference_panel() {
    let (_dirs, store, files) = setup();
    let recorded: Value = hydrus_testkit::fixture_json("export_files.json");
    for case in recorded["previews"].as_array().unwrap() {
        let phrase = case["phrase"].as_str().unwrap();
        if phrase == "../escape" && cfg!(windows) {
            // ntpath.split accepts the slash; Windows sanitization turns the
            // '..' directory into 'empty' before joining the destination.
            let root = export_files::directory_path("/tmp/hydrus-manual-export-oracle").unwrap();
            let rows =
                export_files::preview(&store, &files, root.to_str().unwrap(), phrase).unwrap();
            assert_eq!(rows.len(), files.len());
            for (row, filename) in rows
                .iter()
                .zip(["escape.png", "escape (1).png", "escape.flac"])
            {
                assert_eq!(row.destination, root.join("empty").join(filename));
            }
            continue;
        }
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
    for (row, filename) in rows.iter().zip(["same.png", "same (1).png"]) {
        assert_eq!(row.destination.file_name().unwrap(), filename);
    }
    // Repeated separators are stripped by os.path.split, including '/' on
    // Windows. Earlier '/' inside subdirectories is sanitized there instead.
    for (phrase, parent) in [
        ("nested//same", work.path().join("nested")),
        (
            "nested/deep/same",
            if cfg!(windows) {
                work.path().join("nested_deep")
            } else {
                work.path().join("nested").join("deep")
            },
        ),
    ] {
        let rows = export_files::preview(
            &store,
            &[files[0], files[1]],
            work.path().to_str().unwrap(),
            phrase,
        )
        .unwrap();
        for (row, filename) in rows.iter().zip(["same.png", "same (1).png"]) {
            assert_eq!(row.destination, parent.join(filename));
        }
    }
}

// leaf: audit-network-export-worker
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
    let phrase = format!("nested{}{{#}}", std::path::MAIN_SEPARATOR);
    let mut p = plan(&store, &files, work.path(), &phrase);
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

// leaf: audit-network-export-worker
#[test]
fn cancellation_suppresses_trash_but_missing_sources_trash_successful_prefix() {
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
    assert_eq!(progress.trashed, 1);
    assert!(!current(&store, files[0]));
    assert!(files[1..].iter().all(|&f| current(&store, f)));
}

// leaf: audit-network-export-worker
#[test]
fn export_failure_and_cancellation_match_reference_durable_membership() {
    let recorded = hydrus_testkit::fixture_json("export_failure_prefix.json");
    for case in recorded.as_array().unwrap() {
        let (_dirs, store, files) = setup();
        let work = tempfile::tempdir().unwrap();
        let mut p = plan(&store, &files, work.path(), "{#}");
        p.trash = true;
        let name = case["case"].as_str().unwrap();
        store
            .write(|tx| {
                hydrus_store::settings::set(
                    tx.conn(),
                    &hydrus_store::delete_lock::DeleteLock {
                        archived: true,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        if matches!(name, "fail_first" | "fail_second") {
            let index = usize::from(name == "fail_second");
            std::fs::remove_file(source(&store, files[index])).unwrap();
        }
        if name == "sidecar_second" {
            for &file in &files {
                store
                    .write_content(move |w| w.set_note(file, "test", "recorded note"))
                    .unwrap();
            }
            let naming = naming();
            std::fs::create_dir(naming.path(p.rows[1].destination.to_str().unwrap(), "txt"))
                .unwrap();
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
        }
        let cancel = AtomicBool::new(false);
        let progress = export_files::run(&store, &p, &cancel, |progress| {
            if (name == "cancel_after_first" && progress.completed == 1)
                || (name == "cancel_after_last" && progress.completed == files.len())
            {
                cancel.store(true, Ordering::Release);
            }
        });
        assert_eq!(
            progress.completed,
            case["copied"].as_array().unwrap().len(),
            "{name}"
        );
        assert_eq!(
            progress.cancelled,
            case["cancelled"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(progress.finished, case["done"].as_bool().unwrap(), "{name}");
        assert_eq!(
            usize::from(progress.error.is_some()),
            usize::try_from(case["error_count"].as_u64().unwrap()).unwrap(),
            "{name}"
        );
        let expected = case["durable_trashed"].as_array().unwrap();
        assert_eq!(
            progress.trashed,
            expected.iter().filter(|v| v.as_bool().unwrap()).count()
        );
        // Reopen an independent Store so this asserts committed state rather
        // than a GUI cache or the worker's optimistic completion count.
        let reopened = Store::open(store.dir()).unwrap();
        for (&file, trashed) in files.iter().zip(expected) {
            assert_eq!(
                !current(&reopened, file),
                trashed.as_bool().unwrap(),
                "{name}: {file:?}"
            );
        }
        for filename in case["copied"].as_array().unwrap() {
            assert!(work.path().join(filename.as_str().unwrap()).is_file());
        }
    }
}

// leaf: audit-network-export-worker
#[test]
fn sidecar_failure_prevents_copy_and_trash_and_success_trashes() {
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
    assert!(!p.rows[0].destination.exists());
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

// leaf: audit-network-export-worker
#[test]
fn later_trash_transaction_failure_retains_export_error_and_committed_prefix() {
    let (_dirs, store, original) = setup();
    let template = original[0];
    let bytes = std::fs::read(source(&store, template)).unwrap();
    let local = store.snapshot().services.by_name("my files").unwrap().id;
    let files = store.write_content(move |writer| {
        let mut files = Vec::new();
        for i in 1..=66 {
            let hash: hydrus_core::Sha256 = format!("{i:064x}").parse().unwrap();
            let id = hydrus_store::master::intern_hash(writer.conn(), &hash)?;
            writer.conn().execute("INSERT INTO files(hash_id,size,mime) SELECT ?1,size,mime FROM files WHERE hash_id=?2", rusqlite::params![id, template])?;
            files.push(id);
        }
        let rows: Vec<_> = files.iter().map(|&file| (file, Some(1_700_000_000_000))).collect();
        writer.add_files(local, &rows)?;
        Ok(files)
    }).unwrap();
    for &file in &files[..65] {
        let path = source(&store, file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &bytes).unwrap();
    }
    let failure = files[64];
    store.write(move |tx| {
        tx.conn().execute_batch(&format!("CREATE TRIGGER fail_second_export_trash BEFORE DELETE ON file_domain_current WHEN OLD.hash_id={} AND OLD.service_id={} BEGIN SELECT RAISE(ABORT,'scripted later trash failure'); END;", failure.get(), local.get()))?;
        Ok(())
    }).unwrap();
    let work = tempfile::tempdir().unwrap();
    let mut p = plan(&store, &files, work.path(), "{#}");
    p.trash = true;
    let result = export_files::run(&store, &p, &AtomicBool::new(false), |_| {});
    assert_eq!(result.completed, 65);
    assert_eq!(result.trashed, 64);
    assert!(result.finished);
    let error = result.error.unwrap();
    assert!(error.contains("export file #66"));
    assert!(error.contains("actually missing"));
    assert!(error.contains("scripted later trash failure"));
    let reopened = Store::open(store.dir()).unwrap();
    assert!(files[..64].iter().all(|&file| !current(&reopened, file)));
    assert!(files[64..].iter().all(|&file| current(&reopened, file)));
    assert_eq!(std::fs::read_dir(work.path()).unwrap().count(), 65);
}

#[cfg(unix)]
// leaf: audit-network-export-worker
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

// leaf: audit-network-export-folder-examples
#[test]
fn shared_pattern_menu_copies_recorded_phrases_including_its_heading() {
    let reference = hydrus_testkit::fixture_json("export_pattern_shortcuts.json");
    let labels = reference["menu"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| !row["separator"].as_bool().unwrap())
        .map(|row| row["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels[0], export_files::PATTERN_SHORTCUT_HEADING);
    assert_eq!(
        labels[1..],
        export_files::PATTERN_SHORTCUTS.map(|(label, _)| label)
    );
    let copied = std::iter::once(7)
        .chain(0..7)
        .map(|index| export_files::pattern_shortcut(index).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(serde_json::to_value(copied).unwrap(), reference["copied"]);
    assert!(
        reference["menu"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["enabled"] == true)
    );
    assert_eq!(export_files::pattern_shortcut(-1), None);
    assert_eq!(export_files::pattern_shortcut(8), None);
    assert_eq!(export_files::pattern_shortcut(i32::MAX), None);
}

fn seed_export_sidebar(store: &Store, recorded: &Value) {
    use hydrus_core::Tag;
    use hydrus_store::content::MappingAction;
    let service = store
        .snapshot()
        .services
        .by_name(recorded["seed_service"].as_str().unwrap())
        .unwrap()
        .id;
    let file = HashId(u32::try_from(recorded["seed_hash_id"].as_u64().unwrap()).unwrap());
    store
        .write_content(move |writer| {
            let pending = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("parity:pending example").unwrap(),
            )?;
            let petitioned = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("parity:petitioned example").unwrap(),
            )?;
            writer.update_mappings(service, &MappingAction::Pend, pending, &[file])?;
            writer.update_mappings(service, &MappingAction::Add, petitioned, &[file])?;
            writer.update_mappings(
                service,
                &MappingAction::Petition {
                    reason: "synthetic export preview".into(),
                },
                petitioned,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn selected_export_tags_counts_sort_selection_and_copy_match_actual_panel() {
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
    use hydrus_gui_model::write_tag_menu::{Action, Entry};
    let (_dirs, store, files) = setup();
    let recorded = hydrus_testkit::fixture_json("export_selected_tags.json");
    seed_export_sidebar(&store, &recorded);
    let mut tags = export_files::tags::Tags::new(store.clone()).unwrap();
    let default_sort = tags.sort;
    for case in recorded["states"].as_array().unwrap() {
        let case_name = case["case"].as_str().unwrap();
        tags.sort = TagSort {
            sort_type: [TagSortType::Tag, TagSortType::Subtag, TagSortType::Count]
                [usize::try_from(case["sort"][0].as_u64().unwrap()).unwrap()],
            ascending: case["sort"][1] == 0,
            group_by: [
                TagGroupBy::Nothing,
                TagGroupBy::NamespaceAz,
                TagGroupBy::NamespaceUser,
            ][usize::try_from(case["sort"][2].as_u64().unwrap()).unwrap()],
        };
        let mut selected_files: Vec<_> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| HashId(u32::try_from(id.as_u64().unwrap()).unwrap()))
            .collect();
        if selected_files.is_empty() {
            selected_files = if case_name == "remove_selected_file_fallback" {
                vec![files[0], files[2]]
            } else {
                files.clone()
            };
        }
        tags.refresh(&selected_files).unwrap();
        let expected: Vec<_> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["rendered"].as_str().unwrap())
            .collect();
        assert_eq!(
            tags.rows()
                .iter()
                .map(|row| row.text.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{case_name}"
        );
        let selected: Vec<_> = tags
            .rows()
            .iter()
            .filter(|row| {
                case["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["tag"] == row.tag && r["selected"] == true)
            })
            .map(|row| row.id)
            .collect();
        tags.selection.select_many(&selected);
    }
    tags.sort = TagSort {
        sort_type: TagSortType::Count,
        ascending: true,
        group_by: TagGroupBy::NamespaceAz,
    };
    tags.refresh(&files[..2]).unwrap();
    for menu in recorded["menus"].as_array().unwrap() {
        let selected: Vec<_> = tags
            .rows()
            .iter()
            .filter(|row| {
                menu["selected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s == &row.tag)
            })
            .map(|row| row.id)
            .collect();
        tags.selection.select_many(&selected);
        let Entry::Menu(label, copies) = tags.menu().remove(0) else {
            panic!("copy submenu");
        };
        assert_eq!(label, "copy");
        let copies: Vec<_> = copies
            .iter()
            .filter_map(|entry| {
                if let Entry::Item(label, Action::Copy(text)) = entry {
                    Some((label.as_str(), text.as_str()))
                } else {
                    None
                }
            })
            .collect();
        let expected: Vec<_> = menu["copy"]
            .as_array()
            .unwrap()
            .iter()
            .map(|copy| {
                (
                    copy["label"].as_str().unwrap(),
                    copy["payload"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(copies, expected);
    }
    assert_eq!(
        tags.copy(export_files::tags::CopyOptions::default()),
        recorded["keyboard_copy"]
    );
    assert_eq!(recorded["double_click_activated"], false);
    assert_eq!(recorded["double_click_copied"], false);
    assert!(
        matches!(tags.launch(true), Some(Action::Launch { predicates, duplicate: false, .. }) if matches!(predicates.as_slice(), [hydrus_core::search::predicate::Predicate::Or(children)] if children.len() == 2))
    );
    // Changing media preserves surviving tag identities and forgets vanished ones.
    let source = tags
        .rows()
        .iter()
        .find(|row| row.tag == "source:fixture")
        .unwrap()
        .id;
    let link = tags
        .rows()
        .iter()
        .find(|row| row.tag == "character:link")
        .unwrap()
        .id;
    tags.selection.select_many(&[source, link]);
    // The recorded first-file view retains source:fixture but has no link.
    // The second file contains link, so selecting it would retain both tags.
    tags.refresh(&files[..1]).unwrap();
    assert!(tags.selection.is_selected(source));
    assert!(!tags.selection.is_selected(link));
    // Each order control remembers its own choice, without changing defaults.
    tags.sort_chosen(0, 0);
    tags.sort_chosen(1, 1);
    tags.sort_chosen(0, 2);
    tags.sort_chosen(1, 1);
    tags.sort_chosen(0, 1);
    assert!(!tags.sort.ascending);
    tags.sort_chosen(0, 2);
    assert!(tags.sort.ascending);
    let sort = tags.sort;
    tags.sort_chosen(1, 99);
    assert_eq!(tags.sort, sort);
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<hydrus_core::tag_presentation::TagPresentation>)
            .unwrap()
            .search_page_sort,
        default_sort
    );
}
