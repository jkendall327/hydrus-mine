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
    },
    /// 200 with no body.
    Empty,
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
            } => {
                let mut response = ([(header::CONTENT_TYPE, content_type)], body).into_response();
                if cache {
                    response.headers_mut().insert(
                        header::CACHE_CONTROL,
                        header::HeaderValue::from_static("max-age=2592000"),
                    );
                }
                response
            }
            ApiResponse::Empty => StatusCode::OK.into_response(),
        }
    }
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
