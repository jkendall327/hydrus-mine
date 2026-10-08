//! Replay real Qt controls and pending renderer accounting without decoding in tests.
use hydrus_core::HashId;
use hydrus_gui_model::image_cache::Cache;
use hydrus_store::{
    image_cache::{self, Policy},
    settings,
};
use std::{cell::Cell, rc::Rc, time::Duration};

// leaf: audit-options-speed-and-memory-image-cache-maximum-image-size-in-of-cache-that-can-be-cached
// leaf: audit-options-speed-and-memory-image-cache-memory-reserved-for-image-cache
#[test]
fn actual_pending_rgb_rgba_touch_admission_soft_overflow_policy_and_expiry() {
    let fixture = hydrus_testkit::fixture_json("image_cache.json");
    let mut cache = Cache::new(Policy {
        bytes: 100,
        timeout: 300,
        percentage: 50,
    });
    let mut held = std::collections::BTreeMap::new();
    let id = |name: &str| HashId(u32::from(name.as_bytes()[0]));
    for event in fixture["cache"].as_array().unwrap() {
        let now = Duration::from_secs_f64(event["now"].as_f64().unwrap());
        let action = event["action"].as_str().unwrap();
        if let Some(name) = action.strip_prefix("get ") {
            let value = cache.get(id(name), now, |value: &Rc<Cell<Option<u64>>>| value.get());
            assert_eq!(
                value.is_none(),
                event["created"].as_bool().unwrap(),
                "{action}"
            );
            let value = value.unwrap_or_else(|| {
                let value = Rc::new(Cell::new(None));
                cache.insert(
                    id(name),
                    value.clone(),
                    event["estimate"].as_u64().unwrap(),
                    now,
                );
                value
            });
            held.insert(name.to_owned(), value);
        } else if let Some(name) = action.strip_prefix("finish ") {
            held[name].set(event["loaded_bytes"].as_u64());
        } else {
            match action {
                "set policy" | "lower percentage keeps existing" | "shrink" | "extend timeout" => {
                    let policy = &event["policy"];
                    cache.set_policy(
                        Policy {
                            bytes: policy[0].as_u64().unwrap(),
                            timeout: policy[1].as_u64().unwrap(),
                            percentage: policy[2].as_u64().unwrap(),
                        },
                        now,
                    );
                }
                "clear" => cache.clear(),
                "loaded accounting unchanged" => {}
                "external renderer remains" => {
                    assert_eq!(held["small"].get(), event["external_bytes"].as_u64());
                }
                _ => cache.maintain(now),
            }
        }
        let keys = event["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| id(name.as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(cache.keys(), keys, "{action}");
        assert_eq!(cache.bytes(), event["bytes"].as_u64().unwrap(), "{action}");
    }
}

#[test]
fn actual_legacy_native_wins_changed_fields_and_raw_control_values_survive_reopen() {
    use hydrus_legacy::{
        objects::ClientOptions,
        serialisable::{SerialisableObject, SerialisableType},
    };
    let fixture = hydrus_testkit::fixture_json("image_cache.json");
    let tuple = &fixture["legacy"];
    let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
    let options = ClientOptions::from_object(&object).unwrap();
    let expected = Policy::from_legacy(&options);
    assert_eq!(expected.bytes, fixture["raw"]["saved"][0].as_u64().unwrap());
    assert_eq!(expected.timeout, 299);
    assert_eq!(
        serde_json::from_str::<Policy>("{}").unwrap(),
        Policy::default()
    );
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let version = tuple[1].as_i64().unwrap();
    let info = tuple[2].to_string();
    store.write(move |ctx| {
        ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?;
        Ok(())
    }).unwrap();
    assert_eq!(store.read(image_cache::load).unwrap(), expected);
    let before = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let mut after = before.clone();
    after.image_cache.bytes = 1024;
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    timeout: 900,
                    percentage: 50,
                    ..expected
                },
            )
        })
        .unwrap();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved = Policy {
        bytes: 1024,
        timeout: 900,
        percentage: 50,
    };
    assert_eq!(store.read(image_cache::load).unwrap(), saved);
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(reopened.read(image_cache::load).unwrap(), saved);
    store
        .write(|ctx| {
            ctx.conn().execute(
                "UPDATE settings SET value='invalid' WHERE key='image_cache'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(store.read(image_cache::load).is_err());
}

// leaf: audit-options-speed-and-memory-image-cache-maximum-image-size-in-of-cache-that-can-be-cached
// leaf: audit-options-speed-and-memory-image-cache-memory-reserved-for-image-cache
#[test]
fn actual_byte_timeout_and_percentage_controls_match_the_recording() {
    let fixture = hydrus_testkit::fixture_json("image_cache.json");
    assert_eq!(
        Policy::default().bytes,
        fixture["initial"][0].as_u64().unwrap()
    );
    assert_eq!(
        Policy::default().timeout,
        fixture["initial"][1].as_u64().unwrap()
    );
    assert_eq!(
        Policy::default().percentage,
        fixture["initial"][2].as_u64().unwrap()
    );
    for case in fixture["controls"].as_array().unwrap() {
        let bytes = case["input"][0].as_u64().unwrap();
        let (amount, unit) = hydrus_gui_model::thumbnail_cache::separated(bytes);
        assert_eq!(amount, case["separated"][0].as_i64().unwrap());
        assert_eq!(
            1024_u64.pow(unit as u32),
            case["separated"][1].as_u64().unwrap()
        );
        assert_eq!(
            hydrus_gui_model::thumbnail_cache::combined(amount, unit),
            case["saved"][0].as_u64().unwrap()
        );
        assert_eq!(case["saved"], case["reopened"]);
        let screen = (
            fixture["display"][0].as_u64().unwrap(),
            fixture["display"][1].as_u64().unwrap(),
        );
        assert_eq!(
            hydrus_gui_model::image_cache::screen_estimate(bytes, screen),
            case["estimate"].as_str().unwrap()
        );
        assert_eq!(
            hydrus_gui_model::image_cache::percentage_estimate(
                Policy {
                    bytes,
                    timeout: case["saved"][1].as_u64().unwrap(),
                    percentage: case["saved"][2].as_u64().unwrap()
                },
                true
            ),
            case["percentage_estimate"].as_str().unwrap()
        );
    }
    assert_eq!(fixture["cancel_saved"], fixture["initial"]);
    assert_eq!(fixture["raw"]["edited_minimum"], 300);
}

// leaf: audit-options-speed-and-memory-image-cache-maximum-image-size-in-of-cache-that-can-be-cached
// leaf: audit-options-speed-and-memory-image-cache-memory-reserved-for-image-cache
#[test]
fn untouched_normalized_controls_preserve_concurrent_fields_but_explicit_edits_win() {
    use hydrus_gui_model::options::{self, Settings};
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let raw = Policy {
        bytes: 1_048_577,
        timeout: 299,
        percentage: 5,
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let before = store.read(Settings::load).unwrap();
    let pages = options::pages(&before);
    let (normalized, errors) = options::applied(&pages, &before, &options::values(&pages, &before));
    assert!(errors.is_empty());
    assert_eq!(
        normalized.image_cache,
        Policy {
            bytes: 1_048_576,
            timeout: 299,
            percentage: 10
        }
    );
    let concurrent = Policy {
        bytes: 512,
        timeout: 900,
        percentage: 40,
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &concurrent))
        .unwrap();
    let implicit = normalized.clone();
    let initial = before.clone();
    store
        .write(move |ctx| implicit.save(ctx.conn(), &initial))
        .unwrap();
    assert_eq!(store.read(image_cache::load).unwrap(), concurrent);
    let mut explicit = normalized;
    explicit.image_cache.bytes = 256;
    explicit.image_cache.percentage = 30;
    let initial = before.clone();
    store
        .write(move |ctx| explicit.save(ctx.conn(), &initial))
        .unwrap();
    assert_eq!(
        store.read(image_cache::load).unwrap(),
        Policy {
            bytes: 256,
            percentage: 30,
            ..concurrent
        }
    );
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let (unchanged, errors) = options::applied(&pages, &before, &options::values(&pages, &before));
    assert!(errors.is_empty());
    store
        .write(move |ctx| unchanged.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(image_cache::load).unwrap(),
        Policy {
            bytes: 1_048_576,
            timeout: 299,
            percentage: 10
        }
    );
}
