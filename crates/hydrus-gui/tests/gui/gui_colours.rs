//! Actual Qt colour controls reach staged/persistent settings and owned native paint.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, Theme, bind, headless};
use hydrus_store::{
    gui_colours::{self, Settings},
    services::Rgb,
    settings,
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};
const OVERRIDE: &str = "override what is set in the stylesheet with the colours on this page: ";
const CURRENT: &str = "current colourset: ";
fn values(settings: &Settings) -> Value {
    json!({"override":settings.override_stylesheet,"current":gui_colours::SET_NAMES[settings.current.min(1)],"sets":{"default":settings.sets[0].map(|rgb|rgb.0),"darkmode":settings.sets[1].map(|rgb|rgb.0)}})
}
fn preferences(value: &Value) -> Settings {
    let sets = std::array::from_fn(|set| {
        std::array::from_fn(|role| {
            Rgb(
                serde_json::from_value(value["sets"][gui_colours::SET_NAMES[set]][role].clone())
                    .unwrap(),
            )
        })
    });
    Settings {
        override_stylesheet: value["override"].as_bool().unwrap(),
        current: usize::from(value["current"] == "darkmode"),
        sets,
    }
}
fn menu(ui: &MainWindow, title: &str, label: &str) {
    let title = ui
        .get_menu_titles()
        .iter()
        .position(|value| value.label == title)
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(title).unwrap(), 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|value| value.label == label).unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    menu(ui, "file", "options\u{2026}");
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "colours")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    options
}
fn row(options: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        options
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn colours(options: &OptionsWindow) -> Vec<[u8; 3]> {
    options
        .get_gui_colour_rows()
        .iter()
        .flat_map(|row| {
            row.colours
                .iter()
                .map(|colour| [colour.red(), colour.green(), colour.blue()])
                .collect::<Vec<_>>()
        })
        .collect()
}
fn save_png(windows: &headless::Windows, index: usize, name: &str, width: u32, height: u32) {
    let adapter = windows.get(index).unwrap();
    let pixels = headless::render(&adapter, width, height);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        width,
        height,
    )
    .unwrap();
}
#[test]
fn real_options_stage_all_roles_cancel_hidden_inputs_and_retire_owned_picker() {
    let qt = hydrus_testkit::fixture_json("gui_coloursets.json");
    let (_directories, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    let options_native = windows.get(windows.count() - 1).unwrap();
    assert_eq!(options.get_gui_colour_tab(), 0);
    assert!(!options.get_gui_colour_enabled());
    assert_eq!(
        colours(&options),
        Settings::default().sets[0].map(|rgb| rgb.0)
    );
    options.invoke_gui_colour_chosen(0, 0);
    assert!(hydrus_gui::options_gui_colours::last_opened().is_none());
    options.invoke_check_toggled(row(&options, OVERRIDE), true);
    options.invoke_choice_chosen(row(&options, CURRENT), 1);
    for edit in qt["role_edits"].as_array().unwrap() {
        let set = i32::from(edit["set"] == "darkmode");
        let role = i32::try_from(edit["role"].as_u64().unwrap()).unwrap();
        options.set_gui_colour_tab(set);
        // Process the real tab binding before observing its row model.
        for _ in 0..3 {
            headless::render(&options_native, 950, 700);
        }
        options.invoke_gui_colour_chosen(set, role);
        let picker = hydrus_gui::options_gui_colours::last_opened().unwrap();
        let rgb: [u8; 3] = serde_json::from_value(edit["colour"].clone()).unwrap();
        if set == 0 && role == 0 {
            let original = colours(&options);
            picker.hide().unwrap();
            picker.invoke_accepted(251, 7, 11);
            assert_eq!(
                colours(&options),
                original,
                "hidden retained child cannot stage RGB"
            );
            picker.show().unwrap();
            options.invoke_choice_chosen(row(&options, CURRENT), 0);
            options.invoke_apply();
            assert!(bound.options.borrow().is_some());
            // The final real Store assertion requires darkmode, proving this
            // blocked current-choice callback did not replace the staged choice.
            save_png(
                &windows,
                windows.count() - 1,
                "gui-colour-picker.png",
                440,
                160,
            );
        }
        picker.invoke_accepted(i32::from(rgb[0]), i32::from(rgb[1]), i32::from(rgb[2]));
        assert!(!picker.window().is_visible());
        assert!(!options.get_gui_colour_child_open());
    }
    assert_eq!(
        values(&store.read(gui_colours::load).unwrap()),
        qt["before_update"]
    );
    options.set_gui_colour_tab(0);
    // Process the real tab binding before observing its row model.
    for _ in 0..3 {
        headless::render(&options_native, 950, 700);
    }
    assert_eq!(json!(colours(&options)), qt["saved"]["sets"]["default"]);
    options.set_gui_colour_tab(1);
    // Process the real tab binding before observing its row model.
    for _ in 0..3 {
        headless::render(&options_native, 950, 700);
    }
    assert_eq!(json!(colours(&options)), qt["saved"]["sets"]["darkmode"]);
    save_png(&windows, 1, "gui-coloursets-options.png", 950, 700);
    options.invoke_apply();
    assert!(bound.options.borrow().is_none());
    assert_eq!(values(&store.read(gui_colours::load).unwrap()), qt["saved"]);
    let options = open(&ui, &bound);
    assert_eq!(
        options.get_gui_colour_tab(),
        0,
        "editing tab always starts default even when darkmode active"
    );
    options.hide().unwrap();
    options.invoke_check_toggled(row(&options, OVERRIDE), false);
    options.invoke_choice_chosen(row(&options, CURRENT), 0);
    options.show().unwrap();
    options.invoke_apply();
    assert_eq!(values(&store.read(gui_colours::load).unwrap()), qt["saved"]);
    let options = open(&ui, &bound);
    options.invoke_gui_colour_chosen(0, 0);
    let old = hydrus_gui::options_gui_colours::last_opened().unwrap();
    let weak = old.as_weak();
    options.invoke_cancel();
    assert!(!old.window().is_visible());
    let successor = open(&ui, &bound);
    let before = colours(&successor);
    old.show().unwrap();
    old.invoke_accepted(3, 5, 7);
    old.invoke_cancelled();
    assert_eq!(colours(&successor), before);
    assert_eq!(values(&store.read(gui_colours::load).unwrap()), qt["saved"]);
    drop(old);
    assert!(weak.upgrade().is_none());
    successor.invoke_check_toggled(row(&successor, OVERRIDE), false);
    assert!(!successor.get_gui_colour_enabled());
    save_png(
        &windows,
        windows.count() - 1,
        "gui-coloursets-disabled.png",
        950,
        700,
    );
    successor.invoke_cancel();
    let options = open(&ui, &bound);
    options.invoke_gui_colour_chosen(0, 0);
    let picker = hydrus_gui::options_gui_colours::last_opened().unwrap();
    let weak_picker = picker.as_weak();
    options.invoke_cancel();
    drop(picker);
    assert!(
        weak_picker.upgrade().is_none(),
        "picker refresh must not retain its own owner"
    );
}
#[test]
fn help_warning_hidden_acknowledgement_toggle_rebind_and_final_bound_drop_are_owned() {
    let qt = hydrus_testkit::fixture_json("gui_coloursets.json");
    let (_directories, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let palette = ui.global::<hydrus_gui::Palette<'_>>().get_color_scheme();
    let window_brush = ui.global::<Theme<'_>>().get_window();
    for (index, event) in qt["events"].as_array().unwrap().iter().enumerate() {
        let mut settings = preferences(&event["saved"]);
        settings.current = usize::from(event["before"]["current"] == "darkmode");
        let before = settings.clone();
        store
            .write(move |tx| settings::set(tx.conn(), &settings))
            .unwrap();
        menu(&ui, "help", "darkmode");
        if before.override_stylesheet {
            assert!(hydrus_gui::gui_colour_actions::last_notice().is_none());
        } else {
            let notice = hydrus_gui::gui_colour_actions::last_notice().unwrap();
            assert_eq!(
                notice.get_message(),
                qt["warnings"][index]["message"].as_str().unwrap()
            );
            if index == 0 {
                save_png(
                    &windows,
                    windows.count() - 1,
                    "gui-colourset-darkmode-warning.png",
                    620,
                    300,
                );
            }
            notice.hide().unwrap();
            notice.invoke_cancelled();
            assert_eq!(store.read(gui_colours::load).unwrap(), before);
            notice.show().unwrap();
            notice.invoke_cancelled();
            assert!(!notice.window().is_visible());
        }
        assert_eq!(
            values(&store.read(gui_colours::load).unwrap()),
            event["saved"]
        );
        assert_eq!(
            ui.global::<Theme<'_>>().get_colours_override(),
            before.override_stylesheet
        );
        assert_eq!(
            ui.global::<hydrus_gui::Palette<'_>>().get_color_scheme(),
            palette
        );
        assert_eq!(ui.global::<Theme<'_>>().get_window(), window_brush);
    }
    store
        .write(|tx| {
            let mut colours = gui_colours::load(tx.conn())?;
            colours.override_stylesheet = false;
            settings::set(tx.conn(), &colours)
        })
        .unwrap();
    menu(&ui, "help", "darkmode");
    let old = hydrus_gui::gui_colour_actions::last_notice().unwrap();
    let before = store.read(gui_colours::load).unwrap();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert!(!old.window().is_visible());
    old.show().unwrap();
    old.invoke_cancelled();
    assert_eq!(store.read(gui_colours::load).unwrap(), before);
    old.hide().unwrap();
    drop(bound);
    menu(&ui, "help", "darkmode");
    let retained = hydrus_gui::gui_colour_actions::last_notice().unwrap();
    drop(successor);
    assert!(!retained.window().is_visible());
    retained.show().unwrap();
    retained.invoke_cancelled();
    assert_eq!(
        store.read(gui_colours::load).unwrap(),
        before,
        "last Bound drop leaves producer Store live but retires GUI action"
    );
    retained.hide().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    store
        .write(|tx| {
            let mut gui: settings::GuiSettings = settings::get(tx.conn())?;
            gui.confirm_exit = false;
            settings::set(tx.conn(), &gui)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(tx.conn())?;
            shutdown.action = 0;
            settings::set(tx.conn(), &shutdown)
        })
        .unwrap();
    menu(&ui, "help", "darkmode");
    let pending = hydrus_gui::gui_colour_actions::last_notice().unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !ui.window().is_visible(),
        "completed exit precedes retained callbacks"
    );
    assert!(!pending.window().is_visible());
    ui.show().unwrap();
    pending.show().unwrap();
    pending.invoke_cancelled();
    assert_eq!(store.read(gui_colours::load).unwrap(), before);
    pending.hide().unwrap();
    drop(bound);
    drop(windows);
}
#[test]
#[allow(clippy::float_cmp)]
fn saved_palette_change_during_real_row_fade_discards_old_brushes_without_redecoding() {
    use std::{cell::Cell, rc::Rc};

    let qt = hydrus_testkit::fixture_json("gui_coloursets.json");
    let (_directories, store) = super::subscriptions::store();
    let _windows = headless::init();
    let file = store
        .read(|conn| {
            let registry = hydrus_store::services::ServiceRegistry::load(conn)?;
            let roles = hydrus_store::content::DomainRoles::new(&registry)?;
            Ok(conn.query_row(
                "SELECT hash_id FROM file_domain_current WHERE service_id=?1 ORDER BY hash_id LIMIT 1",
                [roles.local[0]],
                |row| row.get::<_, hydrus_core::HashId>(0),
            )?)
        })
        .unwrap();
    let mut initial = preferences(&qt["saved"]);
    initial.current = 0;
    assert!(initial.override_stylesheet);
    let mut current = initial.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &initial))
        .unwrap();
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &hydrus_store::thumbnail_appearance::Preferences::default(),
            )
        })
        .unwrap();
    let page = SearchPage::restored(
        store.clone(),
        hydrus_search::FileSearchContext::default(),
        false,
        None,
        vec![file],
    );
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page));
    assert!(bound.current.borrow().borrow().new_thumbnail_renderer());
    let now = Rc::new(Cell::new(Duration::ZERO));
    bound.rows.set_paint_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    bound.rows.paint_tick(true, 0, 1);
    bound.rows.row_data(0).unwrap();
    bound.rows.wait();
    let first = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    let brush =
        |rgb: [u8; 3]| slint::Brush::from(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
    let decoded = first().image.to_rgba8().unwrap().as_bytes().to_vec();
    assert!(!decoded.is_empty());
    assert!(!first().selected);
    for remote in [false, true] {
        if remote {
            store
                .write_content(move |writer| {
                    writer.delete_files(writer.roles().local_file_storage, &[file], None)
                })
                .unwrap();
            // Refresh only the real metadata/paint row; decoded bytes stay cached.
            bound.rows.file_changed(0);
            first();
            now.set(now.get() + Duration::from_secs(1));
            bound.rows.paint_tick(true, 0, 1);
        }
        let normal = usize::from(remote) * 2;
        assert_eq!(first().local, !remote);
        assert_eq!(first().paint.fill, brush(current.active()[normal].0));
        assert_eq!(
            first().paint.border_brush,
            brush(current.active()[normal + 4].0)
        );
        ui.invoke_thumbnail_clicked(0, false, false);
        let selected = first();
        assert!(selected.selected);
        assert_eq!(selected.fade_opacity, 0.0);
        assert_eq!(selected.previous.fill, brush(current.active()[normal].0));
        assert_eq!(selected.paint.fill, brush(current.active()[normal + 1].0));
        assert_eq!(
            selected.paint.border_brush,
            brush(current.active()[normal + 5].0)
        );
        assert!(selected.previous.image.size().width > 0);
        now.set(now.get() + Duration::from_secs_f64(13.0 / 120.0));
        bound.rows.paint_tick(true, 0, 1);
        assert!(first().fade_opacity > 0.0 && first().fade_opacity < 1.0);
        let bytes = bound.rows.cached_bytes();
        let entries = bound.rows.cached();
        let old_fill = first().paint.fill;
        current.flip();
        let saved = current.clone();
        store
            .write(move |tx| settings::set(tx.conn(), &saved))
            .unwrap();
        // Exercise the real saved-settings observer and its composed layout hook.
        ui.global::<Theme<'_>>().invoke_refresh_colours();
        let refreshed = first();
        assert_eq!(refreshed.local, !remote);
        assert!(refreshed.selected);
        assert_ne!(refreshed.paint.fill, old_fill);
        assert_eq!(refreshed.paint.fill, brush(current.active()[normal + 1].0));
        assert_eq!(
            refreshed.paint.border_brush,
            brush(current.active()[normal + 5].0)
        );
        assert_eq!(refreshed.previous.image.size().width, 0);
        assert_eq!(refreshed.previous.fill, slint::Brush::default());
        assert_eq!(refreshed.previous.border_brush, slint::Brush::default());
        assert_eq!(refreshed.fade_opacity, 1.0);
        assert_eq!(refreshed.image.to_rgba8().unwrap().as_bytes(), decoded);
        assert_eq!(bound.rows.cached_bytes(), bytes);
        assert_eq!(bound.rows.cached(), entries);
        assert_eq!(
            ui.global::<Theme<'_>>().get_grid_background(),
            brush(current.active()[8].0)
        );
        ui.invoke_thumbnail_clicked(0, true, false);
        first();
        now.set(now.get() + Duration::from_secs(1));
        bound.rows.paint_tick(true, 0, 1);
        assert!(!first().selected);
    }
    store
        .write_content(move |writer| writer.add_files(writer.roles().local[0], &[(file, None)]))
        .unwrap();
    bound.rows.file_changed(0);
    let restored = first();
    assert!(restored.local);
    assert_eq!(restored.paint.fill, brush(current.active()[0].0));
    assert_eq!(restored.paint.border_brush, brush(current.active()[4].0));
    assert_eq!(restored.image.to_rgba8().unwrap().as_bytes(), decoded);
}

#[test]
fn fresh_local_membership_live_roles_and_existing_owned_windows_paint_without_palette_changes() {
    let qt = hydrus_testkit::fixture_json("gui_coloursets.json");
    let (_directories, store) = super::subscriptions::store();
    let windows = headless::init();
    let file = store
        .read(|conn| {
            let registry = hydrus_store::services::ServiceRegistry::load(conn)?;
            let roles = hydrus_store::content::DomainRoles::new(&registry)?;
            Ok(conn.query_row(
                "SELECT hash_id FROM file_domain_current WHERE service_id=?1 ORDER BY hash_id LIMIT 1",
                [roles.local[0]],
                |row| row.get::<_, hydrus_core::HashId>(0),
            )?)
        })
        .unwrap();
    let mut initial = preferences(&qt["saved"]);
    initial.current = 0;
    store
        .write(move |tx| settings::set(tx.conn(), &initial))
        .unwrap();
    // This fixture counts unobscured role colours, independently of animation.
    store
        .write(|tx| {
            let mut appearance = hydrus_store::thumbnail_appearance::load(tx.conn())?;
            appearance.fade = false;
            settings::set(tx.conn(), &appearance)
        })
        .unwrap();
    let page = SearchPage::restored(
        store.clone(),
        hydrus_search::FileSearchContext::default(),
        false,
        None,
        vec![file],
    );
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page));
    let adapter = windows.get(0).unwrap();
    headless::render(&adapter, 1100, 700);
    bound.rows.wait();
    let first = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    let paint = |remote: bool| {
        for selected in [false, true] {
            bound.rows.file_changed(0);
            if first().selected != selected {
                ui.invoke_thumbnail_clicked(0, true, false);
            }
            let mut row = bound.rows.row_data(0).unwrap();
            let mut thumbnail = row.thumbnails.row_data(0).unwrap();
            assert_eq!(thumbnail.local, !remote);
            assert_eq!(thumbnail.selected, selected);
            thumbnail.image = slint::Image::default();
            thumbnail.paint.image = slint::Image::default();
            thumbnail.previous.image = slint::Image::default();
            row.thumbnails = slint::ModelRc::new(slint::VecModel::from(vec![thumbnail]));
            ui.set_thumbnail_rows(slint::ModelRc::new(slint::VecModel::from(vec![row])));
            let pixels = headless::render(&adapter, 1100, 700);
            let role = usize::from(remote) * 2 + usize::from(selected);
            for expected in [role, role + 4] {
                let rgb: [u8; 3] =
                    serde_json::from_value(qt["saved"]["sets"]["default"][expected].clone())
                        .unwrap();
                assert!(
                    pixels
                        .chunks_exact(4)
                        .filter(|pixel| pixel[..3] == rgb)
                        .count()
                        > 20,
                    "real membership/selection must paint role {expected}"
                );
            }
        }
        ui.set_thumbnail_rows(slint::ModelRc::from(bound.rows.clone()));
    };
    assert!(first().local);
    paint(false);
    store
        .write_content(move |writer| writer.delete_files(writer.roles().local[0], &[file], None))
        .unwrap();
    assert!(first().local, "trash remains physically local");
    let paths = hydrus_gui::thumbnail_menu::paths(&store, &[file]);
    assert_eq!(paths.len(), 1);
    store
        .write_content(move |writer| {
            writer.delete_files(writer.roles().local_file_storage, &[file], None)
        })
        .unwrap();
    assert!(
        !first().local,
        "retained disk bytes do not imply current local membership"
    );
    paint(true);
    assert_eq!(hydrus_gui::thumbnail_menu::paths(&store, &[file]), paths);
    store
        .write_content(move |writer| writer.add_files(writer.roles().local[0], &[(file, None)]))
        .unwrap();
    assert!(first().local);
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let viewer_index = windows.count() - 1;
    ui.invoke_manage_tags_selected();
    let tags = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let generic = ui.global::<Theme<'_>>().get_window();
    let palette = ui.global::<hydrus_gui::Palette<'_>>().get_color_scheme();
    let options = open(&ui, &bound);
    options.invoke_choice_chosen(row(&options, CURRENT), 1);
    options.invoke_apply();
    let saved = store.read(gui_colours::load).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        if viewer.global::<Theme<'_>>().get_colours().row_data(12)
            == Some(slint::Color::from_rgb_u8(
                saved.active()[12].0[0],
                saved.active()[12].0[1],
                saved.active()[12].0[2],
            ))
            && tags
                .global::<Theme<'_>>()
                .get_colours()
                .iter()
                .collect::<Vec<_>>()
                == viewer
                    .global::<Theme<'_>>()
                    .get_colours()
                    .iter()
                    .collect::<Vec<_>>()
        {
            break;
        }
        std::thread::yield_now();
    }
    for global in [
        ui.global::<Theme<'_>>(),
        viewer.global::<Theme<'_>>(),
        tags.global::<Theme<'_>>(),
    ] {
        assert!(global.get_colours_override());
        assert_eq!(
            global
                .get_colours()
                .iter()
                .map(|c| [c.red(), c.green(), c.blue()])
                .collect::<Vec<_>>(),
            saved.active().map(|rgb| rgb.0)
        );
    }
    assert_eq!(ui.global::<Theme<'_>>().get_window(), generic);
    assert_eq!(
        ui.global::<hydrus_gui::Palette<'_>>().get_color_scheme(),
        palette
    );
    let pixels = headless::render(&adapter, 1100, 700);
    let grid = saved.active()[8].0;
    let tag = saved.active()[12].0;
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] == grid)
            .count()
            > 10_000,
        "actual grid background paints saved role"
    );
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] == tag)
            .count()
            > 300,
        "actual tag containers paint saved role"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-coloursets-live-main.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    viewer.set_media(slint::Image::default());
    viewer.set_sharp_shown(false);
    // The passive index is computed from the same public inputs used by the viewer.
    viewer.set_draw_index_background(true);
    viewer.set_caption("1/1".into());
    viewer.set_zoom_text("100%".into());
    assert_eq!(viewer.get_index_background_text().as_str(), "100% - 1/1");
    let viewer_pixels = headless::render(&windows.get(viewer_index).unwrap(), 800, 600);
    for (role, minimum) in [(10, 1000), (11, 4)] {
        let rgb = saved.active()[role].0;
        assert!(
            viewer_pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[..3] == rgb)
                .count()
                > minimum,
            "existing real viewer paints saved media role {role}"
        );
    }
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-coloursets-live-viewer.png"),
        &viewer_pixels,
        800,
        600,
    )
    .unwrap();
    let closed = viewer.global::<Theme<'_>>().get_colours();
    viewer.invoke_close_requested();
    viewer.show().unwrap();
    let mut changed = saved.clone();
    changed.sets[1][10] = Rgb([1, 2, 3]);
    store
        .write(move |tx| settings::set(tx.conn(), &changed))
        .unwrap();
    viewer.global::<Theme<'_>>().invoke_refresh_colours();
    assert_eq!(
        viewer.global::<Theme<'_>>().get_colours(),
        closed,
        "closed retained canvas does not refresh"
    );
    viewer.hide().unwrap();
    tags.invoke_cancel();
}
