//! `/manage_database/*`: library statistics, and locking the database so it
//! can be backed up.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use hydrus_core::service::builtin_keys;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, LocationContext, Predicate, SortBy, SortOrder, TagContext,
    search_files as run_search,
};
use hydrus_store::stats::{self, FileSet};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::search::{parse_location, parse_search_tags, parse_tag_service, search_error};

const LOCK_OFF: &str = "/manage_database/lock_off";

/// While the database is locked, every request but unlocking it is refused,
/// before it is even authenticated (as in the reference).
pub async fn refuse_while_locked(
    State(app): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if app.locked.load(Ordering::SeqCst) && request.uri().path() != LOCK_OFF {
        return ApiError::new(
            ErrorKind::ServerBusy,
            "This server is busy, please try again later.",
        )
        .into_response();
    }
    next.run(request).await
}

pub async fn lock_on(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageDatabase)?;
    app.clone()
        .blocking(|app| {
            let mut paused = app.paused.lock();
            if app.locked.swap(true, Ordering::SeqCst) {
                return Err(ApiError::bad_request("The client was already locked!"));
            }
            match app.store.pause() {
                Ok(guard) => {
                    *paused = Some(guard);
                    Ok(ApiResponse::Empty)
                }
                Err(e) => {
                    app.locked.store(false, Ordering::SeqCst);
                    Err(e.into())
                }
            }
        })
        .await
}

pub async fn lock_off(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageDatabase)?;
    app.clone()
        .blocking(|app| {
            // waits for a lock still being taken
            let mut paused = app.paused.lock();
            if !app.locked.load(Ordering::SeqCst) {
                return Err(ApiError::bad_request("The server is not busy!"));
            }
            drop(paused.take());
            app.locked.store(false, Ordering::SeqCst);
            Ok(ApiResponse::Empty)
        })
        .await
}

/// Every write is committed as it finishes; this waits for those queued.
pub async fn force_commit(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageDatabase)?;
    app.clone()
        .blocking(|app| {
            app.store.write(|_| Ok(()))?;
            Ok(ApiResponse::Empty)
        })
        .await
}

/// Counts, sizes, views and duplicates of the files a search finds
/// (`ClientDB._GetBonedStats`).
pub async fn mr_bones(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::ManageDatabase)?;
    let p = &req.params;
    let location = parse_location(&app, p, LocationContext::default())?;
    let tag_service = parse_tag_service(&app, p)?;
    if tag_service.as_bytes() == builtin_keys::COMBINED_TAG && location.is_all_known_files() {
        return Err(ApiError::bad_request(
            "Sorry, search for all known tags over all known files is not supported!",
        ));
    }
    let predicates = parse_search_tags(&perms, p)?;
    let just_everything = predicates.is_empty()
        || matches!(
            &predicates[..],
            [Predicate::System(
                hydrus_search::SystemPredicate::Everything
            )]
        );
    let search = FileSearchContext {
        location,
        tags: TagContext::new(tag_service, true, true),
        predicates,
    };
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let services = &snapshot.services;
            let id = |key: &hydrus_core::ServiceKey| services.by_key(key).map(|s| s.id);
            let find = |search: &FileSearchContext| -> ApiResult<FileSet> {
                let clock = Clock::system();
                let sort = FileSort {
                    by: SortBy::ImportTime,
                    order: SortOrder::Ascending,
                };
                let ids = app
                    .store
                    .read(|conn| Ok(run_search(conn, &snapshot, search, sort, &clock)))?
                    .map_err(search_error)?;
                Ok(FileSet::Files(ids))
            };

            let location = &search.location;
            let current = if just_everything {
                if location.is_all_known_files() {
                    return Err(ApiError::data_missing(
                        "Sorry, this DB Location Context has no single file table!",
                    ));
                }
                let mut domains = Vec::new();
                for key in location.current() {
                    domains.push((id(key)?, false));
                }
                for key in location.deleted() {
                    domains.push((id(key)?, true));
                }
                FileSet::Domains(domains)
            } else {
                find(&search)?
            };

            // a single domain of current files (not the trash or all known
            // files) also counts what was deleted from it
            let mut single = None;
            if let ([key], []) = (
                location.current().iter().collect::<Vec<_>>().as_slice(),
                location.deleted().iter().collect::<Vec<_>>().as_slice(),
            ) && key.as_bytes() != builtin_keys::TRASH
                && key.as_bytes() != builtin_keys::COMBINED_FILE
            {
                single = Some((*key).clone());
            }
            let (times, deleted) = match &single {
                None => (None, None),
                Some(key) => {
                    let service = id(key)?;
                    let deleted = if just_everything {
                        FileSet::Domains(vec![(service, true)])
                    } else {
                        let mut in_deleted = search.clone();
                        in_deleted.location = LocationContext::new(Vec::new(), [key.clone()]);
                        find(&in_deleted)?
                    };
                    (Some(service), Some(deleted))
                }
            };

            let s = app
                .store
                .read(|conn| stats::boned_stats(conn, &current, times, deleted.as_ref(), times))?;
            let mut boned = json!({
                "num_inbox": s.num_inbox,
                "num_archive": s.num_archive,
                "size_inbox": s.size_inbox,
                "size_archive": s.size_archive,
                "total_viewtime": [
                    s.media_views,
                    s.media_viewtime_ms as f64 / 1000.0,
                    s.preview_views,
                    s.preview_viewtime_ms as f64 / 1000.0,
                ],
                "total_duplicate_files": s.duplicate_files,
                "total_alternate_groups": s.alternate_groups,
                "total_alternate_files": s.alternate_files,
            });
            if let Some((num, size)) = s.deleted {
                boned["num_deleted"] = json!(num);
                boned["size_deleted"] = json!(size);
            }
            if let Some(ms) = s.earliest_import_ms {
                boned["earliest_import_time"] = json!(ms.div_euclid(1000));
            }
            Ok(ApiResponse::Json(json!({ "boned_stats": boned }), encoding))
        })
        .await
}
