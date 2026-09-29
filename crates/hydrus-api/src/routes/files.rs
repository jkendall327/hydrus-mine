//! Fetching files, thumbnails, paths and metadata.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use serde_json::{Value as Json, json};

use hydrus_core::{HashId, HashKind, Mime, Sha256};
use hydrus_store::media::{self, MediaResult};
use hydrus_store::{Snapshot, master};

use crate::AppState;
use crate::auth::{AccessPermissions, Permission};
use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::media_json::{self, MetadataOptions};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};
use crate::services_json;

/// The files a request names, by `hash`/`hashes`/`file_id`/`file_ids`, in
/// order, de-duplicated. `None` if it names none at all.
pub fn parse_hashes(app: &AppState, params: &Params) -> ApiResult<Option<Vec<Sha256>>> {
    let mut named = false;
    let mut raw: Vec<Vec<u8>> = Vec::new();
    if let Some(hash) = params.optional::<Vec<u8>>("hash")? {
        named = true;
        raw.push(hash);
    }
    if let Some(hashes) = params.optional::<Vec<Vec<u8>>>("hashes")? {
        named = true;
        raw.extend(hashes);
    }
    let mut file_ids: Vec<i64> = Vec::new();
    if let Some(id) = params.optional::<i64>("file_id")? {
        named = true;
        file_ids.push(id);
    }
    if let Some(ids) = params.optional::<Vec<i64>>("file_ids")? {
        named = true;
        file_ids.extend(ids);
    }
    if !named {
        return Ok(None);
    }
    let mut hashes = Vec::with_capacity(raw.len() + file_ids.len());
    for bytes in raw {
        hashes.push(Sha256::from_slice(&bytes).map_err(|_| {
            ApiError::bad_request(format!(
                "Sorry, one of the given hashes was the wrong length! sha256 hashes should be 32 bytes long, but {} is {} bytes long!",
                hex::encode(&bytes),
                bytes.len()
            ))
        })?);
    }
    if !file_ids.is_empty() {
        if file_ids.iter().any(|&id| id < 0) {
            return Err(ApiError::bad_request("Was asked about a negative hash_id!"));
        }
        let ids: Vec<HashId> = file_ids
            .iter()
            .map(|&id| u32::try_from(id).map(HashId))
            .collect::<Result<_, _>>()
            .map_err(|_| {
                ApiError::bad_request("Was asked about a hash_id that was way too big!")
            })?;
        let found = app.store.read(|c| master::hashes(c, &ids))?;
        let missing: Vec<u32> = ids
            .iter()
            .filter(|id| !found.contains_key(id))
            .map(|id| id.get())
            .collect();
        if !missing.is_empty() {
            return Err(ApiError::not_found(format!(
                "It seems you gave a file_id that does not exist! Was asked about these novel hash_ids: {missing:?}"
            )));
        }
        hashes.extend(ids.iter().map(|id| found[id]));
    }
    let mut seen = std::collections::HashSet::new();
    hashes.retain(|h| seen.insert(*h));
    if hashes.is_empty() {
        return Err(ApiError::bad_request(
            "Sorry, I was expecting at least 1 sha256 hash, but none were given!",
        ));
    }
    Ok(Some(hashes))
}

fn require_hashes(app: &AppState, params: &Params) -> ApiResult<Vec<Sha256>> {
    parse_hashes(app, params)?.ok_or_else(|| {
        ApiError::bad_request("Please include some files in your request--file_id or hash based!")
    })
}

pub async fn file_metadata(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SearchFiles)?;
    let p = &req.params;
    let only_identifiers = p.or("only_return_identifiers", false)?;
    let only_basic = p.or("only_return_basic_information", false)?;
    let create_new_file_ids = p.or("create_new_file_ids", false)?;
    let include_services_object = p.or("include_services_object", true)?;
    let opts = MetadataOptions {
        include_notes: p.or("include_notes", false)?,
        include_milliseconds: p.or("include_milliseconds", false)?,
        include_blurhash: p.or("include_blurhash", false)?,
        hide_service_keys_tags: p.or("hide_service_keys_tags", true)?,
        detailed_urls: p.or("detailed_url_information", false)?,
    };
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let hashes = require_hashes(app, &params)?;
            let mut ids: HashMap<Sha256, HashId> =
                app.store.read(|c| master::hash_ids(c, &hashes))?;
            if create_new_file_ids && ids.len() < hashes.len() {
                let novel: Vec<Sha256> = hashes
                    .iter()
                    .filter(|h| !ids.contains_key(h))
                    .copied()
                    .collect();
                let created = app.store.write(move |ctx| {
                    novel
                        .iter()
                        .map(|h| Ok((*h, master::intern_hash(ctx.conn(), h)?)))
                        .collect::<hydrus_store::Result<Vec<_>>>()
                })?;
                ids.extend(created);
            }
            let hash_ids: Vec<HashId> = hashes.iter().filter_map(|h| ids.get(h).copied()).collect();
            app.access.check_can_see(&perms, &hash_ids)?;

            let snapshot = app.store.snapshot();
            let rows: Vec<Json> = if only_identifiers {
                // known hashes only; nothing else to load
                hashes
                    .iter()
                    .map(|h| match ids.get(h) {
                        Some(id) => json!({ "file_id": id.get(), "hash": h.to_hex() }),
                        None => media_json::missing_row(h),
                    })
                    .collect()
            } else if only_basic {
                let results = app.store.read(|c| media::load_basic(c, &hash_ids))?;
                let by_hash: HashMap<Sha256, &MediaResult> =
                    results.iter().map(|m| (m.hash, m)).collect();
                hashes
                    .iter()
                    .map(|h| match by_hash.get(h) {
                        Some(m) => media_json::basic_row(m, opts.include_blurhash),
                        None => media_json::missing_row(h),
                    })
                    .collect()
            } else {
                let batch = app.store.read(|c| {
                    media::load(c, &snapshot.services, Some(&snapshot.display), &hash_ids)
                })?;
                let by_hash: HashMap<Sha256, &MediaResult> =
                    batch.results.iter().map(|m| (m.hash, m)).collect();
                hashes
                    .iter()
                    .map(|h| match by_hash.get(h) {
                        Some(m) => media_json::full_row(&snapshot, m, &batch.tags, opts),
                        None => media_json::missing_row(h),
                    })
                    .collect()
            };
            let mut body = serde_json::Map::new();
            body.insert("metadata".into(), Json::Array(rows));
            if include_services_object {
                body.insert(
                    "services".into(),
                    services_json::services_dict(&snapshot.services),
                );
                body.insert(
                    "services_v2".into(),
                    services_json::services_list(&snapshot.services),
                );
            }
            Ok(ApiResponse::Json(Json::Object(body), encoding))
        })
        .await
}

pub async fn file_hashes(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::SearchFiles)?;
    let source: HashKind = req
        .params
        .or("source_hash_type", "sha256".to_owned())?
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;
    let desired: HashKind = req
        .params
        .required::<String>("desired_hash_type")?
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;
    let mut raw: Vec<Vec<u8>> = req
        .params
        .optional::<Vec<u8>>("hash")?
        .into_iter()
        .collect();
    raw.extend(
        req.params
            .optional::<Vec<Vec<u8>>>("hashes")?
            .unwrap_or_default(),
    );
    if raw.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Sorry, I was expecting at least 1 {} hash, but none were given!",
            hash_kind_name(source)
        )));
    }
    for h in &raw {
        if h.len() != source.byte_len() {
            return Err(ApiError::bad_request(format!(
                "Sorry, one of the given hashes was the wrong length! {} hashes should be {} bytes long, but {} is {} bytes long!",
                hash_kind_name(source),
                source.byte_len(),
                hex::encode(h),
                h.len()
            )));
        }
    }
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let pairs = app
                .store
                .read(|c| hydrus_store::master::convert_hashes(c, source, desired, &raw))?;
            let map: serde_json::Map<String, Json> = pairs
                .into_iter()
                .map(|(from, to)| (hex::encode(from), json!(hex::encode(to))))
                .collect();
            Ok(ApiResponse::Json(json!({ "hashes": map }), encoding))
        })
        .await
}

fn hash_kind_name(kind: HashKind) -> &'static str {
    match kind {
        HashKind::Sha256 => "sha256",
        HashKind::Md5 => "md5",
        HashKind::Sha1 => "sha1",
        HashKind::Sha512 => "sha512",
    }
}

/// The one file a request names by `file_id` or `hash`, checking the caller
/// may see it. An unknown hash yields an empty result rather than an error.
fn fetch_one(
    app: &AppState,
    perms: &AccessPermissions,
    params: &Params,
) -> ApiResult<(Arc<Snapshot>, Sha256, Option<MediaResult>)> {
    let snapshot = app.store.snapshot();
    let (hash, hash_id) = if let Some(id) = params.optional::<i64>("file_id")? {
        let id = HashId(u32::try_from(id).map_err(|_| {
            ApiError::not_found("One or more of those file identifiers was missing!")
        })?);
        app.access.check_can_see(perms, &[id])?;
        let hash = app.store.read(|c| master::hash(c, id))?.ok_or_else(|| {
            ApiError::not_found("One or more of those file identifiers was missing!")
        })?;
        (hash, Some(id))
    } else if let Some(raw) = params.optional::<Vec<u8>>("hash")? {
        perms.check_can_see_all_files()?;
        let hash = Sha256::from_slice(&raw)
            .map_err(|_| ApiError::bad_request("Sorry, that hash was the wrong length!"))?;
        let id = app.store.read(|c| master::hash_id(c, &hash))?;
        (hash, id)
    } else {
        return Err(ApiError::bad_request(
            "Please include a file_id or hash parameter!",
        ));
    };
    let result = match hash_id {
        Some(id) => app
            .store
            .read(|c| media::load(c, &snapshot.services, None, &[id]))?
            .results
            .pop(),
        None => None,
    };
    Ok((snapshot, hash, result))
}

fn is_local(snapshot: &Snapshot, m: &MediaResult) -> bool {
    snapshot
        .services
        .all()
        .find(|s| {
            matches!(
                s.kind,
                hydrus_store::services::ServiceKind::LocalFileStorage
            )
        })
        .is_some_and(|s| m.is_current_in(s.id))
}

fn mime_of(m: Option<&MediaResult>) -> Mime {
    m.and_then(|m| m.info.as_ref())
        .map_or(Mime::ApplicationUnknown, |i| i.mime)
}

pub async fn file(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SearchFiles)?;
    let params = req.params.clone();
    let (path, mime) = app
        .clone()
        .blocking(move |app| {
            let (snapshot, hash, m) = fetch_one(app, &perms, &params)?;
            if !m.as_ref().is_some_and(|m| is_local(&snapshot, m)) {
                return Err(ApiError::new(
                    ErrorKind::FileMissing,
                    "The client does not have this file!",
                ));
            }
            let mime = mime_of(m.as_ref());
            let path = snapshot
                .storage
                .file_path(&hash, mime)
                .filter(|p| p.is_file())
                .ok_or_else(|| ApiError::not_found("That file seems to be missing!"))?;
            Ok((path, mime))
        })
        .await?;
    let body = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::not_found("That file seems to be missing!"))?;
    Ok(ApiResponse::Bytes {
        content_type: mime.mimetype().to_owned(),
        body: Bytes::from(body),
        cache: true,
    })
}

pub async fn thumbnail(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SearchFiles)?;
    let params = req.params.clone();
    let (stored, mime) = app
        .clone()
        .blocking(move |app| {
            let (snapshot, hash, m) = fetch_one(app, &perms, &params)?;
            let mime = mime_of(m.as_ref());
            let stored = if mime.has_thumbnail() {
                snapshot
                    .storage
                    .thumbnail_path(&hash)
                    .filter(|p| p.is_file())
            } else {
                None
            };
            Ok((stored, mime))
        })
        .await?;
    if let Some(path) = stored
        && let Ok(bytes) = tokio::fs::read(&path).await
    {
        let content_type = thumbnail_content_type(&bytes);
        return Ok(ApiResponse::Bytes {
            content_type: content_type.into(),
            body: Bytes::from(bytes),
            cache: true,
        });
    }
    Ok(ApiResponse::Bytes {
        content_type: "image/png".into(),
        body: Bytes::from_static(default_thumbnail(mime)),
        cache: true,
    })
}

/// Thumbnails are stored without an extension; sniff whether they are PNG.
fn thumbnail_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else {
        "image/jpeg"
    }
}

/// The type icon shown for files without a rendered thumbnail.
fn default_thumbnail(mime: Mime) -> &'static [u8] {
    macro_rules! icon {
        ($name:literal) => {
            include_bytes!(concat!("../../../../static/", $name))
        };
    }
    match mime {
        Mime::ApplicationPdf => icon!("pdf.png"),
        Mime::ApplicationDocx => icon!("docx.png"),
        Mime::ApplicationXlsx => icon!("xlsx.png"),
        Mime::ApplicationPptx => icon!("pptx.png"),
        Mime::ApplicationDoc => icon!("doc.png"),
        Mime::ApplicationXls => icon!("xls.png"),
        Mime::ApplicationPpt => icon!("ppt.png"),
        Mime::ApplicationEpub => icon!("epub.png"),
        Mime::ApplicationDjvu => icon!("djvu.png"),
        Mime::ApplicationPsd => icon!("psd.png"),
        Mime::ApplicationClip => icon!("clip.png"),
        Mime::ApplicationSai2 => icon!("sai.png"),
        Mime::ApplicationKrita => icon!("krita.png"),
        Mime::ApplicationPaintDotNet => icon!("paintnet.png"),
        Mime::ApplicationFlash => icon!("flash.png"),
        Mime::ApplicationXcf => icon!("xcf.png"),
        Mime::ApplicationProcreate => icon!("procreate.png"),
        Mime::ApplicationRtf => icon!("rtf.png"),
        Mime::ImageSvg => icon!("svg.png"),
        Mime::ImageOpenraster => icon!("image.png"),
        m => match m.general_class() {
            Some(Mime::GeneralAudio) => icon!("audio.png"),
            Some(Mime::GeneralVideo | Mime::GeneralAnimation) => icon!("video.png"),
            Some(Mime::GeneralApplicationArchive) => icon!("zip.png"),
            Some(Mime::GeneralImage) => icon!("image.png"),
            _ => icon!("hydrus.png"),
        },
    }
}

pub async fn file_path(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SeeLocalPaths)?;
    perms.check(Permission::SearchFiles)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let (snapshot, hash, m) = fetch_one(app, &perms, &params)?;
            let Some(m) = m.filter(|m| is_local(&snapshot, m)) else {
                return Err(ApiError::new(ErrorKind::FileMissing, "The client does not have this file!"));
            };
            let info = m.info.as_ref().ok_or_else(|| ApiError::not_found("That file seems to be missing!"))?;
            let path = snapshot
                .storage
                .file_path(&hash, info.mime)
                .filter(|p| p.is_file())
                .ok_or_else(|| ApiError::not_found("That file seems to be missing!"))?;
            Ok(ApiResponse::Json(
                json!({ "path": path.to_string_lossy(), "filetype": info.mime.mimetype(), "size": info.size }),
                encoding,
            ))
        })
        .await
}

pub async fn thumbnail_path(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SeeLocalPaths)?;
    perms.check(Permission::SearchFiles)?;
    let include_filetype = req.params.or("include_thumbnail_filetype", false)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let (snapshot, hash, m) = fetch_one(app, &perms, &params)?;
            if !mime_of(m.as_ref()).has_thumbnail() {
                return Err(ApiError::bad_request(
                    "Sorry, this file type does not have a thumbnail!",
                ));
            }
            let path = snapshot
                .storage
                .thumbnail_path(&hash)
                .filter(|p| p.is_file())
                .ok_or_else(|| {
                    ApiError::new(ErrorKind::FileMissing, "Could not find that thumbnail!")
                })?;
            let mut body = json!({ "path": path.to_string_lossy() });
            if include_filetype {
                let bytes = std::fs::read(&path).map_err(|_| {
                    ApiError::new(ErrorKind::FileMissing, "Could not find that thumbnail!")
                })?;
                body["filetype"] = json!(thumbnail_content_type(&bytes));
            }
            Ok(ApiResponse::Json(body, encoding))
        })
        .await
}

pub async fn local_file_storage_locations(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    let perms = app.authenticate(&req)?;
    perms.check(Permission::SeeLocalPaths)?;
    perms.check(Permission::SearchFiles)?;
    let snapshot = app.store.snapshot();
    let mut locations: Vec<&hydrus_store::storage::StorageLocation> =
        snapshot.storage.locations().iter().collect();
    locations.sort_by(|a, b| a.path.cmp(&b.path));
    let list: Vec<Json> = locations
        .into_iter()
        .map(|l| {
            let mut prefixes = l.prefixes.clone();
            prefixes.sort();
            json!({ "path": l.path.to_string_lossy(), "ideal_weight": l.ideal_weight, "max_num_bytes": l.max_bytes, "prefixes": prefixes })
        })
        .collect();
    Ok(ApiResponse::json(json!({ "locations": list }), &req))
}
