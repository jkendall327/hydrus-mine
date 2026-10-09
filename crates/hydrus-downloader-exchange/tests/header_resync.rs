//! A query header the client has not synced to its history is exported as the
//! reference writes one, so the reference's own Sync recalculates it on load
//! (`oracle/fixtures/subscription_header_resync.json`,
//! `record_subscription_header_resync.py`).
use hydrus_downloader_exchange::subscriptions as exchange;
use serde_json::Value;

fn fixture() -> (Value, Vec<exchange::Subscription>) {
    let fixture = hydrus_testkit::fixture_json("subscription_header_resync.json");
    let subs = exchange::decode_text_at(
        &fixture["source"].to_string(),
        fixture["now"].as_i64().unwrap(),
    )
    .unwrap();
    (fixture, subs)
}

/// The cache fields of a v3 header: log status, file status, velocity, its
/// words, and the two example seeds.
fn cached(header: &Value) -> Vec<Value> {
    [8, 9, 13, 14, 15, 16]
        .iter()
        .map(|i| header[2][*i].clone())
        .collect()
}

#[test]
fn a_fresh_native_query_exports_the_header_the_references_add_query_makes() {
    let (fixture, mut subs) = fixture();
    let query = &mut subs[0].queries[0];
    query.reference_header = None;
    exchange::update_file_status(query, fixture["now"].as_i64().unwrap()).unwrap();
    let header = exchange::query_header_tuple(query).unwrap();
    assert_eq!(header[2][8], 1, "fresh queries ask the reference to sync");
    assert_eq!(cached(&header), cached(&fixture["fresh_updated"]));
    assert_eq!(fixture["fresh_wants_resync"], true);
}

#[test]
fn renaming_a_history_marks_the_header_unsynced_as_the_reference_does() {
    let (fixture, mut subs) = fixture();
    let query = &mut subs[0].queries[0];
    exchange::rename_history(query, "a new history".into());
    let header = exchange::query_header_tuple(query).unwrap();
    // (the example seeds are kept until the sync)
    assert_eq!(cached(&header)[..4], cached(&fixture["renamed"])[..4]);
    assert_eq!(header[2][8], 1);
}
