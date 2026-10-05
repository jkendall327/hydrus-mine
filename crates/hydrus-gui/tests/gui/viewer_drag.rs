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
