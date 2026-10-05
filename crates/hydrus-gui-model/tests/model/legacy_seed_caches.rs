//! Historical Qt histories reach staged counts and durable native queues.
use hydrus_downloader_exchange::subscriptions as exchange;
use hydrus_gui_model::{subscription_exchange as model, subscriptions_dialog::Subscriptions};
use hydrus_store::{Store, subscriptions};

#[test]
fn upgraded_historical_histories_reach_native_counts_restore_and_reopen_intact() {
    let fixture = hydrus_testkit::fixture_json("legacy_seed_caches.json");
    for case in fixture["cases"].as_array().unwrap() {
        let decoded = exchange::decode_text_at(
            &case["source"].to_string(),
            fixture["now"].as_i64().unwrap(),
        )
        .unwrap();
        let expected = decoded[0].queries[0].log.clone().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut draft = Subscriptions::new(Vec::new());
        model::stage(&mut draft, decoded).unwrap();
        let query = &draft.subscriptions[0].queries[0];
        assert_eq!(query.files.values().sum::<usize>(), 4);
        assert_eq!(query.ignored_notes, ["local veto"]);
        assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
        let saved = draft.subscriptions[0].clone();
        let queue = store
            .write(move |ctx| {
                let conn = ctx.conn();
                let id = subscriptions::create_subscription(conn, &saved.name, &saved.settings)?
                    .unwrap();
                let query = &saved.queries[0];
                let queue = subscriptions::add_query(conn, id, &query.state, 1_700_000_000)?;
                model::restore(conn, queue, query.exchange.as_ref().unwrap())?;
                Ok(queue)
            })
            .unwrap();
        let reopened = Store::open(directory.path()).unwrap();
        let restored = reopened
            .read(move |conn| {
                model::history(conn, queue, "restored").map_err(hydrus_store::StoreError::Invalid)
            })
            .unwrap();
        assert_eq!(restored.file_seeds, expected.file_seeds);
        assert_eq!(restored.gallery_seeds, expected.gallery_seeds);
        let cached = reopened
            .read(move |conn| {
                Ok(hydrus_store::settings::get::<model::Headers>(conn)?.0[&queue].clone())
            })
            .unwrap();
        assert_eq!(cached[2][9], case["exported"][2][0][3][1][0][2][9]);
        assert_eq!(cached[2][15], case["exported"][2][0][3][1][0][2][15]);
    }
}
