//! Cookies and custom HTTP headers for the client's network requests
//! (what browser extensions send so downloads can log in).

use crate::auth::PermissionChecks as _;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_store::network::{self, Approval, Cookie, DEFAULT_USER_AGENT, NetworkContext};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};

fn domain_context(domain: &str) -> ApiResult<NetworkContext> {
    if !domain.contains('.') {
        return Err(ApiError::bad_request(format!(
            "The value \"{domain}\" does not seem to be a domain!"
        )));
    }
    Ok(NetworkContext::domain(domain))
}

/// The context a header request is about: `domain`, or everything.
fn requested_context(params: &Params) -> ApiResult<NetworkContext> {
    match params.nullable::<String>("domain")?.value() {
        Some(domain) => domain_context(&domain),
        None => Ok(NetworkContext::global()),
    }
}

pub async fn get_cookies(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageHeaders)?;
    let context = domain_context(&req.params.required::<String>("domain")?)?;
    let encoding = req.response_encoding;
    app.blocking(move |app| {
        let cookies = app.store.read(|conn| {
            let session = network::session_for(conn, &context)?;
            network::cookies(conn, &session)
        })?;
        let rows: Vec<Json> = cookies
            .iter()
            .map(|c| json!([c.name, c.value, c.domain, c.path, c.expires]))
            .collect();
        Ok(ApiResponse::Json(json!({ "cookies": rows }), encoding))
    })
    .await
}

/// A `[name, value, domain, path, expires]` row: a cookie to set, or (with
/// a null value) one to clear.
fn parse_cookie_row(row: &Json) -> ApiResult<Cookie> {
    let items = row.as_array().filter(|items| items.len() == 5).ok_or_else(|| {
        ApiError::bad_request(format!(
            "The cookie \"{row}\" did not come in the format [ name, value, domain, path, expires ]!"
        ))
    })?;
    let bad = || {
        ApiError::bad_request(format!(
            "In the row [ name, value, domain, path, expires ], which I received as \"{row}\", name, domain, and path need to be strings, value needs to be null or a string, and expires needs to be null or an integer!"
        ))
    };
    let text = |v: &Json| v.as_str().map(str::to_owned).ok_or_else(bad);
    let value = match &items[1] {
        Json::Null => None,
        Json::String(s) => Some(s.clone()),
        _ => return Err(bad()),
    };
    let expires = match &items[4] {
        Json::Null => None,
        Json::Number(n) => Some(n.as_i64().ok_or_else(bad)?),
        _ => return Err(bad()),
    };
    Ok(Cookie {
        name: text(&items[0])?,
        value,
        domain: text(&items[2])?,
        path: text(&items[3])?,
        expires,
        secure: false,
        rest: Vec::new(),
    })
}

pub async fn set_cookies(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageHeaders)?;
    let rows: Vec<Json> = req.params.required("cookies")?;
    let cookies = rows
        .iter()
        .map(parse_cookie_row)
        .collect::<ApiResult<Vec<_>>>()?;
    app.blocking(move |app| {
        app.store.write(move |ctx| {
            for cookie in &cookies {
                let session =
                    network::session_for(ctx.conn(), &NetworkContext::domain(&cookie.domain))?;
                if cookie.value.is_none() {
                    network::clear_cookie(
                        ctx.conn(),
                        &session,
                        &cookie.domain,
                        &cookie.path,
                        &cookie.name,
                    )?;
                } else {
                    network::set_cookie(ctx.conn(), &session, cookie)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

pub async fn get_headers(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageHeaders)?;
    let context = requested_context(&req.params)?;
    let encoding = req.response_encoding;
    app.blocking(move |app| {
        let headers = app.store.read(|conn| network::headers(conn, &context))?;
        let mut body = Map::new();
        for h in headers {
            body.insert(
                h.name,
                json!({ "value": h.value, "approved": h.approval.name(), "reason": h.reason }),
            );
        }
        let data = if context.kind == network::CONTEXT_GLOBAL {
            Json::Null
        } else {
            Json::String(context.data.clone())
        };
        Ok(ApiResponse::Json(
            json!({
                "network_context": { "type": context.kind, "data": data },
                "headers": body,
            }),
            encoding,
        ))
    })
    .await
}

/// What a `set_headers` request does to a header's value.
enum ValueChange {
    Keep,
    Remove,
    Set(String),
}

/// One header of a `set_headers` request.
struct HeaderChange {
    name: String,
    value: ValueChange,
    approval: Option<Approval>,
    reason: Option<String>,
}

fn parse_header_change(name: &str, info: &Json) -> ApiResult<HeaderChange> {
    let info = info.as_object().ok_or_else(|| {
        ApiError::bad_request(format!(
            "The header \"{name}\" did not come with an Object!"
        ))
    })?;
    let approval = match info.get("approved") {
        None => None,
        Some(v) => Some(v.as_str().and_then(Approval::from_name).ok_or_else(|| {
            ApiError::bad_request(format!("The value \"{v}\" was not in the permitted list!"))
        })?),
    };
    let reason = match info.get("reason") {
        None => None,
        Some(Json::String(s)) => Some(s.clone()),
        Some(v) => {
            return Err(ApiError::bad_request(format!(
                "The reason \"{v}\" was not a string!"
            )));
        }
    };
    let value = match info.get("value") {
        None => ValueChange::Keep,
        Some(Json::Null) => ValueChange::Remove,
        Some(Json::String(s)) => ValueChange::Set(s.clone()),
        Some(v) => {
            return Err(ApiError::bad_request(format!(
                "The value \"{v}\" was not a string!"
            )));
        }
    };
    if matches!(value, ValueChange::Keep) && approval.is_none() && reason.is_none() {
        return Err(ApiError::bad_request(
            "Sorry, you have to set a value, approved, or reason parameter!",
        ));
    }
    Ok(HeaderChange {
        name: name.to_owned(),
        value,
        approval,
        reason,
    })
}

pub async fn set_headers(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageHeaders)?;
    let context = requested_context(&req.params)?;
    let headers: Json = req.params.required("headers")?;
    let headers = headers
        .as_object()
        .ok_or_else(|| ApiError::bad_request("The parameter \"headers\" was not an Object!"))?;
    let changes = headers
        .iter()
        .map(|(name, info)| parse_header_change(name, info))
        .collect::<ApiResult<Vec<_>>>()?;
    app.blocking(move |app| {
        app.store.write(move |ctx| {
            let conn = ctx.conn();
            for change in &changes {
                let existing = network::headers(conn, &context)?
                    .into_iter()
                    .find(|h| h.name == change.name);
                match &change.value {
                    ValueChange::Remove => {
                        network::delete_header(conn, &context, &change.name)?;
                    }
                    // setting the value it already has changes nothing, not
                    // even the approval or reason (as in the reference)
                    ValueChange::Set(value) if existing.as_ref().is_some_and(|h| &h.value == value) => {}
                    ValueChange::Set(value) => network::set_header(
                        conn,
                        &context,
                        &change.name,
                        Some(value),
                        change.approval,
                        change.reason.as_deref(),
                    )?,
                    ValueChange::Keep if existing.is_none() => {
                        return Ok(Err(ApiError::bad_request(format!(
                            "Sorry, you tried to set approved/reason on \"{}\", but that entry does not exist, so there is no value to set them to! Please give a value!",
                            change.name
                        ))));
                    }
                    ValueChange::Keep => network::set_header(
                        conn,
                        &context,
                        &change.name,
                        None,
                        change.approval,
                        change.reason.as_deref(),
                    )?,
                }
            }
            Ok(Ok(()))
        })?
    })
    .await?;
    Ok(ApiResponse::Empty)
}

/// Deprecated in favour of `set_headers`: the User-Agent for everything.
pub async fn set_user_agent(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManageHeaders)?;
    let mut agent: String = req.params.required("user-agent")?;
    if agent.is_empty() {
        DEFAULT_USER_AGENT.clone_into(&mut agent);
    }
    app.blocking(move |app| {
        app.store.write(move |ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::global(),
                "User-Agent",
                Some(&agent),
                None,
                None,
            )
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}
