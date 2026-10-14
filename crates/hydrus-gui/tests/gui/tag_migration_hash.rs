//! The tag migration window's hash controls: every source/destination hash-kind
//! pair of the reference's archive conversions (`oracle/fixtures/tag_archives.json`),
//! launched from the window as a user would and compared with what the reference
//! wrote.
use hydrus_core::{HashId, ServiceKey, Sha256, TagId};
use hydrus_gui::{headless, tag_migration_window};
use hydrus_store::{Store, services};
use slint::Model as _;
use std::sync::Arc;

fn archive_hashes(store: &Store, recording: &serde_json::Value) -> Vec<HashId> {
    let known = recording["known_sha256"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse::<Sha256>().unwrap())
        .collect::<Vec<_>>();
    let digests = (0..known.len())
        .map(|i| {
            ["md5", "sha1", "sha512"].map(|kind| {
                let archive = recording["archives"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["kind"] == kind)
                    .unwrap();
                let row = archive["mappings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| {
                        row["tags"]
                            .as_array()
                            .unwrap()
                            .contains(&serde_json::json!(format!("archive:known-{i}")))
                    })
                    .unwrap();
                hex::decode(row["hash"].as_str().unwrap()).unwrap()
            })
        })
        .collect::<Vec<_>>();
    store
        .write(move |ctx| {
            known
                .iter()
                .zip(digests)
                .map(|(sha, [md5, sha1, sha512])| {
                    let id = hydrus_store::master::intern_hash(ctx.conn(), sha)?;
                    ctx.conn().execute(
                        "INSERT OR REPLACE INTO hash_digests(hash_id,md5,sha1,sha512) VALUES(?1,?2,?3,?4)",
                        rusqlite::params![id, md5, sha1, sha512],
                    )?;
                    Ok(id)
                })
                .collect()
        })
        .unwrap()
}

fn destination_rows(store: &Store, key: &ServiceKey) -> serde_json::Value {
    let service = store.snapshot().services.by_key(key).unwrap().id;
    let table = hydrus_store::schema::MappingTables::new(service).current;
    let rows = store
        .read(|conn| {
            let ids = conn
                .prepare(&format!("SELECT tag_id,hash_id FROM {table}"))?
                .query_map([], |r| Ok((r.get::<_, TagId>(0)?, r.get::<_, HashId>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut result = std::collections::BTreeMap::<String, Vec<String>>::new();
            for (tag, hash) in ids {
                result
                    .entry(hydrus_store::master::hash(conn, hash)?.unwrap().to_hex())
                    .or_default()
                    .push(
                        hydrus_store::master::tag(conn, tag)?
                            .unwrap()
                            .as_str()
                            .to_owned(),
                    );
            }
            Ok(result
                .into_iter()
                .map(|(hash, mut tags)| {
                    tags.sort();
                    serde_json::json!({"hash": hash, "tags": tags})
                })
                .collect::<Vec<_>>())
        })
        .unwrap();
    serde_json::json!(rows)
}

fn archive_rows(path: &std::path::Path) -> serde_json::Value {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut grouped = std::collections::BTreeMap::<String, Vec<String>>::new();
    for row in conn
        .prepare("SELECT hash,tag FROM mappings JOIN hashes USING(hash_id) JOIN tags USING(tag_id)")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
    {
        let (hash, tag) = row.unwrap();
        grouped.entry(hex::encode(hash)).or_default().push(tag);
    }
    serde_json::json!(
        grouped
            .into_iter()
            .map(|(hash, mut tags)| {
                tags.sort();
                serde_json::json!({"hash": hash, "tags": tags})
            })
            .collect::<Vec<_>>()
    )
}

fn settle(window: &hydrus_gui::TagMigrationWindow) {
    while !window.get_question().is_empty() {
        window.invoke_answer(true);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while window.get_running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_running());
}

// leaf: migration-hash
// leaf: migration-location
#[test]
fn the_window_converts_between_every_hash_kind_as_the_reference_did() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    let _windows = headless::init();
    let kinds = ["sha256", "md5", "sha1", "sha512"];
    for case in recording["conversion"].as_array().unwrap() {
        let label = format!(
            "{} -> {} selected {} domain {}",
            case["kind"], case["desired"], case["selected"], case["domain"]
        );
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<Store> = Store::open(dir.path()).unwrap();
        let source = store
            .snapshot()
            .services
            .by_name("my tags")
            .unwrap()
            .key
            .clone();
        let destination = ServiceKey::new(vec![128; 16]);
        let key = destination.clone();
        store
            .write_and_refresh(move |ctx| {
                services::insert(
                    ctx.conn(),
                    &key,
                    "destination",
                    &services::ServiceKind::LocalTags,
                )?;
                Ok(())
            })
            .unwrap();
        let hashes = archive_hashes(&store, &recording);
        let selected = case["selected"].as_bool().unwrap();
        if let Some(domain) = case.get("domain") {
            let service_id = store.snapshot().services.by_name("my files").unwrap().id;
            let membership = recording["domain_membership"].as_array().unwrap().clone();
            store
                .write(move |ctx| {
                    for row in membership {
                        let sha = row["hash"].as_str().unwrap().parse::<Sha256>().unwrap();
                        let hash = hydrus_store::master::hash_id(ctx.conn(), &sha)?.unwrap();
                        for (field, table) in [
                            ("current", "file_domain_current"),
                            ("deleted", "file_domain_deleted"),
                        ] {
                            if row[field].as_bool().unwrap() {
                                ctx.conn().execute(
                                    &format!("INSERT OR IGNORE INTO {table}(service_id,hash_id) VALUES(?1,?2)"),
                                    rusqlite::params![service_id, hash],
                                )?;
                            }
                        }
                    }
                    Ok(())
                })
                .unwrap();
            let _ = domain;
        }
        let slot = tag_migration_window::Slot::default();
        let files = if selected {
            hashes[..1].to_vec()
        } else {
            vec![]
        };
        let window =
            tag_migration_window::open(&store, &source, files, &slot, std::rc::Rc::new(|| {}))
                .unwrap();
        let archive = i32::try_from(window.get_services().row_count() - 1).unwrap();
        window.set_content(0);
        window.invoke_choices_changed();
        window.set_source(archive);
        let desired = case["desired"].as_str().unwrap();
        let output = dir.path().join("converted.db");
        if desired == "sha256" {
            let at = window
                .get_services()
                .iter()
                .position(|s| s == "destination")
                .unwrap();
            window.set_destination(i32::try_from(at).unwrap());
        } else {
            window.set_destination(archive);
        }
        window.invoke_choices_changed();
        let path = hydrus_testkit::fixture_path(format!(
            "tag_archive_{}.db",
            case["kind"].as_str().unwrap()
        ));
        let before = std::fs::read(&path).unwrap();
        window.invoke_archive_path_chosen(true, path.to_string_lossy().as_ref().into());
        assert_eq!(
            window.get_source_hash(),
            format!("hash type: {}", case["kind"].as_str().unwrap()),
            "{label}"
        );
        if desired != "sha256" {
            window.invoke_archive_path_chosen(false, output.to_string_lossy().as_ref().into());
            assert!(!window.get_hash_locked(), "{label}");
            window.set_hash_kind(
                i32::try_from(kinds.iter().position(|k| *k == desired).unwrap()).unwrap(),
            );
            window.invoke_choices_changed();
        }
        let add = window
            .get_actions()
            .iter()
            .position(|a| a == "add")
            .unwrap();
        window.set_action(i32::try_from(add).unwrap());
        window.invoke_choices_changed();
        assert!(
            window.get_error().is_empty(),
            "{label}: {}",
            window.get_error()
        );

        // The scope: the selected files, or the file domain the picker chose (the
        // reference's runs used all known files unless a domain was being tested).
        if !selected {
            window.invoke_edit_location();
            let picker = hydrus_gui::locations_window::last_opened().unwrap();
            let ticks: Vec<_> = picker.get_ticks().iter().collect();
            for (i, tick) in ticks.iter().enumerate() {
                if tick.checked {
                    picker.invoke_toggled(i32::try_from(i).unwrap(), false);
                }
            }
            let domain = case.get("domain").and_then(|d| d.as_str());
            let wanted = ticks
                .iter()
                .position(|t| match domain {
                    None => t.label == "all known files",
                    Some(domain) => {
                        t.label.contains("my files")
                            && t.label.contains("deleted") == (domain == "deleted")
                    }
                })
                .unwrap_or_else(|| {
                    panic!(
                        "the domain in the picker: {:?}",
                        ticks
                            .iter()
                            .map(|t| t.label.to_string())
                            .collect::<Vec<_>>()
                    )
                });
            picker.invoke_toggled(i32::try_from(wanted).unwrap(), true);
            picker.invoke_apply();
        }
        // "creator:" is left out of what is migrated, as in the recording.
        window.invoke_edit_filter(false);
        let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
        filter.invoke_typed(1, "creator:".into());
        filter.invoke_apply();

        window.invoke_go();
        settle(&window);
        assert!(
            window.get_error().is_empty(),
            "{label}: {}",
            window.get_error()
        );
        let got = if desired == "sha256" {
            destination_rows(&store, &destination)
        } else {
            archive_rows(&output)
        };
        assert_eq!(got, case["mappings"], "{label}");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "{label}: the source archive is read-only"
        );
        window.invoke_close_clicked();
    }
}
