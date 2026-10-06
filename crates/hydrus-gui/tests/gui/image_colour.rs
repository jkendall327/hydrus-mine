//! Actual Options, saved pixels, retained renders and stale preview generations.
use hydrus_core::{CanvasType, HashId, Mime};
use hydrus_gui::{MainWindow, MediaViewerWindow, OptionsWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::{
    Store,
    image_colour::{self, ImageColour},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
const LABEL: &str = "Apply image ICC Profile colour adjustments:";
fn set(store: &Store, enabled: bool) {
    store
        .write_and_refresh(move |c| {
            settings::set(
                c.conn(),
                &ImageColour {
                    normalise_icc: enabled,
                },
            )
        })
        .unwrap();
}
fn expected(enabled: bool) -> Vec<u8> {
    let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let case = &fixture["cases"][usize::from(!enabled)];
    let row = case["decode"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file"] == "embedded-linear.png")
        .unwrap();
    serde_json::from_value(row["pixels"].clone()).unwrap()
}
fn rgb(image: &slint::Image) -> Vec<u8> {
    image.to_rgba8().map_or_else(Vec::new, |pixels| {
        pixels
            .as_bytes()
            .chunks_exact(4)
            .flat_map(|pixel| pixel[..3].iter().copied())
            .collect()
    })
}
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "owned image did not finish");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "media playback")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}
fn setup() -> ([tempfile::TempDir; 2], Arc<Store>, HashId) {
    let (dirs, store) = super::subscriptions::store();
    set(&store, true);
    store
        .write(|c| {
            let mut stats: settings::FileViewingStatistics = settings::get(c.conn())?;
            stats.active = true;
            stats.preview_min_ms = None;
            stats.preview_max_ms = None;
            settings::set(c.conn(), &stats)?;
            let mut layout = hydrus_store::page_layout::load(c.conn())?;
            layout.hide_preview = false;
            layout.hpos = 400;
            layout.vpos = -240;
            settings::set(c.conn(), &layout)
        })
        .unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let result = importer
        .import_path(
            &hydrus_testkit::fixture_path("image_decoder_policies/embedded-linear.png"),
            &FileImportOptions::default(),
        )
        .unwrap();
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &result.hash.unwrap()))
        .unwrap()
        .unwrap();
    (dirs, store, id)
}
fn query_select(ui: &MainWindow, bound: &hydrus_gui::Bound, file: HashId) -> i32 {
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);
    index
}
// Re-entering an active Everything predicate removes it. Refresh the owned
// query without changing its predicates, location, or full result membership.
fn refresh_select(ui: &MainWindow, bound: &hydrus_gui::Bound, file: HashId) -> i32 {
    let current = bound.current.borrow().clone();
    let predicates = current.borrow().predicates();
    assert_eq!(predicates, vec!["system:everything".to_owned()]);
    let location = current.borrow().location().clone();
    let mut files = current.borrow().results().to_vec();
    files.sort();
    assert!(
        files.contains(&file),
        "owned query retains the requested file"
    );
    ui.invoke_refresh_page();
    assert_eq!(current.borrow().predicates(), predicates);
    assert_eq!(current.borrow().location(), &location);
    let mut refreshed = current.borrow().results().to_vec();
    refreshed.sort();
    assert_eq!(refreshed, files, "Refresh preserves owned query membership");
    let index = current
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);
    index
}
// A visible owner also needs a measured native preview viewport before SetMedia.
fn settle_viewport(ui: &MainWindow, bound: &hydrus_gui::Bound, windows: &headless::Windows) {
    assert!(ui.window().is_visible());
    let native = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&native, 1400, 1000);
    }
    bound.preview.refresh();
    assert!(ui.get_layout_available_width() > 0.0);
    assert!(ui.get_sidebar_actual_width() > 0.0);
    assert!(ui.get_preview_actual_height() > 0.0);
    assert!(!ui.get_preview_splitter_hidden());
}
fn rect(window: &MediaViewerWindow) -> [f32; 4] {
    [
        window.get_media_x(),
        window.get_media_y(),
        window.get_media_width(),
        window.get_media_height(),
    ]
}
#[test]
fn actual_saved_icc_updates_preview_viewer_tiles_and_archive_without_resetting_owned_state() {
    let (_dirs, store, file) = setup();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    bound.preview.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    settle_viewport(&ui, &bound, &windows);
    let index = query_select(&ui, &bound, file);
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(true)
    });
    let (cancel, row) = options(&ui, &bound);
    cancel.invoke_check_toggled(row, false);
    cancel.invoke_cancel();
    cancel.invoke_apply();
    assert!(store.read(image_colour::load).unwrap().normalise_icc);
    let (hidden, row) = options(&ui, &bound);
    hidden.hide().unwrap();
    hidden.invoke_check_toggled(row, false);
    hidden.invoke_apply();
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert!(
        store.read(image_colour::load).unwrap().normalise_icc,
        "hidden edits cannot be staged for later Apply"
    );
    ui.invoke_thumbnail_activated(index);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let native = windows.get(windows.count() - 1).unwrap();
    headless::render(&native, 800, 600);
    viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "+".into() });
    viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: "+".into() });
    let before_rect = rect(&viewer);
    now.set(2000);
    let (edit, row) = options(&ui, &bound);
    edit.invoke_check_toggled(row, false);
    assert!(store.read(image_colour::load).unwrap().normalise_icc);
    edit.invoke_apply();
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(false)
    });
    assert_eq!(rgb(&viewer.get_media()), expected(false));
    #[expect(
        clippy::float_cmp,
        reason = "ICC repaint must preserve accepted geometry exactly; no new layout is requested"
    )]
    {
        assert_eq!(rect(&viewer), before_rect, "ICC reload preserves zoom/pan");
    }
    wait(|| viewer.get_sharp_shown());
    let raster = hydrus_media::decode_image_with_icc(
        &std::fs::read(hydrus_testkit::fixture_path(
            "image_decoder_policies/embedded-linear.png",
        ))
        .unwrap(),
        false,
    )
    .unwrap();
    let rules: hydrus_core::media_viewer::MediaViewerSettings = store.read(settings::get).unwrap();
    let plan = hydrus_gui::still::plan(
        (
            before_rect[0] as i32,
            before_rect[1] as i32,
            before_rect[2] as i32,
            before_rect[3] as i32,
        ),
        (800, 600),
        1.0,
        (8, 8),
        &rules.view(Mime::ImagePng).zoom,
    )
    .unwrap();
    let tile = hydrus_gui::still::render(&raster, &plan);
    assert_eq!(
        rgb(&viewer.get_sharp()),
        tile.data(),
        "sharp pixels must use the new profile policy"
    );
    assert_eq!(rgb(&ui.get_preview_media()), expected(false));
    let render = headless::render(&native, 800, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("icc-viewer-policy-native.png"),
        &render,
        800,
        600,
    )
    .unwrap();
    // Hidden viewer must not consume the saved change without repainting later.
    viewer.hide().unwrap();
    set(&store, true);
    viewer.invoke_presentation_settings_changed();
    assert_eq!(rgb(&viewer.get_media()), expected(false));
    viewer.show().unwrap();
    viewer.invoke_presentation_settings_changed();
    assert_eq!(rgb(&viewer.get_media()), expected(true));
    #[expect(
        clippy::float_cmp,
        reason = "ICC repaint must preserve accepted geometry exactly; no new layout is requested"
    )]
    {
        assert_eq!(rect(&viewer), before_rect);
    }
    viewer.invoke_close_requested();
    viewer.show().unwrap();
    set(&store, false);
    viewer.invoke_presentation_settings_changed();
    assert_eq!(
        rgb(&viewer.get_media()),
        expected(true),
        "retired viewer cannot repaint"
    );
    viewer.hide().unwrap();
    ui.invoke_archive_delete_filter();
    let archive = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(rgb(&archive.get_media()), expected(false));
    let before = [
        archive.get_media_x(),
        archive.get_media_y(),
        archive.get_media_width(),
        archive.get_media_height(),
    ];
    set(&store, true);
    wait(|| rgb(&archive.get_media()) == expected(true));
    #[expect(
        clippy::float_cmp,
        reason = "ICC repaint must preserve accepted geometry exactly; no new layout is requested"
    )]
    {
        assert_eq!(
            [
                archive.get_media_x(),
                archive.get_media_y(),
                archive.get_media_width(),
                archive.get_media_height()
            ],
            before
        );
    }
    archive.invoke_close_requested();
    // Global hide rejects SetMedia but not ICC cache notifications on accepted media.
    store
        .write(|c| {
            let mut p = hydrus_store::page_layout::load(c.conn())?;
            p.hide_preview = true;
            settings::set(c.conn(), &p)
        })
        .unwrap();
    now.set(3000);
    set(&store, false);
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(false)
    });
    assert_eq!(bound.preview.displayed_file(), Some(file));
    assert!(
        store
            .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
            .unwrap()
            .iter()
            .all(|r| r.canvas != CanvasType::Preview),
        "repaint cannot end the accepted preview interval"
    );
    ui.hide().unwrap();
    set(&store, true);
    bound.preview.refresh();
    ui.show().unwrap();
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(true)
    });
    now.set(4000);
    let (old, row) = options(&ui, &bound);
    old.invoke_check_toggled(row, false);
    store
        .write(|c| {
            let mut prefs: settings::GuiSettings = settings::get(c.conn())?;
            prefs.confirm_exit = false;
            settings::set(c.conn(), &prefs)?;
            // Isolate completed owner retirement from the separate maintenance question.
            let mut shutdown: settings::ShutdownWork = settings::get(c.conn())?;
            shutdown.action = 0;
            settings::set(c.conn(), &shutdown)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !ui.window().is_visible(),
        "completed exit precedes retained ICC callbacks"
    );
    assert!(!old.window().is_visible());
    assert!(store.read(image_colour::load).unwrap().normalise_icc);
    old.show().unwrap();
    old.invoke_apply();
    old.invoke_check_toggled(row, false);
    assert!(store.read(image_colour::load).unwrap().normalise_icc);
    let rows = store
        .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
        .unwrap();
    let stats = rows
        .iter()
        .find(|r| r.canvas == CanvasType::Preview)
        .unwrap();
    assert_eq!((stats.views, stats.viewtime_ms), (1, 3000));
    assert!(
        Store::open(store.dir())
            .unwrap()
            .read(image_colour::load)
            .unwrap()
            .normalise_icc
    );
}

struct Published(crossbeam_channel::Sender<()>);
impl Drop for Published {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}
#[test]
fn held_old_colour_reply_cannot_replace_current_or_rebound_canvas_and_ineligible_preview_stays_empty()
 {
    let (_dirs, store, file) = setup();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    settle_viewport(&ui, &bound, &windows);
    query_select(&ui, &bound, file);
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(true)
    });
    let data = std::fs::read(hydrus_testkit::fixture_path(
        "image_decoder_policies/embedded-linear.png",
    ))
    .unwrap();
    let (started, entered) = crossbeam_channel::bounded(1);
    let (release, held) = crossbeam_channel::bounded(1);
    let (done, published) = crossbeam_channel::bounded(1);
    bound.preview.set_decoder(Arc::new({
        let data = data.clone();
        let publication = Published(done);
        move |store, _| {
            let _ = &publication;
            let policy = store.read(image_colour::load).unwrap();
            let raster = hydrus_media::decode_image_with_icc(&data, policy.normalise_icc).unwrap();
            started.send(()).unwrap();
            held.recv().unwrap();
            Some(raster)
        }
    }));
    set(&store, false);
    bound.preview.refresh();
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    bound.preview.set_decoder(Arc::new({
        let data = data.clone();
        move |store, _| {
            let policy = store.read(image_colour::load).unwrap();
            hydrus_media::decode_image_with_icc(&data, policy.normalise_icc).ok()
        }
    }));
    set(&store, true);
    bound.preview.refresh();
    wait(|| {
        bound.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(true)
    });
    release.send(()).unwrap();
    published.recv_timeout(Duration::from_secs(10)).unwrap();
    bound.preview.refresh();
    assert_eq!(
        rgb(&ui.get_preview_media()),
        expected(true),
        "published old generation is rejected"
    );
    // Hold a second actual conversion across same-window bind retirement.
    let (started, entered) = crossbeam_channel::bounded(1);
    let (release, held) = crossbeam_channel::bounded(1);
    let (done, published) = crossbeam_channel::bounded(1);
    bound.preview.set_decoder(Arc::new({
        let data = data.clone();
        let publication = Published(done);
        move |store, _| {
            let _ = &publication;
            let policy = store.read(image_colour::load).unwrap();
            let raster = hydrus_media::decode_image_with_icc(&data, policy.normalise_icc).unwrap();
            started.send(()).unwrap();
            held.recv().unwrap();
            Some(raster)
        }
    }));
    ui.invoke_select_none();
    bound.preview.refresh();
    refresh_select(&ui, &bound, file);
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    bound.preview.set_decoder(Arc::new(move |store, _| {
        let policy = store.read(image_colour::load).unwrap();
        hydrus_media::decode_image_with_icc(&data, policy.normalise_icc).ok()
    }));
    set(&store, false);
    let (old, row) = options(&ui, &bound);
    old.invoke_check_toggled(row, true);
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    settle_viewport(&ui, &successor, &windows);
    query_select(&ui, &successor, file);
    wait(|| {
        successor.preview.refresh();
        rgb(&ui.get_preview_media()) == expected(false)
    });
    old.show().unwrap();
    old.invoke_apply();
    assert!(!store.read(image_colour::load).unwrap().normalise_icc);
    let before_late: Vec<_> = store
        .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
        .unwrap()
        .into_iter()
        .filter(|r| r.canvas == CanvasType::Preview)
        .map(|r| (r.views, r.viewtime_ms, r.last_viewed))
        .collect();
    release.send(()).unwrap();
    published.recv_timeout(Duration::from_secs(10)).unwrap();
    successor.preview.refresh();
    assert_eq!(
        rgb(&ui.get_preview_media()),
        expected(false),
        "retired owner's published colour cannot replace successor pixels"
    );
    assert!(!ui.get_preview_loading());
    let after_late: Vec<_> = store
        .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
        .unwrap()
        .into_iter()
        .filter(|r| r.canvas == CanvasType::Preview)
        .map(|r| (r.views, r.viewtime_ms, r.last_viewed))
        .collect();
    assert_eq!(
        after_late, before_late,
        "retired pending acceptance cannot add a durable view"
    );
    ui.invoke_select_none();
    successor.preview.refresh();
    set(&store, true);
    successor.preview.refresh();
    store
        .write(|c| {
            let mut rules: hydrus_core::media_viewer::MediaViewerSettings =
                settings::get(c.conn())?;
            let mut view = rules.view(Mime::ImagePng);
            view.preview_show_action = hydrus_core::media_viewer::ShowAction::DoNotShow;
            rules.media_view.insert(Mime::ImagePng.code(), view);
            settings::set(c.conn(), &rules)
        })
        .unwrap();
    query_select(&ui, &successor, file);
    successor.preview.refresh();
    assert!(!ui.get_preview_has_media());
    set(&store, false);
    successor.preview.refresh();
    assert!(!ui.get_preview_has_media());
    assert!(!ui.get_preview_loading());
    assert_eq!(successor.preview.displayed_file(), None);
}

#[test]
fn paused_animation_keeps_accepted_frame_index_and_pixels_on_icc_notification() {
    let (_dirs, store, _) = setup();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let result = importer
        .import_path(
            &hydrus_testkit::fixture_path("image_decoder_policies/embedded-animation.webp"),
            &FileImportOptions::default(),
        )
        .unwrap();
    let file = store
        .read(|c| hydrus_store::master::hash_id(c, &result.hash.unwrap()))
        .unwrap()
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let index = query_select(&ui, &bound, file);
    ui.invoke_thumbnail_activated(index);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_scan_started();
    viewer.invoke_scan(205.0, 210.0);
    let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let pixels: Vec<u8> =
        serde_json::from_value(fixture["cases"][0]["frames"][1]["pixels"].clone()).unwrap();
    wait(|| viewer.get_scanbar_text().starts_with("2/2 - ") && rgb(&viewer.get_media()) == pixels);
    let status = viewer.get_scanbar_text();
    let before = rgb(&viewer.get_media());
    set(&store, false);
    viewer.invoke_presentation_settings_changed();
    assert_eq!(viewer.get_scanbar_text(), status);
    assert_eq!(
        rgb(&viewer.get_media()),
        before,
        "static cache notification cannot reset a paused animation to frame zero"
    );
    viewer.invoke_close_requested();
}
