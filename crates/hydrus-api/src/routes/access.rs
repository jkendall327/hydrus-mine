//! Versions, access keys, services, tag cleaning.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::ServiceType;
use hydrus_core::sort::human_sort;
use hydrus_core::tag::clean_tag_checked;

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::request::{ApiRequest, ApiResponse};
use crate::services_json::{self, brief, services_of};

pub async fn api_version(req: ApiRequest) -> ApiResult<ApiResponse> {
    Ok(ApiResponse::json(json!({}), &req))
}

pub async fn verify_access_key(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    let basic: Vec<u8> = perms.basic.iter().copied().map(u8::from).collect();
    Ok(ApiResponse::json(
        json!({
            "name": perms.name,
            "permits_everything": perms.permits_everything,
            "basic_permissions": basic,
            "human_description": perms.human_description(),
        }),
        &req,
    ))
}

pub async fn session_key(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    let session = app.access.new_session(&perms.access_key);
    Ok(ApiResponse::json(
        json!({ "session_key": hex::encode(session) }),
        &req,
    ))
}

const SERVICE_READERS: &[Permission] = &[
    Permission::AddFiles,
    Permission::EditRatings,
    Permission::AddTags,
    Permission::AddNotes,
    Permission::ManagePages,
    Permission::ManageFileRelationships,
    Permission::SearchFiles,
];

pub async fn get_services(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check_any(SERVICE_READERS)?;
    let snapshot = app.store.snapshot();
    let registry = &snapshot.services;
    let groups: &[(ServiceType, &str)] = &[
        (ServiceType::LocalTag, "local_tags"),
        (ServiceType::TagRepository, "tag_repositories"),
        (ServiceType::LocalFileDomain, "local_files"),
        (ServiceType::LocalFileUpdateDomain, "local_updates"),
        (ServiceType::FileRepository, "file_repositories"),
        (ServiceType::HydrusLocalFileStorage, "all_local_files"),
        (ServiceType::CombinedLocalFileDomains, "all_local_media"),
        (ServiceType::CombinedFile, "all_known_files"),
        (ServiceType::CombinedTag, "all_known_tags"),
        (ServiceType::LocalFileTrashDomain, "trash"),
    ];
    let mut body = Map::new();
    for (service_type, name) in groups {
        let list: Vec<Json> = services_of(registry, &[*service_type])
            .into_iter()
            .map(brief)
            .collect();
        body.insert((*name).into(), Json::Array(list));
    }
    body.insert("services".into(), services_json::services_dict(registry));
    body.insert("services_v2".into(), services_json::services_list(registry));
    Ok(ApiResponse::json(Json::Object(body), &req))
}

pub async fn get_service(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check_any(SERVICE_READERS)?;
    let snapshot = app.store.snapshot();
    let registry = &snapshot.services;
    let allowed = |s: &hydrus_store::services::Service| {
        services_json::API_SERVICE_TYPES.contains(&s.service_type())
    };
    let service = if let Some(key) = req
        .params
        .optional::<hydrus_core::ServiceKey>("service_key")?
    {
        registry.by_key(&key).map_err(|_| {
            ApiError::new(
                ErrorKind::NotFound,
                format!(
                    "Sorry, did not find a service with key \"{}\"!",
                    key.to_hex()
                ),
            )
        })?
    } else if let Some(name) = req.params.optional::<String>("service_name")? {
        registry
            .all()
            .find(|s| s.name == name && allowed(s))
            .ok_or_else(|| {
                ApiError::new(
                    ErrorKind::NotFound,
                    format!("Sorry, did not find a service with name \"{name}\"!"),
                )
            })?
    } else {
        return Err(ApiError::bad_request(
            "Sorry, you need to give a service_key or service_name!",
        ));
    };
    if !allowed(service) {
        return Err(ApiError::bad_request(
            "Sorry, for now, you cannot ask about this service!",
        ));
    }
    Ok(ApiResponse::json(
        json!({ "service": brief(service) }),
        &req,
    ))
}

pub async fn clean_tags(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddTags)?;
    let raw = req.params.required::<Vec<String>>("tags")?;
    let mut tags: Vec<String> = raw.iter().filter_map(|t| clean_tag_checked(t)).collect();
    tags.sort();
    tags.dedup();
    human_sort(&mut tags);
    Ok(ApiResponse::json(json!({ "tags": tags }), &req))
}
