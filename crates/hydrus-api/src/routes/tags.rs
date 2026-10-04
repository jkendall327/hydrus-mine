//! Tag autocomplete and sibling/parent lookups.

use crate::auth::PermissionChecks as _;
use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::ServiceKey;
use hydrus_core::service::builtin_keys;
use hydrus_core::tag::{clean_tag, clean_tag_checked};
use hydrus_core::{ServiceType, TagId};
use hydrus_store::autocomplete::{
    self, AutocompleteInput, AutocompleteSettings, CountDomain, TagDisplayType, TagSearchScope,
};
use hydrus_store::{master, settings};

use crate::AppState;
use crate::auth::Permission;
use crate::domains::{parse_file_domain, parse_tag_service};
use crate::error::{ApiError, ApiResult};
use crate::request::{ApiRequest, ApiResponse};
use crate::services_json;

pub async fn search_tags(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    // the reference asks for search permission here, not tag editing
    perms.check(Permission::SearchFiles)?;
    let search = req.params.required::<String>("search")?;
    let display = if req.params.or("tag_display_type", "storage".to_owned())? == "storage" {
        TagDisplayType::Storage
    } else {
        TagDisplayType::Display
    };
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let registry = &snapshot.services;
            let tag_service = parse_tag_service(registry, &params, "tag_service_key")?;
            let files = parse_file_domain(
                registry,
                &params,
                builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
                true,
            )?;
            let mut domains: Vec<CountDomain> = files
                .current
                .iter()
                .map(|&service| CountDomain {
                    service,
                    exact: true,
                })
                .collect();
            if !files.deleted.is_empty()
                && let Some(deleted) = registry.of_type(ServiceType::CombinedDeletedFile).next()
            {
                domains.push(CountDomain {
                    service: deleted.id,
                    exact: false,
                });
            }
            let input = AutocompleteInput::parse(&search);
            let tag_service_key = match tag_service {
                Some(id) => registry.get(id)?.key.clone(),
                None => ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()),
            };
            let matches = app.store.read(|conn| {
                let rules = settings::get::<AutocompleteSettings>(conn)?.rules(&tag_service_key);
                let Some(query) = input.tag_query(&rules) else {
                    return Ok(Vec::new());
                };
                let scope = TagSearchScope {
                    domains,
                    tag_service,
                    display,
                    include_current: true,
                    include_pending: true,
                };
                autocomplete::search_tags(conn, registry, &snapshot.display, &scope, &query)
            })?;
            let unrestricted = perms.searches_unrestricted();
            let tags: Vec<Json> = matches
                .into_iter()
                .filter(|m| unrestricted || perms.search_filter.tag_ok(&m.tag, false))
                .map(|m| json!({ "value": m.tag, "count": m.count.min_total() }))
                .collect();
            Ok(ApiResponse::Json(
                json!({
                    "autocomplete_text": {
                        "search_text": input.search_text(),
                        "inclusive": input.inclusive(),
                    },
                    "tags": tags,
                }),
                encoding,
            ))
        })
        .await
}

pub async fn get_siblings_and_parents(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddTags)?;
    let raw = req.params.required::<Vec<String>>("tags")?;
    for tag in &raw {
        if clean_tag(tag).is_empty() {
            return Err(ApiError::bad_request(format!("Tag \"{tag}\" was empty!")));
        }
    }
    let tags: BTreeSet<String> = raw.iter().filter_map(|t| clean_tag_checked(t)).collect();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let registry = &snapshot.services;
            let tag_services: Vec<_> = registry.tag_services().collect();
            let wanted: Vec<hydrus_core::Tag> = tags
                .iter()
                .map(|t| hydrus_core::Tag::from_clean(t.clone()))
                .collect();
            let (ids, names) = app.store.read(|conn| {
                let ids = master::tag_ids(conn, &wanted)?;
                // every tag any answer mentions
                let mut related: Vec<TagId> = Vec::new();
                for &id in ids.values() {
                    for service in &tag_services {
                        let graph = snapshot.display.get(service.id);
                        let ideal = graph.ideal(id);
                        related.extend(graph.chain(ideal));
                        related.extend_from_slice(graph.ancestors(ideal));
                        related.extend_from_slice(graph.descendants(ideal));
                    }
                }
                related.sort_unstable();
                related.dedup();
                Ok((ids, master::tags(conn, &related)?))
            })?;
            let name = |id: TagId| names.get(&id).map(|t| t.as_str().to_owned());
            let sorted = |ids: &[TagId]| -> Vec<String> {
                let mut out: Vec<String> = ids.iter().filter_map(|&id| name(id)).collect();
                out.sort();
                out
            };
            let mut body = Map::new();
            for tag in &wanted {
                let mut per_service = Map::new();
                for service in &tag_services {
                    let (siblings, ideal, descendants, ancestors) = match ids.get(tag) {
                        Some(&id) => {
                            let graph = snapshot.display.get(service.id);
                            let ideal = graph.ideal(id);
                            (
                                sorted(&graph.chain(ideal)),
                                name(ideal).unwrap_or_else(|| tag.as_str().to_owned()),
                                sorted(graph.descendants(ideal)),
                                sorted(graph.ancestors(ideal)),
                            )
                        }
                        None => (
                            vec![tag.as_str().to_owned()],
                            tag.as_str().to_owned(),
                            Vec::new(),
                            Vec::new(),
                        ),
                    };
                    per_service.insert(
                        service.key.to_hex(),
                        json!({
                            "siblings": siblings,
                            "ideal_tag": ideal,
                            "descendants": descendants,
                            "ancestors": ancestors,
                        }),
                    );
                }
                body.insert(tag.as_str().to_owned(), Json::Object(per_service));
            }
            Ok(ApiResponse::Json(
                json!({
                    "tags": body,
                    "services": services_json::services_dict(registry),
                    "services_v2": services_json::services_list(registry),
                }),
                encoding,
            ))
        })
        .await
}
