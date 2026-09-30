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
pub mod location;
pub mod media_json;
pub mod params;
pub mod request;
pub mod routes;
pub mod server;
pub mod services_json;

use auth::{AccessPermissions, AccessRegistry};
use error::{ApiError, ApiResult};
use request::ApiRequest;

/// Shared state of a running API server.
#[derive(Debug)]
pub struct AppState {
    pub store: Arc<Store>,
    pub access: AccessRegistry,
    pub importer: hydrus_import::FileImporter,
}

impl AppState {
    pub fn new(store: Arc<Store>) -> hydrus_store::Result<Arc<Self>> {
        let access = store.read(AccessRegistry::load)?;
        let importer =
            hydrus_import::FileImporter::new(Arc::clone(&store), hydrus_media::MediaTools::new());
        Ok(Arc::new(Self {
            store,
            access,
            importer,
        }))
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
    use axum::routing::post;
    use routes::{access, add_files, add_tags, files, metadata, relationships, search, tags, urls};
    Router::new()
        .route("/api_version", get(access::api_version))
        .route("/verify_access_key", get(access::verify_access_key))
        .route("/session_key", get(access::session_key))
        .route("/get_services", get(access::get_services))
        .route("/get_service", get(access::get_service))
        .route("/add_tags/clean_tags", get(access::clean_tags))
        .route("/add_tags/add_tags", post(add_tags::add_tags))
        .route(
            "/add_tags/get_favourite_tags",
            get(add_tags::get_favourite_tags),
        )
        .route(
            "/add_tags/set_favourite_tags",
            post(add_tags::set_favourite_tags),
        )
        .route("/add_files/archive_files", post(add_files::archive_files))
        .route(
            "/add_files/unarchive_files",
            post(add_files::unarchive_files),
        )
        .route("/add_files/delete_files", post(add_files::delete_files))
        .route("/add_files/undelete_files", post(add_files::undelete_files))
        .route(
            "/add_files/clear_file_deletion_record",
            post(add_files::clear_file_deletion_record),
        )
        .route("/add_files/migrate_files", post(add_files::migrate_files))
        .route("/add_files/add_file", post(add_files::add_file))
        .route("/add_urls/associate_url", post(urls::associate_url))
        .route(
            "/add_files/generate_hashes",
            post(add_files::generate_hashes),
        )
        .route("/add_notes/set_notes", post(metadata::set_notes))
        .route("/add_notes/delete_notes", post(metadata::delete_notes))
        .route("/edit_ratings/set_rating", post(metadata::set_rating))
        .route("/edit_times/set_time", post(metadata::set_time))
        .route(
            "/edit_times/increment_file_viewtime",
            post(metadata::increment_file_viewtime),
        )
        .route(
            "/edit_times/set_file_viewtime",
            post(metadata::set_file_viewtime),
        )
        .route("/get_files/search_files", get(search::search_files))
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
        .route(
            "/manage_file_relationships/set_file_relationships",
            post(relationships::set_file_relationships),
        )
        .route(
            "/manage_file_relationships/set_kings",
            post(relationships::set_kings),
        )
        .route(
            "/manage_file_relationships/remove_potentials",
            post(relationships::remove_potentials),
        )
        .fallback(|| async { request::no_such_resource() })
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}
