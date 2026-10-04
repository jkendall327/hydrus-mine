//! `/manage_popups/*`: showing the user messages and progress.

use crate::auth::PermissionChecks as _;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Value as Json, json};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::params::{Nullable, Params};
use crate::popups::{Job, now_seconds, now_whole, to_json};
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

/// A request's changes to a popup (`HandlePopupUpdate`): a value sets,
/// `null` removes. Read from the request first, to apply in the store.
struct Changes {
    title: Nullable<String>,
    text_1: Nullable<String>,
    text_2: Nullable<String>,
    api_data: Nullable<Json>,
    gauge_1: Nullable<(i64, i64)>,
    gauge_2: Nullable<(i64, i64)>,
    files: Option<(Vec<hydrus_core::Sha256>, Option<String>)>,
}

impl Changes {
    fn read(app: &AppState, p: &Params) -> ApiResult<Self> {
        let api_data = match p.nullable::<Json>("api_data")? {
            Nullable::Value(v) if !v.is_object() => {
                return Err(ApiError::bad_request(format!(
                    "The parameter \"api_data\", with value \"{v}\", was not the expected type: dict!"
                )));
            }
            other => other,
        };
        let gauge = |name: &str| -> ApiResult<Nullable<(i64, i64)>> {
            Ok(match p.nullable::<Vec<i64>>(name)? {
                Nullable::Absent => Nullable::Absent,
                Nullable::Null => Nullable::Null,
                Nullable::Value(v) if v.len() == 2 => Nullable::Value((v[0], v[1])),
                Nullable::Value(_) => {
                    return Err(ApiError::bad_request(format!(
                        "The parameter \"{name}\" had an invalid number of items!"
                    )));
                }
            })
        };
        let label = p.optional::<String>("files_label")?;
        let files = match parse_hashes(app, p)? {
            Some(hashes) => {
                if !hashes.is_empty() && label.is_none() {
                    return Err(ApiError::bad_request(
                        "\"files_label\" is required to add files to a popup!",
                    ));
                }
                Some((hashes, label))
            }
            None => None,
        };
        Ok(Self {
            title: p.nullable("status_title")?,
            text_1: p.nullable("status_text_1")?,
            text_2: p.nullable("status_text_2")?,
            api_data,
            gauge_1: gauge("popup_gauge_1")?,
            gauge_2: gauge("popup_gauge_2")?,
            files,
        })
    }

    fn apply(self, job: &mut Job) {
        fn set<T>(slot: &mut Option<T>, value: Nullable<T>) {
            match value {
                Nullable::Absent => {}
                Nullable::Null => *slot = None,
                Nullable::Value(v) => *slot = Some(v),
            }
        }
        set(&mut job.status_title, self.title);
        set(&mut job.status_text_1, self.text_1);
        set(&mut job.status_text_2, self.text_2);
        set(&mut job.api_data, self.api_data);
        set(&mut job.popup_gauge_1, self.gauge_1);
        set(&mut job.popup_gauge_2, self.gauge_2);
        if let Some((hashes, label)) = self.files {
            job.set_files(hashes, label);
        }
    }
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
            let mut job = Job::new(
                p.or("is_pausable", false)?,
                p.or("is_cancellable", false)?,
                now_seconds(),
            );
            job.attached_files_mergable = p.or("attached_files_mergable", false)?;
            Changes::read(app, &p)?.apply(&mut job);
            let body = json!({ "job_status": to_json(&job) });
            app.store
                .write(move |ctx| hydrus_store::popups::add(ctx.conn(), &job, now_whole()))?;
            Ok(ApiResponse::Json(body, encoding))
        })
        .await
}

/// Change one popup in the store; `None` if there is no such popup.
fn change_job<R: Send + 'static>(
    app: &AppState,
    key: Vec<u8>,
    f: impl FnOnce(&mut Job) -> R + Send + 'static,
) -> ApiResult<R> {
    app.store
        .write(move |ctx| hydrus_store::popups::update(ctx.conn(), &key, now_whole(), f))?
        .ok_or_else(no_such_job)
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
            let changes = Changes::read(app, &p)?;
            let body = change_job(app, key, move |job| {
                changes.apply(job);
                json!({ "job_status": to_json(job) })
            })?;
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
            change_job(app, key, move |job| f(job, &p))??;
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
            job.finish_and_dismiss(None, now_whole());
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
        job.finish_and_dismiss(p.optional::<i64>("seconds")?, now_whole());
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
    // (the client shows ten at a time, oldest first, as the reference's)
    let only_in_view = req.params.or("only_in_view", false)?;
    let mut jobs = app
        .store
        .read(|conn| hydrus_store::popups::all(conn, now_whole()))?;
    if only_in_view {
        jobs.truncate(hydrus_store::popups::IN_VIEW);
    }
    let jobs: Vec<Json> = jobs.iter().map(to_json).collect();
    Ok(ApiResponse::json(json!({ "job_statuses": jobs }), &req))
}
