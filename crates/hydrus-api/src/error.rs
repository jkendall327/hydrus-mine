//! API errors and their wire format.
//!
//! Every error response is JSON:
//! `{"error", "exception_type", "status_code", "version", "hydrus_version"}`.
//! Clients branch on the status code and sometimes on `exception_type`, so
//! both follow the reference exactly; the human-readable `error` text is
//! ours to word.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use hydrus_core::{CLIENT_API_VERSION, REFERENCE_VERSION};
use hydrus_store::StoreError;

/// The reference's exception classes, which determine status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    MissingCredentials,
    InsufficientCredentials,
    NotFound,
    DataMissing,
    FileMissing,
    NotAcceptable,
    Conflict,
    RangeNotSatisfiable,
    Session,
    UnprocessableEntity,
    ServerBusy,
    Bandwidth,
    Server,
}

impl ErrorKind {
    pub fn status(self) -> StatusCode {
        match self {
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::MissingCredentials => StatusCode::UNAUTHORIZED,
            ErrorKind::InsufficientCredentials => StatusCode::FORBIDDEN,
            ErrorKind::NotFound | ErrorKind::DataMissing | ErrorKind::FileMissing => {
                StatusCode::NOT_FOUND
            }
            ErrorKind::NotAcceptable => StatusCode::NOT_ACCEPTABLE,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::RangeNotSatisfiable => StatusCode::RANGE_NOT_SATISFIABLE,
            ErrorKind::Session => StatusCode::from_u16(419).expect("valid status"),
            ErrorKind::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::ServerBusy => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::Bandwidth => StatusCode::from_u16(509).expect("valid status"),
            ErrorKind::Server => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The reference's exception class name, reported as `exception_type`.
    pub fn exception_type(self) -> &'static str {
        match self {
            ErrorKind::BadRequest => "BadRequestException",
            ErrorKind::MissingCredentials => "MissingCredentialsException",
            ErrorKind::InsufficientCredentials => "InsufficientCredentialsException",
            ErrorKind::NotFound => "NotFoundException",
            ErrorKind::DataMissing => "DataMissing",
            ErrorKind::FileMissing => "FileMissingException",
            ErrorKind::NotAcceptable => "NotAcceptable",
            ErrorKind::Conflict => "ConflictException",
            ErrorKind::RangeNotSatisfiable => "RangeNotSatisfiableException",
            ErrorKind::Session => "SessionException",
            ErrorKind::UnprocessableEntity => "UnprocessableEntity",
            ErrorKind::ServerBusy => "ServerBusyException",
            ErrorKind::Bandwidth => "BandwidthException",
            ErrorKind::Server => "ServerException",
        }
    }
}

/// An error to report to the API client.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::BadRequest, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn data_missing(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::DataMissing, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InsufficientCredentials, message)
    }

    pub fn server(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Server, message)
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NoSuchService(_) | StoreError::NoSuchServiceKey(_) => {
                ApiError::bad_request(e.to_string())
            }
            StoreError::Invalid(message) => ApiError::bad_request(message),
            other => {
                tracing::error!(error = %other, "store error while handling api request");
                ApiError::server(other.to_string())
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.kind.status();
        let body = json!({
            "error": self.message,
            "exception_type": self.kind.exception_type(),
            "status_code": status.as_u16(),
            "version": CLIENT_API_VERSION,
            "hydrus_version": REFERENCE_VERSION,
        });
        (
            status,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body.to_string(),
        )
            .into_response()
    }
}
