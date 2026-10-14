//! The three "additional fixed time to wait between gallery page fetches"
//! options (download pages, subscriptions, watchers), set in the real options
//! window and applied, then met by the running network engine: a second page
//! fetch of a kind waits until more than the saved number of seconds after the
//! first, as `TryToConsumeAGalleryToken` does in the reference
//! (`oracle/fixtures/gallery_wait_options.json`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{Router, routing::get};
use hydrus_core::bandwidth::{GalleryTokenKind, Manager};
use hydrus_net::{BandwidthScope, Job, NetEngine, NetOptions, Request};
use hydrus_store::network_runtime::WaitReason;
use hydrus_store::settings::GuiFormatting;

use super::options_system_consumers::{client, row_in};

fn secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn kind_of(name: &str) -> GalleryTokenKind {
    match name {
        "download page" => GalleryTokenKind::DownloadPage,
        "subscription" => GalleryTokenKind::Subscription,
        _ => GalleryTokenKind::Watcher,
    }
}

/// The reference's token rule (each kind with its own option, a site's turn
/// only when more than the wait has passed since its last), replayed on the
/// engine's manager for the delays the reference was recorded with.
#[test]
fn the_recorded_reference_token_rule_holds_for_the_engines_manager() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/gallery_wait_options.json"
    ))
    .unwrap();
    let mut manager = Manager::default();
    for step in fixture["steps"].as_array().unwrap() {
        let kind = step["kind"].as_str().unwrap();
        let delay = fixture["waits"][kind].as_i64().unwrap();
        let result = manager.try_to_consume_gallery_token(
            step["sld"].as_str().unwrap(),
            kind_of(kind),
            delay,
            step["now"].as_i64().unwrap(),
        );
        let expected = if step["consumed"].as_bool().unwrap() {
            Ok(())
        } else {
            Err(step["next"].as_i64().unwrap())
        };
        assert_eq!(result, expected, "{step}");
    }
}

// leaf: audit-options-downloading-gallery-downloader-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
// leaf: audit-options-downloading-subscriptions-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
// leaf: audit-options-downloading-watchers-additional-fixed-time-in-seconds-to-wait-between-watcher-checks
#[tokio::test(flavor = "multi_thread")]
async fn each_saved_wait_spaces_that_kinds_page_fetches_in_the_running_engine() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/gallery_wait_options.json"
    ))
    .unwrap();
    // (the recording's three options are three different ones)
    assert_eq!(
        fixture["options"],
        serde_json::json!({
            "download page": "gallery_page_wait_period_pages",
            "subscription": "gallery_page_wait_period_subscriptions",
            "watcher": "watcher_page_wait_period",
        })
    );

    let client = client();
    let window = client.options("downloading");
    let waits = "Additional fixed time (in seconds) to wait between gallery page fetches:";
    let chosen = [
        ("download page", "gallery downloader", waits, 2),
        ("subscription", "subscriptions", waits, 3),
        (
            "watcher",
            "watchers",
            "Additional fixed time (in seconds) to wait between watcher checks:",
            4,
        ),
    ];
    for (_, in_box, label, n) in chosen {
        let (i, found) = row_in(&window, in_box, label);
        assert_eq!((found.kind, found.minimum, found.maximum), (2, 1, 3600));
        window.invoke_number_edited(i, n);
    }
    window.invoke_apply();
    // (timestamps in the job's status, to read the turn exactly)
    client
        .store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &GuiFormatting {
                    iso: true,
                    figures: 3,
                },
            )
        })
        .unwrap();

    // (the imported client has all new network traffic paused, and the
    // reference's default per-domain request limits would hold a local server
    // back for longer than the waits under test)
    client
        .store
        .write(|ctx| {
            let mut bandwidth: hydrus_store::bandwidth::BandwidthSettings =
                hydrus_store::settings::get(ctx.conn())?;
            bandwidth.rules = vec![(
                hydrus_store::network::NetworkContext::global(),
                hydrus_core::bandwidth::Rules::new([]),
            )];
            hydrus_store::settings::set(ctx.conn(), &bandwidth)?;
            let mut pauses: hydrus_store::settings::Pauses =
                hydrus_store::settings::get(ctx.conn())?;
            pauses.network_traffic = false;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/page", get(|| async { "ok" }));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let network = client.get();
    let engine = NetEngine::new(
        client.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..NetOptions::from_settings(&network)
        },
    )
    .unwrap();
    engine.reload_settings().unwrap();
    let saved = engine.bandwidth_settings();
    assert_eq!(
        (
            saved.gallery_page_wait_pages,
            saved.gallery_page_wait_subscriptions,
            saved.watcher_page_wait
        ),
        (2, 3, 4)
    );

    let mut request = Request::get(format!("{base}/page"));
    request.gallery_page = true;
    let scope = |name: &str| BandwidthScope {
        gallery_token: Some(kind_of(name)),
        ..BandwidthScope::default()
    };
    let before = secs();
    for (name, ..) in chosen {
        let job = Job::scoped(scope(name));
        let fetched =
            tokio::time::timeout(Duration::from_secs(10), engine.fetch(&request, &job)).await;
        assert!(
            fetched.is_ok(),
            "first {name} fetch stuck: {:?}",
            job.state()
        );
        fetched.unwrap().unwrap();
    }
    let after = secs();
    // the second fetch of each kind, all at once
    let mut pending = Vec::new();
    for (name, _, _, wait) in chosen {
        let job = Job::scoped(scope(name));
        let handle = {
            let (engine, request, job) = (engine.clone(), request.clone(), job.clone());
            tokio::spawn(async move {
                engine.fetch(&request, &job).await.unwrap();
                secs()
            })
        };
        pending.push((name, wait, job, handle));
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    for (name, wait, job, handle) in pending {
        let state = job.state();
        assert_eq!(state.wait, WaitReason::Gallery, "{name}: {state:?}");
        // its turn is the first fetch's time plus this kind's wait, and no
        // other kind's
        let wait = i64::from(wait);
        let turns: Vec<i64> = (before + wait..=after + wait).collect();
        let turn = turns
            .iter()
            .copied()
            .find(|t| {
                state.status
                    == format!(
                        "waiting to start: {}",
                        hydrus_core::time::timestamp_to_iso(Some(*t))
                    )
            })
            .unwrap_or_else(|| panic!("{name}: {:?} not in {turns:?}", state.status));
        let finished = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .unwrap_or_else(|_| panic!("{name} never went"))
            .unwrap();
        // (more than the wait has to pass, not just the wait)
        assert!(finished > turn, "{name}: {finished} <= {turn}");
    }
}
