//! Domain metadata staging decisions, replaying the recorded reference.
use hydrus_core::network::NetworkContext;
use hydrus_gui_model::downloader_interchange::{self as exchange, Draft};
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::{Store, network, settings};
use serde_json::{Value, json};

fn rule_state(store: &Store) -> Value {
    let rules = store.read(|c| settings::get::<BandwidthSettings>(c)).unwrap().rules;
    let mut map = serde_json::Map::new();
    for (context, rules) in rules {
        if context.kind == hydrus_core::network::CONTEXT_DOMAIN && !context.data.is_empty() {
            let mut rows: Vec<Value> = rules
                .rules()
                .iter()
                .map(|r| json!([r.kind as i64, r.time_delta, r.max_allowed]))
                .collect();
            rows.sort_by_key(|r| (r[0].as_i64(), r[1].as_u64().unwrap_or(0), r[2].as_u64()));
            map.insert(context.data, Value::Array(rows));
        }
    }
    Value::Object(map)
}

fn header_domains(store: &Store) -> Vec<String> {
    let mut domains: Vec<String> = store
        .read(|c| {
            let mut out = Vec::new();
            for context in network::header_contexts(c)? {
                if context.kind == hydrus_core::network::CONTEXT_DOMAIN
                    && !network::headers(c, &context)?.is_empty()
                {
                    out.push(context.data);
                }
            }
            Ok(out)
        })
        .unwrap();
    domains.sort();
    domains
}

fn import(case: &Value) -> (tempfile::TempDir, std::sync::Arc<Store>, exchange::Draft, exchange::Review) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut draft = Draft::load(&store).unwrap();
    let review = draft
        .import(exchange::decode_text(&case["source"].to_string()).unwrap())
        .unwrap();
    (dir, store, draft, review)
}

#[test]
fn a_package_without_rules_stops_the_reference_adding_later_packages_rules() {
    let fixture = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    let case = &fixture["stopped"];
    let (_dir, store, draft, review) = import(case);
    assert_eq!(review.added.len(), 3);
    draft.save(&store).unwrap();
    assert_eq!(rule_state(&store), case["state"]["rules"]);
    let recorded: Vec<String> = case["state"]["headers"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.as_object().unwrap().is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(header_domains(&store), recorded);
}

#[test]
fn only_the_first_eight_new_packages_are_shown_in_detail() {
    let fixture = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    let case = &fixture["nine"];
    let (_dir, store, draft, review) = import(case);
    let notices: Vec<&str> = case["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .filter(|n| n.starts_with("For domain"))
        .collect();
    assert_eq!(review.details, notices);
    assert_eq!(review.added.len(), 9);
    draft.save(&store).unwrap();
    assert_eq!(rule_state(&store), case["state"]["rules"]);
    let context = NetworkContext::domain("site8.example");
    assert!(store
        .read(|c| settings::get::<BandwidthSettings>(c))
        .unwrap()
        .rules
        .iter()
        .any(|(c, _)| c == &context));
}

#[test]
fn summaries_match_the_recorded_reference_wording() {
    let fixture = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    for definition in exchange::decode_text(&fixture["reference"].to_string()).unwrap() {
        if let exchange::Native::Domain(m) = &definition.native {
            let recorded = &fixture["summaries"][&m.domain];
            assert_eq!(m.safe_summary(), recorded["safe"]);
            assert_eq!(m.detailed_summary(), recorded["detailed"]);
        }
    }
}
