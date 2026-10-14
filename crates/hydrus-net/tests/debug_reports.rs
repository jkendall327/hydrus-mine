//! Help > debug > report modes > network report mode, and its silent twin:
//! the engine says what happens to a job (`NetworkReportMode`), compared with
//! what the running reference said for the same three fetches
//! (`oracle/record_debug_report_modes.py`: a redirect, a plain page and a
//! 404, against a local server). The switches are process-wide, so this is
//! tested alone here.
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use hydrus_core::debug_flags::{self, Flag};
use hydrus_net::{Job, NetEngine, NetOptions, Request};
use hydrus_store::Store;

type Lines = Arc<Mutex<Vec<String>>>;

/// What kind of report a line is: the reference's report points, in order.
fn kind(line: &str) -> &'static str {
    for (prefix, kind) in [
        ("Network Job Added: ", "added"),
        ("Network Job Starting: ", "starting"),
        ("Network Jobs Referral URLs for ", "referral"),
        ("Request Headers set by the network domain manager: ", "domain headers"),
        ("No custom headers set", "domain headers"),
        ("Request Headers:\n", "headers"),
        ("Network Jobs Redirect: ", "redirect"),
        ("Network error should follow:", "error"),
        ("Network Job Done: ", "done"),
    ] {
        if line.starts_with(prefix) {
            return kind;
        }
    }
    "other"
}

fn recorded(mode: &str) -> Vec<(String, Vec<String>)> {
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    recording["network"][mode]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| {
            (
                run["path"].as_str().unwrap().to_owned(),
                run["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| m["text"].as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}

/// Checks one fetch's lines against the reference's for the same fetch: the
/// same reports in the same order, and the same words where the words do not
/// depend on this client's headers and server.
fn same_reports(path: &str, said: &[String], reference: &[String], base: &str) {
    let reference: Vec<String> = reference.iter().map(|l| l.replace("<base>", base)).collect();
    // (the reference's retry of its own earlier job can end late: its "done"
    // for the previous path is not this path's)
    let reference: Vec<&String> = reference
        .iter()
        .filter(|l| !(l.starts_with("Network Job Done: ") && !l.ends_with(path)))
        .collect();
    let said_kinds: Vec<&str> = said.iter().map(|l| kind(l)).collect();
    let reference_kinds: Vec<&str> = reference.iter().map(|l| kind(l)).collect();
    // an error is three lines in the reference: the notice, the exception
    // and the text; its kinds are "error", "other", "other"
    assert_eq!(said_kinds, reference_kinds, "{path}: {said:?}");
    for (mine, theirs) in said.iter().zip(&reference) {
        match kind(theirs) {
            "added" | "starting" | "done" | "redirect" | "referral" | "error" => {
                assert_eq!(mine, *theirs, "{path}");
            }
            "domain headers" => {
                assert!(
                    mine.starts_with("Request Headers set by the network domain manager: "),
                    "{mine}"
                );
            }
            "headers" => {
                // the lists, sorted by name, request's then response's
                let (request, response) = mine.split_once("\n\nResponse Headers:\n").unwrap();
                let request = request.strip_prefix("Request Headers:\n").unwrap();
                for block in [request, response] {
                    let lines: Vec<&str> = block.lines().collect();
                    assert!(lines.iter().all(|l| l.contains(": ")), "{mine}");
                    let mut sorted = lines.clone();
                    sorted.sort();
                    assert_eq!(lines, sorted, "{mine}");
                }
                assert!(theirs.contains("\n\nResponse Headers:\n"));
            }
            _ => {}
        }
    }
    if path == "/bad" {
        // the notice, the exception (which says 404) and the server's text
        let at = said.iter().position(|l| kind(l) == "error").unwrap();
        assert!(said[at + 1].contains("404"), "{said:?}");
        assert_eq!(said[at + 2], "no such thing");
    }
}

// leaf: audit-options-help-debug-action-network-report-mode
// leaf: audit-options-help-debug-action-network-report-mode-silent
#[tokio::test]
async fn network_report_mode_says_what_the_reference_says_unless_silent() {
    let seen: Lines = Arc::default();
    let console: Lines = Arc::default();
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |t| {
        sink.lock().unwrap().push(t.to_owned());
    })));
    let printed = console.clone();
    debug_flags::set_console(Some(Box::new(move |t| {
        printed.lock().unwrap().push(t.to_owned());
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
    let fetch = |path: &'static str| {
        let engine = &engine;
        let url = format!("{base}{path}");
        async move {
            let _ = engine.fetch(&Request::get(url), &Job::new()).await;
        }
    };
    let take = |lines: &Lines| std::mem::take(&mut *lines.lock().unwrap());
    let off = recorded("off");
    let loud = recorded("on");
    let silent = recorded("silent");
    assert!(off.iter().all(|(_, lines)| lines.is_empty()));
    assert!(loud.iter().all(|(_, lines)| lines.len() >= 6));
    assert!(silent.iter().all(|(_, lines)| lines.len() >= 6));

    // off: nothing, anywhere
    for (path, _) in &off {
        fetch(Box::leak(path.clone().into_boxed_str())).await;
    }
    assert!(take(&seen).is_empty() && take(&console).is_empty());

    // the plain mode: the popup sink says everything the reference said
    Flag::NetworkReport.set(true);
    for (path, reference) in &loud {
        fetch(Box::leak(path.clone().into_boxed_str())).await;
        same_reports(path, &take(&seen), reference, &base);
    }
    assert!(take(&console).is_empty(), "loud mode keeps to the popups");

    // silent: the same words, to the console only
    Flag::NetworkReportSilent.set(true);
    for (path, reference) in &silent {
        fetch(Box::leak(path.clone().into_boxed_str())).await;
        same_reports(path, &take(&console), reference, &base);
    }
    assert!(take(&seen).is_empty(), "silent mode keeps popups out");
    Flag::NetworkReport.set(false);
    Flag::NetworkReportSilent.set(false);
    debug_flags::set_sink(None);
    debug_flags::set_console(None);
}
