//! Each Client API permission checkbox, driven through the real permission editor.
use hydrus_gui::{client_api_admin_window as api, headless, services_review_window};
use hydrus_store::{Store, api_permissions};
use slint::Model as _;

fn grant_only(
    store: &std::sync::Arc<Store>,
    code: u8,
    byte: u8,
    label: &str,
) -> api_permissions::AccessPermissions {
    let review = services_review_window::open(store.clone()).unwrap();
    let index = review
        .get_services()
        .iter()
        .position(|s| s.ends_with(": client api"))
        .unwrap();
    review.invoke_selected_service(i32::try_from(index).unwrap());
    review.invoke_manage_api_keys();
    let keys = api::last_opened().unwrap();
    keys.invoke_add_clicked();
    let edit = api::last_edit_opened().unwrap();
    edit.set_name(format!("only {code}").into());
    edit.invoke_change_key();
    edit.set_new_key(hex::encode([byte; 32]).into());
    edit.invoke_accept_key();
    edit.set_permits_everything(false);
    edit.invoke_full_toggled();
    // The reference starts a new key as permits-everything; unchecked rows
    // start empty once that is turned off, and the grant is the one toggle.
    assert!(edit.get_permissions().iter().all(|p| !p.checked));
    edit.invoke_permission_toggled(i32::from(code), true);
    let row = edit
        .get_permissions()
        .iter()
        .find(|p| p.code == i32::from(code))
        .unwrap();
    assert!(row.checked);
    assert_eq!(row.label, label);
    assert_eq!(
        edit.get_permissions().iter().filter(|p| p.checked).count(),
        1
    );
    edit.invoke_apply_clicked();
    keys.invoke_apply_clicked();
    review.invoke_close_clicked();
    store
        .read(api_permissions::stored_keys)
        .unwrap()
        .into_iter()
        .find(|k| k.access_key == [byte; 32])
        .unwrap()
}

// leaf: audit-media-permission-add-notes, audit-media-permission-edit-ratings, audit-media-permission-manage-file-relationships, audit-media-permission-add-tags, audit-media-permission-edit-times, audit-media-permission-add-files, audit-media-permission-add-urls, audit-media-permission-manage-headers, audit-media-permission-manage-database, audit-media-permission-manage-pages, audit-media-permission-manage-popups, audit-media-permission-search-files, audit-media-permission-see-local-paths
#[test]
fn each_permission_checkbox_has_the_reference_label_and_serializes_alone() {
    let recorded = hydrus_testkit::fixture_json("client_api_admin.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    for (n, row) in recorded["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let code = u8::try_from(row[0].as_u64().unwrap()).unwrap();
        let stored = grant_only(
            &store,
            code,
            u8::try_from(n + 1).unwrap(),
            row[1].as_str().unwrap(),
        );
        assert!(!stored.permits_everything);
        assert_eq!(stored.basic.len(), 1);
        let only = stored.basic.iter().next().unwrap();
        assert_eq!(u8::from(*only), code);
        assert_eq!(only.description(), row[1].as_str().unwrap());
    }
}

fn call(
    router: &axum::Router,
    runtime: &tokio::runtime::Runtime,
    method: &str,
    uri: &str,
    body: &str,
    byte: u8,
) -> axum::http::StatusCode {
    use tower::ServiceExt as _;
    let request = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("Hydrus-Client-API-Access-Key", hex::encode([byte; 32]))
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(body.to_owned()))
        .unwrap();
    runtime
        .block_on(router.clone().oneshot(request))
        .unwrap()
        .status()
}

// leaf: audit-media-permission-commit-pending
#[test]
fn the_commit_pending_checkbox_grants_the_pending_routes_to_that_key_alone() {
    let recorded = hydrus_testkit::fixture_json("client_api_admin.json");
    let rows = recorded["permissions"].as_array().unwrap();
    let commit = rows
        .iter()
        .find(|row| row[1].as_str().unwrap().contains("commit pending"))
        .expect("the recorded editor has a commit pending row");
    let other = rows
        .iter()
        .find(|row| row[0] != commit[0])
        .expect("another permission");
    let legacy = hydrus_testkit::legacy_fixture("repositories");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let granted = grant_only(
        &store,
        u8::try_from(commit[0].as_u64().unwrap()).unwrap(),
        0x41,
        commit[1].as_str().unwrap(),
    );
    assert_eq!(granted.basic.len(), 1);
    grant_only(
        &store,
        u8::try_from(other[0].as_u64().unwrap()).unwrap(),
        0x42,
        other[1].as_str().unwrap(),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let router = hydrus_api::router(hydrus_api::AppState::new(store.clone()).unwrap());
    let counts = "/manage_services/get_pending_counts";
    // The key the editor granted it to reads the pending counts; the other,
    // saved by the same editor without it, is refused.
    assert_eq!(
        call(&router, &runtime, "GET", counts, "", 0x41),
        axum::http::StatusCode::OK
    );
    assert_eq!(
        call(&router, &runtime, "GET", counts, "", 0x42),
        axum::http::StatusCode::FORBIDDEN
    );
    // Committing passes the permission check, then stops where hydrus-rs
    // cannot upload (DIFFERENCES.md); the other key never gets that far.
    let repository = store
        .snapshot()
        .services
        .all()
        .find(|s| s.service_type() == hydrus_core::ServiceType::TagRepository)
        .map(|s| s.key.to_hex())
        .expect("the repositories fixture has a tag repository");
    {
        let key = repository;
        let uri = "/manage_services/commit_pending";
        let body = format!("{{\"service_key\": \"{key}\"}}");
        assert_eq!(
            call(&router, &runtime, "POST", uri, &body, 0x41),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            call(&router, &runtime, "POST", uri, &body, 0x42),
            axum::http::StatusCode::FORBIDDEN
        );
    }
}
