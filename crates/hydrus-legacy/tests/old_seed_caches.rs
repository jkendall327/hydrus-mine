//! File seed caches from before version 8 (an old install's subscriptions,
//! import folders and sessions) read as the reference upgrades them, against
//! `oracle/fixtures/legacy_seed_caches.json` (`record_legacy_seed_caches.py`).
use hydrus_legacy::objects::subscriptions::file_seed_cache;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};

fn read(
    cache: &Value,
) -> Result<Vec<hydrus_legacy::objects::subscriptions::LegacyFileSeed>, String> {
    let object =
        SerialisableObject::from_tuple_str(&cache.to_string()).map_err(|e| e.to_string())?;
    file_seed_cache(&object).map_err(|e| e.to_string())
}

/// The fields the upgrade sets, from the reference's version 8 rows.
fn recorded(upgraded: &Value) -> Vec<Value> {
    upgraded[2][2]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let f = &row[1][2];
            json!([f[0], f[1], f[3], f[4], f[5], f[6], f[7]])
        })
        .collect()
}

#[test]
fn caches_of_versions_one_to_seven_upgrade_as_the_reference_does() {
    let fixture = hydrus_testkit::fixture_json("legacy_seed_caches.json");
    for case in fixture["cases"].as_array().unwrap() {
        let old = &case["source"][3][1][0][2][8];
        let version = case["version"].as_u64().unwrap();
        assert_eq!(old[1], version);
        let seeds = read(old).unwrap();
        let ours: Vec<Value> = seeds
            .iter()
            .map(|s| {
                json!([
                    s.seed_type,
                    s.data,
                    s.created,
                    s.modified,
                    s.source_time,
                    s.status,
                    s.note
                ])
            })
            .collect();
        assert_eq!(ours, recorded(&case["upgraded_cache"]), "version {version}");
        // Paths compare as themselves; a URL's comparison form needs the
        // client's URL classes.
        for seed in &seeds {
            assert_eq!(seed.data_for_comparison.is_some(), seed.seed_type == 0);
        }
        if version >= 5 {
            let repeated = read(&case["duplicate_cache"]).unwrap();
            let ours: Vec<Value> = repeated
                .iter()
                .map(|s| {
                    json!([
                        s.seed_type,
                        s.data,
                        s.created,
                        s.modified,
                        s.source_time,
                        s.status,
                        s.note
                    ])
                })
                .collect();
            // the reference's list export of the same cache has no repeat ...
            let exported = &case["duplicate_exported"][2][1][2][0][1][3][1];
            assert_eq!(ours, recorded(exported), "version {version}");
            // ... and the upgrade alone keeps them, the first of each winning
            let mut first: Vec<Value> = Vec::new();
            for row in recorded(&case["duplicate_upgraded"]) {
                if !first.iter().any(|f| f[0] == row[0] && f[1] == row[1]) {
                    first.push(row);
                }
            }
            assert_eq!(ours, first, "version {version}");
        }
    }
    for rewrite in fixture["rewrites"].as_array().unwrap() {
        let seeds = read(&rewrite["source"]).unwrap();
        assert_eq!(
            json!(seeds[0].data),
            rewrite["upgraded"][2][2][0][1][2][1],
            "tumblr rewrite (versions up to 6)"
        );
    }
    for note in fixture["notes"].as_array().unwrap() {
        let result = read(&note["source"]);
        if note["source"][2][0][1]["note"].is_array() || note["source"][2][0][1]["note"].is_object()
        {
            assert!(result.is_err());
        } else {
            assert_eq!(
                json!(result.unwrap()[0].note),
                note["upgraded"][2][2][0][1][2][7]
            );
        }
    }
    for failure in fixture["failures"].as_array().unwrap() {
        assert!(
            read(&failure["source"]).is_err(),
            "a seed without a note is refused"
        );
    }
}
