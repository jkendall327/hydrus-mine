//! Looking up URLs: what kind of URL something is, and which files it has
//! been seen for.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use hydrus_core::TimestampMs;
use hydrus_import::status::{ImportStatus, describe};
use hydrus_store::urls::{self, FileState};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::request::{ApiRequest, ApiResponse};

fn required_url(req: &ApiRequest) -> ApiResult<String> {
    let url = req.params.required::<String>("url")?;
    if url.is_empty() {
        return Err(ApiError::bad_request("Given URL was empty!"));
    }
    Ok(url)
}

pub async fn get_url_info(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddUrls)?;
    let url = required_url(&req)?;
    let snapshot = app.store.snapshot();
    let classes = &snapshot.url_classes;
    let normalised = classes
        .normalise(&url, false)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let capability = classes.parse_capability(&normalised);
    let url_type_string = capability.url_type.name().ok_or_else(|| {
        ApiError::server(format!(
            "url type {} has no name",
            capability.url_type.code()
        ))
    })?;
    let request_url = classes
        .url_to_fetch(&normalised)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut body = json!({
        "normalised_url": normalised,
        "url_type": capability.url_type.code(),
        "url_type_string": url_type_string,
        "match_name": capability.match_name,
        "can_parse": capability.parser.is_ok(),
        "request_url": request_url,
    });
    if let Err(reason) = capability.parser {
        body["cannot_parse_reason"] = json!(reason);
    }
    Ok(ApiResponse::json(body, &req))
}

pub async fn get_url_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddUrls)?;
    let url = required_url(&req)?;
    let doublecheck = req.params.or("doublecheck_file_system", false)?;
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let normalised = snapshot
                .url_classes
                .normalise(&url, false)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            let search = snapshot.url_classes.search_urls(&normalised);
            let files = app
                .store
                .read(|conn| urls::files_for_urls(conn, &snapshot.services, &search))?;
            let now = TimestampMs::now();
            let statuses: Vec<Json> = files
                .into_iter()
                .map(|file| {
                    let (mut status, mut note) = describe(&file.state, "url recognised: ", now);
                    if doublecheck
                        && let FileState::Imported {
                            mime: Some(mime), ..
                        } = file.state
                        && !snapshot
                            .storage
                            .file_path(&file.hash, mime)
                            .is_some_and(|p| p.is_file())
                    {
                        status = ImportStatus::Unknown;
                        note = "The client believed this file was already in the db, but it was truly missing! Import will go ahead, in an attempt to fix the situation.".into();
                    }
                    json!({
                        "status": status as u8,
                        "hash": file.hash.to_hex(),
                        "note": note,
                    })
                })
                .collect();
            Ok(ApiResponse::Json(
                json!({ "normalised_url": normalised, "url_file_statuses": statuses }),
                encoding,
            ))
        })
        .await
}

pub async fn associate_url(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::AddUrls)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let gather = |one: &str, many: &str| -> ApiResult<Vec<String>> {
            let mut urls = Vec::new();
            if let Some(url) = params.optional::<String>(one)? {
                urls.push(url);
            }
            urls.extend(params.or::<Vec<String>>(many, Vec::new())?);
            Ok(urls)
        };
        let mut to_add = gather("url_to_add", "urls_to_add")?;
        let to_delete = gather("url_to_delete", "urls_to_delete")?;
        if params.or("normalise_urls", true)? {
            let snapshot = app.store.snapshot();
            to_add = to_add
                .iter()
                .map(|url| snapshot.url_classes.normalise(url, false))
                .collect::<Result<_, _>>()
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
        }
        if to_add.is_empty() && to_delete.is_empty() {
            return Err(ApiError::bad_request(
                "Did not find any URLs to add or delete!",
            ));
        }
        let hashes = crate::routes::files::parse_hashes(app, &params)?.unwrap_or_default();
        if hashes.is_empty() {
            return Err(ApiError::bad_request(
                "Did not find any hashes to apply the urls to!",
            ));
        }
        app.store.write_content(move |w| {
            let ids = hashes
                .iter()
                .map(|h| hydrus_store::master::intern_hash(w.conn(), h))
                .collect::<hydrus_store::Result<Vec<_>>>()?;
            w.add_urls(&ids, &to_add)?;
            w.delete_urls(&ids, &to_delete)
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}
