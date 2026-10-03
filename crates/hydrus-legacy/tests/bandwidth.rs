//! The bandwidth manager and its usage containers, as the reference stores
//! them (`oracle/dump_bandwidth.py`'s `stored` case: rules like the owner's
//! for one site, and usage on several contexts), read as the reference reads
//! them.

use serde_json::Value as Json;

use hydrus_core::pyjson::PyJson;
use hydrus_legacy::objects::bandwidth::{
    LegacyNetworkContext, bandwidth_manager, network_context, tracker_container,
};
use hydrus_legacy::serialisable::SerialisableObject;

fn object(value: &Json) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

fn context_facts(c: &LegacyNetworkContext) -> Json {
    serde_json::json!([c.kind, c.data])
}

#[test]
fn bandwidth_rules_and_usage_read_as_the_reference_reads_them() {
    let fixture = hydrus_testkit::fixture_json("bandwidth.json");
    let stored = &fixture["stored"];
    let facts = &stored["facts"];

    let manager = bandwidth_manager(&object(&stored["manager"])).unwrap();
    let mut rules: Vec<Json> = manager
        .rules
        .iter()
        .map(|(c, r)| {
            let mut r: Vec<Json> = r
                .iter()
                .map(|&(kind, span, max)| serde_json::json!([kind, span, max]))
                .collect();
            r.sort_by_key(|r| (r[0].as_i64(), r[1].as_i64().unwrap_or(-1), r[2].as_i64()));
            serde_json::json!([context_facts(c), r])
        })
        .collect();
    rules.sort_by_key(|r| (r[0][0].as_i64(), r[0][1].as_str().unwrap_or("").to_owned()));
    assert_eq!(Json::Array(rules), facts["rules"]);
    let mut names = manager.tracker_names.clone();
    names.sort();
    assert_eq!(serde_json::json!(names), facts["saved"]);

    let mut usage: Vec<Json> = stored["containers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let t = tracker_container(&object(c)).unwrap();
            let counters: Vec<Json> = t
                .counters
                .iter()
                .map(|c| {
                    let mut c = c.clone();
                    c.sort_unstable();
                    serde_json::json!(c)
                })
                .collect();
            serde_json::json!([context_facts(&t.context), counters])
        })
        .collect();
    usage.sort_by_key(|u| (u[0][0].as_i64(), u[0][1].as_str().unwrap_or("").to_owned()));
    assert_eq!(Json::Array(usage), facts["usage"]);
}

#[test]
fn version_1_contexts_kept_domains_and_subscriptions_as_hex() {
    let stored = |version: u32, kind: i64, data: &str| {
        PyJson::List(vec![
            PyJson::Int(47),
            PyJson::Int(i64::from(version)),
            PyJson::List(vec![PyJson::Int(kind), PyJson::Str(data.into())]),
        ])
    };
    let domain = network_context(&stored(1, 2, &hex::encode("site.example"))).unwrap();
    assert_eq!(domain.data.as_deref(), Some("site.example"));
    let page = network_context(&stored(1, 4, "abab")).unwrap();
    assert_eq!(page.data.as_deref(), Some("abab"));
    let current = network_context(&stored(2, 5, "my sub: query")).unwrap();
    assert_eq!(current.data.as_deref(), Some("my sub: query"));
}
