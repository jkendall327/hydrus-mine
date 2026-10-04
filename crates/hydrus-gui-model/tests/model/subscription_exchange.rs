//! Real Qt list payloads reach native query/file/gallery consumers intact.
use hydrus_downloader_exchange::subscriptions as exchange;
use hydrus_gui_model::{subscription_exchange as model, subscriptions_dialog::Subscriptions};
use hydrus_store::{Store, subscriptions};

#[test]
fn staged_full_subscription_import_resolves_names_and_persists_both_histories() {
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let imported = exchange::decode_text(&reference["single"].to_string()).unwrap();
    let mut dialog = Subscriptions::new(Vec::new());
    model::stage(&mut dialog, imported.clone()).unwrap();
    model::stage(&mut dialog, imported).unwrap();
    assert_eq!(
        dialog
            .subscriptions
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Artist", "Artist (1)"]
    );
    let first = &dialog.subscriptions[0].queries[0];
    let second = &dialog.subscriptions[1].queries[0];
    assert_ne!(
        first.exchange.as_ref().unwrap().log_name,
        second.exchange.as_ref().unwrap().log_name
    );
    let cached = exchange::query_header_tuple(first.exchange.as_ref().unwrap()).unwrap();
    assert_eq!(
        cached[2][8],
        reference["bundle"][2][1][1][2][0][3][1][0][2][8]
    );
    assert_eq!(
        cached[2][13],
        reference["bundle"][2][1][1][2][0][3][1][0][2][13]
    );
    assert_eq!(
        cached[2][14],
        reference["bundle"][2][1][1][2][0][3][1][0][2][14]
    );
    assert_eq!(
        first.files.get(&hydrus_store::queues::SeedStatus::Vetoed),
        Some(&1)
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    let saved = dialog.subscriptions[0].clone();
    let queue = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let id =
                subscriptions::create_subscription(conn, &saved.name, &saved.settings)?.unwrap();
            let q = &saved.queries[0];
            let queue = subscriptions::add_query(conn, id, &q.state, 1_700_000_000)?;
            model::restore(conn, queue, q.exchange.as_ref().unwrap())?;
            Ok(queue)
        })
        .unwrap();
    let log = store
        .read(move |conn| {
            model::history(conn, queue, "stored history").map_err(hydrus_store::StoreError::Invalid)
        })
        .unwrap();
    assert_eq!(
        log.file_seeds,
        first
            .exchange
            .as_ref()
            .unwrap()
            .log
            .as_ref()
            .unwrap()
            .file_seeds
    );
    assert_eq!(
        log.gallery_seeds,
        first
            .exchange
            .as_ref()
            .unwrap()
            .log
            .as_ref()
            .unwrap()
            .gallery_seeds
    );
    let header = store
        .read(move |conn| {
            Ok(hydrus_store::settings::get::<model::Headers>(conn)?.0[&queue].clone())
        })
        .unwrap();
    assert_eq!(header[2][15], reference["single"][2][0][3][1][0][2][15]);
    // Native staged reset must reach the frozen export without erasing gallery history.
    dialog.subscriptions[1].queries[0].reset();
    let exported = model::selected(&store, &dialog, 1_700_000_000).unwrap();
    assert!(
        exported[0].queries[0]
            .log
            .as_ref()
            .unwrap()
            .file_seeds
            .is_empty()
    );
    assert_eq!(
        exported[0].queries[0]
            .log
            .as_ref()
            .unwrap()
            .gallery_seeds
            .len(),
        1
    );
}

#[test]
fn unsupported_seed_runtime_data_leaves_the_entire_owner_draft_unchanged() {
    let mut imported = exchange::decode_text(
        &hydrus_testkit::fixture_json("subscription_exchange.json")["single"].to_string(),
    )
    .unwrap();
    imported[0].queries[0].log.as_mut().unwrap().file_seeds[0].status = 999;
    let mut dialog = Subscriptions::new(Vec::new());
    assert!(model::stage(&mut dialog, imported).is_err());
    assert!(dialog.subscriptions.is_empty());
}

#[test]
fn missing_history_question_matches_actual_qt_message_title_and_decisions() {
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let question = model::missing_history_question("Artist");
    let recorded = &reference["questions"][0];
    assert_eq!(question.title, recorded["title"].as_str().unwrap());
    assert_eq!(question.message, recorded["message"].as_str().unwrap());
    assert_eq!(
        question.choices,
        [
            recorded["yes"].as_str().unwrap(),
            recorded["no"].as_str().unwrap()
        ]
    );
}
