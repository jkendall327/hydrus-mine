//! `/add_notes/*`, `/edit_ratings/*` and `/edit_times/*`: editing what is
//! attached to files.

use crate::auth::PermissionChecks as _;
use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use hydrus_core::content::{CanvasType, TimestampType};
use hydrus_core::notes::{NoteConflict, NoteMerge};
use hydrus_core::{HashId, ServiceKey, ServiceType, Sha256};
use hydrus_store::content::FileTime;
use hydrus_store::master;
use hydrus_store::services::ServiceKind;
use hydrus_store::settings::{self, FileViewingStatistics};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::location::check_file_service;
use crate::params::{Nullable, Params, Value};
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

fn hashes_or(app: &AppState, params: &Params, missing: &str) -> ApiResult<Vec<Sha256>> {
    match parse_hashes(app, params)? {
        Some(h) if !h.is_empty() => Ok(h),
        _ => Err(ApiError::bad_request(missing.to_owned())),
    }
}

/// The one file a notes request is about.
fn one_hash(app: &AppState, params: &Params) -> ApiResult<Sha256> {
    match parse_hashes(app, params)?.as_deref() {
        Some([hash]) => Ok(*hash),
        _ => Err(ApiError::bad_request(
            "There was no file identifier or hash given!",
        )),
    }
}

fn intern_all(conn: &rusqlite::Connection, hashes: &[Sha256]) -> hydrus_store::Result<Vec<HashId>> {
    hashes
        .iter()
        .map(|h| master::intern_hash(conn, h))
        .collect()
}

pub async fn set_notes(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddNotes)?;
    let params = req.params.clone();
    let notes = app
        .blocking(move |app| {
            let hash = one_hash(app, &params)?;
            let bad_notes = || {
                ApiError::bad_request(
                    "The parameter \"notes\" was not a dict of strings to strings!",
                )
            };
            let Json::Object(given) = params.required::<Json>("notes")? else {
                return Err(bad_notes());
            };
            let incoming: Vec<(String, String)> = given
                .into_iter()
                .map(|(name, note)| match note {
                    Json::String(note) => Ok((name, note)),
                    _ => Err(bad_notes()),
                })
                .collect::<ApiResult<_>>()?;
            let merge = if params.or("merge_cleverly", false)? {
                let code = params.or("conflict_resolution", NoteConflict::Rename as i64)?;
                let conflict = NoteConflict::from_code(code).ok_or_else(|| {
                    ApiError::bad_request(
                        "The given conflict resolution type was not in the allowed range!",
                    )
                })?;
                Some(NoteMerge {
                    extend_existing: params.or("extend_existing_note_if_possible", true)?,
                    conflict,
                })
            } else {
                None
            };
            Ok(app.store.write_content(move |w| {
                let id = master::intern_hash(w.conn(), &hash)?;
                let updates: BTreeMap<String, String> = match merge {
                    Some(merge) => merge.merge(&w.notes(id)?, &incoming),
                    None => incoming.into_iter().collect(),
                };
                for (name, note) in &updates {
                    w.set_note(id, name, note)?;
                }
                Ok(updates)
            })?)
        })
        .await?;
    Ok(ApiResponse::json(json!({ "notes": notes }), &req))
}

pub async fn delete_notes(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddNotes)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let hash = one_hash(app, &params)?;
        let names: Vec<String> = params.required("note_names")?;
        app.store.write_content(move |w| {
            let id = master::intern_hash(w.conn(), &hash)?;
            for name in &names {
                w.delete_note(id, name)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

/// A stored rating change.
enum RatingChange {
    /// Like/dislike or numerical, as a fraction; `None` clears.
    Fraction(Option<f64>),
    IncDec(i64),
}

/// A rating as the Client API sends it.
enum ApiRating {
    Null,
    Bool(bool),
    Int(i64),
    Other,
}

pub async fn set_rating(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::EditRatings)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let key: Vec<u8> = params.required("rating_service_key")?;
        let hashes = hashes_or(
            app,
            &params,
            "Did not find any hashes to apply the ratings to!",
        )?;
        let rating = match params.raw_or_null("rating") {
            None => {
                return Err(ApiError::bad_request(
                    "Sorry, you need to give a rating to set it to!",
                ));
            }
            Some(Value::Json(Json::Null)) => ApiRating::Null,
            Some(Value::Json(Json::Bool(b))) => ApiRating::Bool(*b),
            Some(Value::Int(i)) => ApiRating::Int(*i),
            Some(Value::Json(Json::Number(n))) => {
                n.as_i64().map_or(ApiRating::Other, ApiRating::Int)
            }
            Some(_) => ApiRating::Other,
        };
        let snap = app.store.snapshot();
        let service = snap
            .services
            .by_key(&ServiceKey::new(key.clone()))
            .map_err(|_| {
                ApiError::data_missing(format!(
                    "Service with key \"{}\" not found!",
                    hex::encode(&key)
                ))
            })?
            .clone();
        let wrong = |expected: &str| {
            ApiError::bad_request(format!(
                "Sorry, this service expects a \"{expected}\" rating!"
            ))
        };
        let change = match (&service.kind, rating) {
            (ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_), ApiRating::Null) => {
                RatingChange::Fraction(None)
            }
            (ServiceKind::RatingLike(_), ApiRating::Bool(b)) => {
                RatingChange::Fraction(Some(if b { 1.0 } else { 0.0 }))
            }
            (ServiceKind::RatingLike(_), _) => return Err(wrong("bool")),
            (ServiceKind::RatingNumerical(c), ApiRating::Int(stars)) => {
                let stars = u32::try_from(stars.clamp(0, i64::from(c.num_stars))).unwrap_or(0);
                RatingChange::Fraction(Some(c.rating(stars.max(c.min_stars()))))
            }
            (ServiceKind::RatingIncDec(_), ApiRating::Null) => {
                return Err(ApiError::bad_request(
                    "Sorry, this service does not allow a null rating!",
                ));
            }
            (ServiceKind::RatingIncDec(_), ApiRating::Int(value)) => {
                RatingChange::IncDec(value.max(0))
            }
            (ServiceKind::RatingNumerical(_) | ServiceKind::RatingIncDec(_), _) => {
                return Err(wrong("int"));
            }
            _ => {
                return Err(ApiError::bad_request(
                    "That service is not a rating service!",
                ));
            }
        };
        let service = service.id;
        app.store.write_content(move |w| {
            let ids = intern_all(w.conn(), &hashes)?;
            match change {
                RatingChange::Fraction(r) => w.set_rating(service, &ids, r),
                RatingChange::IncDec(r) => w.set_incdec(service, &ids, r),
            }
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

/// `timestamp` (float seconds) or `timestamp_ms`, in milliseconds.
fn given_time(params: &Params) -> ApiResult<Nullable<i64>> {
    match params.nullable::<f64>("timestamp")? {
        // the reference truncates
        Nullable::Absent => params.nullable::<i64>("timestamp_ms"),
        secs => Ok(secs.map(|s| (s * 1000.0) as i64)),
    }
}

fn viewing_canvas(code: Option<i64>) -> ApiResult<CanvasType> {
    match code
        .and_then(|c| u8::try_from(c).ok())
        .and_then(CanvasType::from_code)
    {
        Some(c @ (CanvasType::MediaViewer | CanvasType::Preview | CanvasType::ClientApi)) => Ok(c),
        _ => Err(ApiError::bad_request(
            "Sorry, the canvas type needs to be either 0, 1, or 4!",
        )),
    }
}

pub async fn set_time(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::EditTimes)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let hashes = hashes_or(app, &params, "Did not find any hashes to apply the times to!")?;
        let ms = match given_time(&params)? {
            Nullable::Absent => {
                return Err(ApiError::bad_request(
                    "Sorry, you have to specify a timestamp, even if you want to send \"null\"!",
                ));
            }
            given => given.value(),
        };
        let code: i64 = params.required("timestamp_type")?;
        let kind = u8::try_from(code).ok().and_then(TimestampType::from_code);
        let snap = app.store.snapshot();
        let file_domain = || -> ApiResult<hydrus_core::ServiceId> {
            let key: Vec<u8> = params.required("file_service_key")?;
            let domain = check_file_service(&snap, &key)
                .map_err(|_| ApiError::bad_request("Sorry, do not know that service!"))?;
            if snap.services.get(domain)?.service_type() == ServiceType::CombinedFile {
                return Err(ApiError::bad_request("Sorry, you have to specify a file service service key!"));
            }
            Ok(domain)
        };
        let time = match kind {
            Some(TimestampType::ModifiedDomain) => {
                let domain: String = params.required("domain")?;
                if domain == "local" {
                    FileTime::FileModified
                } else {
                    FileTime::DomainModified(domain)
                }
            }
            Some(TimestampType::LastViewed) => {
                let canvas = params.nullable::<i64>("canvas_type")?.value();
                FileTime::LastViewed(viewing_canvas(Some(canvas.unwrap_or(0)))?)
            }
            Some(TimestampType::Imported) => FileTime::Imported(file_domain()?),
            Some(TimestampType::Deleted) => FileTime::Deleted(file_domain()?),
            Some(TimestampType::PreviouslyImported) => FileTime::PreviouslyImported(file_domain()?),
            Some(TimestampType::ModifiedFile) => FileTime::FileModified,
            Some(TimestampType::Archived) => FileTime::Archived,
            _ => {
                return Err(ApiError::bad_request(format!(
                    "Sorry, do not understand that timestamp type \"{code}\"!"
                )));
            }
        };
        let domain_time = matches!(time, FileTime::DomainModified(_));
        if !domain_time && ms.is_none() {
            return Err(ApiError::bad_request(format!(
                "Sorry, you can only delete web domain timestamps (type 0) for now! You sent ({code})!"
            )));
        }
        app.store.write_content(move |w| {
            let ids = intern_all(w.conn(), &hashes)?;
            if !domain_time {
                // only existing timestamps can be edited
                for (hash, &id) in hashes.iter().zip(&ids) {
                    if w.file_time(id, &time)?.is_none() {
                        return Err(hydrus_store::StoreError::Invalid(format!(
                            "Sorry, if the timestamp type is other than 0 (web domain), then you cannot add new timestamps, only edit existing ones. I did not see the given timestamp type on one of the files you sent, specifically: {hash}"
                        )));
                    }
                }
            }
            match (ms, &time) {
                (Some(ms), time) => w.set_file_time(&ids, time, ms),
                (None, FileTime::DomainModified(domain)) => w.clear_domain_modified_time(&ids, domain),
                (None, _) => unreachable!("checked above"),
            }
        })
        .map_err(|e| match e {
            hydrus_store::StoreError::Invalid(message) => ApiError::bad_request(message),
            other => other.into(),
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

/// `increment_file_viewtime` and `set_file_viewtime`.
async fn edit_views(
    app: Arc<AppState>,
    req: ApiRequest,
    increment: bool,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::EditTimes)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        if !app
            .store
            .read(settings::get::<FileViewingStatistics>)?
            .active
        {
            return Err(ApiError::forbidden(
                "Sorry, the user has disabled file viewing statistics on this client!",
            ));
        }
        let canvas = viewing_canvas(Some(params.required("canvas_type")?))?;
        let viewed_ms = given_time(&params)?;
        let views: i64 = if increment {
            params.or("views", 1)?
        } else {
            params.required("views")?
        };
        let viewtime: f64 = params.required("viewtime")?;
        if views < 0 {
            return Err(ApiError::bad_request("Views cannot be a negative number!"));
        }
        if viewtime < 0.0 {
            return Err(ApiError::bad_request(
                "Viewtime cannot be a negative number!",
            ));
        }
        let viewtime_ms = (viewtime * 1000.0) as i64;
        let hashes = hashes_or(
            app,
            &params,
            "Did not find any hashes to apply the viewtime statistics to!",
        )?;
        app.store.write_content(move |w| {
            let now = w.now_ms();
            for id in intern_all(w.conn(), &hashes)? {
                if increment {
                    // no time given means now
                    let viewed = match viewed_ms {
                        Nullable::Absent => Some(now),
                        given => given.value(),
                    };
                    w.add_views(id, canvas, viewed, views, viewtime_ms)?;
                } else {
                    w.set_views(id, canvas, viewed_ms.value(), views, viewtime_ms)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

pub async fn increment_file_viewtime(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit_views(app, req, true).await
}

pub async fn set_file_viewtime(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit_views(app, req, false).await
}
