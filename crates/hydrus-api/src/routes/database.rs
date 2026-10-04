//! `/manage_database/*`: library statistics, and locking the database so it
//! can be backed up.

use crate::auth::PermissionChecks as _;
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

/// The client's options: the old-style ones the reference still has, and
/// the options object, as migrated (with a new client's defaults for what
/// wasn't stored).
pub async fn get_client_options(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    use hydrus_legacy::objects::{ClientOptions, LegacyOptions};
    use hydrus_legacy::serialisable::{SerialisableObject, SerialisableType};

    app.authenticate(&req)?.check(Permission::ManageDatabase)?;
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let unreadable = |e: &dyn std::fmt::Display| {
                ApiError::server(format!("Could not read the client's options: {e}"))
            };
            let (stored, old) = app.store.read(|conn| {
                Ok((
                    hydrus_store::legacy::singleton(
                        conn,
                        u32::from(SerialisableType::CLIENT_OPTIONS.0),
                    )?,
                    hydrus_store::legacy::old_options(conn)?,
                ))
            })?;
            let defaults = ClientOptions::defaults().map_err(|e| unreadable(&e))?;
            let options = match stored {
                Some((version, dump)) => {
                    let object = SerialisableObject::from_stored(
                        SerialisableType::CLIENT_OPTIONS,
                        None,
                        version,
                        &dump,
                    )
                    .map_err(|e| unreadable(&e))?;
                    let mut options =
                        ClientOptions::from_object(&object).map_err(|e| unreadable(&e))?;
                    options.fill_defaults(&defaults);
                    options
                }
                None => defaults,
            };
            let old = LegacyOptions::parse(old.as_deref()).map_err(|e| unreadable(&e))?;
            let old_options: serde_json::Map<String, serde_json::Value> = old
                .api_entries()
                .map(|(k, v)| (k.to_owned(), options_json::yaml(v)))
                .collect();
            let snapshot = app.store.snapshot();
            let registry = &snapshot.services;
            Ok(ApiResponse::Json(
                json!({
                    "old_options": old_options,
                    "options": options_json::options(&options, registry),
                    "services": crate::services_json::services_dict(registry),
                    "services_v2": crate::services_json::services_list(registry),
                }),
                encoding,
            ))
        })
        .await
}

/// The Client API's forms of option values (`ToDictForAPI`).
mod options_json {
    use serde_json::{Map, Value as Json, json};

    use hydrus_core::ServiceKey;
    use hydrus_legacy::objects::{
        ClientOptions, LocationContext, MediaSort, MediaSortType, TagContext, TagSort, YamlValue,
    };
    use hydrus_store::services::ServiceRegistry;

    /// A legacy YAML value as Python's `json` writes it (tuples as lists,
    /// mapping keys as strings).
    pub fn yaml(value: &YamlValue) -> Json {
        match value {
            YamlValue::None => Json::Null,
            YamlValue::Bool(b) => json!(b),
            YamlValue::Int(i) => json!(i),
            YamlValue::Float(f) => json!(f),
            YamlValue::Str(s) => json!(s),
            YamlValue::Bytes(b) => json!(String::from_utf8_lossy(b)),
            YamlValue::List(items) | YamlValue::Tuple(items) => {
                Json::Array(items.iter().map(yaml).collect())
            }
            YamlValue::Map(entries) => Json::Object(
                entries
                    .iter()
                    .map(|(k, v)| {
                        let key = match k {
                            YamlValue::None => "null".to_owned(),
                            YamlValue::Bool(b) => b.to_string(),
                            YamlValue::Str(s) => s.clone(),
                            other => yaml(other).to_string(),
                        };
                        (key, yaml(v))
                    })
                    .collect(),
            ),
        }
    }

    fn tag_context(c: &TagContext) -> Json {
        json!({
            "service_key": c.service_key.to_hex(),
            "include_current_tags": c.include_current_tags,
            "include_pending_tags": c.include_pending_tags,
            "display_service_key": c.display_service_key.to_hex(),
        })
    }

    fn media_sort(sort: &MediaSort) -> Json {
        let mut out = json!({
            "sort_order": sort.sort_order.code(),
            "tag_context": tag_context(&sort.tag_context),
        });
        match &sort.sort_type {
            MediaSortType::System(code) => {
                out["sort_metatype"] = json!("system");
                out["sort_type"] = json!(code);
            }
            MediaSortType::Namespaces {
                namespaces,
                tag_display_type,
            } => {
                out["sort_metatype"] = json!("namespaces");
                out["namespaces"] = json!(namespaces);
                out["tag_display_type"] = json!(tag_display_type);
            }
            MediaSortType::Rating(key) => {
                out["sort_metatype"] = json!("rating");
                out["service_key"] = json!(key.to_hex());
            }
        }
        out
    }

    fn tag_sort(sort: &TagSort) -> Json {
        json!({
            "sort_type": sort.sort_type,
            "sort_order": sort.sort_order.code(),
            "use_siblings": sort.use_siblings,
            "group_by": sort.group_by,
        })
    }

    /// `GetDefaultLocalLocationContext`: services that no longer exist are
    /// dropped, and if none are left it is all local media.
    fn location(context: Option<&LocationContext>, registry: &ServiceRegistry) -> Json {
        let exists = |k: &&ServiceKey| registry.by_key(k).is_ok();
        let (current, deleted): (Vec<&ServiceKey>, Vec<&ServiceKey>) = match context {
            Some(c) => (
                c.current.iter().filter(exists).collect(),
                c.deleted.iter().filter(exists).collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        if current.is_empty() && deleted.is_empty() {
            return json!({
                "current_service_keys": [hex::encode(hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)],
                "deleted_service_keys": [],
            });
        }
        json!({
            "current_service_keys": current.iter().map(|k| k.to_hex()).collect::<Vec<_>>(),
            "deleted_service_keys": deleted.iter().map(|k| k.to_hex()).collect::<Vec<_>>(),
        })
    }

    pub fn options(o: &ClientOptions, registry: &ServiceRegistry) -> Json {
        // CC.TAG_PRESENTATION_*
        let tag_sort_at = |at: i64| o.default_tag_sorts.get(&at).map_or(Json::Null, tag_sort);
        let colours: Map<String, Json> = o
            .colours
            .iter()
            .map(|(set, colours)| {
                let colours: Map<String, Json> = colours
                    .iter()
                    .map(|(code, rgb)| (code.to_string(), json!(rgb)))
                    .collect();
                (set.clone(), Json::Object(colours))
            })
            .collect();
        let favourites: Map<String, Json> = o
            .suggested_tags_favourites
            .iter()
            .map(|(k, tags)| (k.to_hex(), json!(tags)))
            .collect();
        json!({
            "booleans": o.booleans,
            "strings": o.strings,
            "noneable_strings": o.noneable_strings,
            "integers": o.integers,
            "noneable_integers": o.noneable_integers,
            "keys": o.keys.iter().map(|(k, v)| (k.clone(), json!(hex::encode(v)))).collect::<Map<String, Json>>(),
            "colors": colours,
            "media_zooms": o.media_zooms,
            "slideshow_durations": o.slideshow_durations,
            "default_namespace_sorts": o.default_namespace_sorts.iter().map(media_sort).collect::<Vec<_>>(),
            "default_sort": o.default_sort.as_ref().map_or(Json::Null, media_sort),
            "default_tag_sort": tag_sort_at(0),
            "default_tag_sort_search_page": tag_sort_at(0),
            "default_tag_sort_search_page_manage_tags": tag_sort_at(1),
            "default_tag_sort_media_viewer": tag_sort_at(2),
            "default_tag_sort_media_vewier_manage_tags": tag_sort_at(3),
            "fallback_sort": o.fallback_sort.as_ref().map_or(Json::Null, media_sort),
            "suggested_tags_favourites": favourites,
            "default_local_location_context": location(o.default_local_location_context.as_ref(), registry),
        })
    }
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
