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

/// `/add_urls/add_url`: queue a URL for download, in the URL queue (the
/// reference's "page") named or keyed, else any, else a new one.
pub async fn add_url(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    use std::collections::BTreeSet;

    use hydrus_core::import_options::{CallerType, ImportOptionsSlice};
    use hydrus_core::url::UrlType;

    let perms = app.authenticate(&req)?;
    perms.check(Permission::AddUrls)?;
    let url = required_url(&req)?;
    let clean = |tags: Vec<String>| -> BTreeSet<String> {
        tags.iter()
            .filter_map(|t| hydrus_core::Tag::new(t).map(|t| t.as_str().to_owned()))
            .collect()
    };
    let snapshot = app.store.snapshot();
    let mut filterable_tags = BTreeSet::new();
    if let Some(tags) = req.params.optional::<Vec<String>>("filterable_tags")? {
        perms.check(Permission::AddTags)?;
        filterable_tags = clean(tags);
    }
    let mut additional_tags: Vec<(String, BTreeSet<String>)> = Vec::new();
    if let Some(dict) = req
        .params
        .optional::<Vec<(Vec<u8>, Json)>>("service_keys_to_additional_tags")?
    {
        perms.check(Permission::AddTags)?;
        for (key, tags) in dict {
            crate::location::check_tag_service(&snapshot, &key)?;
            let tags = tags
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .map(|t| t.as_str().map(str::to_owned))
                        .collect::<Option<Vec<String>>>()
                })
                .ok_or_else(|| {
                    ApiError::bad_request(
                        "The tags in service_keys_to_additional_tags should be lists of strings!",
                    )
                })?;
            let tags = clean(tags);
            if !tags.is_empty() {
                additional_tags.push((hex::encode(key), tags));
            }
        }
    }
    let page_name = req.params.optional::<String>("destination_page_name")?;
    let page_key = req.params.optional::<Vec<u8>>("destination_page_key")?;
    let destinations = match crate::location::parse(&snapshot, &req.params, false)? {
        Some(domains) => {
            for &d in &domains.current {
                if snapshot.services.get(d)?.service_type()
                    != hydrus_core::ServiceType::LocalFileDomain
                {
                    return Err(ApiError::bad_request(
                        "Sorry, any custom file domain here must only declare local file domains.",
                    ));
                }
            }
            Some(
                domains
                    .current
                    .iter()
                    .map(|&d| snapshot.services.get(d).map(|s| s.key.to_hex()))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        None => None,
    };
    let Some(downloads) = app.downloads.clone() else {
        return Err(ApiError::server("The downloader is not running."));
    };
    let encoding = req.response_encoding;

    let classes = &snapshot.url_classes;
    if hydrus_core::url::functions::check_full_url(&url).is_err() {
        return Err(ApiError::bad_request(format!(
            "Could not parse \"{url}\" at all!"
        )));
    }
    let collapse = classes.settings().collapse_leading_slashes;
    let url = hydrus_core::url::ensure_url_is_encoded(&url, true, collapse);
    let url = classes
        .normalise(&url, true)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let capability = classes.parse_capability(&url);
    if matches!(
        capability.url_type,
        UrlType::Gallery | UrlType::Post | UrlType::Watchable
    ) && let Err(reason) = &capability.parser
    {
        return Err(ApiError::bad_request(format!(
            "This URL was recognised as a \"{}\" but it cannot be parsed: {reason}\n\nSince this URL cannot be parsed, a downloader cannot be created for it! Please check your url class links under the 'networking' menu.",
            capability.match_name
        )));
    }
    if capability.url_type == UrlType::Watchable {
        return Err(ApiError::bad_request(format!(
            "\"{}\" is a watchable URL, and hydrus-rs cannot watch threads yet.",
            capability.match_name
        )));
    }
    if !matches!(
        capability.url_type,
        UrlType::Unknown | UrlType::File | UrlType::Post | UrlType::Gallery
    ) {
        return Err(ApiError::bad_request(format!(
            "\"{}\" URL was accepted but not added successfully--could not find/generate a new downloader page for it.",
            capability.match_name
        )));
    }
    let match_name = capability.match_name.clone();
    let normalised = url.clone();
    app.blocking(move |app| {
        let slice = match destinations {
            None => None,
            Some(destinations) => {
                let full = downloads
                    .downloader()
                    .full_options(
                        CallerType::PostUrls,
                        &ImportOptionsSlice::default(),
                        &[&url],
                    )
                    .map_err(|e| ApiError::server(e.to_string()))?;
                let mut locations = full.locations;
                locations.destinations = destinations;
                Some(ImportOptionsSlice {
                    locations: Some(locations),
                    ..ImportOptionsSlice::default()
                })
            }
        };
        let queue = downloads
            .url_queue_for(page_name.as_deref(), page_key.as_deref(), slice.as_ref())
            .map_err(|e| ApiError::server(e.to_string()))?;
        downloads
            .pend_urls(queue.id, &[url], &filterable_tags, &additional_tags)
            .map_err(|e| ApiError::server(e.to_string()))?;
        let _ = &app;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Json(
        json!({
            "human_result_text": format!("\"{match_name}\" URL added successfully."),
            "normalised_url": normalised,
        }),
        encoding,
    ))
}
