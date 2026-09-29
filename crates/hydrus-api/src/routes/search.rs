//! File search: `/get_files/search_files`, and the parameters it shares with
//! other endpoints that take a search (file domain, tag domain, `tags`).

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, LocationContext, Predicate, SearchError, SortBy, SortOrder,
    TagContext, parse_api_search, search_files as run_search,
};

use crate::AppState;
use crate::auth::{AccessPermissions, Permission};
use crate::error::{ApiError, ApiResult};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};

/// A service key named by a key parameter or, for old clients, by the
/// matching `..._name` parameter (which wins, as in the reference).
fn service_key_param(
    app: &AppState,
    params: &Params,
    key_param: &str,
    name_param: &str,
) -> ApiResult<Option<ServiceKey>> {
    if let Some(name) = params.optional::<String>(name_param)? {
        let snapshot = app.store.snapshot();
        let service = snapshot
            .services
            .all()
            .find(|s| s.name == name)
            .or_else(|| {
                snapshot
                    .services
                    .all()
                    .find(|s| s.name.to_lowercase() == name.to_lowercase())
            })
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Sorry, did not find a service with name \"{name}\"!"
                ))
            })?;
        return Ok(Some(service.key.clone()));
    }
    params.optional::<ServiceKey>(key_param)
}

fn check_service(
    app: &AppState,
    key: &ServiceKey,
    what: &str,
    ok: impl Fn(ServiceType) -> bool,
) -> ApiResult<()> {
    let snapshot = app.store.snapshot();
    let service = snapshot.services.by_key(key).map_err(|_| {
        ApiError::bad_request(format!(
            "Could not find the {what} service \"{}\"!",
            key.to_hex()
        ))
    })?;
    if ok(service.service_type()) {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" did not give a {what} service!",
            key.to_hex()
        )))
    }
}

/// The file domain: `file_service_key(s)` and `deleted_file_service_key(s)`,
/// or `default` if none are given.
pub fn parse_location(
    app: &AppState,
    params: &Params,
    default: LocationContext,
) -> ApiResult<LocationContext> {
    let keys = |single: &str, list: &str| -> ApiResult<Vec<ServiceKey>> {
        let mut out: Vec<ServiceKey> = Vec::new();
        if let Some(key) = params.optional::<ServiceKey>(single)? {
            out.push(key);
        }
        if let Some(keys) = params.optional::<Vec<Vec<u8>>>(list)? {
            out.extend(keys.into_iter().map(ServiceKey::new));
        }
        Ok(out)
    };
    let mut current = keys("file_service_key", "file_service_keys")?;
    if let Some(key) = service_key_param(app, params, "file_service_key", "file_service_name")?
        && !current.contains(&key)
    {
        current.push(key);
    }
    let deleted = keys("deleted_file_service_key", "deleted_file_service_keys")?;
    for key in current.iter().chain(&deleted) {
        check_service(app, key, "file", ServiceType::is_file_service)?;
    }
    if current.is_empty() && deleted.is_empty() {
        Ok(default)
    } else {
        Ok(LocationContext::new(current, deleted))
    }
}

/// The tag domain's service: `tag_service_key`, default "all known tags".
pub fn parse_tag_service(app: &AppState, params: &Params) -> ApiResult<ServiceKey> {
    match service_key_param(app, params, "tag_service_key", "tag_service_name")? {
        Some(key) => {
            check_service(app, &key, "tag", ServiceType::is_tag_service)?;
            Ok(key)
        }
        None => Ok(ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec())),
    }
}

/// Parse the `tags` search list, applying a restricted key's search
/// permissions the way the reference does: a search needs a positive tag
/// the key may search for, and a key that may not see every file needs at
/// least one plain positive tag.
pub fn parse_search_tags(perms: &AccessPermissions, params: &Params) -> ApiResult<Vec<Predicate>> {
    let tags: Json = params.optional::<Json>("tags")?.unwrap_or(json!([]));
    let items = tags.as_array().ok_or_else(|| {
        ApiError::bad_request("The parameter \"tags\" was not the expected type: list!")
    })?;

    let mut positive: Vec<String> = Vec::new();
    let (mut negated, mut system, mut or_groups) = (false, false, false);
    for item in items {
        match item {
            Json::Array(_) => or_groups = true,
            Json::String(s) if s.starts_with("system:") => system = true,
            Json::String(s) => {
                let clean = hydrus_core::Tag::new(s).ok_or_else(|| {
                    ApiError::bad_request(format!("Could not understand the tag: \"{s}\""))
                })?;
                if s.starts_with('-') {
                    negated = true;
                } else {
                    positive.push(clean.into_string());
                }
            }
            _ => {}
        }
    }
    if positive.is_empty() {
        let needs_all_files = |what: &str| -> ApiResult<()> {
            perms.check_can_see_all_files().map_err(|_| {
                ApiError::forbidden(format!(
                    "Sorry, if you want to search {what} without regular tags, you need permission to search everything!"
                ))
            })
        };
        if negated {
            needs_all_files("negated tags")?;
        }
        if system {
            needs_all_files("system predicates")?;
        }
        if or_groups {
            needs_all_files("OR predicates")?;
        }
    } else {
        perms.check_can_search_tags(&positive)?;
    }

    let predicates = parse_api_search(&tags).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if !predicates.is_empty()
        && !predicates.iter().any(|p| {
            matches!(
                p,
                Predicate::Tag {
                    inclusive: true,
                    ..
                }
            )
        })
    {
        perms.check_can_see_all_files().map_err(|_| {
            ApiError::forbidden(
                "Sorry, you do not have permission to see all files on this client. Please add a regular tag to your search.",
            )
        })?;
    }
    Ok(predicates)
}

/// A search that could not run, as an API error.
pub fn search_error(e: SearchError) -> ApiError {
    match e {
        SearchError::Store(e) => e.into(),
        other => ApiError::bad_request(other.to_string()),
    }
}

pub async fn search_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SearchFiles)?;
    let p = &req.params;

    let location = parse_location(&app, p, LocationContext::default())?;
    let tag_service = parse_tag_service(&app, p)?;
    if tag_service.as_bytes() == builtin_keys::COMBINED_TAG && location.is_all_known_files() {
        return Err(ApiError::bad_request(
            "Sorry, search for all known tags over all known files is not supported!",
        ));
    }
    let tags = TagContext::new(
        tag_service,
        p.or("include_current_tags", true)?,
        p.or("include_pending_tags", true)?,
    );
    let predicates = parse_search_tags(&perms, p)?;

    let sort = FileSort {
        by: match p.optional::<i64>("file_sort_type")? {
            None => SortBy::ImportTime,
            Some(code) => SortBy::from_code(code).ok_or_else(|| {
                ApiError::bad_request("Sorry, did not understand that sort type!")
            })?,
        },
        order: if p.or("file_sort_asc", true)? {
            SortOrder::Ascending
        } else {
            SortOrder::Descending
        },
    };
    let return_hashes = p.or("return_hashes", false)?;
    let return_file_ids = p.or("return_file_ids", true)?;
    let search = FileSearchContext {
        location,
        tags,
        predicates,
    };
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let hash_ids = if search.predicates.is_empty() {
                Vec::new()
            } else {
                let snapshot = app.store.snapshot();
                let clock = Clock::system();
                app.store
                    .read(|conn| Ok(run_search(conn, &snapshot, &search, sort, &clock)))?
                    .map_err(search_error)?
            };
            app.access.record_search(&perms, &hash_ids);

            let mut body = Map::new();
            if return_hashes {
                let hashes = app
                    .store
                    .read(|c| hydrus_store::master::hashes(c, &hash_ids))?;
                body.insert(
                    "hashes".into(),
                    hash_ids
                        .iter()
                        .filter_map(|id| hashes.get(id).map(|h| json!(h.to_hex())))
                        .collect(),
                );
            }
            if return_file_ids {
                body.insert(
                    "file_ids".into(),
                    hash_ids.iter().map(|id| json!(id.get())).collect(),
                );
            }
            Ok(ApiResponse::Json(Json::Object(body), encoding))
        })
        .await
}
