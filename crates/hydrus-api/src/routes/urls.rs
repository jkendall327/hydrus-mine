//! Looking up URLs: what kind of URL something is, and which files it has
//! been seen for.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use hydrus_core::TimestampMs;
use hydrus_core::numbers::human_int;
use hydrus_store::urls::{self, FileState};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::request::{ApiRequest, ApiResponse};

/// Import statuses, with the reference's codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportStatus {
    Unknown = 0,
    SuccessfulButRedundant = 2,
    Deleted = 3,
}

fn required_url(req: &ApiRequest) -> ApiResult<String> {
    let url = req.params.required::<String>("url")?;
    if url.is_empty() {
        return Err(ApiError::bad_request("Given URL was empty!"));
    }
    Ok(url)
}

pub async fn get_url_info(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddUrls)?;
    let url = required_url(&req)?;
    let snapshot = app.store.snapshot();
    let classes = &snapshot.url_classes;
    let normalised = classes
        .normalise(&url, false)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let capability = classes.parse_capability(&normalised);
    let url_type_string = capability.url_type.name().ok_or_else(|| {
        ApiError::server(format!(
            "url type {} has no name",
            capability.url_type.code()
        ))
    })?;
    let request_url = classes
        .url_to_fetch(&normalised)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut body = json!({
        "normalised_url": normalised,
        "url_type": capability.url_type.code(),
        "url_type_string": url_type_string,
        "match_name": capability.match_name,
        "can_parse": capability.parser.is_ok(),
        "request_url": request_url,
    });
    if let Err(reason) = capability.parser {
        body["cannot_parse_reason"] = json!(reason);
    }
    Ok(ApiResponse::json(body, &req))
}

pub async fn get_url_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddUrls)?;
    let url = required_url(&req)?;
    let doublecheck = req.params.or("doublecheck_file_system", false)?;
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let normalised = snapshot
                .url_classes
                .normalise(&url, false)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            let search = snapshot.url_classes.search_urls(&normalised);
            let files = app
                .store
                .read(|conn| urls::files_for_urls(conn, &snapshot.services, &search))?;
            let now = TimestampMs::now();
            let statuses: Vec<Json> = files
                .into_iter()
                .map(|file| {
                    let (mut status, mut note) = describe(&file.state, now);
                    if doublecheck
                        && let FileState::Imported {
                            mime: Some(mime), ..
                        } = file.state
                        && !snapshot
                            .storage
                            .file_path(&file.hash, mime)
                            .is_some_and(|p| p.is_file())
                    {
                        status = ImportStatus::Unknown;
                        note = "The client believed this file was already in the db, but it was truly missing! Import will go ahead, in an attempt to fix the situation.".into();
                    }
                    json!({
                        "status": status as u8,
                        "hash": file.hash.to_hex(),
                        "note": note,
                    })
                })
                .collect();
            Ok(ApiResponse::Json(
                json!({ "normalised_url": normalised, "url_file_statuses": statuses }),
                encoding,
            ))
        })
        .await
}

/// A file's status and the human-readable note the reference gives it.
fn describe(state: &FileState, now: TimestampMs) -> (ImportStatus, String) {
    const PREFIX: &str = "url recognised: ";
    match state {
        FileState::Deleted {
            deleted: None,
            reason,
        } => (
            ImportStatus::Deleted,
            format!("{PREFIX}Deleted from the client before delete times were tracked ({reason})."),
        ),
        FileState::Deleted {
            deleted: Some(at),
            reason,
        } => (
            ImportStatus::Deleted,
            format!(
                "{PREFIX}Deleted from the client {} ({reason}), which was {} before this check.",
                pretty_time(*at),
                pretty_delta(*at, now, 3)
            ),
        ),
        FileState::InTrash { trashed, reason } => (
            ImportStatus::Deleted,
            format!(
                "{PREFIX}Currently in trash ({reason}). Sent there at {}, which was {} before this check.",
                pretty_time(*trashed),
                pretty_delta(*trashed, now, 0)
            ),
        ),
        FileState::Imported { imported, .. } => (
            ImportStatus::SuccessfulButRedundant,
            format!(
                "{PREFIX}Imported at {}, which was {} before this check.",
                pretty_time(*imported),
                pretty_delta(*imported, now, 0)
            ),
        ),
        FileState::Unknown => (ImportStatus::Unknown, String::new()),
    }
}

/// Seconds since the epoch, as the reference rounds them.
fn seconds(t: TimestampMs) -> i64 {
    t.0.div_euclid(1000)
}

/// `YYYY-MM-DD HH:MM:SS`, in UTC (the reference uses the machine's local
/// time; see `docs/rust/DIFFERENCES.md`).
fn pretty_time(t: TimestampMs) -> String {
    let secs = seconds(t);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// e.g. `43 minutes 52 seconds ago`, as the reference words time deltas.
fn pretty_delta(t: TimestampMs, now: TimestampMs, just_now_threshold: i64) -> String {
    let (then, now) = (seconds(t), seconds(now));
    let delta = (then - now).abs();
    if delta <= just_now_threshold {
        return "now".into();
    }
    let text = delta_text(delta);
    if now > then {
        format!("{text} ago")
    } else {
        format!("in {text}")
    }
}

fn delta_text(seconds: i64) -> String {
    if seconds >= 60 {
        const MINUTE: f64 = 60.0;
        const HOUR: f64 = 60.0 * MINUTE;
        const DAY: f64 = 24.0 * HOUR;
        const YEAR: f64 = 365.25 * DAY;
        const MONTH: f64 = YEAR / 12.0;
        let mut rest = seconds as f64;
        let mut parts: Vec<String> = Vec::new();
        for (name, unit) in [
            ("year", YEAR),
            ("month", MONTH),
            ("day", DAY),
            ("hour", HOUR),
            ("minute", MINUTE),
            ("second", 1.0),
        ] {
            let mut quantity = (rest / unit).floor();
            rest %= unit;
            if name == "month" && quantity > 11.0 {
                quantity = 11.0;
            }
            if quantity > 0.0 {
                let n = quantity as u64;
                parts.push(format!(
                    "{} {name}{}",
                    human_int(n),
                    if n > 1 { "s" } else { "" }
                ));
                if parts.len() == 2 {
                    break;
                }
            } else if !parts.is_empty() {
                break;
            }
        }
        parts.join(" ")
    } else if seconds > 1 {
        format!("{seconds} seconds")
    } else {
        "1 second".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_like_the_reference() {
        assert_eq!(
            pretty_time(TimestampMs(1_790_707_134_999)),
            "2026-09-29 18:38:54"
        );
        assert_eq!(pretty_time(TimestampMs(0)), "1970-01-01 00:00:00");
        assert_eq!(delta_text(43 * 60 + 52), "43 minutes 52 seconds");
        assert_eq!(delta_text(3600 + 5), "1 hour");
        assert_eq!(delta_text(2 * 86_400 + 3600), "2 days 1 hour");
        assert_eq!(delta_text(400 * 86_400), "1 year 1 month");
        assert_eq!(delta_text(5), "5 seconds");
        let now = TimestampMs(10_000_000);
        assert_eq!(
            pretty_delta(TimestampMs(9_000_000), now, 0),
            "16 minutes 40 seconds ago"
        );
        assert_eq!(pretty_delta(TimestampMs(9_999_000), now, 3), "now");
    }
}
