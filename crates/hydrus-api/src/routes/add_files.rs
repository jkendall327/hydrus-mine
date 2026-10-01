//! `/add_files/*` endpoints that change which domains files are in.

use std::sync::Arc;

use axum::extract::State;

use hydrus_core::import_options::{CallerType, ImportOptionsManager};
use hydrus_core::{HashId, ServiceId, ServiceType, Sha256};
use hydrus_store::master;

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::location;
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

/// Default deletion reason, as the reference records it.
const API_DELETE_REASON: &str = "Deleted via Client API.";

fn require_hashes(app: &AppState, params: &Params) -> ApiResult<Vec<Sha256>> {
    parse_hashes(app, params)?.ok_or_else(|| {
        ApiError::bad_request("Please include some files in your request--file_id or hash based!")
    })
}

/// Ids of the hashes the store knows, in request order.
fn known_ids(app: &AppState, hashes: &[Sha256]) -> ApiResult<Vec<HashId>> {
    let ids = app.store.read(|c| master::hash_ids(c, hashes))?;
    Ok(hashes.iter().filter_map(|h| ids.get(h).copied()).collect())
}

/// Run a files edit off the async runtime.
async fn edit(
    app: Arc<AppState>,
    req: ApiRequest,
    f: impl FnOnce(&AppState, &Params) -> ApiResult<()> + Send + 'static,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddFiles)?;
    let params = req.params.clone();
    app.blocking(move |app| f(app, &params)).await?;
    Ok(ApiResponse::Empty)
}

pub async fn archive_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let ids = known_ids(app, &require_hashes(app, params)?)?;
        app.store.write_content(move |w| w.archive(&ids))?;
        Ok(())
    })
    .await
}

pub async fn unarchive_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let ids = known_ids(app, &require_hashes(app, params)?)?;
        app.store.write_content(move |w| w.inbox(&ids))?;
        Ok(())
    })
    .await
}

/// Keep only domains of the given types.
fn limit_to(app: &AppState, domains: Vec<ServiceId>, types: &[ServiceType]) -> Vec<ServiceId> {
    let snap = app.store.snapshot();
    domains
        .into_iter()
        .filter(|&d| {
            snap.services
                .get(d)
                .is_ok_and(|s| types.contains(&s.service_type()))
        })
        .collect()
}

fn combined_local_media(app: &AppState) -> ApiResult<ServiceId> {
    let snap = app.store.snapshot();
    Ok(snap
        .services
        .of_type(ServiceType::CombinedLocalFileDomains)
        .next()
        .ok_or_else(|| ApiError::server("there is no combined local file domain"))?
        .id)
}

pub async fn delete_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let snap = app.store.snapshot();
        let domains = match location::parse(&snap, params, false)? {
            Some(d) => d.current,
            None => vec![combined_local_media(app)?],
        };
        let reason = params
            .optional::<String>("reason")?
            .unwrap_or_else(|| API_DELETE_REASON.to_owned());
        let hashes = require_hashes(app, params)?;
        let domains = limit_to(
            app,
            domains,
            &[
                ServiceType::HydrusLocalFileStorage,
                ServiceType::CombinedLocalFileDomains,
                ServiceType::LocalFileDomain,
            ],
        );
        // deleting records the deletion even for files we've never seen
        app.store.write_content(move |w| {
            let ids = hashes
                .iter()
                .map(|h| master::intern_hash(w.conn(), h))
                .collect::<hydrus_store::Result<Vec<_>>>()?;
            for domain in domains {
                w.delete_files(domain, &ids, Some(&reason))?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await
}

pub async fn undelete_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let snap = app.store.snapshot();
        let domains = match location::parse(&snap, params, true)? {
            Some(d) => d.current,
            None => vec![combined_local_media(app)?],
        };
        let ids = known_ids(app, &require_hashes(app, params)?)?;
        let domains = limit_to(
            app,
            domains,
            &[
                ServiceType::CombinedLocalFileDomains,
                ServiceType::LocalFileDomain,
            ],
        );
        app.store.write_content(move |w| {
            // only files we still have can come back
            let stored = w.filter_current(w.roles().local_file_storage, &ids)?;
            for domain in domains {
                w.undelete_files(domain, &stored)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await
}

pub async fn clear_file_deletion_record(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let ids = known_ids(app, &require_hashes(app, params)?)?;
        app.store.write_content(move |w| {
            let deleted = w.filter_deleted(w.roles().local_file_storage, &ids)?;
            w.clear_local_delete_records(Some(&deleted))
        })?;
        Ok(())
    })
    .await
}

pub async fn migrate_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    edit(app, req, |app, params| {
        let hashes = require_hashes(app, params)?;
        let snap = app.store.snapshot();
        let Some(domains) = location::parse(&snap, params, false)? else {
            return Err(ApiError::bad_request(
                "Sorry, you need to set a destination for the migration!",
            ));
        };
        for &d in &domains.current {
            if snap.services.get(d)?.service_type() != ServiceType::LocalFileDomain {
                return Err(ApiError::bad_request(
                    "Sorry, any custom file domain here must only declare local file domains.",
                ));
            }
        }
        let clm = combined_local_media(app)?;
        let local = app.store.read(|c| {
            let ids = master::hash_ids(c, &hashes)?;
            let known: Vec<HashId> = ids.values().copied().collect();
            let local = hydrus_store::media::current_in(c, clm, &known)?;
            Ok(hashes
                .iter()
                .map(|h| (*h, ids.get(h).copied().filter(|id| local.contains(id))))
                .collect::<Vec<_>>())
        })?;
        if let Some((hash, _)) = local.iter().find(|(_, id)| id.is_none()) {
            return Err(ApiError::bad_request(format!(
                "The file \"{hash} is not in any local file domains, so I cannot copy!"
            )));
        }
        let local: Vec<HashId> = local.into_iter().filter_map(|(_, id)| id).collect();
        app.store.write_content(move |w| {
            for domain in domains.current {
                // copying to a domain the file was deleted from is an undelete
                let deleted = w.filter_deleted(domain, &local)?;
                let rows: Vec<(HashId, Option<i64>)> = local
                    .iter()
                    .filter(|h| !deleted.contains(h))
                    .map(|&h| (h, Some(w.now_ms())))
                    .collect();
                w.add_files(domain, &rows)?;
                w.undelete_files(domain, &deleted)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await
}

/// A file named by a request: uploaded in the body, or a `path` on this
/// machine (which must exist and be a file).
enum GivenFile {
    Upload(axum::body::Bytes),
    Path(std::path::PathBuf),
}

fn given_file(req: &ApiRequest) -> ApiResult<GivenFile> {
    if let Some(bytes) = &req.upload {
        return Ok(GivenFile::Upload(bytes.clone()));
    }
    let path: String = req.params.required("path")?;
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        return Err(ApiError::bad_request(format!(
            "Path \"{}\" does not exist!",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(ApiError::bad_request(format!(
            "Path \"{}\" is not a file!",
            path.display()
        )));
    }
    Ok(GivenFile::Path(path))
}

pub async fn add_file(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddFiles)?;
    let file = given_file(&req)?;
    let params = req.params.clone();
    let delete_after = params.or("delete_after_successful_import", false)?;
    let encoding = req.response_encoding;
    let result = app
        .blocking(move |app| {
            let snap = app.store.snapshot();
            let manager: ImportOptionsManager = app
                .store
                .read(hydrus_store::settings::get)
                .map_err(|e| ApiError::server(e.to_string()))?;
            let full = manager.full(CallerType::ClientApi, None, &[]);
            let mut options = hydrus_import::FileImportOptions::from_full(&full, &snap.services);
            if let Some(domains) = location::parse(&snap, &params, false)? {
                for &d in &domains.current {
                    if snap.services.get(d)?.service_type() != ServiceType::LocalFileDomain {
                        return Err(ApiError::bad_request(
                            "Sorry, any custom file domain here must only declare local file domains.",
                        ));
                    }
                }
                options.destinations = domains.current;
            }
            let result = match &file {
                GivenFile::Upload(bytes) => app.importer.import_bytes(bytes, &options),
                GivenFile::Path(path) => app.importer.import_path(path, &options),
            }
            .map_err(|e| ApiError::server(e.to_string()))?;
            if let GivenFile::Path(path) = &file
                && delete_after
                && result.status.is_successful()
            {
                let recycle = app
                    .store
                    .read(hydrus_store::settings::get::<hydrus_store::settings::FolderSettings>)
                    .map_or(true, |s| s.delete_to_recycle_bin);
                if let Err(e) = hydrus_store::paths::delete_or_recycle(path, recycle) {
                    tracing::warn!(path = %path.display(), error = %e, "deleting an imported file failed");
                }
            }
            Ok(result)
        })
        .await?;
    Ok(ApiResponse::Json(
        serde_json::json!({
            // (the reference reports an import that raised as an error)
            "status": if result.raised.is_some() {
                hydrus_import::ImportStatus::Error.code()
            } else {
                result.status.code()
            },
            "hash": result.hash.map(|h| h.to_hex()),
            "note": result.note,
        }),
        encoding,
    ))
}

pub async fn generate_hashes(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddFiles)?;
    let file = given_file(&req)?;
    let encoding = req.response_encoding;
    let body = app
        .blocking(move |app| {
            // work on a copy, as the reference does, so the source can change
            let temp =
                tempfile::NamedTempFile::new().map_err(|e| ApiError::server(e.to_string()))?;
            match &file {
                GivenFile::Upload(bytes) => std::fs::write(temp.path(), bytes),
                GivenFile::Path(path) => std::fs::copy(path, temp.path()).map(|_| ()),
            }
            .map_err(|e| ApiError::server(e.to_string()))?;
            let tools = app.importer.tools();
            let hashes = hydrus_media::hash_file(temp.path())
                .map_err(|e| ApiError::server(e.to_string()))?;
            let mut body = serde_json::Map::new();
            body.insert("hash".into(), hashes.sha256.to_hex().into());
            if let Ok(mime) = tools.detect_mime(temp.path()) {
                if hydrus_media::mimes::has_perceptual_hash(mime) {
                    let phashes: Vec<String> = tools
                        .perceptual_hashes(temp.path(), mime)
                        .iter()
                        .map(|p| hex::encode(p.0))
                        .collect();
                    body.insert("perceptual_hashes".into(), phashes.into());
                }
                if hydrus_media::mimes::can_have_pixel_hash(mime)
                    && let Ok(info) = tools.inspect_as(temp.path(), mime)
                    && let Some(pixel) = tools.pixel_hash(temp.path(), &info)
                {
                    body.insert("pixel_hash".into(), pixel.to_hex().into());
                }
            }
            Ok(serde_json::Value::Object(body))
        })
        .await?;
    Ok(ApiResponse::Json(body, encoding))
}
