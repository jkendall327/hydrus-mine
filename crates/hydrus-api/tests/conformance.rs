//! Client API conformance: replay the requests recorded against the
//! reference client (`oracle/recordings/`) and compare responses.
//!
//! Every endpoint marked `done` in `parity/manifest.toml` must match on every
//! recorded step. Other endpoints are reported but may fail. Run with
//! `CONFORMANCE_VERBOSE=1` to print each mismatch.
//!
//! Normalisation (the reviewed list of what we don't compare):
//! - error responses compare status and `exception_type`, not message text;
//! - absolute paths of the db dir and media dir are placeholders;
//! - steps marked `unordered_lists` compare arrays as multisets;
//! - in scenarios that write, times from while the scenario ran (a deletion
//!   time, an archive time, ...) are `{NOW}` on both sides, recognised by a
//!   time-like key and a value at or after the scenario's start;
//! - `known_differences.toml` lists recorded reference bugs we don't copy.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request};
use http_body_util::BodyExt as _;
use serde_json::Value as Json;
use tower::ServiceExt as _;

mod common;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn media_dir() -> PathBuf {
    repo_root().join("oracle/fixtures/import_media")
}

fn substitute(value: &Json, media: &str) -> Json {
    match value {
        Json::String(s) => Json::String(s.replace("{MEDIA}", media)),
        Json::Array(items) => Json::Array(items.iter().map(|v| substitute(v, media)).collect()),
        Json::Object(map) => Json::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), substitute(v, media)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn unsubstitute(value: Json, db_dir: &str, media: &str) -> Json {
    match value {
        Json::String(s) => Json::String(s.replace(db_dir, "{DB_DIR}").replace(media, "{MEDIA}")),
        Json::Array(items) => Json::Array(
            items
                .into_iter()
                .map(|v| unsubstitute(v, db_dir, media))
                .collect(),
        ),
        Json::Object(map) => Json::Object(
            map.into_iter()
                .map(|(k, v)| {
                    (
                        k.replace(db_dir, "{DB_DIR}"),
                        unsubstitute(v, db_dir, media),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// A recorded response we deliberately don't match (`known_differences.toml`).
#[derive(Debug, serde::Deserialize)]
struct KnownDifference {
    scenario: String,
    step: usize,
    ignore: Vec<String>,
    #[allow(dead_code)] // documentation for reviewers
    reason: String,
}

fn known_differences() -> Vec<KnownDifference> {
    #[derive(serde::Deserialize)]
    struct File {
        difference: Vec<KnownDifference>,
    }
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/known_differences.toml"),
    )
    .unwrap();
    toml::from_str::<File>(&text).unwrap().difference
}

/// Remove the value at `path` (`$.a.b[0].c`) if present.
fn remove_path(value: &mut Json, path: &str) {
    let mut segments = Vec::new();
    for part in path
        .trim_start_matches('$')
        .split('.')
        .filter(|p| !p.is_empty())
    {
        let (name, indices) = part
            .split_once('[')
            .map_or((part, ""), |(n, rest)| (n, rest));
        if !name.is_empty() {
            segments.push(Json::String(name.to_owned()));
        }
        for index in indices.split('[').filter(|i| !i.is_empty()) {
            segments.push(Json::from(
                index.trim_end_matches(']').parse::<u64>().unwrap(),
            ));
        }
    }
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut current = value;
    for segment in parents {
        let next = match segment {
            Json::String(key) => current.get_mut(key.as_str()),
            index => current.get_mut(index.as_u64().unwrap() as usize),
        };
        match next {
            Some(v) => current = v,
            None => return,
        }
    }
    match (current, last) {
        (Json::Object(map), Json::String(key)) => {
            map.remove(key);
        }
        (Json::Array(items), index) => {
            let i = index.as_u64().unwrap() as usize;
            if i < items.len() {
                items.remove(i);
            }
        }
        _ => {}
    }
}

/// Replace times at or after `start_ms` under time-like keys with `{NOW}`.
fn normalise_now(value: &mut Json, start_ms: i64) {
    const SLACK_MS: i64 = 2_000;
    const DAY_MS: i64 = 86_400_000;
    let is_now = |v: &Json| {
        v.as_f64().is_some_and(|n| {
            // seconds or milliseconds
            let lo = (start_ms - SLACK_MS) as f64;
            let hi = (start_ms + DAY_MS) as f64;
            (lo..hi).contains(&n) || (lo / 1000.0..hi / 1000.0).contains(&n)
        })
    };
    match value {
        Json::Array(items) => items.iter_mut().for_each(|v| normalise_now(v, start_ms)),
        Json::Object(map) => {
            for (key, v) in map.iter_mut() {
                if (key.contains("time") || key.contains("viewed")) && is_now(v) {
                    *v = Json::String("{NOW}".into());
                } else {
                    normalise_now(v, start_ms);
                }
            }
        }
        _ => {}
    }
}

/// Sort every array, for comparing collections whose order is unspecified.
fn sort_arrays(value: &mut Json) {
    match value {
        Json::Array(items) => {
            items.iter_mut().for_each(sort_arrays);
            items.sort_by_key(ToString::to_string);
        }
        Json::Object(map) => map.values_mut().for_each(sort_arrays),
        _ => {}
    }
}

/// First path at which two JSON values differ.
fn first_difference(expected: &Json, actual: &Json, path: &str) -> Option<String> {
    match (expected, actual) {
        (Json::Object(want), Json::Object(got)) => {
            for key in want.keys().chain(got.keys()) {
                match (want.get(key), got.get(key)) {
                    (Some(w), Some(g)) => {
                        if let Some(d) = first_difference(w, g, &format!("{path}.{key}")) {
                            return Some(d);
                        }
                    }
                    (w, g) => {
                        return Some(format!("{path}.{key}: expected {} got {}", fmt(w), fmt(g)));
                    }
                }
            }
            None
        }
        (Json::Array(want), Json::Array(got)) if want.len() == got.len() => want
            .iter()
            .zip(got)
            .enumerate()
            .find_map(|(i, (w, g))| first_difference(w, g, &format!("{path}[{i}]"))),
        _ if expected == actual => None,
        _ => Some(format!(
            "{path}: expected {} got {}",
            truncate(&expected.to_string()),
            truncate(&actual.to_string())
        )),
    }
}

fn fmt(v: Option<&Json>) -> String {
    v.map_or_else(|| "(missing)".into(), |v| truncate(&v.to_string()))
}

fn truncate(s: &str) -> String {
    if s.len() > 200 {
        format!(
            "{}…",
            &s[..s.char_indices().nth(200).map_or(s.len(), |(i, _)| i)]
        )
    } else {
        s.to_owned()
    }
}

struct Outcome {
    endpoint: String,
    passed: bool,
    detail: String,
}

async fn replay_step(
    router: &axum::Router,
    step: &Json,
    recorded: &Json,
    manifest: &Json,
    db_dir: &str,
    // (recording started, replay started) for scenarios that write
    now_window: Option<(i64, i64)>,
    ignored_paths: &[String],
) -> Outcome {
    let media = media_dir().to_string_lossy().into_owned();
    let path = step["path"].as_str().unwrap().to_owned();
    let mut uri = path.clone();
    if let Some(query) = step["query"].as_array().filter(|q| !q.is_empty()) {
        let mut encoder = form_urlencoded::Serializer::new(String::new());
        for pair in query {
            let value = pair[1].as_str().unwrap().replace("{MEDIA}", &media);
            encoder.append_pair(pair[0].as_str().unwrap(), &value);
        }
        uri = format!("{uri}?{}", encoder.finish());
    }
    let method: Method = step["method"].as_str().unwrap().parse().unwrap();
    let mut builder = Request::builder().method(method).uri(&uri);
    match step["key"].as_str().unwrap_or("full") {
        "none" => {}
        spec => {
            let key = manifest["access_keys"][spec].as_str().unwrap_or(spec);
            builder = builder.header("Hydrus-Client-API-Access-Key", key);
        }
    }
    let body = if let Some(json) = step.get("json") {
        builder = builder.header("Content-Type", "application/json");
        Body::from(substitute(json, &media).to_string())
    } else if let Some(file) = step["body_file"].as_str() {
        builder = builder.header("Content-Type", "application/octet-stream");
        Body::from(std::fs::read(repo_root().join("oracle/fixtures").join(file)).unwrap())
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or("").trim().to_owned())
        .unwrap_or_default();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();

    let expected_status = recorded["status"].as_u64().unwrap() as u16;
    let mut problems = Vec::new();
    if status != expected_status {
        problems.push(format!(
            "status: expected {expected_status} got {status} ({})",
            truncate(&String::from_utf8_lossy(&bytes))
        ));
    }
    let expected_type = recorded["content_type"].as_str().unwrap_or("");
    if content_type != expected_type {
        problems.push(format!(
            "content type: expected {expected_type} got {content_type}"
        ));
    }
    if let Some(expected) = recorded.get("json") {
        match serde_json::from_slice::<Json>(&bytes) {
            Ok(actual) => {
                let mut actual = unsubstitute(actual, db_dir, &media);
                let mut expected = expected.clone();
                if expected.get("exception_type").is_some() {
                    // errors: compare the class, not the wording
                    for v in [&mut actual, &mut expected] {
                        if let Json::Object(m) = v {
                            m.remove("error");
                        }
                    }
                }
                for path in ignored_paths {
                    remove_path(&mut expected, path);
                    remove_path(&mut actual, path);
                }
                if let Some((recorded_start, replay_start)) = now_window {
                    normalise_now(&mut expected, recorded_start);
                    normalise_now(&mut actual, replay_start);
                }
                if step["compare"].as_str() == Some("unordered_lists") {
                    sort_arrays(&mut actual);
                    sort_arrays(&mut expected);
                }
                if let Some(d) = first_difference(&expected, &actual, "$") {
                    problems.push(d);
                }
            }
            Err(e) => problems.push(format!("body is not json: {e}")),
        }
    } else if let Some(expected_hash) = recorded["sha256"].as_str() {
        use sha2::Digest as _;
        let actual_hash = hex::encode(sha2::Sha256::digest(&bytes));
        if actual_hash != expected_hash {
            problems.push(format!("body sha256 differs ({} bytes)", bytes.len()));
        }
    } else if let Some(expected_text) = recorded["text"].as_str()
        && String::from_utf8_lossy(&bytes) != expected_text
    {
        problems.push("text body differs".into());
    }
    Outcome {
        endpoint: path,
        passed: problems.is_empty(),
        detail: format!("{} {uri}: {}", step["method"], problems.join("; ")),
    }
}

fn done_endpoints() -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join("parity/manifest.toml")).unwrap();
    let manifest: toml::Value = toml::from_str(&text).unwrap();
    manifest["api"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["status"].as_str() == Some("done"))
        .map(|e| e["path"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replay_recorded_scenarios() {
    let root = repo_root();
    let manifest: Json = serde_json::from_str(
        &std::fs::read_to_string(root.join("oracle/fixtures/legacy_db/basic.manifest.json"))
            .unwrap(),
    )
    .unwrap();
    let mut names: Vec<String> = std::fs::read_dir(root.join("oracle/scenarios"))
        .unwrap()
        .map(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".json")
                .to_owned()
        })
        .collect();
    names.sort();

    let differences = known_differences();
    let shared = common::imported_store("basic");
    let mut outcomes = Vec::new();
    for name in &names {
        let scenario: Json = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("oracle/scenarios/{name}.json"))).unwrap(),
        )
        .unwrap();
        let recording: Json = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("oracle/recordings/{name}.json"))).unwrap(),
        )
        .unwrap();
        let fresh;
        let read_only = scenario["read_only"].as_bool().unwrap();
        let fixture = if read_only {
            &shared
        } else {
            fresh = common::imported_store("basic");
            &fresh
        };
        let now_window = (!read_only).then(|| {
            (
                recording["recorded_at_ms"].as_i64().unwrap(),
                hydrus_core::time::TimestampMs::now().millis(),
            )
        });
        let router = hydrus_api::router(Arc::clone(&fixture.state));
        let db_dir = fixture.legacy_dir.path().to_string_lossy().into_owned();
        for (index, (step, recorded)) in scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(recording["responses"].as_array().unwrap())
            .enumerate()
        {
            let ignored: Vec<String> = differences
                .iter()
                .filter(|d| &d.scenario == name && d.step == index)
                .flat_map(|d| d.ignore.iter().cloned())
                .collect();
            outcomes.push(
                replay_step(
                    &router, step, recorded, &manifest, &db_dir, now_window, &ignored,
                )
                .await,
            );
        }
    }

    let done = done_endpoints();
    let mut per_endpoint: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut regressions = Vec::new();
    let verbose = std::env::var_os("CONFORMANCE_VERBOSE").is_some();
    for o in &outcomes {
        let entry = per_endpoint.entry(o.endpoint.clone()).or_default();
        entry.1 += 1;
        if o.passed {
            entry.0 += 1;
        } else {
            if done.contains(&o.endpoint) {
                regressions.push(o.detail.clone());
            }
            if verbose {
                eprintln!("MISMATCH {}", o.detail);
            }
        }
    }
    let passed = outcomes.iter().filter(|o| o.passed).count();
    eprintln!(
        "conformance: {passed}/{} recorded steps match the reference",
        outcomes.len()
    );
    for (endpoint, (ok, total)) in &per_endpoint {
        let mark = if done.contains(endpoint) {
            "done"
        } else {
            "    "
        };
        eprintln!("  {mark} {ok:>4}/{total:<4} {endpoint}");
    }
    // write a machine-readable summary for the ratchet
    let summary: HashMap<&String, (usize, usize)> =
        per_endpoint.iter().map(|(k, v)| (k, *v)).collect();
    let _ = std::fs::write(
        root.join("target/conformance.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    );
    assert!(
        regressions.is_empty(),
        "endpoints marked done no longer conform:\n{}",
        regressions.join("\n")
    );
}
