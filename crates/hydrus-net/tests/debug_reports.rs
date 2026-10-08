//! Help > debug > report modes > network report mode, and its silent twin:
//! the engine says what happened to a job (`NetworkReportMode`). The
//! switches are process-wide, so this is tested alone here.
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use hydrus_core::debug_flags::{self, Flag};
use hydrus_net::{Job, NetEngine, NetOptions, Request};
use hydrus_store::Store;

// leaf: audit-options-help-debug-action-network-report-mode
// leaf: audit-options-help-debug-action-network-report-mode-silent
#[tokio::test]
async fn network_report_mode_reports_redirects_and_errors_unless_silent() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned())
    })));
    let app = Router::new()
        .route("/ok", get(|| async { "fine" }))
        .route("/bounce", get(|| async { Redirect::temporary("/ok") }))
        .route(
            "/bad",
            get(|| async { (StatusCode::NOT_FOUND, "no such thing") }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let engine = NetEngine::new(
        Store::open(dir.path()).unwrap(),
        NetOptions {
            connection_error_wait_time: 0,
            serverside_bandwidth_wait_time: 0,
            network_timeout: 2,
            obey_bandwidth: false,
            ..NetOptions::default()
        },
    )
    .unwrap();
    let run = |path: &'static str| {
        let engine = &engine;
        let url = format!("{base}{path}");
        async move { engine.fetch(&Request::get(url), &Job::new()).await }
    };

    run("/bounce").await.unwrap();
    run("/bad").await.unwrap_err();
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );

    Flag::NetworkReport.set(true);
    run("/bounce").await.unwrap();
    run("/bad").await.unwrap_err();
    let loud = std::mem::take(&mut *seen.lock().unwrap());
    assert_eq!(loud.len(), 2, "{loud:?}");
    assert_eq!(
        loud[0],
        format!("Network Jobs Redirect: {base}/bounce -> {base}/ok")
    );
    assert!(
        loud[1].starts_with("Network error should follow:\n"),
        "{}",
        loud[1]
    );
    assert!(
        loud[1].contains("no such thing") || loud[1].contains("404"),
        "{}",
        loud[1]
    );

    Flag::NetworkReportSilent.set(true);
    run("/bounce").await.unwrap();
    run("/bad").await.unwrap_err();
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent mode keeps popups out"
    );
    Flag::NetworkReport.set(false);
    Flag::NetworkReportSilent.set(false);
    debug_flags::set_sink(None);
}
