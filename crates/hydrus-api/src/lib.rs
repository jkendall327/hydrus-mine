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
pub mod location;
pub mod media_json;
pub mod params;
pub mod popups;
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
    /// The downloader's URL queues (`/add_urls/add_url`).
    pub downloads: Option<Arc<hydrus_download::QueueRunner>>,
    /// Runs subscriptions as they come due.
    pub subscriptions: Option<Arc<hydrus_download::subscriptions::SubscriptionRunner>>,
    /// The database is locked (`/manage_database/lock_on`): requests are
    /// refused until it is unlocked.
    pub locked: std::sync::atomic::AtomicBool,
    /// Holds the database paused while it is locked.
    pub paused: parking_lot::Mutex<Option<hydrus_store::Paused>>,
    /// When the access keys were last read from the database.
    pub keys_read: parking_lot::Mutex<std::time::Instant>,
    /// Last committed permission revision loaded into the registry.
    pub keys_revision: parking_lot::Mutex<u64>,
    /// Identifies this run of the client (`/client_info`).
    pub boot_id: [u8; 32],
    pub boot_time_ms: i64,
    /// Last published request-start time; serializes the timestamp-only IPC.
    pub api_activity: parking_lot::Mutex<i64>,
}

impl AppState {
    pub fn new(store: Arc<Store>) -> hydrus_store::Result<Arc<Self>> {
        let (access, keys_revision) = store.read(|conn| {
            // Store::read already owns one SQLite read transaction.
            let access = AccessRegistry::load(conn)?;
            let revision = hydrus_store::api_permissions::revision(conn)?;
            Ok((access, revision))
        })?;
        let importer =
            hydrus_import::FileImporter::new(Arc::clone(&store), hydrus_media::MediaTools::new());
        let network: hydrus_store::network::NetworkSettings =
            store.read(hydrus_store::settings::get)?;
        let downloads = hydrus_net::NetEngine::new(
            Arc::clone(&store),
            hydrus_net::NetOptions::from_settings(&network),
        )
        .map_err(|e| tracing::error!(error = %e, "the network engine could not start"))
        .ok()
        .and_then(|net| {
            hydrus_download::Downloader::new(Arc::clone(&store), Arc::new(net), importer.clone())
                .ok()
        })
        .map(|downloader| {
            hydrus_download::QueueRunner::new(
                Arc::new(downloader),
                network.downloader_network_error_delay,
            )
        });
        let subscriptions = downloads.as_ref().map(|runner| {
            hydrus_download::subscriptions::SubscriptionRunner::new(Arc::clone(runner.downloader()))
        });
        Ok(Arc::new(Self {
            store,
            access,
            importer,
            downloads,
            subscriptions,
            locked: std::sync::atomic::AtomicBool::new(false),
            paused: parking_lot::Mutex::new(None),
            keys_read: parking_lot::Mutex::new(std::time::Instant::now()),
            keys_revision: parking_lot::Mutex::new(keys_revision),
            boot_id: rand::random(),
            boot_time_ms: hydrus_core::time::TimestampMs::now().millis(),
            api_activity: parking_lot::Mutex::new(0),
        }))
    }

    /// Who is making this request. Keys can be added or removed by another
    /// process, so each request checks its committed permission revision. Raw
    /// legacy writes without a revision are reread once a minute and on unknown keys.
    pub fn authenticate(&self, req: &ApiRequest) -> ApiResult<AccessPermissions> {
        // lock_off must authenticate while the writer/read pool is paused.
        if self.locked.load(std::sync::atomic::Ordering::SeqCst) {
            return self.access.authenticate(&req.headers, &req.params);
        }
        let mut revision = self.keys_revision.lock();
        self.store
            .read(|conn| {
                let committed = hydrus_store::api_permissions::revision(conn)?;
                if committed != *revision
                    || self.keys_read.lock().elapsed() > std::time::Duration::from_secs(60)
                {
                    self.access.reload_keys(conn)?;
                    *revision = committed;
                    *self.keys_read.lock() = std::time::Instant::now();
                }
                Ok(())
            })
            .map_err(ApiError::from)?;
        match self.access.authenticate(&req.headers, &req.params) {
            Err(e) if e.message == auth::UNKNOWN_KEY => {
                self.store
                    .read(|c| self.access.reload_keys(c))
                    .map_err(ApiError::from)?;
                self.access.authenticate(&req.headers, &req.params)
            }
            other => other,
        }
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
    use routes::{
        access, add_files, add_tags, database, files, metadata, network, pages, popups,
        relationships, search, services, tags, urls,
    };
    Router::new()
        .route("/api_version", get(access::api_version))
        .route("/verify_access_key", get(access::verify_access_key))
        .route("/session_key", get(access::session_key))
        .route("/client_info", get(access::client_info))
        .route(
            "/request_new_permissions",
            get(access::request_new_permissions),
        )
        .route(
            "/get_service_rating_svg",
            get(access::get_service_rating_svg),
        )
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
        .route("/add_urls/add_url", post(urls::add_url))
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
        .route("/get_files/render", get(files::render))
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
            "/manage_file_relationships/get_random_potentials",
            get(relationships::get_random_potentials),
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
        .route("/manage_cookies/get_cookies", get(network::get_cookies))
        .route("/manage_cookies/set_cookies", post(network::set_cookies))
        .route("/manage_headers/get_headers", get(network::get_headers))
        .route("/manage_headers/set_headers", post(network::set_headers))
        .route(
            "/manage_headers/set_user_agent",
            post(network::set_user_agent),
        )
        .route(
            "/manage_services/get_pending_counts",
            get(services::get_pending_counts),
        )
        .route(
            "/manage_services/forget_pending",
            post(services::forget_pending),
        )
        .route(
            "/manage_services/commit_pending",
            post(services::commit_pending),
        )
        .route("/manage_popups/add_popup", post(popups::add_popup))
        .route("/manage_popups/update_popup", post(popups::update_popup))
        .route("/manage_popups/cancel_popup", post(popups::cancel_popup))
        .route("/manage_popups/dismiss_popup", post(popups::dismiss_popup))
        .route("/manage_popups/finish_popup", post(popups::finish_popup))
        .route(
            "/manage_popups/finish_and_dismiss_popup",
            post(popups::finish_and_dismiss_popup),
        )
        .route(
            "/manage_popups/call_user_callable",
            post(popups::call_user_callable),
        )
        .route("/manage_popups/get_popups", get(popups::get_popups))
        .route("/manage_pages/get_pages", get(pages::get_pages))
        .route("/manage_pages/get_page_info", get(pages::get_page_info))
        .route(
            "/manage_pages/get_media_viewers",
            get(pages::get_media_viewers),
        )
        .route("/manage_pages/add_files", post(pages::add_files))
        .route("/manage_pages/focus_page", post(pages::focus_page))
        .route("/manage_pages/refresh_page", post(pages::refresh_page))
        .route("/manage_database/mr_bones", get(database::mr_bones))
        .route(
            "/manage_database/get_client_options",
            get(database::get_client_options),
        )
        .route(
            "/manage_database/force_commit",
            post(database::force_commit),
        )
        .route("/manage_database/lock_on", post(database::lock_on))
        .route("/manage_database/lock_off", post(database::lock_off))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            database::refuse_while_locked,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            server::note_activity,
        ))
        .fallback(|| async { request::no_such_resource() })
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}
