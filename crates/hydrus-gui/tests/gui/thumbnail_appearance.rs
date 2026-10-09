//! Native Options, missing recovery, owned whole paints and clipped viewport background.
use hydrus_core::{HashId, thumbnail::ThumbnailSettings};
use hydrus_gui::{
    MainWindow, OptionsWindow, Pages, SearchPage, Thumbnail, ThumbnailPaint, bind, headless,
};
use hydrus_store::{Store, settings, thumbnail_appearance::Preferences};
use slint::{ComponentHandle as _, Model as _};
use std::{rc::Rc, sync::Arc, time::Duration};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let source = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    ([source, directory], store)
}
fn save(store: &Store, preferences: Preferences) {
    store
        .write(move |ctx| settings::set(ctx.conn(), &preferences))
        .unwrap();
}
fn choose(ui: &MainWindow, label: &str) {
    let panes = ui.get_menu_panes();
    let pane = panes.row_count() - 1;
    let i = panes
        .row_data(pane)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == label)
        .unwrap();
    ui.invoke_menu_line_clicked(pane as i32, i as i32, 100., 100., 100.);
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    let title = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "file")
        .unwrap();
    ui.invoke_menu_title_pressed(title as i32, 100., 20.);
    choose(ui, "options…");
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "thumbnails")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    window
        .get_rows()
        .iter()
        .position(|row| row.label.trim() == label)
        .unwrap() as i32
}
const PATH: &str =
    "EXPERIMENTAL: Image path for thumbnail panel background image (set blank to clear):";

// leaf: audit-options-thumbnails-new-rendering-tech-use-the-new-thumbnail-rendering-tech-only-applies-to-new-pages
// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn staged_browse_cancel_apply_new_page_policy_and_permanent_owner_retirement() {
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    assert!(bound.current.borrow().borrow().new_thumbnail_renderer());
    let window = options(&ui, &bound);
    let path = row(&window, PATH);
    assert_eq!(window.get_rows().row_data(path as usize).unwrap().kind, 20);
    hydrus_gui::set_picker(|_, _| Vec::new());
    window.invoke_directory_browse(path);
    assert_eq!(window.get_rows().row_data(path as usize).unwrap().text, "");
    let selected = dirs[1].path().join("staged/../synthetic.png");
    hydrus_gui::set_picker(move |kind, title| {
        assert_eq!(kind, hydrus_gui::Pick::Files);
        assert_eq!(title, "");
        vec![selected.clone()]
    });
    window.invoke_directory_browse(path);
    assert!(
        window
            .get_rows()
            .row_data(path as usize)
            .unwrap()
            .text
            .ends_with("synthetic.png")
    );
    assert!(
        !window
            .get_rows()
            .row_data(path as usize)
            .unwrap()
            .text
            .contains("..")
    );
    let rendering = row(
        &window,
        "Use the new thumbnail rendering tech (only applies to new pages):",
    );
    window.invoke_check_toggled(rendering, false);
    window.invoke_cancel();
    assert!(
        store
            .read(settings::get::<Preferences>)
            .unwrap()
            .new_renderer
    );
    let window = options(&ui, &bound);
    window.invoke_check_toggled(
        row(
            &window,
            "Use the new thumbnail rendering tech (only applies to new pages):",
        ),
        false,
    );
    window.invoke_apply();
    assert!(bound.current.borrow().borrow().new_thumbnail_renderer());
    assert!(!SearchPage::new(store.clone()).new_thumbnail_renderer());
    let window = options(&ui, &bound);
    let path = row(&window, PATH);
    let retained = window.clone_strong();
    let weak = window.as_weak();
    hydrus_gui::set_picker(move |_, _| {
        weak.upgrade().unwrap().invoke_cancel();
        vec!["retired.png".into()]
    });
    window.invoke_directory_browse(path);
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap().background,
        None
    );
    retained.show().unwrap();
    retained.invoke_directory_browse(path);
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap().background,
        None
    );
    retained.hide().unwrap();
    let live = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let window = options(&ui, &live);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1000, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-appearance-options.png"),
        &pixels,
        1000,
        850,
    )
    .unwrap();
    window.invoke_cancel();
    hydrus_gui::set_picker(|_, _| Vec::new());
}

// leaf: audit-options-thumbnails-appearance-use-blurhash-missing-thumbnail-fallback
#[test]
fn blurhash_real_metadata_default_invalid_disable_and_owned_cache_policy() {
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let id = bound.current.borrow().borrow().results()[0];
    let code = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE files SET blurhash=?1 WHERE hash_id=?2",
                rusqlite::params![code, id.0],
            )?;
            Ok(())
        })
        .unwrap();
    let settings = ThumbnailSettings::default();
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .unwrap()
        .pop()
        .unwrap()
        .info
        .unwrap();
    let size = settings.resolution(info.width, info.height);
    let media = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .unwrap()
        .pop()
        .unwrap();
    for path in [
        store.snapshot().storage.thumbnail_path(&media.hash),
        store.snapshot().storage.file_path(&media.hash, info.mime),
    ]
    .into_iter()
    .flatten()
    {
        assert!(
            path.starts_with(dirs[0].path()) || path.starts_with(dirs[1].path()),
            "only private fixture files may be removed: {}",
            path.display()
        );
        if path.is_file() {
            std::fs::remove_file(path).unwrap();
        }
    }
    bound.rows.thumbnails_changed();
    bound.rows.row_data(0).unwrap();
    bound.rows.wait();
    let loaded = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
            .image
    };
    let expected = hydrus_media::decode_blurhash(code, size.0, size.1).unwrap();
    let expected_rgba: Vec<u8> = expected
        .data()
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    assert_eq!(loaded().to_rgba8().unwrap().as_bytes(), expected_rgba);
    let capture_recovery = |name: &str, expected: &[u8]| {
        assert_eq!(bound.current.borrow().borrow().results()[0], id);
        for path in [
            store.snapshot().storage.thumbnail_path(&media.hash),
            store.snapshot().storage.file_path(&media.hash, info.mime),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !path.is_file(),
                "recovery must still use the missing source"
            );
        }
        assert!(store.read(settings::get::<Preferences>).unwrap().fade);
        let native = windows.get(0).unwrap();
        let now = Rc::new(std::cell::Cell::new(Duration::ZERO));
        bound.rows.set_paint_clock(Rc::new({
            let now = now.clone();
            move || now.get()
        }));
        headless::render(&native, 1100, 700);
        for index in 0..bound.rows.row_count().min(5) {
            bound.rows.row_data(index).unwrap();
        }
        bound.rows.wait();
        headless::render(&native, 1100, 700);
        now.set(Duration::from_secs(2));
        bound.rows.paint_tick(true, 0, 10);
        let pixels = headless::render(&native, 1100, 700);
        let thumbnail = bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap();
        assert_eq!(bound.current.borrow().borrow().results()[0], id);
        assert_eq!(
            thumbnail.paint.image.to_rgba8().unwrap().as_bytes(),
            expected
        );
        assert_eq!(thumbnail.fade_opacity.to_bits(), 1.0_f32.to_bits());
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
            &pixels,
            1100,
            700,
        )
        .unwrap();
    };
    capture_recovery("thumbnail-blurhash-enabled-native.png", &expected_rgba);

    let image = hydrus_gui::thumbnail_recovery(&store, id, &settings, true);
    assert_eq!(
        image,
        hydrus_media::decode_blurhash(code, size.0, size.1).unwrap()
    );
    let default = hydrus_gui::thumbnail_recovery(&store, id, &settings, false);
    assert_ne!(image, default);
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE files SET blurhash='invalid' WHERE hash_id=?1",
                [id.0],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        hydrus_gui::thumbnail_recovery(&store, id, &settings, true),
        default
    );
    // Keep a valid source hash for the actual disabled-policy consumer; an
    // invalid hash would default even if the worker ignored the saved toggle.
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE files SET blurhash=?1 WHERE hash_id=?2",
                rusqlite::params![code, id.0],
            )?;
            Ok(())
        })
        .unwrap();
    let window = options(&ui, &bound);
    window.invoke_check_toggled(
        row(&window, "Use blurhash missing thumbnail fallback:"),
        false,
    );
    window.invoke_cancel();
    assert!(store.read(settings::get::<Preferences>).unwrap().blurhash);
    let window = options(&ui, &bound);
    window.invoke_check_toggled(
        row(&window, "Use blurhash missing thumbnail fallback:"),
        false,
    );
    window.invoke_apply();
    assert!(!store.read(settings::get::<Preferences>).unwrap().blurhash);
    bound.rows.row_data(0).unwrap();
    bound.rows.wait();
    let default_rgba: Vec<u8> = default
        .data()
        .chunks_exact(usize::from(default.channels()))
        .flat_map(|p| [p[0], p[1], p[2], p.get(3).copied().unwrap_or(255)])
        .collect();
    assert_eq!(loaded().to_rgba8().unwrap().as_bytes(), default_rgba);
    assert_ne!(loaded().to_rgba8().unwrap().as_bytes(), expected_rgba);
    capture_recovery("thumbnail-blurhash-disabled-native.png", &default_rgba);
    let reopened = options(&ui, &bound);
    assert!(
        !reopened
            .get_rows()
            .row_data(row(&reopened, "Use blurhash missing thumbnail fallback:") as usize)
            .unwrap()
            .checked
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1000, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("thumbnail-blurhash-disabled-options.png"),
        &pixels,
        1000,
        850,
    )
    .unwrap();
    reopened.invoke_cancel();
    ui.invoke_retire_external_launches();
    assert_eq!(bound.rows.cached(), 0);
    ui.show().unwrap();
    bound.rows.receive();
    assert_eq!(bound.rows.cached(), 0);
    drop(windows);
}

#[test]
#[allow(clippy::float_cmp)]
fn whole_cell_snapshots_exact_default_threshold_interruptions_and_cached_revisit() {
    use hydrus_gui::thumbnail_paint::Paints;
    let _windows = headless::init();
    let image = slint::Image::from_rgb8(slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(2, 2));
    let mut paints = Paints::default();
    let mut thumbnail = Thumbnail::default();
    let mut paint = ThumbnailPaint {
        image,
        top: "old banner".into(),
        files: "3".into(),
        ..ThumbnailPaint::default()
    };
    paints.decorate(
        HashId(1),
        0,
        paint.clone(),
        &mut thumbnail,
        Duration::ZERO,
        true,
        false,
    );
    assert_eq!(thumbnail.fade_opacity, 1.0);
    paints.dirty(HashId(1));
    paint.selected = true;
    paint.top = "new banner".into();
    paint.files = "4".into();
    paints.decorate(
        HashId(1),
        0,
        paint.clone(),
        &mut thumbnail,
        Duration::ZERO,
        true,
        false,
    );
    assert_eq!(thumbnail.previous.top, "old banner");
    assert_eq!(thumbnail.previous.files, "3");
    assert!(!thumbnail.previous.selected);
    assert!(thumbnail.paint.selected);
    assert_eq!(thumbnail.fade_opacity, 0.0);
    paints.tick(Duration::from_secs_f64(13.0 / 120.0), true, true);
    paints.decorate(
        HashId(1),
        0,
        paint.clone(),
        &mut thumbnail,
        Duration::ZERO,
        true,
        false,
    );
    assert!((thumbnail.fade_opacity - 0.5).abs() < 1e-5);
    paints.dirty(HashId(1));
    paint.bottom = "interrupted".into();
    paints.decorate(
        HashId(1),
        0,
        paint.clone(),
        &mut thumbnail,
        Duration::from_secs(1),
        true,
        false,
    );
    assert_eq!(thumbnail.previous.top, "new banner");
    paints.tick(Duration::from_secs_f64(1.0 + 13.0 / 60.0), true, true);
    paints.decorate(
        HashId(1),
        0,
        paint.clone(),
        &mut thumbnail,
        Duration::from_secs(2),
        true,
        false,
    );
    assert_eq!(thumbnail.fade_opacity, 1.0);
    assert_eq!(thumbnail.previous.image.size().width, 0);
    let (_dirs, store) = store();
    let page = SearchPage::new(store);
    let owner = Rc::new(());
    assert!(page.admit_thumbnail_fade(HashId(1), &owner, 7));
    assert!(!page.admit_thumbnail_fade(HashId(1), &owner, 7));
    assert!(page.admit_thumbnail_fade(HashId(1), &owner, 8));
    assert!(page.admit_thumbnail_fade(HashId(1), &Rc::new(()), 8));
    paints.retain(&std::collections::BTreeSet::new());
    paints.decorate(
        HashId(1),
        0,
        paint,
        &mut thumbnail,
        Duration::from_secs(3),
        true,
        false,
    );
    assert_eq!(thumbnail.fade_opacity, 1.0);
}

// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn unscaled_background_clips_oversized_pixels_and_stays_fixed_on_scroll_and_clear() {
    let (dirs, store) = store();
    let path = dirs[1].path().join("oversized.png");
    headless::save_png(&path, &[240, 20, 90, 255].repeat(1400 * 900), 1400, 900).unwrap();
    save(
        &store,
        Preferences {
            new_renderer: false,
            background: Some(path.to_string_lossy().into_owned()),
            fade: false,
            ..Preferences::default()
        },
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let native = windows.get(0).unwrap();
    let pixels = headless::render(&native, 1100, 700);
    let left = ui.get_grid_origin_x().round() as usize;
    let top = ui.get_grid_origin_y().round() as usize;
    let right = left + ui.get_grid_visible_width().round() as usize;
    let bottom = top + ui.get_grid_visible_height().round() as usize;
    let pink = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(i, p)| (p == [240, 20, 90, 255]).then_some((i % 1100, i / 1100)))
            .collect::<Vec<_>>()
    };
    let points = pink(&pixels);
    assert!(!points.is_empty());
    assert!(
        points
            .iter()
            .all(|&(x, y)| x >= left && x < right && y >= top && y < bottom)
    );
    let corner = (bottom - 3) * 1100 + (right - 3);
    assert_eq!(&pixels[corner * 4..corner * 4 + 4], &[240, 20, 90, 255]);
    ui.set_grid_scroll(-300.);
    let scrolled = headless::render(&native, 1100, 700);
    assert_eq!(&scrolled[corner * 4..corner * 4 + 4], &[240, 20, 90, 255]);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-background-native.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    let window = options(&ui, &bound);
    window.invoke_text_edited(row(&window, PATH), "".into());
    window.invoke_cancel();
    assert!(ui.get_thumbnail_background().size().width > 0);
    let window = options(&ui, &bound);
    window.invoke_text_edited(row(&window, PATH), "".into());
    window.invoke_apply();
    assert_eq!(ui.get_thumbnail_background().size().width, 0);
    let cleared = headless::render(&native, 1100, 700);
    assert!(pink(&cleared).is_empty());
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("thumbnail-background-cleared-native.png"),
        &cleared,
        1100,
        700,
    )
    .unwrap();
    ui.invoke_retire_external_launches();
    ui.show().unwrap();
    assert_eq!(ui.get_thumbnail_background().size().width, 0);
}

// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn exit_cancel_preserves_nonempty_background_and_accepted_exit_permanently_clears_it() {
    let (dirs, store) = store();
    let path = dirs[1].path().join("exit-background.png");
    headless::save_png(&path, &[140, 33, 201, 255].repeat(31 * 17), 31, 17).unwrap();
    save(
        &store,
        Preferences {
            background: Some(path.to_string_lossy().into_owned()),
            ..Preferences::default()
        },
    );
    store
        .write(|ctx| {
            let mut gui: hydrus_store::settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    let has_background = |pixels: &[u8]| pixels.chunks_exact(4).any(|p| p == [140, 33, 201, 255]);
    assert_eq!(ui.get_thumbnail_background().size().width, 31);
    assert!(has_background(&headless::render(&native, 1000, 700)));
    let close = || {
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(
            ui.get_question()
                .starts_with("Are you sure you want to exit the client?")
        );
    };
    close();
    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    assert_eq!(ui.get_thumbnail_background().size().width, 31);
    assert!(has_background(&headless::render(&native, 1000, 700)));
    close();
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!(ui.get_thumbnail_background().size().width, 0);
    assert_eq!(bound.rows.background().size().width, 0);
    assert!(
        store
            .read(hydrus_store::thumbnail_appearance::load)
            .unwrap()
            .background
            .is_some()
    );
    ui.show().unwrap();
    ui.global::<hydrus_gui::Theme<'_>>()
        .invoke_refresh_colours();
    bound.rows.receive();
    assert_eq!(ui.get_thumbnail_background().size().width, 0);
    assert!(!has_background(&headless::render(&native, 1000, 700)));
    let fresh = bind(&ui, Pages::single(SearchPage::new(store)));
    assert_eq!(ui.get_thumbnail_background().size().width, 31);
    assert_eq!(fresh.rows.background().size().width, 31);
    assert!(has_background(&headless::render(&native, 1000, 700)));
}

#[test]
#[allow(clippy::float_cmp)]
fn real_main_selection_whole_cell_render_midpoint_finish_hide_and_retire() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(super::common::all_local_page(store)));
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 700);
    let now = Rc::new(std::cell::Cell::new(Duration::ZERO));
    bound.rows.set_paint_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    bound.rows.paint_tick(true, 0, 10);
    for row in 0..bound.rows.row_count().min(5) {
        bound.rows.row_data(row).unwrap();
    }
    bound.rows.wait();
    let before = headless::render(&native, 1100, 700);
    let thumb = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    assert_eq!(thumb().fade_opacity, 1.0);
    let left = (ui.get_grid_origin_x() + ui.get_thumbnail_margin()).round() as usize;
    let top = (ui.get_grid_origin_y() + ui.get_thumbnail_margin()).round() as usize;
    let width = ui.get_thumbnail_width().round() as usize;
    let height = ui.get_thumbnail_height().round() as usize;
    let crop = |pixels: &[u8]| {
        (top..top + height)
            .flat_map(|y| {
                pixels[(y * 1100 + left) * 4..(y * 1100 + left + width) * 4]
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>()
    };
    ui.invoke_thumbnail_clicked(0, false, false);
    assert!(thumb().paint.selected);
    assert!(!thumb().previous.selected);
    assert_eq!(thumb().fade_opacity, 0.0);
    assert_eq!(crop(&before), crop(&headless::render(&native, 1100, 700)));
    now.set(Duration::from_secs_f64(13.0 / 120.0));
    bound.rows.paint_tick(true, 0, 10);
    let middle = headless::render(&native, 1100, 700);
    assert_ne!(crop(&before), crop(&middle));
    now.set(Duration::from_secs_f64(13.0 / 60.0));
    bound.rows.paint_tick(true, 0, 10);
    let finished = headless::render(&native, 1100, 700);
    assert_eq!(thumb().fade_opacity, 1.0);
    assert_ne!(crop(&middle), crop(&finished));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("thumbnail-whole-cell-fade-native.png"),
        &middle,
        1100,
        700,
    )
    .unwrap();
    ui.hide().unwrap();
    bound.rows.paint_tick(false, 0, 10);
    ui.show().unwrap();
    bound.rows.paint_tick(true, 0, 10);
    assert_eq!(thumb().fade_opacity, 1.0);
    ui.invoke_retire_external_launches();
    ui.show().unwrap();
    bound.rows.paint_tick(true, 0, 10);
    assert_eq!(thumb().paint.image.size().width, 0);
}

#[test]
#[allow(clippy::float_cmp)]
fn old_page_cached_scroll_revisit_is_instant_and_fresh_decode_rearms_fade() {
    let (_dirs, store) = store();
    save(
        &store,
        Preferences {
            new_renderer: false,
            ..Preferences::default()
        },
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(super::common::all_local_page(store)));
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let native = windows.get(0).unwrap();
    headless::render(&native, 850, 700);
    let now = Rc::new(std::cell::Cell::new(Duration::ZERO));
    bound.rows.set_paint_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    bound.rows.clear_thumbnail_cache();
    bound.rows.paint_tick(true, 0, 10);
    bound.rows.row_data(0).unwrap();
    bound.rows.wait();
    let thumb = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    assert_eq!(thumb().fade_opacity, 0.0);
    now.set(Duration::from_secs(1));
    bound.rows.paint_tick(true, 0, 10);
    assert_eq!(thumb().fade_opacity, 1.0);
    headless::render(&native, 850, 700);
    assert!(bound.rows.row_count() > ui.get_grid_visible_rows() as usize);
    ui.set_grid_scroll(-ui.get_thumbnail_height() * bound.rows.row_count() as f32);
    headless::render(&native, 850, 700);
    bound.rows.paint_tick(
        true,
        ui.get_grid_first_row() as usize,
        ui.get_grid_visible_rows() as usize,
    );
    ui.set_grid_scroll(0.);
    headless::render(&native, 850, 700);
    bound
        .rows
        .paint_tick(true, 0, ui.get_grid_visible_rows() as usize);
    assert_eq!(thumb().fade_opacity, 1.0);
    bound.rows.clear_thumbnail_cache();
    bound.rows.row_data(0).unwrap();
    bound.rows.wait();
    assert_eq!(thumb().fade_opacity, 0.0);
}

#[test]
fn browse_uses_typed_draft_seed_and_hidden_appearance_controls_cannot_stage() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let window = options(&ui, &bound);
    let index = row(&window, PATH);
    window.invoke_text_edited(index, "typed/staged.png".into());
    let seeds = Rc::new(std::cell::RefCell::new(Vec::new()));
    window.on_background_path_pick({
        let seeds = seeds.clone();
        move |seed| {
            seeds.borrow_mut().push(seed.to_string());
            "".into()
        }
    });
    window.invoke_directory_browse(index);
    assert_eq!(seeds.borrow().as_slice(), ["typed/staged.png"]);
    window.invoke_cancel();
    let before: Preferences = store.read(settings::get).unwrap();
    let window = options(&ui, &bound);
    let indices = [
        row(&window, "Fade thumbnails:"),
        row(&window, "Use blurhash missing thumbnail fallback:"),
        row(
            &window,
            "Use the new thumbnail rendering tech (only applies to new pages):",
        ),
    ];
    window.hide().unwrap();
    for (index, value) in
        indices
            .into_iter()
            .zip([before.fade, before.blurhash, before.new_renderer])
    {
        window.invoke_check_toggled(index, !value);
    }
    window.show().unwrap();
    window.invoke_apply();
    assert_eq!(store.read(settings::get::<Preferences>).unwrap(), before);
}

// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn default_new_page_small_nonuniform_background_has_exact_unscaled_extent_on_resize() {
    let (dirs, store) = store();
    let path = dirs[1].path().join("small.png");
    let authored: Vec<u8> = (0_u8..17)
        .flat_map(|y| (0_u8..31).flat_map(move |x| [20 + x * 5, 120 + y * 3, 40 + x + y, 255]))
        .collect();
    headless::save_png(&path, &authored, 31, 17).unwrap();
    save(
        &store,
        Preferences {
            background: Some(path.to_string_lossy().into_owned()),
            ..Preferences::default()
        },
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.show().unwrap();
    assert!(bound.current.borrow().borrow().new_thumbnail_renderer());
    let native = windows.get(0).unwrap();
    for (width, height) in [(1100, 700), (950, 760)] {
        let pixels = headless::render(&native, width, height);
        let right = (ui.get_grid_origin_x() + ui.get_grid_visible_width()).round() as usize;
        let bottom = (ui.get_grid_origin_y() + ui.get_grid_visible_height()).round() as usize;
        assert_eq!(
            (
                ui.get_thumbnail_background().size().width,
                ui.get_thumbnail_background().size().height
            ),
            (31, 17)
        );
        for y in 0..17 {
            for x in 0..31 {
                let actual = ((bottom - 17 + y) * width as usize + right - 31 + x) * 4;
                let expected = (y * 31 + x) * 4;
                assert_eq!(
                    &pixels[actual..actual + 4],
                    &authored[expected..expected + 4]
                );
            }
        }
        let adjacent = ((bottom - 1) * width as usize + right - 32) * 4;
        assert_ne!(
            &pixels[adjacent..adjacent + 4],
            &authored[16 * 31 * 4..16 * 31 * 4 + 4]
        );
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "thumbnail-background-small-new-{width}x{height}-native.png"
            )),
            &pixels,
            width,
            height,
        )
        .unwrap();
    }
}

// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn qt_marker_background_has_exact_extent_in_old_and_default_new_owners() {
    let (dirs, store) = store();
    let path = dirs[1].path().join("qt-marker.png");
    let marker = [240, 20, 90, 255];
    headless::save_png(&path, &marker.repeat(31 * 17), 31, 17).unwrap();
    let windows = headless::init();
    for new_renderer in [false, true] {
        save(
            &store,
            Preferences {
                new_renderer,
                ..Preferences::default()
            },
        );
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.show().unwrap();
        assert_eq!(
            bound.current.borrow().borrow().new_thumbnail_renderer(),
            new_renderer
        );
        let native = windows.get(windows.count() - 1).unwrap();
        let blank = headless::render(&native, 1100, 700);
        let window = options(&ui, &bound);
        window.invoke_text_edited(row(&window, PATH), path.to_string_lossy().as_ref().into());
        window.invoke_apply();
        let pixels = headless::render(&native, 1100, 700);
        let right = (ui.get_grid_origin_x() + ui.get_grid_visible_width()).round() as usize;
        let bottom = (ui.get_grid_origin_y() + ui.get_grid_visible_height()).round() as usize;
        assert_eq!(
            (
                ui.get_thumbnail_background().size().width,
                ui.get_thumbnail_background().size().height
            ),
            (31, 17)
        );
        for y in bottom - 17..bottom {
            for x in right - 31..right {
                let offset = (y * 1100 + x) * 4;
                assert_eq!(&pixels[offset..offset + 4], &marker);
            }
        }
        for (x, y) in [(right - 32, bottom - 1), (right - 1, bottom - 18)] {
            let offset = (y * 1100 + x) * 4;
            assert_ne!(&pixels[offset..offset + 4], &marker);
            assert_eq!(&pixels[offset..offset + 4], &blank[offset..offset + 4]);
        }
        let mode = if new_renderer { "new" } else { "old" };
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "thumbnail-background-small-{mode}-qt-marker-native.png"
            )),
            &pixels,
            1100,
            700,
        )
        .unwrap();
        ui.invoke_retire_external_launches();
        ui.hide().unwrap();
    }
}

// Replacing a thumbnail's whole cell fades the old one out over the opacity
// curve the reference's new renderer was recorded drawing (`new_fade`), with
// the old paint kept until the fade completes.
#[test]
fn new_renderer_fade_follows_the_recorded_opacity_curve() {
    use hydrus_gui::thumbnail_paint::Paints;
    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("thumbnail_appearance.json");
    let image = slint::Image::from_rgb8(slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(2, 2));
    for sample in recorded["new_fade"]["samples"].as_array().unwrap() {
        let mut paints = Paints::default();
        let mut thumbnail = Thumbnail::default();
        let mut paint = ThumbnailPaint {
            image: image.clone(),
            top: "old".into(),
            ..ThumbnailPaint::default()
        };
        let start = Duration::from_secs(10);
        let decorate = |paints: &mut Paints, thumbnail: &mut Thumbnail, paint: &ThumbnailPaint| {
            paints.decorate(HashId(1), 0, paint.clone(), thumbnail, start, true, false);
        };
        decorate(&mut paints, &mut thumbnail, &paint);
        paints.dirty(HashId(1));
        paint.top = "new".into();
        decorate(&mut paints, &mut thumbnail, &paint);
        assert!(recorded["new_fade"]["old_at_start"].as_bool().unwrap());
        assert_eq!(thumbnail.previous.top, "old");
        let elapsed = Duration::from_secs_f64(sample["elapsed"].as_f64().unwrap());
        paints.tick(start + elapsed, true, true);
        decorate(&mut paints, &mut thumbnail, &paint);
        let expected = sample["opacity"].as_f64().unwrap();
        assert!(
            (f64::from(thumbnail.fade_opacity) - expected).abs() < 0.01,
            "at {elapsed:?}: {} vs {expected}",
            thumbnail.fade_opacity
        );
        assert_eq!(
            thumbnail.previous.image.size().width > 0,
            sample["old_retained"].as_bool().unwrap(),
            "at {elapsed:?}"
        );
    }
}
