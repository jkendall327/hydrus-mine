//! Running the Client API as a server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderValue, Method, header};
use tower_http::cors::{Any, CorsLayer};

use crate::AppState;

/// How to serve.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub addr: SocketAddr,
    /// Answer cross-origin requests from any site, as the reference does when
    /// its "support CORS" option is on.
    pub cors: bool,
    /// Log anonymous request method/path, status and duration, omitting keys and query strings.
    pub log_requests: bool,
}

/// Serve until `shutdown` resolves.
pub async fn serve(
    state: Arc<AppState>,
    options: &ServerOptions,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let listener = bind(options).await?;
    serve_on(listener, state, options, shutdown).await
}

/// Listen where `options` says (a port in use fails here).
pub async fn bind(options: &ServerOptions) -> std::io::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(options.addr).await
}

/// Serve on `listener` until `shutdown` resolves.
pub async fn serve_on(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    options: &ServerOptions,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let mut app = crate::router(state);
    if options.log_requests {
        app = app.layer(axum::middleware::from_fn(log_request));
    }
    if options.cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .max_age(Duration::from_secs(86_400)),
        );
    }
    let app = app.layer(
        tower_http::set_header::SetResponseHeaderLayer::if_not_present(
            header::SERVER,
            HeaderValue::from_static(concat!("hydrus-rs/", env!("CARGO_PKG_VERSION"))),
        ),
    );
    let addr = listener.local_addr().unwrap_or(options.addr);
    tracing::info!(%addr, "Client API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

async fn log_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    tracing::info!(%method,%path,status=response.status().as_u16(),elapsed_ms=started.elapsed().as_millis(),"Client API request");
    response
}

/// The reference records activity before authenticating or refusing busy routes.
/// Use timestamp-only filesystem IPC so database lock_on cannot block this path.
pub(crate) async fn note_activity(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let now = hydrus_core::TimestampMs::now().0;
    let written = tokio::task::spawn_blocking(move || {
        let mut previous = state.api_activity.lock();
        if now > *previous {
            hydrus_store::api_activity::touch(state.store.dir(), now)?;
            *previous = now;
        }
        Ok::<(), std::io::Error>(())
    })
    .await;
    if !matches!(written, Ok(Ok(()))) {
        tracing::warn!("could not publish Client API activity timestamp");
    }
    next.run(request).await
}
