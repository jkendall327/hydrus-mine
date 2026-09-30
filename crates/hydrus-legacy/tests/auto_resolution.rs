//! Duplicates auto-resolution rules as the reference stores them
//! (`oracle/fixtures/auto_resolution.json`, made by
//! `oracle/dump_auto_resolution.py`): its suggested rules and rules using
//! every comparator and setting, at v3 and as v1/v2 stored them, decode to
//! what the reference holds.

use hydrus_legacy::objects::FileSearchContext;
use hydrus_legacy::objects::auto_resolution::{AutoResolutionRule, Comparator};
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};

fn search_json(search: &FileSearchContext) -> Value {
    let mut location: Vec<String> = search
        .location_context
        .current
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    location.sort();
    let predicates: Vec<Value> = search
        .predicates
        .iter()
        .map(|p| serde_json::from_str(&p.to_tuple_string()).unwrap())
        .collect();
    json!({ "location": location, "predicates": predicates })
}

fn comparator_json(c: &Comparator) -> Value {
    match c {
        Comparator::OneFileMetadata { looking_at, search } => {
            json!({ "type": "one_file_metadata", "looking_at": looking_at, "search": search_json(search) })
        }
        Comparator::OneFileHardcoded { looking_at, test } => {
            json!({ "type": "one_file_hardcoded", "looking_at": looking_at, "test": test })
        }
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => {
            use hydrus_core::search::number::NumberOp as O;
            let (op, extra) = match test.op {
                O::Less => (0, Value::Null),
                O::Greater => (1, Value::Null),
                O::Equal => (2, Value::Null),
                O::ApproxPercent { percent } => (3, json!(f64::from(percent) / 100.0)),
                O::NotEqual => (4, Value::Null),
                O::ApproxAbsolute { tolerance } => (5, json!(tolerance)),
                O::LessOrEqual => (6, Value::Null),
                O::GreaterOrEqual => (7, Value::Null),
            };
            json!({
                "type": "relative_file_info", "property": property, "op": op,
                "value": test.value, "extra": extra, "multiplier": multiplier, "delta": delta,
            })
        }
        Comparator::RelativeHardcoded(test) => {
            json!({ "type": "relative_hardcoded", "test": test })
        }
        Comparator::VisualDuplicates(confidence) => {
            json!({ "type": "visual_duplicates", "confidence": confidence })
        }
        Comparator::Or(members) => {
            json!({ "type": "or", "members": members.iter().map(comparator_json).collect::<Vec<_>>() })
        }
        Comparator::And(members) => {
            json!({ "type": "and", "members": members.iter().map(comparator_json).collect::<Vec<_>>() })
        }
    }
}

/// Numbers compare by value (the reference writes `1.0` and `1` alike).
fn normalise(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn stored_rules_decode_to_what_the_reference_holds() {
    let fixture = hydrus_testkit::fixture_json("auto_resolution.json");
    let rules = fixture["rules"].as_array().unwrap();
    assert!(rules.len() > 10);
    for case in rules {
        let stored = SerialisableObject::from_tuple_str(&case["stored"].to_string()).unwrap();
        let rule = AutoResolutionRule::from_object(&stored)
            .unwrap_or_else(|e| panic!("{}: {e}", case["expected"]["name"]));
        let expected = &case["expected"];
        let ours = json!({
            "name": rule.name,
            "id": rule.id,
            "paused": rule.paused,
            "operation_mode": rule.operation_mode,
            "max_pending_pairs": rule.max_pending_pairs,
            "search_1": search_json(&rule.search.search_1),
            "search_2": search_json(&rule.search.search_2),
            "dupe_search_type": rule.search.dupe_search_type,
            "pixel_dupes": rule.search.pixel_dupes,
            "max_hamming_distance": rule.search.max_hamming_distance,
            "comparators": rule.comparators.iter().map(comparator_json).collect::<Vec<_>>(),
            "action": rule.action,
            "delete": [rule.delete_a, rule.delete_b],
        });
        let mut expected_without_merge = expected.clone();
        let merge = expected_without_merge
            .as_object_mut()
            .unwrap()
            .remove("custom_merge")
            .unwrap();
        assert_eq!(
            normalise(&ours),
            normalise(&expected_without_merge),
            "{} (v{})",
            rule.name,
            stored.version
        );
        assert_eq!(
            rule.custom_merge_options.is_some(),
            !merge.is_null(),
            "{}",
            rule.name
        );
    }
}
