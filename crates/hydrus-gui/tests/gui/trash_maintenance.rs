//! The trash limits of Options > files and trash against the reference's own
//! trash maintenance (`DAEMONMaintainTrash`), replaying
//! `oracle/fixtures/trash_maintenance.json` (`oracle/record_trash_maintenance.py`):
//! each scenario trashes the recorded files (the `basic` fixture's, and
//! generated BMPs of the recorded exact sizes) at the recorded times before
//! "now", sets the limits through the real Options window, lets the real
//! maintenance runtime run its trash pass, and compares what is left in the
//! trash with what the reference left.

use std::sync::Arc;

use hydrus_core::{HashId, Sha256};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::maintenance_gates::Worker;
use hydrus_store::trash::TrashSettings;
use serde_json::Value;

use super::normal_time_maintenance::wait;
use super::options_system_consumers::{Client, client, noneable, store};

/// The files in the trash, by hash id.
fn in_trash(client: &Client) -> Vec<HashId> {
    let trash = client
        .store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::TRASH)
        .unwrap()
        .id;
    client
        .store
        .read(move |c| {
            let mut q = c.prepare(
                "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 ORDER BY hash_id",
            )?;
            Ok(q.query_map([trash], |r| r.get(0))?
                .collect::<Result<_, _>>()?)
        })
        .unwrap()
}

/// What the native trash pass leaves of `scenario`'s files, by name.
fn replay(client: &mut Client, scenario: &Value) -> Vec<String> {
    let name = scenario["name"].as_str().unwrap();
    // a fresh `basic` store, bound to the window, its trash emptied
    let (dirs, fresh) = store();
    client.bound = hydrus_gui::bind(&client.ui, hydrus_gui::Pages::open(fresh.clone()).unwrap());
    client.use_store(dirs, fresh);
    let store = client.store.clone();
    let already = in_trash(client);
    store
        .write_content(move |w| {
            let storage = w.roles().local_file_storage;
            w.delete_files(storage, &already, None)
        })
        .unwrap();
    // the recorded files: the generated ones imported, sizes as recorded
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let mut files: Vec<(String, HashId, i64)> = Vec::new();
    for file in scenario["trash"].as_array().unwrap() {
        let file_name = file["name"].as_str().unwrap();
        let size = file["size"].as_u64().unwrap() as usize;
        if let Some(seed) = file_name.strip_prefix("bmp") {
            importer
                .import_bytes(
                    &hydrus_testkit::bmp(seed.parse().unwrap(), size),
                    &FileImportOptions::default(),
                )
                .unwrap();
        }
        let hash: Sha256 = file["hash"].as_str().unwrap().parse().unwrap();
        let id = store
            .read(move |c| hydrus_store::master::hash_id(c, &hash))
            .unwrap()
            .unwrap_or_else(|| panic!("{name}: {file_name} imported, as in the recording"));
        let stored: i64 = store
            .read(move |c| {
                Ok(
                    c.query_row("SELECT size FROM files WHERE hash_id = ?1", [id], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(stored as usize, size, "{name}: {file_name}");
        files.push((file_name.to_owned(), id, file["ago_ms"].as_i64().unwrap()));
    }
    let ids: Vec<HashId> = files.iter().map(|f| f.1).collect();
    store
        .write_content(move |w| {
            let media = w.roles().combined_local_media;
            w.delete_files(media, &ids, None)
        })
        .unwrap();
    assert_eq!(in_trash(client).len(), files.len(), "{name}");

    // the limits, through File > options
    let window = client.options("files and trash");
    let age = scenario["max_age_hours"].as_i64().map(|n| n as i32);
    let size = scenario["max_size_mb"].as_i64().map(|n| n as i32);
    noneable(
        &window,
        "Number of hours a file will stay in the trash before being deleted: ",
        "no age limit",
        age,
    );
    noneable(
        &window,
        "Maximum size of trash (MB): ",
        "no size limit",
        size,
    );
    window.invoke_apply();
    assert_eq!(
        client.get::<TrashSettings>(),
        TrashSettings {
            max_age_hours: age.map(|n| n as u64),
            max_size_mb: size.map(|n| n as u64),
        }
    );

    // The recording's "now" is half a second into a second. Start early in
    // a second, call half a second into it "now", trash the files at their
    // times before that, and run the pass before the second is out: the
    // whole-second cutoff then falls where the reference's did.
    let pass = || {
        while hydrus_core::TimestampMs::now().0 % 1000 >= 100 {
            std::thread::yield_now();
        }
        let now = hydrus_core::TimestampMs::now().0 / 1000 * 1000 + 500;
        let times: Vec<(HashId, i64)> = files.iter().map(|f| (f.1, now - f.2)).collect();
        store
            .write_content(move |w| {
                let trash = w.roles().trash;
                for (id, at) in &times {
                    w.conn().execute(
                        "UPDATE file_domain_current SET added_ms = ?1 WHERE service_id = ?2 AND hash_id = ?3",
                        rusqlite::params![at, trash, id],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let passes = client.bound.maintenance.statistics().trash_passes;
        let due = client.bound.maintenance.deadline(Worker::Trash);
        client.bound.maintenance.poll_at(due).unwrap();
        wait(|| {
            client.bound.maintenance.poll_at(due).unwrap();
            client.bound.maintenance.statistics().trash_passes > passes
        });
        assert!(
            hydrus_core::TimestampMs::now().0 < now + 500,
            "{name}: the pass ran within the second"
        );
    };
    pass();
    assert_eq!(client.bound.maintenance.statistics().last_error, None);
    let left = in_trash(client);
    let mut names: Vec<String> = files
        .iter()
        .filter(|f| left.contains(&f.1))
        .map(|f| f.0.clone())
        .collect();
    names.sort();
    names
}

/// The reference's groups for `scenario`, in order.
fn groups(scenario: &Value) -> Vec<usize> {
    scenario["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_array().unwrap().len())
        .collect()
}

// leaf: audit-options-files-and-trash-number-of-hours-a-file-will-stay-in-the-trash-before-being-deleted
// leaf: audit-options-files-and-trash-maximum-size-of-trash-mb
#[test]
fn trash_limits_delete_what_the_reference_deletes() {
    let recorded = hydrus_testkit::fixture_json("trash_maintenance.json");
    let scenarios = recorded["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 9);
    let mut client = client();
    // the rows are the reference's
    let window = client.options("files and trash");
    let (_, shown) = super::options_system_consumers::row(
        &window,
        "Number of hours a file will stay in the trash before being deleted: ",
    );
    assert_eq!(
        (shown.kind, shown.number, shown.minimum, shown.maximum),
        (3, 72, 0, 8640)
    );
    let (_, shown) = super::options_system_consumers::row(&window, "Maximum size of trash (MB): ");
    assert_eq!(
        (shown.kind, shown.number, shown.minimum, shown.maximum),
        (3, 2048, 0, 20480)
    );
    window.invoke_cancel();

    for scenario in scenarios {
        let name = scenario["name"].as_str().unwrap();
        let left = replay(&mut client, scenario);
        let deleted: usize = groups(scenario).iter().sum();
        assert_eq!(
            left.len() + deleted,
            scenario["trash"].as_array().unwrap().len(),
            "{name}"
        );
        let recorded_left: Vec<String> = scenario["left"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect();
        match name {
            // The reference means to delete the oldest first (its comment
            // says so) but turns the oldest 256 into a set of ids before
            // deleting, so it deletes as many, in eights, but by file id.
            // hydrus-rs deletes the oldest (docs/rust/DIFFERENCES.md).
            "oldest first, eight at a time, size checked between" => {
                assert_eq!(groups(scenario), [8, 8], "{name}");
                assert_eq!(
                    left,
                    ["bmp10", "bmp13", "bmp16", "bmp19"],
                    "the youngest four"
                );
                assert_eq!(recorded_left.len(), left.len());
            }
            "small files go with the big ones in a group" => {
                assert_eq!(groups(scenario), [8], "{name}");
                assert_eq!(left, ["bmp6", "jpeg_04.jpg"], "the youngest two");
                assert_eq!(recorded_left.len(), left.len());
            }
            _ => assert_eq!(left, recorded_left, "{name}"),
        }
    }
}
