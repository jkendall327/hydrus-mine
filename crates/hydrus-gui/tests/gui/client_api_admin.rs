//! Native store regressions through actual service-review and permission widgets.
use hydrus_gui::{client_api_admin_window as api, headless, services_review_window};
use hydrus_store::{
    Store, api_permissions,
    services::{ServerConfig, ServiceKind},
};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

fn review(store: &std::sync::Arc<Store>) -> hydrus_gui::ServicesReviewWindow {
    let review = services_review_window::open(store.clone()).unwrap();
    let index = review
        .get_services()
        .iter()
        .position(|s| s.ends_with(": client api"))
        .unwrap();
    review.invoke_selected_service(i32::try_from(index).unwrap());
    assert!(review.get_client_api());
    review.invoke_manage_api_keys();
    review
}
fn add(keys: &hydrus_gui::ClientApiKeysWindow, name: &str, byte: u8) {
    keys.invoke_add_clicked();
    let edit = api::last_edit_opened().unwrap();
    edit.set_name(name.into());
    edit.invoke_change_key();
    edit.set_new_key(hex::encode([byte; 32]).into());
    edit.invoke_accept_key();
    edit.invoke_apply_clicked();
}
fn name(keys: &hydrus_gui::ClientApiKeysWindow, row: usize) -> String {
    keys.get_rows()
        .row_data(row)
        .unwrap()
        .cells
        .row_data(0)
        .unwrap()
        .to_string()
}
/// The three enabled flags the reference recorder reads off its editor
/// (`oracle/record_client_api_admin.py`): the permission list, "check all" and
/// the tag-filter button.
fn enabled_states(edit: &hydrus_gui::EditApiPermissionsWindow) -> serde_json::Value {
    serde_json::json!({
        "basic": !edit.get_permits_everything(),
        "all": edit.get_can_check_all(),
        "filter": edit.get_can_filter(),
    })
}
fn screenshot(windows: &headless::Windows, index: usize, filename: &str, width: u32, height: u32) {
    let pixels = headless::render(&windows.get(index).unwrap(), width, height);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(filename),
        &pixels,
        width,
        height,
    )
    .unwrap();
}
// leaf: audit-media-permissions-filter, audit-media-permissions-key
#[test]
fn cancellation_nested_ownership_permissions_and_key_validation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let parent = review(&store);
    let keys = api::last_opened().unwrap();
    assert_eq!(keys.get_rows().row_count(), 0);
    keys.invoke_add_clicked();
    let edit = api::last_edit_opened().unwrap();
    let permission_index = windows.count() - 1;
    let recorded = hydrus_testkit::fixture_json("client_api_admin.json");
    assert_eq!(enabled_states(&edit), recorded["full_enabled"]);
    assert_eq!(edit.get_permissions().row_count(), 14);
    edit.set_permits_everything(false);
    edit.invoke_full_toggled();
    assert!(edit.get_can_check_all());
    edit.invoke_permission_toggled(3, true);
    assert_eq!(enabled_states(&edit), recorded["search_enabled"]);
    edit.invoke_permission_toggled(3, false);
    assert_eq!(enabled_states(&edit), recorded["none_enabled"]);
    edit.invoke_permission_toggled(3, true);
    edit.invoke_edit_filter();
    let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
    assert!(edit.get_filtering());
    filter.invoke_cancel();
    assert!(!edit.get_filtering());
    edit.invoke_check_all();
    assert!(!edit.get_can_check_all());
    assert!(edit.get_permissions().iter().all(|p| p.checked));
    edit.invoke_change_key();
    edit.set_new_key("bad".into());
    edit.invoke_accept_key();
    assert!(edit.get_error().contains("3 characters long"));
    assert!(edit.get_changing_key());
    edit.invoke_cancel_key();
    screenshot(
        &windows,
        permission_index,
        "client_api_permissions.png",
        720,
        740,
    );
    edit.invoke_edit_filter();
    let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
    parent.invoke_close_clicked();
    assert!(!keys.window().is_visible());
    assert!(!edit.window().is_visible());
    assert!(!filter.window().is_visible());
    // Retained stale widget handles cannot write or consume a replacement slot.
    edit.invoke_apply_clicked();
    filter.invoke_apply();
    keys.invoke_apply_clicked();
    assert!(store.read(api_permissions::stored_keys).unwrap().is_empty());
    let parent = review(&store);
    let replacement = api::last_opened().unwrap();
    replacement.invoke_add_clicked();
    let accepted = api::last_edit_opened().unwrap();
    accepted.set_name("accepted".into());
    accepted.set_permits_everything(false);
    accepted.invoke_full_toggled();
    accepted.invoke_permission_toggled(3, true);
    accepted.invoke_edit_filter();
    let permitted = hydrus_gui::tag_filter_window::last_opened().unwrap();
    permitted.invoke_block_everything();
    permitted.invoke_typed(0, "safe".into());
    permitted.invoke_apply();
    accepted.invoke_apply_clicked();
    edit.invoke_cancel_clicked();
    assert!(replacement.window().is_visible());
    replacement.invoke_apply_clicked();
    let stored = store.read(api_permissions::stored_keys).unwrap();
    assert_eq!(stored[0].name, "accepted");
    assert!(stored[0].search_filter.tag_ok("safe", false));
    assert!(!stored[0].search_filter.tag_ok("other", false));
    parent.invoke_close_clicked();
}
// leaf: audit-media-api-apply, audit-media-api-copy, audit-media-api-key-list, audit-media-permissions-key
#[test]
fn sorted_selection_crud_copy_base_url_persistence_and_stale_apply() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let id = store.snapshot().services.by_name("client api").unwrap().id;
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::update_config(
                ctx.conn(),
                id,
                &ServiceKind::ClientApi(ServerConfig {
                    port: Some(45869),
                    ..ServerConfig::default()
                }),
            )
        })
        .unwrap();
    let parent = review(&store);
    let keys = api::last_opened().unwrap();
    let keys_index = windows.count() - 1;
    add(&keys, "z last", 1);
    add(&keys, "a first", 2);
    assert_eq!(name(&keys, 0), "a first");
    keys.invoke_row_clicked(0, false, false);
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    keys.invoke_copy_key();
    assert_eq!(
        copied.borrow()[0],
        hydrus_gui::Clip::Text(hex::encode([2; 32]))
    );
    keys.invoke_sort(0, false);
    assert_eq!(name(&keys, 0), "z last");
    keys.invoke_row_clicked(0, false, false);
    keys.invoke_copy_key();
    assert_eq!(
        copied.borrow()[1],
        hydrus_gui::Clip::Text(hex::encode([1; 32]))
    );
    keys.invoke_duplicate_clicked();
    assert!(
        keys.get_rows()
            .iter()
            .any(|r| r.cells.row_data(0).unwrap() == "z last (1)")
    );
    keys.invoke_sort(0, true);
    let row = keys
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "z last")
        .unwrap();
    keys.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    keys.invoke_edit_clicked();
    let edit = api::last_edit_opened().unwrap();
    edit.invoke_change_key();
    edit.set_new_key(hex::encode([2; 32]).into());
    edit.invoke_accept_key();
    edit.invoke_apply_clicked();
    assert!(edit.get_error().contains("already exists"));
    assert!(edit.window().is_visible());
    edit.invoke_change_key();
    edit.set_new_key(hex::encode([3; 32]).into());
    edit.invoke_accept_key();
    edit.invoke_apply_clicked();
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |url| launched.borrow_mut().push(url.to_owned())
    });
    keys.invoke_open_base_url();
    assert_eq!(launched.borrow().as_slice(), ["http://127.0.0.1:45869/"]);
    for port in [None, Some(45869)] {
        store
            .write_and_refresh(move |ctx| {
                hydrus_store::services::update_config(
                    ctx.conn(),
                    id,
                    &ServiceKind::ClientApi(ServerConfig {
                        port,
                        ..ServerConfig::default()
                    }),
                )?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ClientApiStatus {
                        pid: 123,
                        state: hydrus_store::settings::ClientApiState::Listening(
                            "127.0.0.1:45990".into(),
                        ),
                    },
                )
            })
            .unwrap();
        keys.invoke_open_base_url();
        assert_eq!(launched.borrow().last().unwrap(), "http://127.0.0.1:45990/");
    }

    screenshot(&windows, keys_index, "client_api_keys.png", 1000, 560);
    keys.invoke_apply_clicked();
    let stored = store.read(api_permissions::stored_keys).unwrap();
    assert_eq!(stored.len(), 3);
    assert!(!stored.iter().any(|p| p.access_key == [1; 32]));
    assert!(stored.iter().any(|p| p.access_key == [3; 32]));
    parent.invoke_manage_api_keys();
    let keys = api::last_opened().unwrap();
    let row = keys
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "a first")
        .unwrap();
    keys.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    keys.invoke_delete_clicked();
    assert_eq!(keys.get_question(), "Remove all selected?");
    keys.invoke_answered(false);
    assert_eq!(keys.get_rows().row_count(), 3);
    keys.invoke_delete_clicked();
    keys.invoke_answered(true);
    keys.invoke_cancel_clicked();
    assert_eq!(store.read(api_permissions::stored_keys).unwrap().len(), 3);
    parent.invoke_manage_api_keys();
    let keys = api::last_opened().unwrap();
    keys.invoke_row_clicked(0, false, false);
    keys.invoke_delete_clicked();
    keys.invoke_answered(true);
    keys.invoke_apply_clicked();
    assert!(
        !store
            .read(api_permissions::stored_keys)
            .unwrap()
            .iter()
            .any(|p| p.access_key == [2; 32])
    );
    parent.invoke_manage_api_keys();
    let keys = api::last_opened().unwrap();
    let competing = api_permissions::AccessPermissions {
        access_key: vec![88; 32],
        name: "another editor".into(),
        permits_everything: true,
        basic: std::collections::BTreeSet::default(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    store
        .write(move |ctx| api_permissions::save_key(ctx.conn(), &competing))
        .unwrap();
    keys.invoke_apply_clicked();
    assert!(
        keys.get_error()
            .contains("changed while this editor was open")
    );
    parent.invoke_close_clicked();
}
// leaf: audit-media-service-api-listener
#[test]
fn supported_service_listener_fields_stage_cancel_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let id = store.snapshot().services.by_name("client api").unwrap().id;
    let imported = ServerConfig {
        use_https: true,
        use_normie_eris: true,
        external_host_override: Some("preserve.example".into()),
        ..ServerConfig::default()
    };
    let original = imported.clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::update_config(ctx.conn(), id, &ServiceKind::ClientApi(imported))
        })
        .unwrap();
    let slots = hydrus_gui::services_editor_window::Slots::default();
    let open = || {
        let window =
            hydrus_gui::services_editor_window::open(&store, &slots, Rc::new(|| {})).unwrap();
        *slots.manage.borrow_mut() = Some(window.clone_strong());
        let row = window
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap() == "client api")
            .unwrap();
        window.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
        assert!(window.get_can_edit());
        assert!(!window.get_can_delete());
        window.invoke_edit_clicked();
        (window, slots.edit.borrow().as_ref().unwrap().clone_strong())
    };
    let (manage, edit) = open();
    assert!(edit.get_client_api());
    assert!(edit.get_api_https());
    edit.set_api_running(true);
    edit.set_api_port(12345);
    edit.set_api_cors(true);
    edit.set_api_logs(true);
    edit.set_api_disable_https(true);
    edit.invoke_apply_clicked();
    manage.invoke_cancel_clicked();
    assert_eq!(
        store.snapshot().services.get(id).unwrap().kind,
        ServiceKind::ClientApi(original.clone())
    );
    // The reference's service editor (recorded `server` block): the port
    // spin box runs `port_range`, and the service dictionary has exactly the
    // fields `ServerConfig` has, with these defaults.
    let recorded = hydrus_testkit::fixture_json("client_api_admin.json");
    let fields = recorded["server"]["fields"].as_object().unwrap();
    let native = serde_json::to_value(ServerConfig::default()).unwrap();
    assert_eq!(native.as_object().unwrap().len(), fields.len());
    for (name, value) in fields {
        if name != "port" {
            assert_eq!(&native[name], value, "{name}");
        }
    }
    let min = recorded["server"]["port_range"][0].as_i64().unwrap();
    let max = recorded["server"]["port_range"][1].as_i64().unwrap();
    for outside in [min - 1, max + 1] {
        let (manage, edit) = open();
        edit.set_api_running(true);
        edit.set_api_port(i32::try_from(outside).unwrap());
        edit.invoke_apply_clicked();
        assert!(!edit.get_error().is_empty(), "port {outside}");
        edit.invoke_cancel_clicked();
        manage.invoke_cancel_clicked();
    }
    for inside in [min, max] {
        let (manage, edit) = open();
        edit.set_api_running(true);
        edit.set_api_port(i32::try_from(inside).unwrap());
        edit.invoke_apply_clicked();
        assert!(edit.get_error().is_empty(), "port {inside}");
        manage.invoke_cancel_clicked();
    }
    let (manage, edit) = open();
    edit.set_api_running(true);
    edit.set_api_port(12345);
    edit.set_api_cors(true);
    edit.set_api_logs(true);
    edit.set_api_disable_https(true);
    screenshot(
        &windows,
        windows.count() - 1,
        "client_api_service.png",
        640,
        640,
    );
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    let ServiceKind::ClientApi(config) = store.snapshot().services.get(id).unwrap().kind.clone()
    else {
        panic!("API")
    };
    assert_eq!(config.port, Some(12345));
    assert!(config.support_cors && config.log_requests && !config.use_https);
    assert!(config.use_normie_eris);
    assert_eq!(
        config.external_host_override,
        original.external_host_override
    );
}

fn spin_until(what: &str, mut ready: impl FnMut() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready() {
        assert!(std::time::Instant::now() < until, "{what}");
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
}

fn ask_as_a_tool(store: &Store, key: &[u8; 32], name: &str) {
    use api_permissions::{Permission, Registration};
    let request = (
        hex::encode(key),
        name.to_owned(),
        false,
        vec![Permission::AddUrls, Permission::SearchFiles],
    );
    // (what `/request_new_permissions` records while a window is waiting)
    store
        .write(move |ctx| {
            let mut registration: Registration = hydrus_store::settings::get(ctx.conn())?;
            assert!(registration.open_until_ms.is_some());
            registration.requests.push(request);
            hydrus_store::settings::set(ctx.conn(), &registration)
        })
        .unwrap();
}

// leaf: audit-media-services-missing-api-capture
#[test]
fn add_from_api_request_waits_for_a_tool_s_request_and_edits_what_it_asked_for() {
    use api_permissions::{Permission, Registration};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let registration =
        |store: &Store| -> Registration { store.read(hydrus_store::settings::get).unwrap() };

    // a service that isn't running can't take requests
    let parent = review(&store);
    let keys = api::last_opened().unwrap();
    keys.invoke_add_from_api_clicked();
    assert_eq!(
        keys.get_error(),
        "The service is not running, so you cannot add new access via the API!"
    );
    assert!(api::last_request_opened().is_none());
    assert_eq!(registration(&store).open_until_ms, None);
    keys.invoke_cancel_clicked();
    drop(parent);

    // running: a window waits, taking requests
    let id = store.snapshot().services.by_name("client api").unwrap().id;
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::update_config(
                ctx.conn(),
                id,
                &ServiceKind::ClientApi(ServerConfig {
                    port: Some(45869),
                    ..ServerConfig::default()
                }),
            )
        })
        .unwrap();
    let parent = review(&store);
    let keys = api::last_opened().unwrap();
    keys.invoke_add_from_api_clicked();
    let waiting = api::last_request_opened().expect("the waiting window opens");
    assert_eq!(waiting.get_text(), "waiting for request…");
    assert!(keys.get_editing(), "the list waits");
    // registration is a short lease the window's timer renews, not a fixed
    // hour: a crash leaves it open for seconds
    let now = || hydrus_core::time::TimestampMs::now().millis();
    let until = registration(&store).open_until_ms.unwrap();
    assert!(until <= now() + 10_000, "{until}");
    store
        .write(|ctx| {
            let mut registration: Registration = hydrus_store::settings::get(ctx.conn())?;
            registration.open_until_ms =
                Some(hydrus_core::time::TimestampMs::now().millis() + 1_000);
            hydrus_store::settings::set(ctx.conn(), &registration)
        })
        .unwrap();
    spin_until("the lease was not renewed", || {
        registration(&store).open_until_ms.unwrap() > now() + 4_000
    });

    // closing it stops taking requests
    waiting.invoke_cancel_clicked();
    assert!(api::last_request_opened().is_none());
    assert_eq!(registration(&store), Registration::default());
    assert!(!keys.get_editing());

    // a tool asks while it waits: the request is shown as the permissions
    // editor, with the key the tool was given
    keys.invoke_add_from_api_clicked();
    assert!(api::last_request_opened().is_some());
    let key = [7u8; 32];
    ask_as_a_tool(&store, &key, "my tool");
    spin_until("the request was not picked up", || {
        api::last_edit_opened().is_some()
    });
    assert!(api::last_request_opened().is_none());
    assert_eq!(registration(&store), Registration::default());
    let edit = api::last_edit_opened().unwrap();
    assert_eq!(edit.get_name(), "my tool");
    assert!(!edit.get_permits_everything());
    edit.invoke_apply_clicked();
    assert_eq!(keys.get_rows().row_count(), 1);
    assert_eq!(name(&keys, 0), "my tool");
    keys.invoke_apply_clicked();
    let stored = store.read(api_permissions::stored_keys).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].access_key, key.to_vec());
    assert!(!stored[0].permits_everything);
    assert_eq!(
        stored[0].basic.iter().copied().collect::<Vec<_>>(),
        [Permission::AddUrls, Permission::SearchFiles]
    );

    // closing the key list while waiting stops it too
    drop(parent);
    let parent = review(&store);
    let keys = api::last_opened().unwrap();
    keys.invoke_add_from_api_clicked();
    assert!(registration(&store).open_until_ms.is_some());
    keys.invoke_cancel_clicked();
    assert!(api::last_request_opened().is_none());
    assert_eq!(registration(&store), Registration::default());
    drop(parent);
}
