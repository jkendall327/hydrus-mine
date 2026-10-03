//! An importer's file log window against the reference's, recorded by
//! `oracle/record_file_log.py`: each file's row, the whole log's menu for
//! logs of many counts, and the right-click menu on selected rows.

use serde_json::{Value as Json, json};

use hydrus_gui_model::file_log::{Entry, LogFacts, log_menu, row, row_menu};
use hydrus_store::queues::{
    FileSeed, FileSeedMeta, SeedStatus, SeedType, StatusCounts, file_log_status,
};

fn status(code: i64) -> SeedStatus {
    SeedStatus::from_code(code).unwrap()
}

/// A recorded seed, as the store would hold it.
fn seed(recorded: &Json, now: i64) -> FileSeed {
    let data = recorded[1].as_str().unwrap().to_owned();
    let mut meta = FileSeedMeta::default();
    if recorded[7].as_bool().unwrap() {
        use sha2::Digest as _;
        meta.set_hash("sha256", hex::encode(sha2::Sha256::digest(data.as_bytes())));
    }
    FileSeed {
        id: 0,
        queue_id: 0,
        seed_type: if recorded[0].as_bool().unwrap() {
            SeedType::Url
        } else {
            SeedType::Path
        },
        data_for_comparison: data.clone(),
        data,
        created: now - recorded[3].as_i64().unwrap(),
        modified: now - recorded[4].as_i64().unwrap(),
        source_time: recorded[5].as_i64().map(|ago| now - ago),
        status: status(recorded[2].as_i64().unwrap()),
        note: recorded[6].as_str().unwrap().into(),
        referral_url: None,
        meta,
    }
}

/// A menu as the recording writes it.
fn tree(entries: &[Entry]) -> Json {
    Json::Array(
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(label, _) | Entry::Label(label) => json!(label),
                Entry::Separator => json!("---"),
                Entry::Menu(title, entries) => json!({"menu": title, "entries": tree(entries)}),
            })
            .collect(),
    )
}

fn facts(seeds: &[FileSeed]) -> LogFacts {
    let mut counts = StatusCounts::new();
    for s in seeds {
        *counts.entry(s.status).or_default() += 1;
    }
    LogFacts {
        counts,
        len: seeds.len(),
        urls: seeds.first().is_none_or(|s| s.seed_type == SeedType::Url),
    }
}

#[test]
fn the_file_log_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("file_log.json");
    let now = recorded["now"].as_i64().unwrap();
    let seeds: Vec<FileSeed> = recorded["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| seed(s, now))
        .collect();
    for (i, (s, want)) in seeds
        .iter()
        .zip(recorded["rows"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(json!(row(s, i, now)), *want, "row {i}");
    }
    let log = facts(&seeds);
    assert_eq!(file_log_status(&log.counts), recorded["header"]);

    for menu in recorded["menus"].as_array().unwrap() {
        let mut counts = StatusCounts::new();
        for (code, n) in menu["counts"].as_object().unwrap() {
            counts.insert(status(code.parse().unwrap()), n.as_u64().unwrap() as usize);
        }
        let log = LogFacts {
            len: counts.values().sum(),
            counts,
            urls: menu["urls"].as_bool().unwrap(),
        };
        assert_eq!(
            tree(&log_menu(&log, false)),
            menu["menu"],
            "{}",
            menu["counts"]
        );
    }

    let row_menus = recorded["row_menus"].as_array().unwrap();
    for menu in &row_menus[..2] {
        let selected: Vec<&FileSeed> = menu["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| &seeds[i.as_u64().unwrap() as usize])
            .collect();
        assert_eq!(
            tree(&row_menu(&selected, &log)),
            menu["menu"],
            "{}",
            menu["selected"]
        );
    }
    // a log of paths
    let path = seed(
        &json!([false, "/home/me/a.jpg", 1, 3600, 3600, null, "", true]),
        now,
    );
    let paths = facts(std::slice::from_ref(&path));
    assert_eq!(tree(&row_menu(&[&path], &paths)), row_menus[2]["menu"]);
}
