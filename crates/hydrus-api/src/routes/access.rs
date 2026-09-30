//! Versions, access keys, services, tag cleaning.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::ServiceType;
use hydrus_core::sort::human_sort;
use hydrus_core::tag::clean_tag_checked;
use hydrus_store::services::{ServiceKind, StarAppearance};

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
    let basic: Vec<u8> = if perms.permits_everything {
        Permission::ALL.iter().copied().map(u8::from).collect()
    } else {
        perms.basic.iter().copied().map(u8::from).collect()
    };
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

/// When this client started, and an id for this run of it.
pub async fn client_info(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?;
    Ok(ApiResponse::json(
        json!({
            "boot_id": hex::encode(app.boot_id),
            "boot_time": app.boot_time_ms as f64 / 1000.0,
            // without a GUI there is no user to be idle
            "currently_idle": false,
        }),
        &req,
    ))
}

/// Access keys are requested while the reference's registration dialog is
/// open; hydrus-rs has no such dialog yet.
pub async fn request_new_permissions() -> ApiResult<ApiResponse> {
    Err(ApiError::new(
        ErrorKind::Conflict,
        "The permission registration dialog is not open. hydrus-rs has no such dialog yet, so access keys cannot be requested this way.",
    ))
}

/// The rating SVGs the reference ships (`static/star_shapes`).
const BUNDLED_RATING_SVGS: &[(&str, &[u8])] = &[
    (
        "architecture",
        include_bytes!("../../../../static/star_shapes/architecture.svg"),
    ),
    (
        "art-palette",
        include_bytes!("../../../../static/star_shapes/art-palette.svg"),
    ),
    (
        "cinema-reel",
        include_bytes!("../../../../static/star_shapes/cinema-reel.svg"),
    ),
    (
        "eye",
        include_bytes!("../../../../static/star_shapes/eye.svg"),
    ),
    (
        "heart-cute",
        include_bytes!("../../../../static/star_shapes/heart-cute.svg"),
    ),
    (
        "inspiration",
        include_bytes!("../../../../static/star_shapes/inspiration.svg"),
    ),
    (
        "scenery",
        include_bytes!("../../../../static/star_shapes/scenery.svg"),
    ),
    (
        "smile",
        include_bytes!("../../../../static/star_shapes/smile.svg"),
    ),
    (
        "spiral",
        include_bytes!("../../../../static/star_shapes/spiral.svg"),
    ),
    (
        "star",
        include_bytes!("../../../../static/star_shapes/star.svg"),
    ),
    (
        "wallpaper",
        include_bytes!("../../../../static/star_shapes/wallpaper.svg"),
    ),
];

/// A rating service's SVG icon: the user's own (`static/star_shapes` in the
/// store) before the bundled one of that name.
pub async fn get_service_rating_svg(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check_any(SERVICE_READERS)?;
    let snapshot = app.store.snapshot();
    let registry = &snapshot.services;
    let is_rating = |t: ServiceType| {
        matches!(
            t,
            ServiceType::LocalRatingLike | ServiceType::LocalRatingNumerical
        )
    };
    let key = if let Some(key) = req
        .params
        .optional::<hydrus_core::ServiceKey>("service_key")?
    {
        key
    } else if let Some(name) = req.params.optional::<String>("service_name")? {
        registry
            .all()
            .find(|s| s.name == name && is_rating(s.service_type()))
            .map(|s| s.key.clone())
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
    let service = registry.by_key(&key).map_err(|_| {
        ApiError::new(
            ErrorKind::NotFound,
            format!(
                "Sorry, did not find a service with key \"{}\"!",
                key.to_hex()
            ),
        )
    })?;
    let appearance = match &service.kind {
        ServiceKind::RatingLike(config) => &config.appearance,
        ServiceKind::RatingNumerical(config) => &config.appearance,
        _ => {
            return Err(ApiError::bad_request(
                "This type of service cannot have a SVG associated with it!",
            ));
        }
    };
    let StarAppearance::Svg(name) = appearance else {
        return Err(ApiError::new(
            ErrorKind::NotFound,
            format!(
                "Rating service \"{}\" does not use a SVG icon!",
                service.name
            ),
        ));
    };
    let custom = app
        .store
        .dir()
        .join("static")
        .join("star_shapes")
        .join(format!("{name}.svg"));
    let body = if custom.is_file() {
        std::fs::read(&custom).map_err(|e| {
            ApiError::server(format!(
                "There was a problem getting the SVG file for rating service \"{}\"! Error follows: {e}",
                service.name
            ))
        })?
    } else if let Some((_, svg)) = BUNDLED_RATING_SVGS.iter().find(|(n, _)| n == name) {
        svg.to_vec()
    } else {
        return Err(ApiError::new(
            ErrorKind::NotFound,
            format!("No SVG with the name \"star_shapes/{name}\"!"),
        ));
    };
    Ok(ApiResponse::Bytes {
        content_type: "image/svg+xml".into(),
        body: body.into(),
        cache: false,
        attachment: false,
    })
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
