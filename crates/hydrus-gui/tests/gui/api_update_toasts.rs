//! Real authenticated API producer reaches staged preferences and owned toaster.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    api_update_toasts::{self, Preferences},
    popups, settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tower::ServiceExt as _;
const LABEL: &str = "Make a short-lived popup on cookie/header updates through the Client API: ";
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let menu = ui.get_menu_panes().row_data(0).unwrap();
    let i = menu
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "popup notifications")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}
fn current(store: &Store) -> Vec<popups::Job> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
}
fn wait(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "owned short-lived toast did not refresh"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn api(store: Arc<Store>) -> axum::Router {
    let permissions = hydrus_api::auth::AccessPermissions {
        access_key: vec![71; 32],
        name: "real toast fixture".into(),
        permits_everything: false,
        basic: [hydrus_api::auth::Permission::ManageHeaders].into(),
        search_filter: Default::default(),
    };
    store
        .write(move |ctx| hydrus_api::auth::save_key(ctx.conn(), &permissions))
        .unwrap();
    hydrus_api::router(hydrus_api::AppState::new(store).unwrap())
}
fn request(
    runtime: &tokio::runtime::Runtime,
    router: &axum::Router,
    worker: &str,
    body: serde_json::Value,
) {
    let path = if worker == "cookies" {
        "/manage_cookies/set_cookies"
    } else {
        "/manage_headers/set_headers"
    };
    let request = axum::http::Request::builder()
        .method("POST")
        .uri(path)
        .header("Hydrus-Client-API-Access-Key", hex::encode([71; 32]))
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let response = runtime.block_on(router.clone().oneshot(request)).unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}
#[test]
fn real_checkbox_cancel_reopen_hidden_retired_and_concurrent_width_merge() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let fixture = hydrus_testkit::fixture_json("api_update_toasts.json");
    for case in fixture["controls"].as_array().unwrap() {
        let enabled = case["before"].as_bool().unwrap();
        store
            .write(move |ctx| settings::set(ctx.conn(), &Preferences { enabled }))
            .unwrap();
        let (w, row) = open(&ui, &bound);
        assert_eq!(
            w.get_rows()
                .row_data(usize::try_from(row).unwrap())
                .unwrap()
                .checked,
            enabled
        );
        w.invoke_check_toggled(row, case["typed"].as_bool().unwrap());
        if case["apply"] == true {
            w.invoke_apply();
        } else {
            w.invoke_cancel();
        }
        assert_eq!(
            store.read(api_update_toasts::load).unwrap().enabled,
            case["saved"].as_bool().unwrap()
        );
        let (w, row) = open(&ui, &bound);
        assert_eq!(
            w.get_rows()
                .row_data(usize::try_from(row).unwrap())
                .unwrap()
                .checked,
            case["reopened"].as_bool().unwrap()
        );
        w.invoke_cancel();
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
        .unwrap();
    let (w, row) = open(&ui, &bound);
    w.hide().unwrap();
    w.invoke_check_toggled(row, true);
    w.show().unwrap();
    w.invoke_apply();
    assert!(!store.read(api_update_toasts::load).unwrap().enabled);
    let (w, row) = open(&ui, &bound);
    w.invoke_check_toggled(row, true);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::popup_width::PopupWidth {
                    characters: 32,
                    fixed: true,
                },
            )
        })
        .unwrap();
    w.invoke_apply();
    assert!(store.read(api_update_toasts::load).unwrap().enabled);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::popup_width::PopupWidth>)
            .unwrap(),
        hydrus_store::popup_width::PopupWidth {
            characters: 32,
            fixed: true
        }
    );
    let (old, row) = open(&ui, &bound);
    old.invoke_check_toggled(row, false);
    let fresh = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    old.show().unwrap();
    old.invoke_apply();
    old.hide().unwrap();
    assert!(store.read(api_update_toasts::load).unwrap().enabled);
    let (w, _) = open(&ui, &fresh);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_update_toasts_options.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    w.invoke_cancel();
}
#[test]
fn actual_api_jobs_render_expire_and_preserve_retired_gui_incarnation() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let windows = headless::init();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let router = api(store.clone());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    // Warm real layout before creating the finite-lived notification.
    let _ = headless::render(&windows.get(0).unwrap(), 1100, 700);
    let fixture = hydrus_testkit::fixture_json("api_update_toasts.json");
    let cases = fixture["requests"].as_array().unwrap();
    request(&runtime, &router, "cookies", cases[0]["body"].clone());
    assert!(current(&store).is_empty());
    assert_eq!(ui.get_popups().row_count(), 0);
    let (w, row) = open(&ui, &bound);
    w.invoke_check_toggled(row, true);
    w.invoke_apply();
    let recorded = cases
        .iter()
        .find(|c| c["name"] == "cookies_sorted_deduplicated")
        .unwrap();
    request(&runtime, &router, "cookies", recorded["body"].clone());
    let expected = recorded["jobs"][0]["text"].as_str().unwrap();
    wait(|| ui.get_popups().iter().any(|p| p.text_1 == expected));
    let jobs = current(&store);
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].done && !jobs[0].pausable && !jobs[0].cancellable);
    assert_eq!(jobs[0].dismiss_at, Some(jobs[0].creation_time as i64 + 5));
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_update_toasts_cookie.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    let recorded = cases
        .iter()
        .find(|c| c["name"] == "headers_sorted_set")
        .unwrap();
    request(&runtime, &router, "headers", recorded["body"].clone());
    let expected = recorded["jobs"][0]["text"].as_str().unwrap();
    wait(|| ui.get_popups().iter().any(|p| p.text_1 == expected));
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_update_toasts_header.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    // The actual job's persisted deadline is consumed by the real live toaster.
    // Exact natural five-second boundaries are replayed separately at Store/API.
    let jobs = current(&store);
    let keys = jobs.iter().map(|j| j.key).collect::<Vec<_>>();
    let now = hydrus_core::TimestampMs::now().0 / 1000;
    store
        .write(move |ctx| {
            for key in keys {
                popups::update(ctx.conn(), &key, 0, |job| job.dismiss_at = Some(now - 1))?;
            }
            Ok(())
        })
        .unwrap();
    wait(|| ui.get_popups().row_count() == 0);
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    let body = serde_json::json!({"headers":{"Live-After-Cancel":{"value":"live"}}});
    request(&runtime, &router, "headers", body);
    wait(|| {
        ui.get_popups()
            .iter()
            .any(|p| p.text_1 == "Headers sent from API:\nSet: Live-After-Cancel")
    });
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    ui.show().unwrap();
    request(
        &runtime,
        &router,
        "headers",
        serde_json::json!({"headers":{"Owned-Late":{"value":"late"}}}),
    );
    slint::platform::update_timers_and_animations();
    assert!(
        !ui.get_popups()
            .iter()
            .any(|p| p.text_1 == "Headers sent from API:\nSet: Owned-Late")
    );
    let fresh = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(
        ui.get_popups()
            .iter()
            .any(|p| p.text_1 == "Headers sent from API:\nSet: Owned-Late")
    );
    assert_eq!(fresh.pages.borrow().session().pages.len(), 1);
    drop(bound);
}
