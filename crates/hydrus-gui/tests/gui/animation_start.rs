//! Staged Options and the real viewer's saved initial animation seek.
use hydrus_gui::{MainWindow, MediaViewerWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::{
    animation_start::{self, Preferences},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};
const LABEL: &str = "Start animations this % in:";
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, index as i32, 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "media playback")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABEL)
        .unwrap() as i32;
    (window, row)
}
fn first_pixel(viewer: &MediaViewerWindow) -> Option<[u8; 4]> {
    viewer
        .get_media()
        .to_rgba8()
        .map(|image| image.as_bytes()[..4].try_into().unwrap())
}
fn until(done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "native animation did not reach expected state"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
#[allow(clippy::float_cmp)]
fn constructor_cancel_hidden_and_retired_controls_then_real_next_media_initial_seek() {
    let (directories, store) = super::subscriptions::store();
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { fraction: 0.119 }))
        .unwrap();
    let qt = hydrus_testkit::fixture_json("animation_start.json");
    let original = hex::decode(qt["rendered"]["bytes"].as_str().unwrap()).unwrap();
    let offset = original
        .windows(4)
        .position(|bytes| bytes == b"ANIM")
        .unwrap()
        + 12;
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let mut ids = Vec::new();
    for count in [0_u16, 1] {
        let mut bytes = original.clone();
        bytes[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
        let path = directories[1].path().join(format!("initial-{count}.webp"));
        std::fs::write(&path, bytes).unwrap();
        let result = importer
            .import_path(&path, &FileImportOptions::default())
            .unwrap();
        ids.push(
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &result.hash.unwrap()))
                .unwrap()
                .unwrap(),
        );
    }
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let mut pages = Pages::single(SearchPage::new(store.clone()));
    pages.open_files(
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
        )),
        ids.clone(),
        None,
        None,
    );
    let bound = bind(&ui, pages);
    ui.show().unwrap();
    assert_eq!(bound.current.borrow().borrow().results(), ids);
    let (window, row) = options(&ui, &bound);
    let control = window.get_rows().row_data(row as usize).unwrap();
    assert_eq!(
        (
            control.kind,
            control.number,
            control.minimum,
            control.maximum
        ),
        (2, 11, 0, 100)
    );
    assert!(control.enabled);
    window.invoke_number_edited(row, 72);
    window.invoke_cancel();
    assert_eq!(store.read(animation_start::load).unwrap().fraction, 0.119);
    window.show().unwrap();
    window.invoke_number_edited(row, 100);
    window.invoke_apply();
    assert_eq!(
        store.read(animation_start::load).unwrap().fraction,
        0.119,
        "retired retained Options cannot save"
    );
    window.hide().unwrap();
    let (window, row) = options(&ui, &bound);
    window.hide().unwrap();
    window.invoke_number_edited(row, 72);
    window.show().unwrap();
    window.invoke_apply();
    assert_eq!(
        store.read(animation_start::load).unwrap().fraction,
        0.11,
        "hidden edit refused; visible unchanged Apply normalizes the raw constructor"
    );
    let (window, row) = options(&ui, &bound);
    window.invoke_number_edited(row, 100);
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 900, 640);
    headless::save_png(
        &hydrus_testkit::artifacts_dir().join("animation_start_options.png"),
        &pixels,
        900,
        640,
    )
    .unwrap();
    window.invoke_apply();
    assert_eq!(store.read(animation_start::load).unwrap().percent(), 100);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    // Fresh widget uses previous count1, not this file's3, even at100%.
    assert_eq!(viewer.get_scanbar_text(), "1/3 - 0.000/0.720");
    viewer.invoke_next();
    until(|| first_pixel(&viewer) == Some([10, 20, 240, 255]));
    viewer.invoke_toggle_pause();
    until(|| viewer.get_scanbar_text().starts_with("3/3 - "));
    assert_eq!(viewer.get_scanbar_text(), "3/3 - 0.360/0.720");
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 600, 420);
    headless::save_png(
        &hydrus_testkit::artifacts_dir().join("animation_start_viewer.png"),
        &pixels,
        600,
        420,
    )
    .unwrap();
    // Saving affects the next media admission, preserving the paused current frame.
    let (window, row) = options(&ui, &bound);
    window.invoke_number_edited(row, 0);
    window.invoke_apply();
    assert_eq!(first_pixel(&viewer), Some([10, 20, 240, 255]));
    assert_eq!(viewer.get_scanbar_text(), "3/3 - 0.360/0.720");
    viewer.invoke_previous();
    until(|| first_pixel(&viewer) == Some([240, 10, 20, 255]));
    viewer.invoke_close_requested();
    let closed = viewer.get_media().to_rgba8().unwrap().as_bytes().to_vec();
    viewer.show().unwrap();
    viewer.invoke_next();
    slint::platform::update_timers_and_animations();
    assert_eq!(
        viewer.get_media().to_rgba8().unwrap().as_bytes(),
        closed,
        "retired viewer does not restart decoder admission"
    );
    viewer.hide().unwrap();
}
