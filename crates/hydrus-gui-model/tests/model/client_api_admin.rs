use hydrus_gui_model::client_api_admin::{self, Editor, Row};
use hydrus_store::api_permissions::{AccessPermissions, Permission};
fn key(byte: u8, name: &str, full: bool) -> AccessPermissions {
    AccessPermissions {
        access_key: vec![byte; 32],
        name: name.into(),
        permits_everything: full,
        basic: [Permission::SearchFiles].into(),
        search_filter: hydrus_core::TagFilter::default(),
    }
}
#[test]
fn reference_rows_permissions_questions_and_duplicates() {
    let fixture: serde_json::Value = hydrus_testkit::fixture_json("client_api_admin.json");
    assert_eq!(
        serde_json::json!(client_api_admin::COLUMNS),
        fixture["columns"]
    );
    for p in Permission::ALL {
        assert!(
            fixture["permissions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row == &serde_json::json!([u8::from(p), p.description()]))
        );
    }
    for (token, permissions) in [key(1, "searcher", false), key(2, "full", true)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            serde_json::json!(Row { token, permissions }.cells()),
            fixture["rows"][token]
        );
    }
    assert_eq!(
        client_api_admin::DELETE_QUESTION,
        fixture["actions"][0]["question"]
    );
    assert_eq!(
        client_api_admin::CHANGE_KEY_WARNING,
        fixture["actions"][1]["question"]
    );
    assert_eq!(
        client_api_admin::parse_key("bad").unwrap_err(),
        fixture["actions"][2]["message"]
    );
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let mut editor = Editor::new(&store).unwrap();
    editor.edit(None, key(1, "searcher", false)).unwrap();
    editor.edit(None, key(2, "full", true)).unwrap();
    editor.selection.select_many(&editor.order());
    let mut byte = 2;
    editor
        .duplicate(|| {
            byte += 1;
            vec![byte; 32]
        })
        .unwrap();
    let actual = editor.rows.iter().map(Row::cells).collect::<Vec<_>>();
    let mut expected: Vec<[String; 3]> =
        serde_json::from_value(fixture["duplicates"].clone()).unwrap();
    expected.sort();
    assert_eq!(actual, expected);
}
// leaf: audit-media-api-apply
#[test]
fn staged_keys_cancel_persist_collision_and_stale_apply() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    {
        let mut canceled = Editor::new(&store).unwrap();
        canceled.edit(None, key(1, "canceled", false)).unwrap();
    }
    assert!(
        store
            .read(hydrus_store::api_permissions::stored_keys)
            .unwrap()
            .is_empty()
    );
    let mut editor = Editor::new(&store).unwrap();
    editor.edit(None, key(1, "saved", false)).unwrap();
    assert_eq!(
        editor.edit(None, key(1, "collision", true)).unwrap_err(),
        client_api_admin::COLLISION
    );
    editor.apply(&store).unwrap();
    let mut fresh = Editor::new(&store).unwrap();
    assert_eq!(fresh.rows[0].permissions.name, "saved");
    fresh.edit(Some(0), key(2, "rotated", true)).unwrap();
    fresh.apply(&store).unwrap();
    assert!(editor.apply(&store).is_err());
    let mut deletion = Editor::new(&store).unwrap();
    deletion.selection.select_only(Some(0));
    deletion.delete_selected();
    deletion.apply(&store).unwrap();
    assert!(Editor::new(&store).unwrap().rows.is_empty());
    assert!(client_api_admin::parse_key(&"x".repeat(64)).is_err());
    assert_eq!(
        client_api_admin::parse_key(&format!(" {} ", "AB".repeat(32))).unwrap(),
        vec![171; 32]
    );
}

#[test]
fn listener_settings_preserve_unrelated_services_and_imported_flags() {
    use client_api_admin::{Binding, ListenerEdit, server_config};
    use hydrus_store::services::{ServerConfig, ServiceKind};
    let original = ServerConfig {
        use_https: true,
        use_normie_eris: true,
        external_host_override: Some("existing.example".into()),
        ..ServerConfig::default()
    };
    let fields = ListenerEdit {
        port: Some(12345),
        binding: Binding::Network,
        cors: true,
        logs: true,
        use_https: false,
        normie_eris: true,
        external_scheme: Some("https".into()),
        external_host: original.external_host_override.clone(),
        external_port: Some(String::new()),
    };
    let edited = server_config(&original, &fields).unwrap();
    assert_eq!(edited.port, Some(12345));
    assert!(edited.allow_non_local_connections && edited.support_cors && edited.log_requests);
    assert!(!edited.use_https);
    assert!(edited.use_normie_eris);
    assert_eq!(
        edited.external_host_override,
        original.external_host_override
    );
    assert_eq!(edited.external_port_override.as_deref(), Some(""));
    assert!(
        server_config(
            &original,
            &ListenerEdit {
                port: Some(0),
                ..fields.clone()
            }
        )
        .is_err()
    );
    assert!(
        server_config(
            &original,
            &ListenerEdit {
                port: Some(65536),
                ..fields.clone()
            }
        )
        .is_err()
    );
    assert_eq!(
        client_api_admin::base_url(&ServerConfig {
            port: Some(1),
            ..original.clone()
        })
        .unwrap(),
        "https://127.0.0.1:1/"
    );
    assert!(client_api_admin::base_url(&original).is_err());
    assert_eq!(
        client_api_admin::base_url(&edited).unwrap(),
        "http://127.0.0.1:12345/"
    );
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let mut services = hydrus_gui_model::services_editor::Editor::new(&store).unwrap();
    let other = services
        .rows
        .iter()
        .filter(|r| !matches!(r.service.kind, ServiceKind::ClientApi(_)))
        .map(|r| r.service.clone())
        .collect::<Vec<_>>();
    let api = services
        .rows
        .iter_mut()
        .find(|r| matches!(r.service.kind, ServiceKind::ClientApi(_)))
        .unwrap();
    // Unsupported fields cannot be introduced through a protected service edit.
    api.service.kind = ServiceKind::ClientApi(edited.clone());
    assert!(services.apply(&store).is_err());
    let ServiceKind::ClientApi(existing) = store
        .snapshot()
        .services
        .by_name("client api")
        .unwrap()
        .kind
        .clone()
    else {
        panic!("API")
    };
    let edited = server_config(&existing, &fields).unwrap();
    services
        .rows
        .iter_mut()
        .find(|r| matches!(r.service.kind, ServiceKind::ClientApi(_)))
        .unwrap()
        .service
        .kind = ServiceKind::ClientApi(edited.clone());
    services.apply(&store).unwrap();
    let reopened = hydrus_gui_model::services_editor::Editor::new(&store).unwrap();
    assert_eq!(
        reopened
            .rows
            .iter()
            .filter(|r| !matches!(r.service.kind, ServiceKind::ClientApi(_)))
            .map(|r| r.service.clone())
            .collect::<Vec<_>>(),
        other
    );
    assert!(
        reopened
            .rows
            .iter()
            .any(|r| r.service.kind == ServiceKind::ClientApi(edited.clone()))
    );
}

#[test]
fn browser_url_uses_reported_listener_over_cli_overridden_service_settings() {
    use hydrus_store::{services::ServerConfig, settings::ClientApiState};
    let reported = ClientApiState::Listening("127.0.0.1:45990".into());
    for port in [None, Some(45869)] {
        let config = ServerConfig {
            port,
            ..ServerConfig::default()
        };
        assert_eq!(
            client_api_admin::reported_base_url(&config, &reported).unwrap(),
            "http://127.0.0.1:45990/"
        );
    }
    let config = ServerConfig {
        port: Some(45869),
        ..ServerConfig::default()
    };
    for (address, url) in [
        ("0.0.0.0:45990", "http://127.0.0.1:45990/"),
        ("[::]:45990", "http://[::1]:45990/"),
        ("[::1]:45990", "http://[::1]:45990/"),
        ("192.0.2.3:45990", "http://192.0.2.3:45990/"),
    ] {
        assert_eq!(
            client_api_admin::reported_base_url(
                &config,
                &ClientApiState::Listening(address.into())
            )
            .unwrap(),
            url
        );
    }
    assert_eq!(
        client_api_admin::reported_base_url(&config, &ClientApiState::Off).unwrap(),
        "http://127.0.0.1:45869/"
    );
    assert!(
        client_api_admin::reported_base_url(&ServerConfig::default(), &ClientApiState::Off)
            .is_err()
    );
    assert!(
        client_api_admin::reported_base_url(&config, &ClientApiState::Listening("invalid".into()))
            .is_err()
    );
}
