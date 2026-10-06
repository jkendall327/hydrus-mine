//! Anchored/touch panning through actual pointer events and owned Options.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, ViewerPointerSettings},
};
use slint::{
    ComponentHandle as _, LogicalPosition, Model as _,
    platform::{PointerEventButton, WindowEvent},
};

const ANCHOR: &str = "Anchor mouse cursor during media viewer drags:";
const TOUCH: &str = "If set to anchor drags, undo on apparent touchscreen drag:";
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "media viewer")
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
#[test]
fn actual_pointer_drags_replay_qt_and_options_refresh_cancel_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("viewer_anchor_options.json");
    let (dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    for (i, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let options = options(&ui, &bound);
        options.invoke_check_toggled(row(&options, ANCHOR), case["anchor"].as_bool().unwrap());
        options.invoke_check_toggled(
            row(&options, TOUCH),
            case["touch_override"].as_bool().unwrap(),
        );
        options.invoke_apply();
        assert_eq!(viewer.get_anchor_drag(), case["anchor"].as_bool().unwrap());
        assert_eq!(
            viewer.get_touch_drag_unanchor(),
            case["touch_override"].as_bool().unwrap()
        );
        let x = 420.0 + i as f32 * 80.0;
        let at = |dx: f32, dy: f32| LogicalPosition::new(x + dx, 350.0 + dy);
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: at(0.0, 0.0),
        });
        viewer.window().dispatch_event(WindowEvent::PointerPressed {
            position: at(0.0, 0.0),
            button: PointerEventButton::Left,
        });
        assert!(viewer.get_drag_accepted());
        for step in case["moves"].as_array().unwrap() {
            let before = (viewer.get_media_x(), viewer.get_media_y());
            let dx = step["point"][0].as_i64().unwrap() as f32 - 120.0;
            let dy = step["point"][1].as_i64().unwrap() as f32 - 140.0;
            viewer.window().dispatch_event(WindowEvent::PointerMoved {
                position: at(dx, dy),
            });
            let delta = (
                viewer.get_media_x() - before.0,
                viewer.get_media_y() - before.1,
            );
            assert_eq!(
                (delta.0.round() as i32, delta.1.round() as i32),
                (
                    i32::try_from(step["delta"][0].as_i64().unwrap()).unwrap(),
                    i32::try_from(step["delta"][1].as_i64().unwrap()).unwrap()
                ),
                "case {i}, {step}"
            );
        }
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position: at(1.0, 0.0),
                button: PointerEventButton::Left,
            });
        let before = viewer.get_media_x();
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: at(30.0, 10.0),
        });
        assert_eq!(
            viewer.get_media_x().to_bits(),
            before.to_bits(),
            "released pointer cannot pan"
        );
    }
    let saved = store.read(settings::get::<ViewerPointerSettings>).unwrap();
    assert!(saved.anchor_drag && saved.touch_unanchors);
    // The live viewer after the existing recorded pointer/delta assertions.
    let pixels = headless::render_snapshot(&drawn, 1000, 750);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer_drag_anchor.png"),
        &pixels,
        1000,
        750,
    )
    .unwrap();
    let draft = options(&ui, &bound);
    draft.invoke_check_toggled(row(&draft, ANCHOR), false);
    draft.invoke_check_toggled(row(&draft, TOUCH), false);
    draft.invoke_cancel();
    assert!(viewer.get_anchor_drag() && viewer.get_touch_drag_unanchor());
    assert_eq!(
        store.read(settings::get::<ViewerPointerSettings>).unwrap(),
        saved
    );
    let disk = Store::open(dirs[1].path()).unwrap();
    assert_eq!(
        disk.read(settings::get::<ViewerPointerSettings>).unwrap(),
        saved
    );
    let reopened = options(&ui, &bound);
    assert!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, ANCHOR)).unwrap())
            .unwrap()
            .checked
    );
    assert!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, TOUCH)).unwrap())
            .unwrap()
            .checked
    );
    // Both reopened anchor controls above are verified against persisted settings.
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("options_viewer_drag_anchor.png"),
        &pixels,
        1100,
        850,
    )
    .unwrap();
    reopened.invoke_cancel();
    viewer.invoke_close_requested();
    viewer.show().unwrap();
    let before = viewer.get_media_x();
    viewer.invoke_drag_started(420.0, 350.0);
    viewer.invoke_drag_moved(470.0, 350.0);
    assert_eq!(
        viewer.get_media_x().to_bits(),
        before.to_bits(),
        "retired owner cannot pan even if retained and shown"
    );
    viewer.hide().unwrap();
}

fn ordinary_drag(viewer: &hydrus_gui::MediaViewerWindow, x: f32) -> (i32, i32) {
    let at = |dx: f32, dy: f32| LogicalPosition::new(x + dx, 350.0 + dy);
    let before = (viewer.get_media_x(), viewer.get_media_y());
    viewer.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(0.0, 0.0),
    });
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position: at(0.0, 0.0),
        button: PointerEventButton::Left,
    });
    viewer.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(-20.0, 10.0),
    });
    viewer.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(-30.0, 30.0),
    });
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: at(-30.0, 30.0),
            button: PointerEventButton::Left,
        });
    (
        (viewer.get_media_x() - before.0).round() as i32,
        (viewer.get_media_y() - before.1).round() as i32,
    )
}
#[test]
fn two_visible_viewers_read_saved_preferences_and_keep_independent_live_lifetimes() {
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            let mut pointer = settings::get::<ViewerPointerSettings>(ctx.conn())?;
            pointer.anchor_drag = true;
            pointer.touch_unanchors = false;
            settings::set(ctx.conn(), &pointer)?;
            settings::set(
                ctx.conn(),
                &hydrus_store::settings::ViewerTagScrollSettings(
                    hydrus_store::settings::TagWheelPropagation::Never,
                ),
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert!(files.len() > 3);
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            let tag = hydrus_core::Tag::new("owner:visible").unwrap();
            let id = hydrus_store::master::intern_tag(writer.conn(), &tag)?;
            writer.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                id,
                &files,
            )?;
            for i in 0..200 {
                let tag = hydrus_core::Tag::new(&format!("owner:long-{i:03}")).unwrap();
                let id = hydrus_store::master::intern_tag(writer.conn(), &tag)?;
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    id,
                    &files[..1],
                )?;
            }
            Ok(())
        })
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let first = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let first_native = windows.get(windows.count() - 1).unwrap();
    ui.invoke_thumbnail_activated(1);
    let second = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let second_native = windows.get(windows.count() - 1).unwrap();
    assert!(!std::ptr::eq(first.window(), second.window()));
    assert!(first.window().is_visible() && second.window().is_visible());
    headless::render(&first_native, 1000, 600);
    headless::render(&second_native, 1000, 600);
    let draft = options(&ui, &bound);
    draft.invoke_check_toggled(row(&draft, ANCHOR), false);
    draft.invoke_check_toggled(row(&draft, TOUCH), false);
    draft.invoke_apply();
    let second_position = (
        second.get_media_x().to_bits(),
        second.get_media_y().to_bits(),
    );
    assert_eq!(
        ordinary_drag(&first, 500.0),
        (-30, 30),
        "earlier visible owner reads newly saved unanchored mode"
    );
    assert!(!first.get_anchor_drag());
    assert_eq!(
        (
            second.get_media_x().to_bits(),
            second.get_media_y().to_bits()
        ),
        second_position,
        "pointer state belongs to first viewer"
    );
    let cancelled = options(&ui, &bound);
    cancelled.invoke_check_toggled(row(&cancelled, ANCHOR), true);
    cancelled.invoke_cancel();
    assert_eq!(
        ordinary_drag(&second, 420.0),
        (-30, 30),
        "Cancel leaves successor's saved mode unchanged"
    );
    let draft = options(&ui, &bound);
    draft.invoke_check_toggled(row(&draft, ANCHOR), true);
    draft.invoke_check_toggled(row(&draft, TOUCH), true);
    draft.invoke_apply();
    let before = (first.get_media_x(), first.get_media_y());
    let at = |dx: f32, dy: f32| LogicalPosition::new(620.0 + dx, 350.0 + dy);
    first.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(0.0, 0.0),
    });
    first.window().dispatch_event(WindowEvent::PointerPressed {
        position: at(0.0, 0.0),
        button: PointerEventButton::Left,
    });
    for (x, y) in [(50.0, 0.0), (51.0, 0.0), (61.0, 10.0)] {
        first
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position: at(x, y) });
    }
    first.window().dispatch_event(WindowEvent::PointerReleased {
        position: at(61.0, 10.0),
        button: PointerEventButton::Left,
    });
    assert_eq!(
        (
            (first.get_media_x() - before.0).round() as i32,
            (first.get_media_y() - before.1).round() as i32
        ),
        (111, 10),
        "same real Qt 50/51 latch after live Apply to earlier owner"
    );
    first.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(50.0, 140.0),
    });
    headless::render(&first_native, 1000, 600);
    assert!(first.get_tags_showing());
    assert!(first.get_tag_scroll_maximum() > 0.0);
    let first_caption = first.get_caption();
    let second_caption = second.get_caption();
    crate::viewer_tag_wheel::wheel(&first);
    assert_eq!(first.get_caption(), first_caption, "saved never policy");
    assert!(
        first.get_tag_scroll_offset() > 0.0,
        "earlier live list still scrolls with successor shown"
    );
    crate::viewer_tag_wheel::choose(&ui, &bound, 3);
    first.set_tag_scroll_offset(first.get_tag_scroll_maximum());
    crate::viewer_tag_wheel::wheel(&first);
    assert_ne!(
        first.get_caption(),
        first_caption,
        "earlier live hover reads policy Apply and navigates"
    );
    assert_eq!(first.get_tag_wheel_policy(), 3);
    assert_eq!(
        second.get_caption(),
        second_caption,
        "navigation stays per viewer"
    );
    first.invoke_close_requested();
    assert!(!first.window().is_visible());
    assert!(second.window().is_visible());
    assert!(
        std::ptr::eq(
            bound.viewer.borrow().as_ref().unwrap().window(),
            second.window()
        ),
        "closing earlier owner preserves successor slot"
    );
    first.show().unwrap();
    assert_eq!(
        ordinary_drag(&first, 720.0),
        (0, 0),
        "closed and retained owner stays retired"
    );
    let stale = first.get_caption();
    first.invoke_tag_wheel(-60.0, false, 50.0, 140.0);
    assert_eq!(first.get_caption(), stale);
    first.hide().unwrap();
    let draft = options(&ui, &bound);
    draft.invoke_check_toggled(row(&draft, ANCHOR), false);
    draft.invoke_apply();
    assert_eq!(
        ordinary_drag(&second, 600.0),
        (-30, 30),
        "successor stays active after other owner closes"
    );
    second.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(50.0, 140.0),
    });
    headless::render(&second_native, 1000, 600);
    let before = second.get_caption();
    crate::viewer_tag_wheel::wheel(&second);
    assert_ne!(
        second.get_caption(),
        before,
        "successor wheel remains active"
    );
    second.invoke_close_requested();
}
