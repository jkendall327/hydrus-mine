//! Saved preview zoom reaches measured image geometry only after accepted raster/resize.
use hydrus_core::{
    HashId, Sha256,
    media_viewer::{MediaViewerSettings, ZoomType},
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    preview_zoom::{self, Settings},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
const LABEL: &str = "Preview Viewer default zoom:";
fn open_options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    let index = ui
        .get_menu_titles()
        .iter()
        .position(|t| t.label == "file")
        .unwrap() as i32;
    ui.invoke_menu_title_pressed(index, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|r| r.label == "options…").unwrap() as i32;
    ui.invoke_menu_line_clicked(0, index, 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|p| p.text == "media playback")
        .unwrap() as i32;
    options.invoke_page_chosen(page);
    options
}
fn row(options: &OptionsWindow) -> i32 {
    options
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap() as i32
}
fn synthetic_file(store: &Store) -> HashId {
    let qt = hydrus_testkit::fixture_json("preview_default_zoom.json");
    let hash: Sha256 = qt["file"].as_str().unwrap().parse().unwrap();
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    // record_preview_default_zoom copies this file and changes its metadata to
    // 120x80 before SetMedia. Match that metadata and the injected raster.
    store
        .write(move |tx| {
            assert_eq!(
                tx.conn().execute(
                    "UPDATE files SET width=120, height=80 WHERE hash_id=?1",
                    [id],
                )?,
                1
            );
            Ok(())
        })
        .unwrap();
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .unwrap()
        .pop()
        .unwrap()
        .info
        .unwrap();
    assert_eq!(info.width.zip(info.height), Some((120, 80)));
    id
}
fn rect(ui: &MainWindow) -> (i32, i32, i32, i32) {
    (
        ui.get_preview_media_x() as i32,
        ui.get_preview_media_y() as i32,
        ui.get_preview_media_width() as i32,
        ui.get_preview_media_height() as i32,
    )
}
fn measure(ui: &MainWindow, bound: &hydrus_gui::Bound, windows: &headless::Windows, width: u32) {
    for _ in 0..3 {
        headless::render(&windows.get(0).unwrap(), width, 1000);
    }
    bound.preview.refresh();
    assert!(ui.window().is_visible());
    assert!(ui.get_sidebar_actual_width() > 0.0);
    assert!(ui.get_preview_actual_height() > 0.0);
    assert!(!ui.get_preview_splitter_hidden());
}
fn wait(ui: &MainWindow, bound: &hydrus_gui::Bound) {
    let start = Instant::now();
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn select(ui: &MainWindow, bound: &hydrus_gui::Bound) {
    ui.invoke_select_none();
    bound.preview.refresh();
    ui.invoke_thumbnail_clicked(0, false, false);
    bound.preview.refresh();
}
// leaf: audit-options-media-playback-zoom-and-position-preview-viewer-default-zoom
#[test]
fn real_options_cancel_reopen_six_modes_paint_clipped_geometry_and_preserve_current_on_save() {
    let qt = hydrus_testkit::fixture_json("preview_default_zoom.json");
    let (_dirs, store) = super::subscriptions::store();
    let file = synthetic_file(&store);
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &qt["legacy_options"].to_string(),
    )
    .unwrap();
    let legacy = hydrus_legacy::objects::ClientOptions::from_object(&object).unwrap();
    let viewer = legacy.media_viewer_settings();
    store
        .write(move |tx| settings::set(tx.conn(), &viewer))
        .unwrap();
    store
        .write(|tx| {
            let mut layout = hydrus_store::page_layout::load(tx.conn())?;
            layout.hide_preview = false;
            layout.hpos = 360;
            layout.vpos = -240;
            settings::set(tx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "preview zoom",
            None,
            vec![file],
        )),
    );
    measure(&ui, &bound, &windows, 1400);
    let options = open_options(&ui, &bound);
    assert_eq!(
        options
            .get_rows()
            .row_data(row(&options) as usize)
            .unwrap()
            .items
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>(),
        qt["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    options.invoke_choice_chosen(row(&options), 1);
    options.invoke_cancel();
    assert_eq!(
        store.read(preview_zoom::load).unwrap().default_zoom as i64,
        qt["default"]
    );
    options.invoke_choice_chosen(row(&options), 5);
    options.invoke_apply();
    assert_eq!(
        store.read(preview_zoom::load).unwrap().default_zoom as i64,
        qt["default"],
        "retired Options cannot save"
    );
    bound.preview.set_decoder(Arc::new(|_, _| {
        Some(hydrus_media::Raster::new(120, 80, 3, vec![43; 120 * 80 * 3]).unwrap())
    }));
    let viewer_before = store.read(settings::get::<MediaViewerSettings>).unwrap();
    for (index, item) in qt["choices"].as_array().unwrap().iter().enumerate() {
        let options = open_options(&ui, &bound);
        options.invoke_choice_chosen(row(&options), index as i32);
        options.invoke_apply();
        assert_eq!(
            store.read(preview_zoom::load).unwrap().default_zoom as i64,
            item["code"]
        );
        let reopened = open_options(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened) as usize)
                .unwrap()
                .index,
            index as i32
        );
        reopened.invoke_cancel();
        select(&ui, &bound);
        wait(&ui, &bound);
        assert_eq!(bound.preview.displayed_file(), Some(file));
        let actual = rect(&ui);
        let expected = qt["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["resolution"] == serde_json::json!([120, 80])
                    && c["canvas"] == serde_json::json!([360, 240])
                    && c["code"] == item["code"]
            })
            .unwrap();
        assert_eq!(
            serde_json::json!([actual.0, actual.1, actual.2, actual.3]),
            expected["rect"]
        );
        assert_eq!(
            store.read(settings::get::<MediaViewerSettings>).unwrap(),
            viewer_before
        );
    }
    ui.invoke_sidebar_resized(ui.get_layout_page_key(), ui.get_layout_epoch(), true, 179.0);
    measure(&ui, &bound, &windows, 1400);
    let fill = open_options(&ui, &bound);
    fill.invoke_choice_chosen(row(&fill), 3);
    fill.invoke_apply();
    select(&ui, &bound);
    wait(&ui, &bound);
    assert_eq!(
        rect(&ui),
        (0, -31, 360, 240),
        "fill overflows the measured pane and stays centered"
    );
    let fill_pixels = headless::render(&windows.get(0).unwrap(), 1400, 1000);
    let painted = fill_pixels
        .chunks_exact(4)
        .filter(|p| p[..3] == [43; 3])
        .count();
    assert!(
        painted > 50_000 && painted < 360 * 180,
        "overflowing raster is clipped to the real preview pane"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("preview-default-zoom-fill-native.png"),
        &fill_pixels,
        1400,
        1000,
    )
    .unwrap();
    let current = rect(&ui);
    let options = open_options(&ui, &bound);
    options.invoke_choice_chosen(row(&options), 1);
    options.invoke_apply();
    bound.preview.refresh();
    assert_eq!(
        rect(&ui),
        current,
        "saving alone does not reset accepted image geometry"
    );
    store
        .write(|tx| {
            let mut colour = hydrus_store::image_colour::load(tx.conn())?;
            colour.normalise_icc = !colour.normalise_icc;
            settings::set(tx.conn(), &colour)
        })
        .unwrap();
    bound.preview.refresh();
    wait(&ui, &bound);
    assert_eq!(
        rect(&ui),
        current,
        "ICC raster replacement keeps accepted zoom geometry"
    );
    assert_eq!(bound.preview.displayed_file(), Some(file));
    ui.invoke_sidebar_resized(ui.get_layout_page_key(), ui.get_layout_epoch(), true, 241.0);
    measure(&ui, &bound, &windows, 1400);
    assert_eq!(
        rect(&ui),
        (120, 80, 120, 80),
        "actual resize reads new 100% default and centers"
    );
    let pixels = headless::render(&windows.get(0).unwrap(), 1400, 1000);
    assert!(
        pixels.chunks_exact(4).filter(|p| p[..3] == [43; 3]).count() > 1000,
        "accepted raster is actually painted"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("preview-default-zoom-native.png"),
        &pixels,
        1400,
        1000,
    )
    .unwrap();
    let saved = store.read(preview_zoom::load).unwrap();
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(reopened.read(preview_zoom::load).unwrap(), saved);
}
#[test]
fn held_raster_acceptance_samples_saved_preview_policy_without_restart_or_prohibited_admission() {
    let (_dirs, store) = super::subscriptions::store();
    let file = synthetic_file(&store);
    store
        .write(|tx| {
            tx.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            let mut stats: settings::FileViewingStatistics = settings::get(tx.conn())?;
            stats.active = true;
            stats.preview_min_ms = None;
            stats.preview_max_ms = None;
            settings::set(tx.conn(), &stats)?;
            let mut layout = hydrus_store::page_layout::load(tx.conn())?;
            layout.hide_preview = false;
            layout.hpos = 360;
            layout.vpos = -240;
            settings::set(tx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "accepted preview",
            None,
            vec![file],
        )),
    );
    let now = Rc::new(Cell::new(1000));
    bound.preview.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    measure(&ui, &bound, &windows, 1400);
    let (entered, entries) = crossbeam_channel::bounded(1);
    let (release, released) = crossbeam_channel::bounded(1);
    bound.preview.set_decoder(Arc::new(move |_, _| {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        Some(hydrus_media::Raster::new(120, 80, 3, vec![73; 120 * 80 * 3]).unwrap())
    }));
    select(&ui, &bound);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(bound.preview.displayed_file(), None);
    assert_eq!(rect(&ui).2, 0);
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &Settings {
                    default_zoom: ZoomType::Full,
                },
            )
        })
        .unwrap();
    now.set(2000);
    release.send(()).unwrap();
    wait(&ui, &bound);
    assert_eq!(rect(&ui), (120, 80, 120, 80));
    assert_eq!(bound.preview.displayed_file(), Some(file));
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &Settings {
                    default_zoom: ZoomType::FillX,
                },
            )
        })
        .unwrap();
    bound.preview.refresh();
    assert_eq!(rect(&ui), (120, 80, 120, 80));
    now.set(3000);
    ui.invoke_select_none();
    bound.preview.refresh();
    let stats = store
        .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
        .unwrap();
    let preview = stats
        .iter()
        .find(|s| s.canvas == hydrus_core::CanvasType::Preview)
        .unwrap();
    assert_eq!(
        (preview.views, preview.viewtime_ms),
        (1, 2000),
        "accepted decode retains original request timestamp"
    );
    store
        .write(|tx| {
            let mut viewer: MediaViewerSettings = settings::get(tx.conn())?;
            let mime = hydrus_core::Mime::ImageJpeg;
            let mut view = viewer.view(mime);
            view.preview_show_action = hydrus_core::media_viewer::ShowAction::DoNotShow;
            viewer.media_view.insert(mime.code(), view);
            settings::set(tx.conn(), &viewer)
        })
        .unwrap();
    select(&ui, &bound);
    assert!(!ui.get_preview_loading());
    assert_eq!(bound.preview.displayed_file(), None);
    assert_eq!(rect(&ui).2, 0);
    bound.preview.close();
    ui.show().unwrap();
    bound.preview.refresh();
    assert_eq!(
        rect(&ui).2,
        0,
        "closed retained monitor cannot republish geometry"
    );
}
