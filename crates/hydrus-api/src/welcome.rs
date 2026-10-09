//! The root page: the reference's ASCII-art welcome (`GenerateEris`), or its
//! "normie-friendly" alternative when the service asks for it.
use std::sync::Arc;

use axum::extract::State;
use hydrus_core::{CLIENT_API_VERSION, REFERENCE_VERSION, ServiceType};
use hydrus_store::services::{ServerConfig, ServiceKind, ServiceRegistry};

use crate::AppState;
use crate::error::ApiResult;
use crate::request::ApiResponse;

// The reference's templates, verbatim (the normie page's style block filled
// in); each `{}` takes the next argument, as Python's `str.format` does.
const ERIS: &str = include_str!("welcome/eris.html");
const NORMIE_ERIS: &str = include_str!("welcome/normie_eris.html");

/// The page for the service called `name` with `config`.
pub fn page(name: &str, config: &ServerConfig) -> String {
    let texts = [
        name.to_owned(),
        format!("This is <b>{name}</b>,"),
        format!("a {}.", ServiceType::ClientApiService.name()),
        format!("Software version {REFERENCE_VERSION}"),
        format!("API version {CLIENT_API_VERSION}"),
        if config.allow_non_local_connections {
            "It responds to requests from any host.".to_owned()
        } else {
            "It only responds to requests from localhost.".to_owned()
        },
    ];
    let template = if config.use_normie_eris {
        NORMIE_ERIS
    } else {
        ERIS
    };
    let mut out = String::with_capacity(template.len() + 256);
    let mut rest = template;
    for text in &texts {
        let Some(at) = rest.find("{}") else { break };
        out.push_str(&rest[..at]);
        out.push_str(text);
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// `GET /`.
pub async fn welcome(State(app): State<Arc<AppState>>) -> ApiResult<ApiResponse> {
    let (name, config) = app.store.read(|conn| {
        Ok(ServiceRegistry::load(conn)?
            .of_type(ServiceType::ClientApiService)
            .find_map(|s| match &s.kind {
                ServiceKind::ClientApi(c) => Some((s.name.clone(), c.clone())),
                _ => None,
            })
            .unwrap_or_else(|| ("client api".into(), ServerConfig::default())))
    })?;
    Ok(ApiResponse::Bytes {
        content_type: "text/html; charset=UTF-8".into(),
        body: page(&name, &config).into(),
        max_age: None,
        attachment: false,
    })
}
