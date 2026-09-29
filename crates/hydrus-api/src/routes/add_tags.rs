//! `/add_tags/*` endpoints that change tags.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use hydrus_core::content::ContentUpdateAction;
use hydrus_core::sort::human_sort;
use hydrus_core::tag::{Tag, clean_tag_checked};
use hydrus_core::{HashId, ServiceId, ServiceType};
use hydrus_store::content::MappingAction;
use hydrus_store::settings::{self, FavouriteTags};
use hydrus_store::{master, schema::MappingTables};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::location::check_tag_service;
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

/// Default petition reason for tags petitioned through the API.
const API_PETITION_REASON: &str = "Petitioned from API";

/// One requested change: an action on some tags in a tag service.
struct Edit {
    /// `None` for "all known tags", which accepts edits and ignores them.
    service: Option<ServiceId>,
    action: ContentUpdateAction,
    /// `(tag, petition reason)`
    tags: Vec<(String, String)>,
}

fn not_a_list(what: &str) -> ApiError {
    ApiError::bad_request(format!(
        "The parameter \"{what}\" was not a list of strings!"
    ))
}

/// Parse `service_keys_to_tags` / `service_keys_to_actions_to_tags`.
fn parse_edits(app: &AppState, params: &crate::params::Params) -> ApiResult<Vec<Edit>> {
    let snap = app.store.snapshot();
    let resolve = |key: &[u8]| -> ApiResult<(Option<ServiceId>, ServiceType)> {
        let service = check_tag_service(&snap, key)?;
        let t = service.service_type();
        Ok(((t != ServiceType::CombinedTag).then_some(service.id), t))
    };
    let mut edits = None;
    if let Some(dict) = params.optional::<Vec<(Vec<u8>, Json)>>("service_keys_to_tags")? {
        let mut out = Vec::new();
        for (key, tags) in dict {
            let (service, service_type) = resolve(&key)?;
            let tags = tags
                .as_array()
                .ok_or_else(|| not_a_list("tags in service_keys_to_tags"))?
                .iter()
                .map(|t| {
                    t.as_str()
                        .ok_or_else(|| not_a_list("tags in service_keys_to_tags"))
                })
                .collect::<ApiResult<Vec<&str>>>()?;
            let tags: BTreeSet<String> = tags.into_iter().filter_map(clean_tag_checked).collect();
            if tags.is_empty() {
                continue;
            }
            let action = if service_type == ServiceType::LocalTag {
                ContentUpdateAction::Add
            } else {
                ContentUpdateAction::Pend
            };
            out.push(Edit {
                service,
                action,
                tags: tags
                    .into_iter()
                    .map(|t| (t, API_PETITION_REASON.to_owned()))
                    .collect(),
            });
        }
        edits = Some(out);
    }
    // like the reference, this form wins if both are given
    if let Some(dict) =
        params.optional::<Vec<(Vec<u8>, Json)>>("service_keys_to_actions_to_tags")?
    {
        let mut out = Vec::new();
        for (key, actions) in dict {
            let (service, service_type) = resolve(&key)?;
            let Json::Object(actions) = actions else {
                return Err(ApiError::bad_request(
                    "The parameter \"actions_to_tags\" was not a dict!",
                ));
            };
            for (raw_action, tags) in actions {
                let code: i64 = raw_action.trim().parse().map_err(|_| {
                    ApiError::bad_request(format!(
                        "Sorry, got an action, \"{raw_action}\", that was not an integer!"
                    ))
                })?;
                let action = u8::try_from(code)
                    .ok()
                    .and_then(ContentUpdateAction::from_code);
                let add_or_delete = matches!(
                    action,
                    Some(ContentUpdateAction::Add | ContentUpdateAction::Delete)
                );
                let hex = hex::encode(&key);
                if service_type == ServiceType::LocalTag && !add_or_delete {
                    return Err(ApiError::bad_request(format!(
                        "Sorry, you submitted a content action of \"{raw_action}\" for service \"{hex}\", but you can only add/delete on a local tag domain!"
                    )));
                }
                if service_type != ServiceType::LocalTag && add_or_delete {
                    return Err(ApiError::bad_request(format!(
                        "Sorry, you submitted a content action of \"{raw_action}\" for service \"{hex}\", but you cannot add/delete on a remote tag repository!"
                    )));
                }
                let items = tags
                    .as_array()
                    .ok_or_else(|| not_a_list("tags in actions_to_tags"))?;
                let tags: Vec<(String, String)> = items
                    .iter()
                    .filter_map(|item| {
                        let (tag, reason) = match item {
                            Json::String(tag) => (tag.as_str(), API_PETITION_REASON),
                            Json::Array(pair) => match pair.as_slice() {
                                [Json::String(tag), Json::String(reason)] => {
                                    (tag.as_str(), reason.as_str())
                                }
                                _ => return None,
                            },
                            _ => return None,
                        };
                        Some((clean_tag_checked(tag)?, reason.to_owned()))
                    })
                    .collect();
                let Some(action) = action else { continue };
                if tags.is_empty() {
                    continue;
                }
                out.push(Edit {
                    service,
                    action,
                    tags,
                });
            }
        }
        edits = Some(out);
    }
    edits.ok_or_else(|| {
        ApiError::bad_request(
            "Need a service_keys_to_tags or service_keys_to_actions_to_tags parameter!",
        )
    })
}

pub async fn add_tags(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddTags)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let hashes = parse_hashes(app, &params)?.ok_or_else(|| {
            ApiError::bad_request(
                "Please include some files in your request--file_id or hash based!",
            )
        })?;
        let override_deleted = params.or("override_previously_deleted_mappings", true)?;
        let create_deleted = params.or("create_new_deleted_mappings", true)?;
        let edits = parse_edits(app, &params)?;
        app.store.write_content(move |w| {
            let ids = hashes
                .iter()
                .map(|h| master::intern_hash(w.conn(), h))
                .collect::<hydrus_store::Result<Vec<HashId>>>()?;
            for edit in edits {
                let Some(service) = edit.service else {
                    continue;
                };
                for (tag, reason) in edit.tags {
                    let tag_id = master::intern_tag(w.conn(), &Tag::from_clean(tag))?;
                    let action = match edit.action {
                        ContentUpdateAction::Add => MappingAction::Add,
                        ContentUpdateAction::Delete => MappingAction::Delete,
                        ContentUpdateAction::Pend => MappingAction::Pend,
                        ContentUpdateAction::RescindPend => MappingAction::RescindPend,
                        ContentUpdateAction::Petition => MappingAction::Petition { reason },
                        ContentUpdateAction::RescindPetition => MappingAction::RescindPetition,
                        _ => continue,
                    };
                    let tables = MappingTables::new(service);
                    let targets = match action {
                        MappingAction::Add | MappingAction::Pend if !override_deleted => {
                            let deleted = with_tag(w.conn(), &tables.deleted, tag_id, &ids)?;
                            ids.iter()
                                .copied()
                                .filter(|h| !deleted.contains(h))
                                .collect()
                        }
                        MappingAction::Delete | MappingAction::Petition { .. }
                            if !create_deleted =>
                        {
                            let current = with_tag(w.conn(), &tables.current, tag_id, &ids)?;
                            ids.iter()
                                .copied()
                                .filter(|h| current.contains(h))
                                .collect()
                        }
                        _ => ids.clone(),
                    };
                    w.update_mappings(service, &action, tag_id, &targets)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

fn with_tag(
    conn: &rusqlite::Connection,
    table: &str,
    tag: hydrus_core::TagId,
    hashes: &[HashId],
) -> hydrus_store::Result<BTreeSet<HashId>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT hash_id FROM {table} WHERE tag_id = ?1 AND hash_id = ?2"
    ))?;
    let mut out = BTreeSet::new();
    for &hash in hashes {
        if stmt.exists(rusqlite::params![tag, hash])? {
            out.insert(hash);
        }
    }
    Ok(out)
}

pub async fn get_favourite_tags(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddTags)?;
    let tags = app
        .blocking(|app| Ok(app.store.read(settings::get::<FavouriteTags>)?))
        .await?;
    Ok(ApiResponse::json(json!({ "favourite_tags": tags.0 }), &req))
}

pub async fn set_favourite_tags(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddTags)?;
    let clean = |name: &str| -> ApiResult<Option<BTreeSet<String>>> {
        Ok(req
            .params
            .optional::<Vec<String>>(name)?
            .map(|tags| tags.iter().filter_map(|t| clean_tag_checked(t)).collect()))
    };
    let set = clean("set")?;
    let add = clean("add")?.unwrap_or_default();
    let remove = clean("remove")?.unwrap_or_default();
    let tags = app
        .blocking(move |app| {
            Ok(app.store.write(move |ctx| {
                let tags: BTreeSet<String> = if let Some(set) = set {
                    set
                } else {
                    let mut tags: BTreeSet<String> = settings::get::<FavouriteTags>(ctx.conn())?
                        .0
                        .into_iter()
                        .collect();
                    tags.extend(add);
                    tags.retain(|t| !remove.contains(t));
                    tags
                };
                let mut sorted: Vec<String> = tags.into_iter().collect();
                human_sort(&mut sorted);
                settings::set(ctx.conn(), &FavouriteTags(sorted.clone()))?;
                Ok(sorted)
            })?)
        })
        .await?;
    Ok(ApiResponse::json(json!({ "favourite_tags": tags }), &req))
}
