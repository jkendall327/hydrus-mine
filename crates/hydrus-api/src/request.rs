//! Turning HTTP requests into [`ApiRequest`]s and results into responses.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value as Json, json};

use hydrus_core::{CLIENT_API_VERSION, REFERENCE_VERSION};

use crate::error::ApiError;
use crate::params::Params;

/// Response encodings the API speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Json,
    Cbor,
}

/// A parsed API request.
#[derive(Debug)]
pub struct ApiRequest {
    pub method: Method,
    pub headers: HeaderMap,
    pub params: Params,
    /// The raw body, for uploads that aren't JSON/CBOR (e.g. file bytes).
    pub upload: Option<Bytes>,
    pub response_encoding: Encoding,
}

fn response_encoding(headers: &HeaderMap, params: &Params) -> Encoding {
    let header = |name: header::HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    let accept = header(header::ACCEPT);
    if accept.contains("cbor") && !accept.contains("json") {
        return Encoding::Cbor;
    }
    if accept.contains("json") && !accept.contains("cbor") {
        return Encoding::Json;
    }
    let content_type = header(header::CONTENT_TYPE);
    if content_type.contains("cbor") {
        return Encoding::Cbor;
    }
    if content_type.contains("json") {
        return Encoding::Json;
    }
    if params.cbor_requested {
        Encoding::Cbor
    } else {
        Encoding::Json
    }
}

impl<S: Send + Sync> FromRequest<S> for ApiRequest {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let method = req.method().clone();
        let headers = req.headers().clone();
        let raw_query = req.uri().query().unwrap_or("").to_owned();

        let mut params = Params::from_query(&raw_query)?;
        let mut upload = None;
        if method == Method::POST {
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.split(';').next().unwrap_or("").trim().to_owned());
            let body = Bytes::from_request(req, state).await.map_err(|e| {
                ApiError::bad_request(format!("Could not read the request body: {e}"))
            })?;
            // query-string auth still works for POSTs; body params win
            let query_params = params;
            params = match content_type.as_deref() {
                None | Some("application/json") => {
                    Params::from_json_body(if body.is_empty() { b"{}" } else { &body })?
                }
                Some("application/cbor") => Params::from_cbor_body(&body)?,
                Some(ct) if is_known_mime_type(ct) => {
                    upload = Some(body);
                    Params::default()
                }
                Some(_) => {
                    return Err(ApiError::bad_request(
                        "Did not recognise Content-Type header!",
                    ));
                }
            };
            for key in [
                crate::auth::ACCESS_KEY_HEADER,
                crate::auth::SESSION_KEY_HEADER,
            ] {
                if !params.contains(key)
                    && let Some(v) = query_params.raw(key)
                {
                    params.insert(key, v.clone());
                }
            }
        }
        let response_encoding = response_encoding(&headers, &params);
        Ok(ApiRequest {
            method,
            headers,
            params,
            upload,
            response_encoding,
        })
    }
}

/// The reference accepts any mime type it knows as a raw upload body.
fn is_known_mime_type(content_type: &str) -> bool {
    content_type == "application/octet-stream"
        || hydrus_core::Mime::ALL
            .iter()
            .any(|m| m.mimetype() == content_type)
        || content_type.starts_with("image/")
        || content_type.starts_with("video/")
        || content_type.starts_with("audio/")
}

/// A successful API response.
#[derive(Debug)]
pub enum ApiResponse {
    /// A JSON object; `version` and `hydrus_version` are added if absent.
    Json(Json, Encoding),
    /// Raw bytes with a content type (files, thumbnails).
    Bytes {
        content_type: String,
        body: Bytes,
        cache: bool,
        /// `Content-Disposition: attachment` (else `inline`).
        attachment: bool,
    },
    /// A file, streamed: all of it, or one byte range (206).
    File {
        source: FileSource,
        /// For `Content-Disposition`.
        filename: String,
        content_type: String,
        /// `Content-Disposition: attachment` rather than `inline`.
        attachment: bool,
        range: Option<ByteRange>,
    },
    /// 200 with no body.
    Empty,
}

/// Where a file response's bytes come from.
#[derive(Debug, Clone)]
pub enum FileSource {
    Path(std::path::PathBuf),
    /// Built in (e.g. a default thumbnail).
    Static(&'static [u8]),
}

impl FileSource {
    pub fn size(&self) -> std::io::Result<u64> {
        match self {
            FileSource::Path(path) => std::fs::metadata(path).map(|m| m.len()),
            FileSource::Static(bytes) => Ok(bytes.len() as u64),
        }
    }
}

/// Part of a file: `length` bytes from `offset`, of a file of `size`
/// bytes, reported as ending at `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
    pub end: u64,
    pub size: u64,
}

impl ApiResponse {
    pub fn json(value: Json, request: &ApiRequest) -> Self {
        ApiResponse::Json(value, request.response_encoding)
    }
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> Response {
        match self {
            ApiResponse::Json(mut value, Encoding::Json) => {
                if let Json::Object(map) = &mut value {
                    map.entry("version").or_insert(json!(CLIENT_API_VERSION));
                    map.entry("hydrus_version")
                        .or_insert(json!(REFERENCE_VERSION));
                }
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    value.to_string(),
                )
                    .into_response()
            }
            ApiResponse::Json(value, Encoding::Cbor) => {
                let mut out = Vec::new();
                if let Err(e) = ciborium::into_writer(&value, &mut out) {
                    return ApiError::server(format!("could not encode cbor: {e}")).into_response();
                }
                ([(header::CONTENT_TYPE, "application/cbor")], out).into_response()
            }
            ApiResponse::Bytes {
                content_type,
                body,
                cache,
                attachment,
            } => {
                let disposition = if attachment { "attachment" } else { "inline" };
                let mut response = (
                    [
                        (header::CONTENT_TYPE, content_type),
                        (header::CONTENT_DISPOSITION, disposition.to_owned()),
                    ],
                    body,
                )
                    .into_response();
                if cache {
                    response.headers_mut().insert(
                        header::CACHE_CONTROL,
                        header::HeaderValue::from_static("max-age=2592000"),
                    );
                }
                response
            }
            ApiResponse::File {
                source,
                filename,
                content_type,
                attachment,
                range,
            } => file_response(source, &filename, content_type, attachment, range),
            ApiResponse::Empty => StatusCode::OK.into_response(),
        }
    }
}

/// Files are cached for a year, as the reference serves them.
const FILE_MAX_AGE: u64 = 86400 * 365;

fn file_response(
    source: FileSource,
    filename: &str,
    content_type: String,
    attachment: bool,
    range: Option<ByteRange>,
) -> Response {
    use futures_util::TryStreamExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
    let Ok(size) = source.size() else {
        return ApiError::not_found("That file seems to be missing!").into_response();
    };
    let (offset, length) = range.map_or((0, size), |r| (r.offset, r.length));
    let body = match source {
        FileSource::Path(path) => {
            let stream = futures_util::stream::once(async move {
                let mut file = tokio::fs::File::open(&path).await?;
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                Ok::<_, std::io::Error>(tokio_util::io::ReaderStream::new(file.take(length)))
            })
            .try_flatten();
            axum::body::Body::from_stream(stream)
        }
        FileSource::Static(bytes) => {
            let start = usize::try_from(offset).unwrap_or(bytes.len());
            let end = usize::try_from(offset + length).unwrap_or(bytes.len());
            axum::body::Body::from(Bytes::from_static(bytes.get(start..end).unwrap_or(&[])))
        }
    };
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    let disposition = format!(
        "{}; filename=\"{filename}\"",
        if attachment { "attachment" } else { "inline" },
    );
    let mut set = |name: header::HeaderName, value: String| {
        if let Ok(value) = header::HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    };
    set(header::CONTENT_TYPE, content_type);
    set(header::CONTENT_DISPOSITION, disposition);
    set(header::CONTENT_LENGTH, length.to_string());
    set(header::CACHE_CONTROL, format!("max-age={FILE_MAX_AGE}"));
    if let Some(r) = range {
        set(header::ACCEPT_RANGES, "bytes".into());
        set(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", r.offset, r.end, r.size),
        );
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    response
}

/// The response for paths that don't exist, matching the reference's web server.
pub fn no_such_resource() -> Response {
    let body = "\n<html>\n  <head><title>404 - No Such Resource</title></head>\n  <body>\n    <h1>No Such Resource</h1>\n    <p>Sorry. No luck finding that resource.</p>\n  </body>\n</html>\n";
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/html")],
        body,
    )
        .into_response()
}
