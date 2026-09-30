//! `/manage_services/*`: content waiting to be uploaded to repositories.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::{ServiceKey, ServiceType};
use hydrus_store::services::Service;

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::request::{ApiRequest, ApiResponse};
use crate::services_json;

pub async fn get_pending_counts(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::CommitPending)?;
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let registry = &snapshot.services;
            let counts = app
                .store
                .read(|conn| hydrus_store::pending::counts(conn, registry))?;
            let pending: Map<String, Json> = counts
                .into_iter()
                .map(|(key, counts)| {
                    let counts: Map<String, Json> = counts
                        .into_iter()
                        .map(|(name, n)| (name.to_owned(), json!(n)))
                        .collect();
                    (key.to_hex(), Json::Object(counts))
                })
                .collect();
            Ok(ApiResponse::Json(
                json!({
                    "pending_counts": pending,
                    "services": services_json::services_dict(registry),
                    "services_v2": services_json::services_list(registry),
                }),
                encoding,
            ))
        })
        .await
}

/// The repository a request names (`CheckUploadableService`).
fn uploadable(app: &AppState, req: &ApiRequest) -> ApiResult<Arc<Service>> {
    let key: ServiceKey = req.params.required("service_key")?;
    let snapshot = app.store.snapshot();
    let service = snapshot.services.by_key(&key).map_err(|_| {
        ApiError::bad_request(format!("Could not find the service \"{}\"!", key.to_hex()))
    })?;
    if !matches!(
        service.service_type(),
        ServiceType::Ipfs | ServiceType::FileRepository | ServiceType::TagRepository
    ) {
        return Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" was not for an uploadable service!",
            key.to_hex()
        )));
    }
    Ok(Arc::clone(service))
}

pub async fn forget_pending(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::CommitPending)?;
    let service = uploadable(&app, &req)?;
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            hydrus_store::pending::forget(&app.store, service.id)?;
            Ok(ApiResponse::Json(json!({}), encoding))
        })
        .await
}

/// Uploading needs a connection to the repository's server, which hydrus-rs
/// doesn't make yet.
pub async fn commit_pending(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::CommitPending)?;
    uploadable(&app, &req)?;
    Err(ApiError::new(
        ErrorKind::UnprocessableEntity,
        "Sorry, hydrus-rs does not upload to repositories yet!",
    ))
}
