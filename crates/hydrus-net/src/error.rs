//! Why a request failed, as the reference's network exceptions say it.

/// What an error status from a server means (the reference's exception
/// classes for HTTP statuses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    NotModified,
    BadRequest,
    MissingCredentials,
    InsufficientCredentials,
    NotFound,
    NotAcceptable,
    Conflict,
    RangeNotSatisfiable,
    Session,
    UnprocessableEntity,
    NetworkVersion,
    Censorship,
    Server,
    /// Any other status the reference has no special class for.
    Other,
}

/// Why a request failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    /// The server answered with an error status. The message is the
    /// reference's: `"404: "` and the start of what the server said.
    #[error("{message}")]
    Status {
        kind: StatusKind,
        code: u16,
        message: String,
    },
    /// The server kept saying it was too busy (429, 509, 529).
    #[error("{0}")]
    Bandwidth(String),
    /// The server kept failing in a way worth retrying (502, 503, 522).
    #[error("{0}")]
    Infrastructure(String),
    /// No connection could be made.
    #[error("{0}")]
    Connection(String),
    /// The connection was made but the response stalled or broke off.
    #[error("{0}")]
    StreamTimeout(String),
    /// Something else went wrong with the exchange (too many redirects, a
    /// server sending more than it said it would).
    #[error("{0}")]
    Network(String),
    #[error("Cancelled!")]
    Cancelled,
    /// Writing what was downloaded failed.
    #[error("{0}")]
    Io(String),
}

impl NetError {
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            NetError::Status {
                kind: StatusKind::NotFound,
                ..
            }
        )
    }
}

/// What a response's status becomes when it is not a success: an error to
/// report, or one worth retrying.
pub(crate) enum StatusOutcome {
    Fail(NetError),
    /// 429, 509, 529: wait (as long as the server asks, if it says) and try
    /// again.
    ServersideBandwidth(String),
    /// 502, 503, 522: try again straight away.
    Reattempt(String),
}

/// The reference's `ConvertStatusCodeAndDataIntoExceptionInfo` (for
/// websites, not hydrus services).
pub(crate) fn status_outcome(code: u16, body: &[u8]) -> StatusOutcome {
    let mut text = String::from_utf8_lossy(body).into_owned();
    let (kind, long_text_is_fine) = match code {
        304 => (Some(StatusKind::NotModified), false),
        400 => (Some(StatusKind::BadRequest), true),
        401 => (Some(StatusKind::MissingCredentials), true),
        403 => (Some(StatusKind::InsufficientCredentials), false),
        404 => (Some(StatusKind::NotFound), false),
        406 => (Some(StatusKind::NotAcceptable), true),
        409 => (Some(StatusKind::Conflict), true),
        416 => (Some(StatusKind::RangeNotSatisfiable), true),
        419 => (Some(StatusKind::Session), true),
        422 => (Some(StatusKind::UnprocessableEntity), true),
        426 => (Some(StatusKind::NetworkVersion), true),
        429 => (None, true),
        451 => (Some(StatusKind::Censorship), true),
        509 | 529 | 502 | 522 | 503 => (None, false),
        500.. => (Some(StatusKind::Server), true),
        _ => (Some(StatusKind::Other), true),
    };
    if long_text_is_fine && text.chars().count() > 1024 {
        let start: String = text.chars().take(256).collect();
        text = format!(
            "The server's error text was too long to display. The first part follows, while a larger chunk has been written to the log.\n{start}"
        );
    }
    let message = format!("{code}: {text}");
    match (code, kind) {
        (429 | 509 | 529, _) => StatusOutcome::ServersideBandwidth(message),
        (502 | 503 | 522, _) => StatusOutcome::Reattempt(message),
        (_, Some(kind)) => StatusOutcome::Fail(NetError::Status {
            kind,
            code,
            message,
        }),
        (_, None) => unreachable!("every status has a kind or a retry"),
    }
}
