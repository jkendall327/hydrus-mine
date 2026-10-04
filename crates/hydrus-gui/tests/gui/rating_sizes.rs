//! Ratings Options reach the real dialog and four service-editor sample controls.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::settings::{self, RatingContextSizes};
use slint::{ComponentHandle as _, Model as _};
use std::rc::Rc;

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let index = window
        .get_pages()
        .iter()
        .position(|page| page.text == "ratings")
        .unwrap();
    window.set_page(i32::try_from(index).unwrap());
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    window
}

fn edit(window: &OptionsWindow, fixture: &serde_json::Value, values: &serde_json::Value) {
    for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
        let row = window
            .get_rows()
            .iter()
            .position(|row| row.label == label.as_str().unwrap())
            .unwrap();
        window.invoke_text_edited(
            i32::try_from(row).unwrap(),
            values[index].to_string().into(),
        );
    }
}

fn values(sizes: &RatingContextSizes) -> serde_json::Value {
    serde_json::json!([
        sizes.preview_icon_size,
        sizes.preview_incdec_height,
        sizes.dialog_icon_size,
        sizes.dialog_incdec_height
    ])
}

#[test]
fn sizes_replay_owned_options_and_reach_dialog_and_service_examples_after_reopen() {
    let fixture = hydrus_testkit::fixture_json("rating_context_sizes.json");
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    let services = store.snapshot().services.clone();
    let configs = services
        .all()
        .map(|service| (**service).clone())
        .collect::<Vec<_>>();
    for event in fixture["events"].as_array().unwrap() {
        let before: RatingContextSizes = store.read(settings::get).unwrap();
        let cancelled = options(&ui, &bound);
        edit(&cancelled, &fixture, &event["input"]);
        assert_eq!(
            store.read(settings::get::<RatingContextSizes>).unwrap(),
            before
        );
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        assert_eq!(
            store.read(settings::get::<RatingContextSizes>).unwrap(),
            before
        );
        let accepted = options(&ui, &bound);
        edit(&accepted, &fixture, &event["input"]);
        accepted.invoke_apply();
        let sizes: RatingContextSizes = store.read(settings::get).unwrap();
        assert_eq!(values(&sizes), event["saved"]);
        let reopened_store = hydrus_store::Store::open(store.dir()).unwrap();
        assert_eq!(
            reopened_store
                .read(settings::get::<RatingContextSizes>)
                .unwrap(),
            sizes
        );
        let reopened = options(&ui, &bound);
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let row = reopened
                .get_rows()
                .iter()
                .find(|row| row.label == label.as_str().unwrap())
                .unwrap();
            let shown: f64 = row.text.parse().unwrap();
            assert_eq!(
                shown.to_bits(),
                event["reopened"][index].as_f64().unwrap().to_bits()
            );
        }
        reopened.invoke_cancel();
        let slots = hydrus_gui::services_editor_window::Slots::default();
        let manage =
            hydrus_gui::services_editor_window::open(&store, &slots, Rc::new(|| {})).unwrap();
        *slots.manage.borrow_mut() = Some(manage.clone_strong());
        for name in ["favourites", "stars", "counter"] {
            let index = manage
                .get_rows()
                .iter()
                .position(|row| row.cells.row_data(0).unwrap() == name)
                .unwrap();
            manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
            manage.invoke_edit_clicked();
            let child = slots.edit.borrow().as_ref().unwrap().clone_strong();
            let preview = child.get_examples().row_data(2).unwrap();
            let dialog = child.get_examples().row_data(3).unwrap();
            assert_eq!(
                preview.icon_size.to_bits(),
                (sizes.preview_icon_size.trunc() as f32).to_bits()
            );
            assert_eq!(
                preview.incdec_height.to_bits(),
                (sizes.preview_incdec_height.trunc() as f32).to_bits()
            );
            assert_eq!(
                dialog.icon_size.to_bits(),
                (sizes.dialog_icon_size.trunc() as f32).to_bits()
            );
            assert_eq!(
                dialog.incdec_height.to_bits(),
                (sizes.dialog_incdec_height.trunc() as f32).to_bits()
            );
            child.invoke_cancel_clicked();
            assert!(slots.edit.borrow().is_none());
        }
        manage.invoke_cancel_clicked();
        assert!(slots.manage.borrow().is_none());
        assert!(bound.options.borrow().is_none());
    }
    let accepted = options(&ui, &bound);
    edit(&accepted, &fixture, &fixture["cancel_before"]);
    accepted.invoke_apply();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound.current.borrow().borrow().results()[0];
    let original_ratings = store
        .read(|conn| {
            Ok(
                hydrus_store::media::load(conn, &services, None, &[file])?.results[0]
                    .ratings
                    .clone(),
            )
        })
        .unwrap();
    let counts = store
        .read(|conn| {
            Ok((
                conn.query_row("SELECT COUNT(*) FROM ratings", [], |row| {
                    row.get::<_, i64>(0)
                })?,
                conn.query_row("SELECT COUNT(*) FROM ratings_incdec", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_menu_requested(0);
    let item = ui
        .get_thumbnail_menu()
        .manage
        .iter()
        .find(|item| item.label == "ratings")
        .unwrap();
    ui.invoke_menu_chosen(item.id);
    let dialog = bound
        .manage_ratings
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(dialog.get_rating_size().to_bits(), 26.0_f32.to_bits());
    assert_eq!(dialog.get_incdec_height().to_bits(), 41.0_f32.to_bits());
    assert!(dialog.get_rating_outline() > 1.0);
    let counter = dialog
        .get_names()
        .iter()
        .position(|name| name == "counter")
        .unwrap();
    assert_eq!(
        dialog
            .get_counter_widths()
            .row_data(counter)
            .unwrap()
            .to_bits(),
        82.0_f32.to_bits()
    );
    let numerical = dialog
        .get_names()
        .iter()
        .position(|name| name == "stars")
        .unwrap();
    // The actual TouchArea delivers moved while any button is grabbed, but only
    // a held left button may route a numerical drag (the recorded Qt boundary).
    dialog
        .window()
        .set_size(slint::LogicalSize::new(640.0, 300.0));
    let drawn = windows.get(windows.count() - 1).unwrap();
    let _ = headless::render(&drawn, 640, 300);
    // Locate the live numerical control after layout, rather than assuming row
    // y coordinates in a stretched window. These presses only edit this draft.
    let rating_model = dialog.get_ratings();
    let numerical_row = rating_model.row_data(numerical).unwrap();
    let width = numerical_row.shapes.row_count() as f32
        * (dialog.get_rating_size() + numerical_row.pad)
        - numerical_row.pad;
    let left = 640.0 - 10.0 - width;
    let numerical_index = i32::try_from(numerical).unwrap();
    dialog.invoke_rating_clicked(numerical_index, false, 0.0);
    let null = dialog.get_ratings().row_data(numerical).unwrap().shapes;
    let y = (10..250)
        .step_by(4)
        .find(|y| {
            let position = slint::LogicalPosition::new(left + width * 0.1, *y as f32);
            dialog
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                });
            dialog
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                });
            dialog
                .get_ratings()
                .row_data(numerical)
                .unwrap()
                .shapes
                .iter()
                .collect::<Vec<_>>()
                != null.iter().collect::<Vec<_>>()
        })
        .expect("actual numerical TouchArea receives a left press");
    let position = slint::LogicalPosition::new(left + width * 0.1, y as f32);
    let drag_position = slint::LogicalPosition::new(left + width * 0.8, y as f32);
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Right,
        });
    assert_eq!(
        rating_model,
        dialog.get_ratings(),
        "pointer press must retain the active rating row model"
    );
    let after_right = dialog.get_ratings().row_data(numerical).unwrap().shapes;
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: drag_position,
        });
    assert_eq!(
        dialog
            .get_ratings()
            .row_data(numerical)
            .unwrap()
            .shapes
            .iter()
            .collect::<Vec<_>>(),
        after_right.iter().collect::<Vec<_>>()
    );
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Right,
        });
    assert_eq!(
        fixture["dragging"][0]["before"],
        fixture["dragging"][0]["after"]
    );
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: drag_position,
        });
    dialog
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    let hydrus_store::services::ServiceKind::RatingNumerical(config) =
        &services.by_name("stars").unwrap().kind
    else {
        panic!("numerical service")
    };
    let expected_stars = config.stars(fixture["dragging"][1]["after"]["rating"].as_f64().unwrap());
    assert_eq!(
        rating_model,
        dialog.get_ratings(),
        "left drag must retain the active TouchArea"
    );
    let shapes = dialog.get_ratings().row_data(numerical).unwrap().shapes;
    for (index, shape) in shapes.iter().enumerate() {
        let brush = if index < usize::try_from(expected_stars).unwrap() {
            config.display.colours.like.brush
        } else {
            config.display.colours.dislike.brush
        };
        assert_eq!(
            shape.brush,
            slint::Color::from_rgb_u8(brush.0[0], brush.0[1], brush.0[2])
        );
    }
    let pixels = headless::render(&drawn, 640, 300);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("rating-dialog-sizes.png"),
        &pixels,
        640,
        300,
    )
    .unwrap();
    dialog.invoke_cancel();
    assert_eq!(
        store
            .read(|conn| Ok((
                conn.query_row("SELECT COUNT(*) FROM ratings", [], |row| row
                    .get::<_, i64>(0))?,
                conn.query_row("SELECT COUNT(*) FROM ratings_incdec", [], |row| row
                    .get::<_, i64>(0))?
            )))
            .unwrap(),
        counts
    );
    assert_eq!(
        store
            .snapshot()
            .services
            .all()
            .map(|service| (**service).clone())
            .collect::<Vec<_>>(),
        configs
    );
    let reopened_store = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened_store
            .read(|conn| {
                Ok(
                    hydrus_store::media::load(conn, &services, None, &[file])?.results[0]
                        .ratings
                        .clone(),
                )
            })
            .unwrap(),
        original_ratings
    );
    assert!(bound.current.borrow().borrow().results().contains(&file));
    assert!(bound.manage_ratings.borrow().is_none());
    ui.hide().unwrap();
    drop(dialog);
    drop(accepted);
    drop(bound);
    drop(ui);
    drop(windows);
}
