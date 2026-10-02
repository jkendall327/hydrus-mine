//! Popup messages (`/manage_popups/*`) as the Client API reports them: the
//! queue itself is the store's (`hydrus_store::popups`), which the client
//! shows.

use serde_json::{Map, Value as Json, json};

use hydrus_core::Sha256;
pub use hydrus_store::popups::Job;

/// Seconds since the epoch.
pub fn now_seconds() -> f64 {
    hydrus_core::time::TimestampMs::now().millis() as f64 / 1000.0
}

/// Whole seconds since the epoch.
pub fn now_whole() -> i64 {
    now_seconds().floor() as i64
}

/// A popup as the Client API reports it (`JobStatusToDict`).
pub fn to_json(job: &Job) -> Json {
    let mut out = Map::new();
    let mut put = |k: &str, v: Json| {
        if !v.is_null() {
            out.insert(k.to_owned(), v);
        }
    };
    let gauge = |g: Option<(i64, i64)>| g.map_or(Json::Null, |(v, r)| json!([v, r]));
    put("key", json!(hex::encode(job.key)));
    put("creation_time", json!(job.creation_time));
    put("status_title", json!(job.status_title));
    put("status_text_1", json!(job.status_text_1));
    put("status_text_2", json!(job.status_text_2));
    put("had_error", json!(job.had_error));
    put("is_cancellable", json!(job.cancellable));
    put("is_cancelled", json!(job.cancelled));
    put("is_done", json!(job.done));
    put("is_pausable", json!(job.pausable));
    put("is_paused", json!(job.paused));
    put("nice_string", json!(job.nice_string()));
    put("popup_gauge_1", gauge(job.popup_gauge_1));
    put("popup_gauge_2", gauge(job.popup_gauge_2));
    if job.attached_files_mergable {
        put("attached_files_mergable", json!(true));
    }
    put("api_data", job.api_data.clone().unwrap_or(Json::Null));
    if let Some((hashes, label)) = &job.files {
        put(
            "files",
            json!({
                "hashes": hashes.iter().map(Sha256::to_hex).collect::<Vec<_>>(),
                "label": label,
            }),
        );
    }
    if let Some(traceback) = &job.traceback {
        put("traceback", json!(traceback));
    }
    Json::Object(out)
}
