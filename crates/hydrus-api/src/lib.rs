//! The hydrus Client API.
//!
//! A REST API compatible with the reference client's, served by axum. Every
//! endpoint's behaviour is checked against responses recorded from the
//! reference (see `oracle/` and `tests/conformance.rs`).

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;

use hydrus_store::Store;

pub mod auth;
pub mod domains;
pub mod error;
pub mod file_filter;
pub mod media_json;
pub mod params;
pub mod request;
pub mod routes;
pub mod services_json;

use auth::{AccessPermissions, AccessRegistry};
use error::{ApiError, ApiResult};
use request::ApiRequest;

/// Shared state of a running API server.
#[derive(Debug)]
pub struct AppState {
    pub store: Arc<Store>,
    pub access: AccessRegistry,
}

impl AppState {
    pub fn new(store: Arc<Store>) -> hydrus_store::Result<Arc<Self>> {
        let access = store.read(AccessRegistry::load)?;
        Ok(Arc::new(Self { store, access }))
    }

    /// Who is making this request.
    pub fn authenticate(&self, req: &ApiRequest) -> ApiResult<AccessPermissions> {
        self.access.authenticate(&req.headers, &req.params)
    }

    /// Run blocking store work off the async runtime.
    pub async fn blocking<R, F>(self: &Arc<Self>, f: F) -> ApiResult<R>
    where
        R: Send + 'static,
        F: FnOnce(&AppState) -> ApiResult<R> + Send + 'static,
    {
        let app = Arc::clone(self);
        tokio::task::spawn_blocking(move || f(&app))
            .await
            .map_err(|e| ApiError::server(format!("request handler failed: {e}")))?
    }
}

/// The API's routes.
pub fn router(state: Arc<AppState>) -> Router {
    use routes::{access, files, relationships, tags, urls};
    Router::new()
        .route("/api_version", get(access::api_version))
        .route("/verify_access_key", get(access::verify_access_key))
        .route("/session_key", get(access::session_key))
        .route("/get_services", get(access::get_services))
        .route("/get_service", get(access::get_service))
        .route("/add_tags/clean_tags", get(access::clean_tags))
        .route("/add_tags/search_tags", get(tags::search_tags))
        .route(
            "/add_tags/get_siblings_and_parents",
            get(tags::get_siblings_and_parents),
        )
        .route("/get_files/file_metadata", get(files::file_metadata))
        .route("/get_files/file_hashes", get(files::file_hashes))
        .route("/get_files/file", get(files::file))
        .route("/get_files/thumbnail", get(files::thumbnail))
        .route("/get_files/file_path", get(files::file_path))
        .route("/get_files/thumbnail_path", get(files::thumbnail_path))
        .route(
            "/get_files/local_file_storage_locations",
            get(files::local_file_storage_locations),
        )
        .route("/add_urls/get_url_info", get(urls::get_url_info))
        .route("/add_urls/get_url_files", get(urls::get_url_files))
        .route(
            "/manage_file_relationships/get_file_relationships",
            get(relationships::get_file_relationships),
        )
        .route(
            "/manage_file_relationships/get_potentials_count",
            get(relationships::get_potentials_count),
        )
        .route(
            "/manage_file_relationships/get_potential_pairs",
            get(relationships::get_potential_pairs),
        )
        .fallback(|| async { request::no_such_resource() })
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}
