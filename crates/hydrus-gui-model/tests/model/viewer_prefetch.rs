//! Exact real Qt neighbourhood, one-miss renderer budgets and atomic finished-only flush.
use hydrus_core::HashId;
use hydrus_gui_model::{
    image_cache::Cache,
    viewer_prefetch::{self, Budget, Entry, Step},
};
use hydrus_store::{image_cache::Policy, viewer_prefetch::Preferences};
use std::{cell::Cell, collections::BTreeMap, rc::Rc, time::Duration};
fn id(name: &str) -> HashId {
    HashId(u32::from(name.as_bytes()[0]))
}
fn names(values: &serde_json::Value) -> Vec<HashId> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|name| id(name.as_str().unwrap()))
        .collect()
}
fn estimate(name: &str) -> u64 {
    match name {
        "small" => 27,
        "large" => 33,
        _ => 30,
    }
}
fn loaded(name: &str) -> u64 {
    if name == "b" { 40 } else { estimate(name) }
}
#[test]
fn all_actual_circular_next_first_neighbour_orders_and_controls() {
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    for case in fixture["neighbours"].as_array().unwrap() {
        assert_eq!(
            viewer_prefetch::neighbours(
                &names(&case["files"]),
                case["index"].as_u64().unwrap() as usize,
                case["previous"].as_u64().unwrap(),
                case["next"].as_u64().unwrap()
            ),
            names(&case["neighbours"]),
            "{case}"
        );
    }
    for case in fixture["controls"].as_array().unwrap() {
        let input = &case["input"];
        let raw = Preferences {
            previous: input[0].as_i64().unwrap().max(0) as u64,
            next: input[1].as_i64().unwrap().max(0) as u64,
            percentage: input[2].as_i64().unwrap().max(0) as u64,
            duplicate_pairs: case["duplicate_pairs"].as_u64().unwrap(),
        };
        let shown = raw.displayed();
        assert_eq!(
            serde_json::json!([shown.previous, shown.next, shown.percentage]),
            case["saved"]
        );
        assert_eq!(case["reopened"], case["saved"]);
        let bytes = case["cache_bytes"].as_u64().unwrap();
        assert_eq!(
            viewer_prefetch::percentage_estimate(
                bytes,
                shown.percentage,
                case["nice"].as_bool().unwrap()
            ),
            case["caption"].as_str().unwrap()
        );
        assert_eq!(
            viewer_prefetch::warning(bytes, shown),
            case["warning"].as_str().unwrap()
        );
    }
    assert_eq!(fixture["cancel_saved"], fixture["initial"]);
    assert!(
        fixture["hidden"]
            .as_array()
            .unwrap()
            .iter()
            .all(|case| !case["pending"].as_array().unwrap().is_empty())
    );
}
#[test]
fn actual_pending_loaded_recounts_equality_unknown_and_one_miss_passes() {
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    for trace in fixture["prefetch"].as_array().unwrap() {
        let policy = &trace["policy"];
        let policy = Policy {
            bytes: policy[0].as_u64().unwrap(),
            timeout: 300,
            percentage: policy[1].as_u64().unwrap(),
        };
        let percent = trace["policy"][2].as_u64().unwrap();
        let mut cache = Cache::new(policy);
        let mut held: BTreeMap<String, Rc<Cell<Option<u64>>>> = BTreeMap::new();
        for event in trace["events"].as_array().unwrap() {
            let mut created = Vec::new();
            match event["action"].as_str().unwrap() {
                "get" => {
                    let name = event["input"].as_str().unwrap();
                    let value = Rc::new(Cell::new(None));
                    cache.insert(id(name), value.clone(), estimate(name), Duration::ZERO);
                    held.insert(name.to_owned(), value);
                }
                "finish" => {
                    let name = event["input"].as_str().unwrap();
                    held[name].set(Some(loaded(name)));
                }
                action => {
                    let mut budget = Budget::new(policy, percent);
                    for (index, name) in event["input"].as_array().unwrap().iter().enumerate() {
                        let name = name.as_str().unwrap();
                        if let Some(value) =
                            cache.get(id(name), Duration::ZERO, |value: &Rc<Cell<Option<u64>>>| {
                                value.get()
                            })
                        {
                            if budget.consider(value.get().map_or(Entry::Pending, Entry::Ready))
                                == Step::Wait
                            {
                                break;
                            }
                        } else {
                            let bytes = if action == "prefetch_unknown" && index == 0 {
                                None
                            } else {
                                Some(estimate(name))
                            };
                            let Step::Decode(bytes) = budget.consider(Entry::Missing(bytes)) else {
                                break;
                            };
                            if !cache.try_flush_finished_space(bytes, |value| value.get().is_some())
                            {
                                break;
                            }
                            let value = Rc::new(Cell::new(None));
                            cache.insert(id(name), value.clone(), bytes, Duration::ZERO);
                            held.insert(name.to_owned(), value);
                            created.push(name.to_owned());
                            break;
                        }
                    }
                }
            }
            assert_eq!(
                serde_json::json!(created),
                event["created"],
                "{} {event}",
                trace["name"]
            );
            assert_eq!(
                cache.keys(),
                names(&event["keys"]),
                "{} {event}",
                trace["name"]
            );
            assert_eq!(
                cache.bytes(),
                event["bytes"].as_u64().unwrap(),
                "{} {event}",
                trace["name"]
            );
        }
    }
}
#[test]
fn actual_atomic_finished_only_flush_preserves_all_entries_on_failure() {
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    for case in fixture["flush"].as_array().unwrap() {
        let only_pending = case.get("note").is_some();
        let mut cache = Cache::new(Policy {
            bytes: if only_pending { 60 } else { 100 },
            timeout: 300,
            percentage: 100,
        });
        for name in case["before"]["keys"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            let value = Rc::new(Cell::new(if name == "a" && !only_pending {
                Some(30)
            } else {
                None
            }));
            cache.insert(id(name), value, 30, Duration::ZERO);
        }
        assert_eq!(
            cache.try_flush_finished_space(case["wanted"].as_u64().unwrap(), |value| value
                .get()
                .is_some()),
            case["success"].as_bool().unwrap()
        );
        assert_eq!(cache.keys(), names(&case["after"]["keys"]));
        assert_eq!(cache.bytes(), case["after"]["bytes"].as_u64().unwrap());
    }
}
#[test]
fn legacy_native_wins_and_normalized_passive_apply_preserves_concurrent_fields() {
    use hydrus_legacy::{
        objects::ClientOptions,
        serialisable::{SerialisableObject, SerialisableType},
    };
    use hydrus_store::{settings, viewer_prefetch};
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    let tuple = &fixture["legacy"];
    let legacy = ClientOptions::from_object(
        &SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap(),
    )
    .unwrap();
    let expected = Preferences::from_legacy(&legacy);
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let version = tuple[1].as_i64().unwrap();
    let info = tuple[2].to_string();
    store.write(move |ctx| {ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())}).unwrap();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<Preferences>("{}").unwrap(),
        Preferences::default()
    );
    let raw = Preferences {
        previous: 99,
        next: 4,
        percentage: 5,
        duplicate_pairs: 7,
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), raw);
    let concurrent = Preferences {
        previous: 8,
        next: 9,
        percentage: 40,
        duplicate_pairs: 11,
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &concurrent))
        .unwrap();
    store
        .write(move |ctx| raw.displayed().save_changed(ctx.conn(), raw))
        .unwrap();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), concurrent);
    let explicit = Preferences {
        previous: 1,
        percentage: 30,
        ..raw.displayed()
    };
    store
        .write(move |ctx| explicit.save_changed(ctx.conn(), raw))
        .unwrap();
    assert_eq!(
        store.read(viewer_prefetch::load).unwrap(),
        Preferences {
            previous: 1,
            percentage: 30,
            ..concurrent
        }
    );
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    store
        .write(move |ctx| raw.displayed().save_changed(ctx.conn(), raw))
        .unwrap();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), raw.displayed());
}
