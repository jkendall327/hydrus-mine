//! `/add_files/*` endpoints that change which domains files are in.

use std::sync::Arc;

use axum::extract::State;

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
