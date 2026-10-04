//! Options' real frame table/geometry child and saved placement consumers.
use hydrus_core::windows::WindowSettings;
use hydrus_gui::{Bound, FrameLocationWindow, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::import::import_legacy;
use slint::{ComponentHandle as _, Model as _};

fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let index = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    window
}
fn child(bound: &Bound) -> FrameLocationWindow {
    bound
        .options_frame_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn index(window: &OptionsWindow, name: &str) -> i32 {
    let rows = window.get_frame_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().cells.row_data(0).unwrap() == name)
            .unwrap(),
    )
    .unwrap()
}
fn cells(window: &OptionsWindow) -> serde_json::Value {
    let rows = window.get_frame_rows();
    serde_json::json!(
        (0..rows.row_count())
            .map(|i| {
                let row = rows.row_data(i).unwrap();
                (0..row.cells.row_count())
                    .map(|j| row.cells.row_data(j).unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    )
}
fn edit_recorded(window: &FrameLocationWindow) {
    window.set_remember_size(true);
    window.set_remember_position(true);
    window.set_size_none(false);
    window.set_position_none(false);
    window.set_last_width(912);
    window.set_last_height(678);
    window.set_last_x(-45);
    window.set_last_y(67);
    window.set_gravity_x(0);
    window.set_gravity_y(1);
    window.set_default_position(1);
    window.set_maximised(false);
    window.set_fullscreen(false);
}

#[test]
fn frame_options_table_child_staging_and_saved_viewer_geometry_match_reference() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let reference = hydrus_testkit::fixture_json("frame_locations.json");
    let kept = || {
        store
            .read(hydrus_store::settings::get::<WindowSettings>)
            .unwrap()
    };
    let before = kept();
    let window = options(&ui, &bound);
    assert_eq!(cells(&window), reference["events"][0]["state"]["cells"]);
    assert!(!window.get_frame_selected());
    window.invoke_frame_action("edit".into());
    assert!(bound.options_frame_child.borrow().is_none());
    for name in ["main_gui", "media_viewer"] {
        window.invoke_frame_clicked(index(&window, name), name == "media_viewer", false);
    }
    assert!(window.get_frame_selected());
    assert!(!window.get_frame_single());
    for (i, action) in ["flip-size", "flip-position", "reset-size", "reset-position"]
        .into_iter()
        .enumerate()
    {
        window.invoke_frame_action(action.into());
        assert_eq!(cells(&window), reference["events"][i + 1]["state"]["cells"]);
    }
    assert_eq!(kept(), before);
    window.invoke_cancel();
    let window = options(&ui, &bound);
    assert_eq!(cells(&window), reference["cancel_reopen"]["cells"]);
    window.invoke_frame_clicked(index(&window, "main_gui"), false, false);
    window.invoke_frame_action("edit".into());
    let geometry = child(&bound);
    assert_eq!(
        geometry.get_message(),
        "Setting frame location info for main_gui."
    );
    assert_eq!(
        (geometry.get_last_width(), geometry.get_last_height()),
        (800, 600)
    );
    edit_recorded(&geometry);
    geometry.invoke_cancel();
    assert!(bound.options_frame_child.borrow().is_none());
    assert_eq!(cells(&window), reference["cancel_reopen"]["cells"]);
    window.invoke_frame_action("edit".into());
    let stale = child(&bound);
    edit_recorded(&stale);
    window.invoke_apply();
    assert!(
        bound.options.borrow().is_some(),
        "geometry child blocks parent Apply"
    );
    window.invoke_cancel();
    assert!(!stale.window().is_visible());
    assert!(bound.options_frame_child.borrow().is_none());
    stale.invoke_apply();
    assert_eq!(kept(), before);

    let window = options(&ui, &bound);
    window.invoke_frame_clicked(index(&window, "media_viewer"), false, false);
    window.invoke_frame_action("edit".into());
    let geometry = child(&bound);
    edit_recorded(&geometry);
    geometry.set_size_none(true);
    geometry.set_size_none(false);
    assert_eq!(
        (geometry.get_last_width(), geometry.get_last_height()),
        (912, 678)
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 470);
    assert!(pixels.chunks_exact(4).any(|p| p != &pixels[..4]));
    geometry.invoke_apply();
    assert_eq!(kept(), before);
    window.invoke_apply();
    let saved = kept();
    assert_eq!(saved.media_viewer.last_size, Some((912, 678)));
    assert_eq!(saved.media_viewer.last_position, Some((-45, 67)));
    assert_eq!(saved.media_viewer.default_gravity, (1, -1));
    assert_eq!(saved.media_viewer.default_position, "center");
    assert!(!saved.media_viewer.maximised && !saved.media_viewer.fullscreen);
    assert_eq!(saved.main_gui, before.main_gui);
    let reopened = options(&ui, &bound);
    reopened.invoke_frame_clicked(index(&reopened, "media_viewer"), false, false);
    reopened.invoke_frame_action("edit".into());
    let geometry = child(&bound);
    assert_eq!(
        (geometry.get_last_width(), geometry.get_last_height()),
        (912, 678)
    );
    assert_eq!(geometry.get_default_position(), 1);
    reopened.invoke_cancel();
    geometry.invoke_apply();
    assert_eq!(kept(), saved);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(!viewer.window().is_maximized() && !viewer.window().is_fullscreen());
    assert_eq!(
        viewer
            .window()
            .size()
            .to_logical(viewer.window().scale_factor()),
        slint::LogicalSize::new(912.0, 678.0)
    );
    // MinimalSoftwareWindow has no native-position adapter; persisted coordinates
    // are asserted above, while this actual owner verifies size/state placement.
    viewer.invoke_close_requested();
}
