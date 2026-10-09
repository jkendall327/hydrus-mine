//! Real socket requests update timestamp-only IPC before auth and database locks.
use hydrus_api::{
    AppState,
    auth::{self, AccessPermissions},
    server::{self, ServerOptions},
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

async fn request(addr: std::net::SocketAddr, path: &str, key: Option<&str>) -> u16 {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let method = if path.starts_with("/manage_database/lock_") {
        "POST"
    } else {
        "GET"
    };
    let key_header = key.map_or(String::new(), |key| {
        format!("Hydrus-Client-API-Access-Key: {key}\r\n")
    });
    stream.write_all(format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{key_header}Content-Length: 0\r\n\r\n").as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    std::str::from_utf8(&response)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn rejected_and_read_requests_publish_activity_while_locked_without_waiting_on_sqlite() {
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    let permissions = AccessPermissions {
        access_key: vec![77; 32],
        name: "activity admin".into(),
        permits_everything: true,
        basic: std::collections::BTreeSet::default(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &permissions))
        .unwrap();
    let state = AppState::new(store.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let options = ServerOptions {
        addr,
        cors: false,
        log_requests: false,
        tls: None,
    };
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        server::serve_on(listener, state, &options, async {
            let _ = finished.await;
        })
        .await
        .unwrap();
    });
    assert_eq!(
        hydrus_store::api_activity::latest(directory.path()).unwrap(),
        None
    );
    let before = hydrus_core::TimestampMs::now().0;
    assert_eq!(
        request(addr, "/verify_access_key", Some(&hex::encode([99; 32]))).await,
        403
    );
    let at = hydrus_store::api_activity::latest(directory.path())
        .unwrap()
        .unwrap();
    assert!((before..=hydrus_core::TimestampMs::now().0).contains(&at));
    let key = hex::encode([77; 32]);
    assert_eq!(request(addr, "/client_info", Some(&key)).await, 200);
    assert!(
        hydrus_store::api_activity::latest(directory.path())
            .unwrap()
            .unwrap()
            >= at
    );
    assert_eq!(
        request(addr, "/manage_database/lock_on", Some(&key)).await,
        200
    );
    // Force publication to be observable; SQLite is now genuinely paused.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let before = hydrus_core::TimestampMs::now().0;
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        request(addr, "/verify_access_key", None),
    )
    .await
    .expect("activity publication must not wait on paused SQLite");
    assert_eq!(status, 503);
    let at = hydrus_store::api_activity::latest(directory.path())
        .unwrap()
        .unwrap();
    assert!((before..=hydrus_core::TimestampMs::now().0).contains(&at));
    assert_eq!(
        std::fs::read(directory.path().join("client_api_activity"))
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            request(addr, "/manage_database/lock_off", Some(&key))
        )
        .await
        .unwrap(),
        200
    );
    assert_eq!(request(addr, "/verify_access_key", Some(&key)).await, 200);
    finish.send(()).unwrap();
    server.await.unwrap();
}
