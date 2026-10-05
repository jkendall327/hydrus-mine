//! Real staged Options and pointer/key/wheel input reach thumbnail preferences.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::settings::{self, ThumbnailNavigation};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, row as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "thumbnails")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    window
}
fn row(w: &OptionsWindow, label: &str) -> i32 {
    w.get_rows()
        .iter()
        .position(|row| row.label == label)
        .unwrap() as i32
}
fn value(p: &ThumbnailNavigation) -> serde_json::Value {
    serde_json::json!({"shift":p.shift_moves_origin,"percent":p.visibility_percent,"rate":p.scroll_rate})
}
fn settle(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    for _ in 0..8 {
        headless::render(native, 700, 600);
    }
}
fn key(native: &slint::platform::software_renderer::MinimalSoftwareWindow, key: Key) {
    let text: slint::SharedString = key.into();
    native.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    native.dispatch_event(WindowEvent::KeyReleased { text });
}
fn click(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    index: usize,
    shift: bool,
) {
    let columns = ui.get_grid_columns() as usize;
    let width = ui.get_thumbnail_width() + 2.0 * ui.get_thumbnail_margin();
    let height = ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin();
    let position = slint::LogicalPosition::new(
        ui.get_grid_origin_x() + (index % columns) as f32 * width + width / 2.0,
        ui.get_grid_origin_y()
            + ui.get_grid_scroll()
            + (index / columns) as f32 * height
            + height / 2.0,
    );
    if shift {
        native.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
    }
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    if shift {
        native.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    }
}
#[test]
fn owned_options_replay_cancel_save_reopen_and_real_keyboard_wheel_consumers() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_navigation.json");
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    settle(&native);
    assert!(bound.current.borrow().borrow().results().len() > 12);
    let labels = fixture["labels"].as_array().unwrap();
    let before = store.read(settings::get::<ThumbnailNavigation>).unwrap();
    let cancelled = options(&ui, &bound);
    cancelled.invoke_check_toggled(row(&cancelled, labels[0].as_str().unwrap()), true);
    cancelled.invoke_number_edited(row(&cancelled, labels[1].as_str().unwrap()), 40);
    cancelled.invoke_text_edited(row(&cancelled, labels[2].as_str().unwrap()), "0.5".into());
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store.read(settings::get::<ThumbnailNavigation>).unwrap(),
        before
    );
    for event in fixture["events"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_check_toggled(
            row(&w, labels[0].as_str().unwrap()),
            event["input"][0].as_bool().unwrap(),
        );
        w.invoke_number_edited(
            row(&w, labels[1].as_str().unwrap()),
            event["input"][1].as_i64().unwrap() as i32,
        );
        w.invoke_text_edited(
            row(&w, labels[2].as_str().unwrap()),
            event["input"][2].as_str().unwrap().into(),
        );
        w.invoke_apply();
        assert_eq!(value(&store.read(settings::get).unwrap()), event["saved"]);
        let reopened = options(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[0].as_str().unwrap()) as usize)
                .unwrap()
                .checked,
            event["reopened"]["shift"].as_bool().unwrap()
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[1].as_str().unwrap()) as usize)
                .unwrap()
                .number,
            event["reopened"]["percent"].as_i64().unwrap() as i32
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[2].as_str().unwrap()) as usize)
                .unwrap()
                .text,
            event["reopened"]["rate"].as_str().unwrap()
        );
        reopened.invoke_cancel();
    }
    for case in fixture["selections"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_check_toggled(
            row(&w, labels[0].as_str().unwrap()),
            case["enabled"].as_bool().unwrap(),
        );
        w.invoke_text_edited(row(&w, labels[2].as_str().unwrap()), "1.0".into());
        w.invoke_apply();
        ui.invoke_select_none();
        ui.set_grid_scroll(0.0);
        settle(&native);
        for step in case["steps"].as_array().unwrap() {
            let action = step["action"].as_str().unwrap();
            if action == "move" {
                key(&native, Key::RightArrow);
            } else {
                click(
                    &native,
                    &ui,
                    step["index"].as_u64().unwrap() as usize,
                    action == "shift",
                );
            }
            settle(&native);
            let page = bound.current.borrow().clone();
            let page = page.borrow();
            assert_eq!(
                serde_json::json!(page.selected_indices()),
                step["after"]["selected"]
            );
            assert_eq!(serde_json::json!(page.focused()), step["after"]["focused"]);
        }
    }
    // Real wheel events use Winit's60px-per-line normalization; Qt's fixture
    // uses angle120. Both represent one tick at the default three wheel lines.
    let span = ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin();
    for rate in fixture["rates"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_text_edited(
            row(&w, labels[2].as_str().unwrap()),
            rate["rate"].as_str().unwrap().into(),
        );
        w.invoke_apply();
        assert_eq!(
            i64::from(ui.get_thumbnail_wheel_step()),
            rate["step"].as_i64().unwrap()
        );
        ui.set_grid_scroll(0.0);
        settle(&native);
        let selected = bound.current.borrow().borrow().selected_files();
        native.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(
                ui.get_grid_origin_x() + span / 2.0,
                ui.get_grid_origin_y() + span / 2.0,
            ),
            delta_x: 0.0,
            delta_y: -60.0,
        });
        settle(&native);
        let expected = (3.0 * ui.get_thumbnail_wheel_step() as f32)
            .min(ui.get_grid_visible_height())
            .min((ui.get_grid_content_height() - ui.get_grid_visible_height()).max(0.0));
        assert!(
            (-ui.get_grid_scroll() - expected).abs() < 0.1,
            "actual wheel distance"
        );
        assert_eq!(bound.current.borrow().borrow().selected_files(), selected);
    }
    // Partially visible target rows exercise strict threshold changes through
    // actual Down keys; a fully hidden End target would merely hit every cap.
    for percent in [1, 40, 75, 99] {
        let w = options(&ui, &bound);
        w.invoke_number_edited(row(&w, labels[1].as_str().unwrap()), percent);
        w.invoke_apply();
        let count = bound.current.borrow().borrow().results().len();
        let columns = ui.get_grid_columns() as usize;
        let top = ((count - 1) / columns) as f32 * span + ui.get_thumbnail_margin();
        for extra in [-1.0, 1.0] {
            ui.invoke_select_none();
            let offset =
                (top - ui.get_grid_visible_height() + span * percent as f32 / 100.0 + extra).clamp(
                    0.0,
                    (ui.get_grid_content_height() - ui.get_grid_visible_height()).max(0.0),
                );
            ui.set_grid_scroll(-offset);
            settle(&native);
            click(&native, &ui, count - 1 - columns, false);
            settle(&native);
            let target = hydrus_gui_model::thumbnail_navigation::scroll_target(
                f64::from(top),
                f64::from(span),
                f64::from(-ui.get_grid_scroll()),
                f64::from(ui.get_grid_visible_height()),
                f64::from(ui.get_grid_content_height()),
                percent as u8,
            ) as f32;
            key(&native, Key::DownArrow);
            settle(&native);
            assert_eq!(bound.current.borrow().borrow().focused(), Some(count - 1));
            assert!(
                (-ui.get_grid_scroll() - target).abs() < 0.1,
                "percent{percent}, edge{extra}"
            );
        }
    }
    let pixels = headless::render_snapshot(&native, 700, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-navigation.png"),
        &pixels,
        700,
        600,
    )
    .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<ThumbnailNavigation>)
            .unwrap()
            .visibility_percent,
        99
    );
}
