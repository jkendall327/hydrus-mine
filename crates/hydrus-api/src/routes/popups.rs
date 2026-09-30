//! `/manage_popups/*`: showing the user messages and progress.

use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::params::{Nullable, Params};
use crate::popups::Job;
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

/// Apply a request's changes to a popup (`HandlePopupUpdate`): a value
/// sets, `null` removes.
fn apply(app: &AppState, job: &mut Job, p: &Params) -> ApiResult<()> {
    fn text(slot: &mut Option<String>, value: Nullable<String>) {
        match value {
            Nullable::Absent => {}
            Nullable::Null => *slot = None,
            Nullable::Value(v) => *slot = Some(v),
        }
    }
    text(&mut job.status_title, p.nullable("status_title")?);
    text(&mut job.status_text_1, p.nullable("status_text_1")?);
    text(&mut job.status_text_2, p.nullable("status_text_2")?);
    match p.nullable::<Json>("api_data")? {
        Nullable::Absent => {}
        Nullable::Null => job.api_data = None,
        Nullable::Value(v) if v.is_object() => job.api_data = Some(v),
        Nullable::Value(v) => {
            return Err(ApiError::bad_request(format!(
                "The parameter \"api_data\", with value \"{v}\", was not the expected type: dict!"
            )));
        }
    }
    for (name, slot) in [
        ("popup_gauge_1", &mut job.popup_gauge_1),
        ("popup_gauge_2", &mut job.popup_gauge_2),
    ] {
        match p.nullable::<Vec<i64>>(name)? {
            Nullable::Absent => {}
            Nullable::Null => *slot = None,
            Nullable::Value(v) if v.len() == 2 => *slot = Some(json!(v)),
            Nullable::Value(_) => {
                return Err(ApiError::bad_request(format!(
                    "The parameter \"{name}\" had an invalid number of items!"
                )));
            }
        }
    }
    let label = p.optional::<String>("files_label")?;
    if let Some(hashes) = parse_hashes(app, p)? {
        if !hashes.is_empty() && label.is_none() {
            return Err(ApiError::bad_request(
                "\"files_label\" is required to add files to a popup!",
            ));
        }
        job.set_files(hashes, label);
    }
    Ok(())
}

fn job_key(p: &Params) -> ApiResult<Vec<u8>> {
    p.required::<Vec<u8>>("job_status_key")
}

fn no_such_job() -> ApiError {
    ApiError::bad_request("This job key doesn't exist!")
}

pub async fn add_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePopups)?;
    let p = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let mut job = Job::new(p.or("is_pausable", false)?, p.or("is_cancellable", false)?);
            job.attached_files_mergable = p.or("attached_files_mergable", false)?;
            apply(app, &mut job, &p)?;
            let body = json!({ "job_status": job.to_json() });
            app.popups.add(job);
            Ok(ApiResponse::Json(body, encoding))
        })
        .await
}

pub async fn update_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePopups)?;
    let p = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let key = job_key(&p)?;
            let body = app
                .popups
                .update(&key, |job| {
                    apply(app, job, &p).map(|()| json!({ "job_status": job.to_json() }))
                })
                .ok_or_else(no_such_job)??;
            Ok(ApiResponse::Json(body, encoding))
        })
        .await
}

/// A change to one popup that answers with an empty 200.
async fn change(
    app: Arc<AppState>,
    req: ApiRequest,
    f: impl FnOnce(&mut Job, &Params) -> ApiResult<()> + Send + 'static,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePopups)?;
    let p = req.params.clone();
    app.clone()
        .blocking(move |app| {
            let key = job_key(&p)?;
            app.popups
                .update(&key, |job| f(job, &p))
                .ok_or_else(no_such_job)??;
            Ok(ApiResponse::Empty)
        })
        .await
}

pub async fn cancel_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    change(app, req, |job, _| {
        if job.cancellable {
            job.cancel();
        }
        Ok(())
    })
    .await
}

pub async fn dismiss_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    change(app, req, |job, _| {
        if job.done {
            job.finish_and_dismiss(None);
        }
        Ok(())
    })
    .await
}

pub async fn finish_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    change(app, req, |job, _| {
        job.finish();
        Ok(())
    })
    .await
}

pub async fn finish_and_dismiss_popup(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    change(app, req, |job, p| {
        job.finish_and_dismiss(p.optional::<i64>("seconds")?);
        Ok(())
    })
    .await
}

/// Popups made through the API never have a button to press.
pub async fn call_user_callable(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    change(app, req, |_, _| {
        Err(ApiError::bad_request(
            "This job doesn't have a user callable!",
        ))
    })
    .await
}

pub async fn get_popups(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePopups)?;
    // every popup is in view: there is no GUI to hold some back
    let _only_in_view = req.params.or("only_in_view", false)?;
    let jobs: Vec<Json> = app.popups.all().iter().map(Job::to_json).collect();
    Ok(ApiResponse::json(json!({ "job_statuses": jobs }), &req))
}
