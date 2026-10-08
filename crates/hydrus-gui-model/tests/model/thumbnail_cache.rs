//! Replay actual reference DataCache, byte controls and last-access expiry.
use hydrus_core::HashId;
use hydrus_gui_model::thumbnail_cache::{Cache, combined, separated};
use hydrus_store::settings::ThumbnailCacheSettings;
use std::time::Duration;
// leaf: audit-options-help-debug-action-clear-thumbnail-cache
// leaf: audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout
#[test]
fn byte_lru_soft_overflow_timeout_touch_policy_and_clear_match_actual_reference() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_cache.json");
    let mut cache = Cache::new(ThumbnailCacheSettings {
        bytes: 100,
        timeout: 300,
    });
    let id = |name: &str| HashId(u32::from(name.as_bytes()[0]));
    for event in fixture["cache"].as_array().unwrap() {
        let now = Duration::from_secs_f64(event["now"].as_f64().unwrap());
        let action = event["action"].as_str().unwrap();
        if let Some(key) = action.strip_prefix("add ") {
            let size = match key {
                "a" => 60,
                "b" => 40,
                "c" | "e" | "f" => 20,
                "d" => 10,
                "huge" => 120,
                _ => panic!("unknown recorded bitmap"),
            };
            cache.insert(id(key), key.to_owned(), size, now);
        } else {
            match action {
                "get a" => {
                    assert_eq!(cache.get(id("a"), now), Some("a".into()));
                }
                "touch d" => {
                    cache.get(id("d"), now);
                }
                "shrink" => cache.set_policy(
                    ThumbnailCacheSettings {
                        bytes: 50,
                        timeout: 300,
                    },
                    now,
                ),
                "extend timeout" => cache.set_policy(
                    ThumbnailCacheSettings {
                        bytes: 50,
                        timeout: 600,
                    },
                    now,
                ),
                "clear" => cache.clear(),
                "reload" => {}
                name if name.starts_with("maintain") => cache.maintain(now),
                _ => panic!("unknown recorded cache operation"),
            }
        }
        let expected = event["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| id(v.as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(cache.keys(), expected, "{action}");
        assert_eq!(cache.bytes(), event["bytes"].as_u64().unwrap(), "{action}");
        assert_eq!(cache.policy().bytes, event["limit"].as_u64().unwrap());
    }
}
// leaf: audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout
#[test]
fn byte_amount_unit_decomposition_matches_real_qt_controls() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_cache.json");
    for case in fixture["controls"].as_array().unwrap() {
        let bytes = case["input"][0].as_u64().unwrap();
        let (amount, unit) = separated(bytes);
        assert_eq!(amount, case["separated"][0].as_i64().unwrap());
        assert_eq!(
            1024_u64.pow(unit as u32),
            case["separated"][1].as_u64().unwrap()
        );
        assert_eq!(combined(amount, unit), case["saved"][0].as_u64().unwrap());
    }
    assert_eq!(separated(0), (0, 4));
    assert_eq!(combined(0, 4), 0);
    assert_eq!(combined(1_048_577, 0), 1_048_576);
}

// leaf: audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout
#[test]
fn independent_options_edits_merge_with_a_newer_saved_timeout() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let before = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let mut after = before.clone();
    after.thumbnail_cache.bytes = 1024;
    store
        .write(|w| {
            hydrus_store::settings::set(
                w.conn(),
                &ThumbnailCacheSettings {
                    bytes: 32 * 1024 * 1024,
                    timeout: 600,
                },
            )
        })
        .unwrap();
    store.write(move |w| after.save(w.conn(), &before)).unwrap();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<ThumbnailCacheSettings>)
            .unwrap(),
        ThumbnailCacheSettings {
            bytes: 1024,
            timeout: 600
        }
    );
}

// leaf: audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout
#[test]
fn raw_timeout_is_preserved_until_its_minute_fields_are_edited() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_cache.json");
    let source = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    store
        .write(|w| {
            hydrus_store::settings::set(
                w.conn(),
                &ThumbnailCacheSettings {
                    bytes: 1_048_577,
                    timeout: 299,
                },
            )
        })
        .unwrap();
    let settings = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let mut editor = hydrus_gui_model::options::Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|p| *p == "speed and memory")
        .unwrap();
    editor.show_page(page);
    let row=editor.rows().iter().position(|r|matches!(r,hydrus_gui_model::options::Row::Opt{option,..} if option.label=="Thumbnail cache timeout:")).unwrap();
    assert_eq!(
        editor.applied().0.thumbnail_cache.timeout,
        fixture["timeout_boundaries"]["unchanged_apply"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        editor.applied().0.thumbnail_cache.bytes,
        fixture["bytes_boundaries"]["unchanged_apply"]
            .as_u64()
            .unwrap()
    );
    editor.field(row, 2, 0);
    assert_eq!(
        editor.applied().0.thumbnail_cache.timeout,
        fixture["timeout_boundaries"]["edited_minimum"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<ThumbnailCacheSettings>)
            .unwrap()
            .timeout,
        299,
        "draft has no write"
    );
}
