//! Help > debug > report modes > subscription report mode: working out when a
//! subscription is next due says why, per query (`Subscription` /
//! `SubscriptionQueryLegacy`'s `IsSyncDue` report). The switches are
//! process-wide, so this is tested alone here.
use std::sync::{Arc, Mutex};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_download::Downloader;
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_store::Store;
use hydrus_store::subscriptions as store_subs;

// leaf: audit-options-help-debug-action-subscription-report-mode
#[test]
fn subscription_report_mode_says_why_each_query_is_or_is_not_due() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned());
    })));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let net = Arc::new(NetEngine::new(Arc::clone(&store), NetOptions::default()).unwrap());
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let downloader = Downloader::new(Arc::clone(&store), net, importer).unwrap();
    let sub = store
        .write(|ctx| {
            let id = store_subs::create_subscription(
                ctx.conn(),
                "blue eyes",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let mut state = QueryState::new("blue_eyes");
            state.check_now = true;
            store_subs::add_query(ctx.conn(), id, &state, 0)?;
            Ok(store_subs::subscription(ctx.conn(), id)?.unwrap())
        })
        .unwrap();

    downloader.next_work_time(&sub).unwrap();
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );
    Flag::SubscriptionReport.set(true);
    downloader.next_work_time(&sub).unwrap();
    Flag::SubscriptionReport.set(false);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(
        seen[0],
        "Query \"blue eyes: blue_eyes\" IsSyncDue test. Paused/dead status is False/False, check time due is True, and check_now is True."
    );
    debug_flags::set_sink(None);
}
