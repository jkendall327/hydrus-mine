//! Fetching files, thumbnails, paths and metadata.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, header};
use serde_json::{Value as Json, json};

use hydrus_core::{HashId, HashKind, Mime, Sha256};
use hydrus_store::media::{self, MediaResult};
use hydrus_store::{Snapshot, master};

use crate::AppState;
use crate::auth::{AccessPermissions, Permission};
use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::media_json::{self, MetadataOptions};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse, ByteRange, FileSource};
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
                let tag_names = media_json::TagNames::new(&batch.tags);
                hashes
                    .iter()
                    .map(|h| match by_hash.get(h) {
                        Some(m) => media_json::full_row(&snapshot, m, &tag_names, opts),
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
    let attachment = req.params.or("download", false)?;
    file_from_path(path, mime.mimetype().to_owned(), attachment, &req)
}

/// A file on disk as a response, with the request's byte range.
fn file_from_path(
    path: std::path::PathBuf,
    content_type: String,
    attachment: bool,
    req: &ApiRequest,
) -> ApiResult<ApiResponse> {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source = FileSource::Path(path);
    let size = source
        .size()
        .map_err(|_| ApiError::not_found("That file seems to be missing!"))?;
    Ok(ApiResponse::File {
        range: parse_range(&req.headers, size)?,
        source,
        filename,
        content_type,
        attachment,
    })
}

/// The `Range` header as the reference reads it (`_parseRangeHeader`): at
/// most one range of bytes. Where the reference fails (an end at exactly the
/// file's size, a start past the end with no end, a suffix longer than the
/// file) we give what was meant: the range clamped to the file, the whole
/// file, the whole file.
pub(crate) fn parse_range(headers: &HeaderMap, size: u64) -> ApiResult<Option<ByteRange>> {
    let Some(value) = headers.get(header::RANGE) else {
        return Ok(None);
    };
    let value = String::from_utf8_lossy(value.as_bytes());
    let unsatisfiable = |m: &str| ApiError::new(ErrorKind::RangeNotSatisfiable, m);
    let Some((unit, pairs)) = value.split_once('=') else {
        return Err(ApiError::bad_request("Did not understand range header!"));
    };
    if unit != "bytes" {
        return Err(unsatisfiable(
            "Do not support anything other than bytes in Range header!",
        ));
    }
    let pairs: Vec<&str> = pairs.split(',').collect();
    if pairs.iter().any(|p| !p.contains('-')) {
        return Err(unsatisfiable(
            "Did not understand the Range header's range pair(s)!",
        ));
    }
    let mut ranges = Vec::new();
    for pair in pairs {
        let parts: Vec<&str> = pair.trim().split('-').collect();
        let [start, end] = parts[..] else {
            return Err(ApiError::new(
                ErrorKind::ValueError,
                "too many values to unpack (expected 2)",
            ));
        };
        let start = if start.is_empty() {
            if end.is_empty() {
                return Err(unsatisfiable("Undefined Range header pair given!"));
            }
            None
        } else {
            Some(python_int(start)?)
        };
        let end = if end.is_empty() {
            None
        } else {
            Some(python_int(end)?)
        };
        match (start, end) {
            (Some(s), Some(e)) if s > e => {
                return Err(unsatisfiable("The Range header had an invalid pair!"));
            }
            (None, Some(e)) => {
                let length = e.min(size);
                ranges.push(ByteRange {
                    offset: size - length,
                    length,
                    end: size.saturating_sub(1),
                    size,
                });
            }
            (Some(s), None) if s <= size => ranges.push(ByteRange {
                offset: s,
                length: size - s,
                end: size.saturating_sub(1),
                size,
            }),
            (Some(s), Some(e)) if s <= size => {
                let e = e.min(size.saturating_sub(1));
                ranges.push(ByteRange {
                    offset: s,
                    length: (e + 1).saturating_sub(s).min(size - s),
                    end: e,
                    size,
                });
            }
            // a range starting past the end: the whole file
            _ => {
                ranges.clear();
                break;
            }
        }
    }
    match ranges[..] {
        [] => Ok(None),
        [range] => Ok(Some(range)),
        _ => Err(unsatisfiable(
            "Can only support Single Range requests at the moment!",
        )),
    }
}

/// `abs(int(text))`, as Python reads an integer: surrounding whitespace, a
/// sign, and underscores between digits allowed.
fn python_int(text: &str) -> ApiResult<u64> {
    let invalid = || {
        ApiError::new(
            ErrorKind::ValueError,
            format!("invalid literal for int() with base 10: '{text}'"),
        )
    };
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return Err(invalid());
    }
    digits.replace('_', "").parse().map_err(|_| invalid())
}

/// Whether `/get_files/render` renders this kind of file
/// (`MediaResult.IsStaticImage`).
fn is_static_image(mime: Mime) -> bool {
    matches!(
        mime,
        Mime::ImageJpeg
            | Mime::ImagePng
            | Mime::ImageGif
            | Mime::ImageWebp
            | Mime::ImageJxl
            | Mime::ImageAvif
            | Mime::ImageBmp
            | Mime::ImageHeic
            | Mime::ImageHeif
            | Mime::ImageIcon
            | Mime::ImageQoi
            | Mime::ImageTiff
            | Mime::ApplicationPsd
            | Mime::ApplicationKrita
            | Mime::ImageOpenraster
    )
}

/// A static image, decoded as the reference's media viewer decodes it,
/// optionally resized, and encoded as PNG, JPEG or WebP.
pub async fn render(State(app): State<Arc<AppState>>, req: ApiRequest) -> ApiResult<ApiResponse> {
    use hydrus_media::encode::{RenderFormat, encode_render};
    use hydrus_media::resample::{Interpolation, resize};

    let perms = app.authenticate(&req)?;
    perms.check(Permission::SearchFiles)?;
    let p = req.params.clone();
    let not_an_image = || ApiError::bad_request("Requested file is not an image!");
    let attachment = p.or("download", false)?;
    let encoding = app
        .clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            // an unknown file is not an image (the reference makes an empty
            // media result for it)
            let hash_id = if let Some(id) = p.optional::<i64>("file_id")? {
                let id = HashId(u32::try_from(id).map_err(|_| not_an_image())?);
                app.access.check_can_see(&perms, &[id])?;
                Some(id)
            } else if let Some(raw) = p.optional::<Vec<u8>>("hash")? {
                perms.check_can_see_all_files()?;
                let hash = Sha256::from_slice(&raw)
                    .map_err(|_| ApiError::bad_request("Sorry, that hash was the wrong length!"))?;
                app.store.read(|c| master::hash_id(c, &hash))?
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
                    .into_iter()
                    .next(),
                None => None,
            };
            let Some((hash, mime)) = result.and_then(|m| Some((m.hash, m.info?.mime))) else {
                return Err(not_an_image());
            };
            if mime == Mime::AnimationUgoira {
                return Err(ApiError::bad_request(
                    "Sorry, rendering ugoiras is not supported yet!",
                ));
            }
            if !is_static_image(mime) {
                return Err(not_an_image());
            }
            let format = match p.optional::<i64>("render_format")? {
                None => RenderFormat::Png,
                Some(code) => match u8::try_from(code).ok().and_then(Mime::from_code) {
                    Some(Mime::ImagePng) => RenderFormat::Png,
                    Some(Mime::ImageJpeg) => RenderFormat::Jpeg,
                    Some(Mime::ImageWebp) => RenderFormat::Webp,
                    _ => return Err(ApiError::bad_request("Invalid render format!")),
                },
            };
            let path = snapshot
                .storage
                .file_path(&hash, mime)
                .filter(|p| p.is_file())
                .ok_or_else(|| ApiError::not_found("That file seems to be missing!"))?;
            let mut image = app
                .importer
                .tools()
                .load_image(&path, mime)
                .map_err(|e| ApiError::server(format!("Could not render that file: {e}")))?;
            if let (Some(width), Some(height)) =
                (p.optional::<i64>("width")?, p.optional::<i64>("height")?)
            {
                if width < 1 {
                    return Err(ApiError::bad_request("Width must be greater than 0!"));
                }
                if height < 1 {
                    return Err(ApiError::bad_request("Height must be greater than 0!"));
                }
                let (w, h) = (
                    u32::try_from(width).map_err(|_| ApiError::bad_request("Width is too big!"))?,
                    u32::try_from(height)
                        .map_err(|_| ApiError::bad_request("Height is too big!"))?,
                );
                // `ResizeNumPyImage`, with its comparisons as they are
                let (iw, ih) = (image.width(), image.height());
                if !(w == iw && h == w) {
                    let interpolation = if w > ih || h > iw {
                        Interpolation::Lanczos4
                    } else {
                        Interpolation::Area
                    };
                    image = resize(&image, w, h, interpolation);
                }
            }
            let quality = match p.optional::<i64>("render_quality")? {
                Some(q) => q,
                None if format == RenderFormat::Png => 1,
                None => 80,
            };
            let body = encode_render(&image, format, quality)
                .map_err(|e| ApiError::server(e.to_string()))?;
            let content_type = match format {
                RenderFormat::Png => "image/png",
                RenderFormat::Jpeg => "image/jpeg",
                RenderFormat::Webp => "image/webp",
            };
            Ok((content_type, body))
        })
        .await?;
    let (content_type, body) = encoding;
    Ok(ApiResponse::Bytes {
        content_type: content_type.into(),
        body: body.into(),
        cache: true,
        attachment,
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
                    .or_else(|| regenerated_thumbnail(app, m.as_ref()))
            } else {
                None
            };
            Ok((stored, mime))
        })
        .await?;
    if let Some(path) = stored {
        let mut head = [0u8; 4];
        if let Ok(mut f) = std::fs::File::open(&path) {
            use std::io::Read as _;
            let _ = f.read(&mut head);
            let content_type = thumbnail_content_type(&head).to_owned();
            return file_from_path(path, content_type, false, &req);
        }
    }
    let (filename, icon) = default_thumbnail(mime);
    let source = FileSource::Static(icon);
    Ok(ApiResponse::File {
        range: parse_range(&req.headers, icon.len() as u64)?,
        source,
        filename: filename.into(),
        content_type: "image/png".into(),
        attachment: false,
    })
}

/// A missing thumbnail, made again from its file (`GetThumbnailPath`).
fn regenerated_thumbnail(
    app: &AppState,
    media: Option<&MediaResult>,
) -> Option<std::path::PathBuf> {
    let media = media?;
    match app.importer.regenerate_thumbnail(media) {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(hash = %media.hash, error = %e, "regenerating a thumbnail failed");
            None
        }
    }
}

/// Thumbnails are stored without an extension; sniff whether they are PNG.
fn thumbnail_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else {
        "image/jpeg"
    }
}

/// The type icon shown for files without a rendered thumbnail, and its
/// file name.
fn default_thumbnail(mime: Mime) -> (&'static str, &'static [u8]) {
    macro_rules! icon {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!("../../../../static/", $name)).as_slice(),
            )
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
                .or_else(|| regenerated_thumbnail(app, m.as_ref()))
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
