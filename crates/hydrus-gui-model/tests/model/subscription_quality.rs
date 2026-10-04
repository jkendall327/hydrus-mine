//! Actual Qt quality summaries and durable queue/media consumers.
use hydrus_core::{
    Sha256,
    subscriptions::{QueryState, SubscriptionSettings},
};
use hydrus_gui_model::subscription_quality;
use hydrus_store::{
    Store,
    queues::{self, FileSeedMeta, NewFileSeed, SeedType},
    subscriptions,
};
use serde_json::json;
use std::sync::atomic::AtomicBool;

#[test]
fn durable_logs_and_current_file_states_match_reference_quality_outputs() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hashes: Vec<Sha256> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
        .collect();
    let ids = store
        .read(|c| hydrus_store::master::hash_ids(c, &hashes))
        .unwrap();
    let ordered: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
    store
        .write_content({
            let ordered = ordered.clone();
            move |w| {
                w.inbox(&ordered)?;
                w.archive(&[ordered[1]])?;
                w.delete_files(
                    w.roles().combined_local_media,
                    &[ordered[2]],
                    Some("synthetic quality recording"),
                )
            }
        })
        .unwrap();
    let (full, empty) = store
        .write(move |ctx| {
            let sub = subscriptions::create_subscription(
                ctx.conn(),
                "quality",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let full = subscriptions::add_query(
                ctx.conn(),
                sub,
                &QueryState::new("synthetic query"),
                123,
            )?;
            let empty = subscriptions::add_query(ctx.conn(), sub, &QueryState::new("empty"), 123)?;
            let hashes = [
                hashes[0],
                hashes[1],
                hashes[2],
                hashes[0],
                Sha256([170; 32]),
            ];
            let seeds: Vec<_> = hashes
                .iter()
                .enumerate()
                .map(|(i, h)| NewFileSeed {
                    seed_type: SeedType::Url,
                    data: format!("https://quality.example/{i}"),
                    data_for_comparison: format!("https://quality.example/{i}"),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta {
                        hashes: vec![("sha256".into(), h.to_string())],
                        ..FileSeedMeta::default()
                    },
                })
                .collect();
            queues::add_file_seeds(ctx.conn(), full, &seeds, false, 123)?;
            let mut seeds = queues::file_seeds(ctx.conn(), full)?;
            seeds[1].status = queues::SeedStatus::Error;
            queues::update_file_seed(ctx.conn(), &seeds[1])?;
            Ok((full, empty))
        })
        .unwrap();
    let cancel = AtomicBool::new(false);
    let selected = vec![(full, "display, name".into()), (empty, "empty".into())];
    let reports = store
        .read(|c| hydrus_store::subscription_quality::read(c, &selected, &cancel))
        .unwrap();
    let recorded = hydrus_testkit::fixture_json("subscription_quality.json");
    let data: Vec<_> = reports
        .iter()
        .map(|r| json!([r.name, r.inbox, r.archived, r.deleted]))
        .collect();
    assert_eq!(json!(data), recorded["data"]);
    assert_eq!(
        json!(subscription_quality::information(&reports)),
        recorded["messages"][0]
    );
    assert_eq!(
        json!(subscription_quality::csv(&reports)),
        recorded["clipboard"][0]
    );
    assert_eq!(
        json!(subscription_quality::MENU.map(|(label, _)| label)),
        recorded["menu"]
    );
    let cancelled = AtomicBool::new(true);
    assert!(
        store
            .read(|c| hydrus_store::subscription_quality::read(c, &selected, &cancelled))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .read(|c| hydrus_store::subscription_quality::read(
                c,
                &[(i64::MAX, "missing".into())],
                &cancel
            ))
            .is_err()
    );
    store
        .write_content(move |w| w.archive(&[ordered[0]]))
        .unwrap();
    let changed = store
        .read(|c| hydrus_store::subscription_quality::read(c, &selected, &cancel))
        .unwrap();
    assert_eq!(
        (changed[0].inbox, changed[0].archived, changed[0].deleted),
        (0, 2, 2)
    );
    assert!(subscription_quality::information(&changed).contains("good 50%"));
}
