//! Actual Options namespace questions, persistence and live tag/OR colour consumers.
use hydrus_core::{Tag, search::predicate::Predicate, tag_presentation::NamespaceColours};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, WindowEvent},
};

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    assert_eq!(
        options
            .get_rows()
            .iter()
            .filter(|row| row.kind == 31)
            .count(),
        1,
        "namespace colours renders its own editor once"
    );
    assert!(
        options.get_rows().iter().all(|row| row.kind != 34),
        "namespace colours does not render the unrelated routing table"
    );
    options
}
fn labels(options: &OptionsWindow) -> Value {
    json!(
        options
            .get_namespace_colour_rows()
            .iter()
            .map(|row| row.label.to_string())
            .collect::<Vec<_>>()
    )
}
fn expected_labels(rows: &Value) -> Value {
    json!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["label"].as_str().unwrap())
            .collect::<Vec<_>>()
    )
}
fn namespace_label(namespace: &Value) -> String {
    if namespace.is_null() {
        "namespaced tags".to_owned()
    } else if namespace == "" {
        "unnamespaced tags".to_owned()
    } else if namespace.as_str().unwrap().contains(':') {
        format!("{}:", namespace.as_str().unwrap())
    } else {
        format!("'{}' tags", namespace.as_str().unwrap())
    }
}
fn rgb(colour: slint::Color) -> [u8; 3] {
    [colour.red(), colour.green(), colour.blue()]
}

// leaf: audit-options-tag-presentation-namespace-colours-add
// leaf: audit-options-tag-presentation-namespace-colours-delete
// leaf: audit-options-tag-presentation-other-rendering-namespace-for-the-or-top-row
#[test]
fn actual_namespace_questions_cancel_retired_owners_reopen_and_live_colours_replay_qt() {
    let fixture = hydrus_testkit::fixture_json("namespace_colour_controls.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound.current.borrow().borrow().results()[0];
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("parity artists:alpha").unwrap(),
            )?;
            writer.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    let count = bound.current.borrow().borrow().predicates().len();
    for index in (0..count).rev() {
        ui.invoke_remove_predicate(i32::try_from(index).unwrap());
    }
    bound
        .current
        .borrow()
        .borrow_mut()
        .apply_or_editor(vec![Predicate::Or(vec![
            Predicate::Tag {
                tag: Tag::new("parity artists:alpha").unwrap(),
                inclusive: true,
            },
            Predicate::Tag {
                tag: Tag::new("series:beta").unwrap(),
                inclusive: true,
            },
        ])]);
    ui.invoke_refresh_page();
    assert_eq!(bound.current.borrow().borrow().results().len(), 1);
    let query = bound
        .current
        .borrow()
        .borrow()
        .favourite_to_save()
        .unwrap()
        .search
        .predicates;
    let before = store.read::<NamespaceColours>(settings::get).unwrap();
    let options = open(&ui, &bound);
    assert_eq!(labels(&options), expected_labels(&fixture["initial"]));
    for event in fixture["events"].as_array().unwrap() {
        if event["action"] == "add" {
            options.invoke_namespace_colour_action("add".into());
            let child = bound
                .options_colour_child
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            assert_eq!(json!([child.get_message().as_str()]), event["questions"]);
            options.invoke_apply();
            assert!(
                bound.options.borrow().is_some(),
                "pending question blocks parent Apply"
            );
            if event["cancel"] == true {
                child.invoke_cancelled();
                child.invoke_name_entered("retired namespace".into());
            } else if event["input"] == " -Parity Artists::: " {
                // One accepted add uses actual native LineEdit input and Return dispatch.
                let native = windows.get(windows.count() - 1).unwrap();
                let pixels = headless::render(&native, 520, 200);
                headless::save_png(
                    &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join("namespace-colours-add-entry.png"),
                    &pixels,
                    520,
                    200,
                )
                .unwrap();
                native.dispatch_event(WindowEvent::WindowActiveChanged(true));
                native.dispatch_event(WindowEvent::KeyPressed {
                    text: event["input"].as_str().unwrap().into(),
                });
                native.dispatch_event(WindowEvent::KeyReleased {
                    text: event["input"].as_str().unwrap().into(),
                });
                native.dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Return.into(),
                });
                native.dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Return.into(),
                });
            } else {
                child.invoke_name_entered(event["input"].as_str().unwrap().into());
            }
            if let Some(warning) = event["warnings"].as_array().unwrap().first() {
                let notice = bound
                    .options_colour_child
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .clone_strong();
                assert!(notice.get_notice_only());
                assert!(!notice.get_asking_name());
                let recorded = &event["warning_dialogs"][0];
                assert_eq!(recorded["parent_is_panel"], true);
                assert_eq!(recorded["ok_only"], true);
                assert_eq!(
                    json!([notice.get_notice_ok_label().as_str()]),
                    recorded["button_labels"]
                );
                assert_eq!(
                    notice.get_window_title(),
                    recorded["title"].as_str().unwrap()
                );
                assert_eq!(notice.get_message(), warning.as_str().unwrap());
                assert_eq!(notice.get_message(), recorded["message"].as_str().unwrap());
                assert!(notice.window().is_visible());
                assert!(options.get_namespace_colour_child_open());
                assert_eq!(options.get_namespace_colour_error(), "");
                child.invoke_name_entered("retired invalid question".into());
                child.invoke_cancelled();
                assert!(notice.window().is_visible());
                assert!(bound.options_colour_child.borrow().is_some());
                options.invoke_apply();
                assert!(
                    bound.options.borrow().is_some(),
                    "warning blocks parent Apply"
                );
                let native = windows.get(windows.count() - 1).unwrap();
                let pixels = headless::render(&native, 520, 200);
                assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
                headless::save_png(
                    &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join("namespace-colours-warning.png"),
                    &pixels,
                    520,
                    200,
                )
                .unwrap();
                notice.invoke_cancelled();
                assert!(!notice.window().is_visible());
                assert!(!options.get_namespace_colour_child_open());
            }
        } else {
            if event["clear_selection"] == true {
                for (index, row) in options.get_namespace_colour_rows().iter().enumerate() {
                    if row.selected {
                        options.invoke_namespace_colour_clicked(
                            i32::try_from(index).unwrap(),
                            true,
                            false,
                        );
                    }
                }
            }
            if event["yes"] == false {
                for namespace in event["selected"].as_array().unwrap() {
                    let label = namespace_label(namespace);
                    let index = options
                        .get_namespace_colour_rows()
                        .iter()
                        .position(|row| row.label == label)
                        .unwrap();
                    if !options
                        .get_namespace_colour_rows()
                        .row_data(index)
                        .unwrap()
                        .selected
                    {
                        options.invoke_namespace_colour_clicked(
                            i32::try_from(index).unwrap(),
                            true,
                            false,
                        );
                    }
                }
            }
            options.invoke_namespace_colour_action("delete".into());
            if event["questions"].as_array().unwrap().is_empty() {
                assert!(
                    bound.options_colour_child.borrow().is_none(),
                    "protected/empty Delete asks nothing"
                );
            } else {
                let child = bound
                    .options_colour_child
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .clone_strong();
                assert_eq!(json!([child.get_message().as_str()]), event["questions"]);
                assert_eq!(child.get_window_title(), "Are you sure?");
                assert!(!child.get_asking_name());
                assert!(!child.get_notice_only());
                if event["yes"] == false {
                    let native = windows.get(windows.count() - 1).unwrap();
                    headless::save_png(
                        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                            .join("namespace-colours-delete-question.png"),
                        &headless::render_snapshot(&native, 520, 200),
                        520,
                        200,
                    )
                    .unwrap();
                }
                child.invoke_answered(event["yes"].as_bool().unwrap());
            }
        }
        assert!(bound.options_colour_child.borrow().is_none());
        assert_eq!(labels(&options), expected_labels(&event["rows"]));
        assert_eq!(
            store.read::<NamespaceColours>(settings::get).unwrap(),
            before
        );
    }
    // No question is offered when only the protected fallback row is selected.
    let index = options
        .get_namespace_colour_rows()
        .iter()
        .position(|row| row.label == "namespaced tags")
        .unwrap();
    options.invoke_namespace_colour_clicked(i32::try_from(index).unwrap(), false, false);
    options.invoke_namespace_colour_action("delete".into());
    assert!(bound.options_colour_child.borrow().is_none());
    let noneable_frames =
        std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
    options.on_noneable_text_geometry({
        let frames = noneable_frames.clone();
        move |index, x, y, width, preferred| {
            frames.borrow_mut().insert(index, (x, y, width, preferred));
        }
    });
    let connector = options
        .get_rows()
        .iter()
        .position(|row| row.none_phrase == "use ideal tag colour")
        .unwrap();
    let connector = i32::try_from(connector).unwrap();
    let started = std::time::Instant::now();
    let pixels = loop {
        let pixels = headless::render(&windows.get(1).unwrap(), 950, 1600);
        if noneable_frames.borrow().contains_key(&connector) {
            break pixels;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "initial native checkbox measurement must be published"
        );
        std::thread::yield_now();
    };
    let (x, y, width, preferred) = noneable_frames.borrow()[&connector];
    assert!(y > 0.0 && y < 1600.0, "actual checkbox is in the viewport");
    assert!(
        width + 0.1 >= preferred,
        "full checkbox caption fits its native preferred width"
    );
    assert!(
        x + width <= 950.0,
        "full checkbox remains inside the actual window"
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("namespace-colours-draft.png"),
        &pixels,
        950,
        1600,
    )
    .unwrap();
    options.invoke_cancel();
    options.invoke_apply();
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before
    );
    let options = open(&ui, &bound);
    assert_eq!(
        labels(&options),
        expected_labels(&fixture["cancelled_rows"])
    );
    // A warning is owned by the same parent Cancel/stale boundary as its question.
    options.invoke_namespace_colour_action("add".into());
    let rejected = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    rejected.invoke_name_entered("".into());
    let retired_warning = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(retired_warning.get_notice_only());
    options.invoke_cancel();
    assert!(!retired_warning.window().is_visible());
    assert!(bound.options_colour_child.borrow().is_none());
    let options = open(&ui, &bound);
    options.invoke_namespace_colour_action("add".into());
    let retired = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    options.invoke_cancel();
    retired.invoke_name_entered("retired parent namespace".into());
    retired.invoke_answered(true);
    let options = open(&ui, &bound);
    options.invoke_namespace_colour_action("add".into());
    let successor = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    retired.invoke_cancelled();
    retired_warning.invoke_cancelled();
    retired_warning.invoke_name_entered("retired warning namespace".into());
    retired_warning.invoke_answered(true);
    assert!(
        bound.options_colour_child.borrow().is_some(),
        "retired close cannot clear successor slot"
    );
    // Hiding the live owner synchronously rejects a retained child acceptance.
    options.hide().unwrap();
    successor.invoke_name_entered("hidden owner namespace".into());
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before
    );
    successor.invoke_cancelled();
    options.show().unwrap();
    options.invoke_namespace_colour_action("add".into());
    let child = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_name_entered(" Parity Artists::: ".into());
    let added_colour = rgb(options
        .get_namespace_colour_rows()
        .iter()
        .find(|row| row.label == "'parity artists' tags")
        .unwrap()
        .colour);
    options.invoke_apply();
    let saved = store.read::<NamespaceColours>(settings::get).unwrap();
    assert_eq!(saved.colour(Some("parity artists")), added_colour);
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .favourite_to_save()
            .unwrap()
            .search
            .predicates,
        query
    );
    let tag = ui
        .get_tags()
        .iter()
        .find(|row| row.text.starts_with("parity artists:alpha"))
        .unwrap();
    assert_eq!(
        rgb(tag.colour),
        added_colour,
        "accepted namespace reaches actual media tag list"
    );
    // Sorting keeps Qt's numeric Shift range state while selections follow terms.
    for case in fixture["selection_cases"].as_array().unwrap() {
        let options = open(&ui, &bound);
        for event in case["events"].as_array().unwrap() {
            match event["action"].as_str().unwrap() {
                "initial" => {}
                "hit" => {
                    let label = namespace_label(&event["namespace"]);
                    let index = options
                        .get_namespace_colour_rows()
                        .iter()
                        .position(|row| row.label == label)
                        .unwrap();
                    options.invoke_namespace_colour_clicked(
                        i32::try_from(index).unwrap(),
                        event["ctrl"].as_bool().unwrap(),
                        event["shift"].as_bool().unwrap(),
                    );
                }
                "add" => {
                    options.invoke_namespace_colour_action("add".into());
                    let child = bound
                        .options_colour_child
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .clone_strong();
                    child.invoke_name_entered(event["input"].as_str().unwrap().into());
                }
                "delete" => {
                    options.invoke_namespace_colour_action("delete".into());
                    let child = bound
                        .options_colour_child
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .clone_strong();
                    assert_eq!(child.get_message(), "Delete all selected colours?");
                    child.invoke_answered(true);
                }
                other => panic!("unrecorded selection action {other}"),
            }
            assert_eq!(labels(&options), expected_labels(&event["rows"]));
            let selected: Vec<_> = options
                .get_namespace_colour_rows()
                .iter()
                .filter(|row| row.selected)
                .map(|row| row.label.to_string())
                .collect();
            let expected: Vec<_> = event["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(namespace_label)
                .collect();
            assert_eq!(selected, expected, "{case}");
            assert_eq!(
                store.read::<NamespaceColours>(settings::get).unwrap(),
                saved,
                "sorted selection stays in the parent draft"
            );
        }
        options.invoke_cancel();
    }
    // An explicit empty field starts from legacy None, but Cancel still preserves it.
    let cancelled = open(&ui, &bound);
    let index = cancelled
        .get_rows()
        .iter()
        .position(|row| row.label == fixture["labels"][0].as_str().unwrap())
        .unwrap();
    cancelled.invoke_text_edited(i32::try_from(index).unwrap(), "".into());
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        saved
    );
    for case in fixture["colour_cases"].as_array().unwrap() {
        let options = open(&ui, &bound);
        let index = options
            .get_rows()
            .iter()
            .position(|row| row.label == fixture["labels"][0].as_str().unwrap())
            .unwrap();
        options.invoke_text_edited(
            i32::try_from(index).unwrap(),
            case["input"].as_str().unwrap().into(),
        );
        options.invoke_apply();
        let expected = if case["input"] == "parity artists" {
            added_colour
        } else {
            serde_json::from_value(case["rows"][0][0][1].clone()).unwrap()
        };
        assert_eq!(
            rgb(ui.get_predicates().row_data(0).unwrap().colour),
            expected
        );
        assert_eq!(
            bound
                .current
                .borrow()
                .borrow()
                .favourite_to_save()
                .unwrap()
                .search
                .predicates,
            query
        );
        let reopened = open(&ui, &bound);
        assert_eq!(
            reopened.get_rows().row_data(index).unwrap().text,
            case["saved"].as_str().unwrap()
        );
        reopened.invoke_cancel();
    }
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened.read::<NamespaceColours>(settings::get).unwrap(),
        store.read::<NamespaceColours>(settings::get).unwrap()
    );
}

// leaf: audit-options-tag-presentation-namespace-colours-add
#[test]
fn blank_native_add_apply_and_return_keep_entry_until_acknowledgement_and_parent_cancel() {
    use slint::platform::{PointerEventButton, WindowAdapter as _};
    use std::{
        cell::RefCell,
        rc::Rc,
        time::{Duration, Instant},
    };
    fn key(
        window: &slint::platform::software_renderer::MinimalSoftwareWindow,
        text: impl Into<slint::SharedString>,
    ) {
        let text = text.into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let fixture = hydrus_testkit::fixture_json("namespace_entry_validation.json");
    assert_eq!(fixture["errors"], json!([]));
    let before = store.read::<NamespaceColours>(settings::get).unwrap();
    for use_apply in [false, true] {
        let recorded = fixture["add_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["route"] == if use_apply { "apply" } else { "return" })
            .unwrap();
        let accepted_text = recorded["valid_entered_text"].as_str().unwrap();
        let options = open(&ui, &bound);
        let original_rows = labels(&options);
        options.invoke_namespace_colour_action("add".into());
        let entry = bound
            .options_colour_child
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(
            entry.get_window_title(),
            recorded["title"].as_str().unwrap()
        );
        assert_eq!(
            json!([entry.get_message().as_str()]),
            recorded["message_labels"]
        );
        assert_eq!(recorded["allow_blank"], false);
        assert_eq!(entry.get_text(), "");
        assert!(entry.get_reject_blank_submission());
        let native = windows.get(windows.count() - 1).unwrap();
        native.dispatch_event(WindowEvent::WindowActiveChanged(true));
        let frame = Rc::new(RefCell::new(None));
        entry.on_name_apply_measured({
            let frame = frame.clone();
            move |value| {
                *frame.borrow_mut() = Some(value);
            }
        });
        entry.set_measure_name_apply(true);
        let start = Instant::now();
        loop {
            headless::render(&native, 520, 200);
            if frame
                .borrow()
                .as_ref()
                .is_some_and(|f| f.w > 0.0 && f.h > 0.0)
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "actual Apply geometry must arrive"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        entry.set_measure_name_apply(false);
        let count = windows.count();
        if use_apply {
            let f = frame.borrow().clone().unwrap();
            let position = slint::LogicalPosition::new(f.x + f.w / 2.0, f.y + f.h / 2.0);
            assert!(
                position.x > 0.0 && position.x < 520.0 && position.y > 0.0 && position.y < 200.0
            );
            native.dispatch_event(WindowEvent::PointerMoved { position });
            native.dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            native.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        } else {
            key(&native, Key::Return);
        }
        assert_eq!(
            windows.count(),
            count + 1,
            "empty widget submission owns one warning"
        );
        let notice = windows.get(windows.count() - 1).unwrap();
        let pixels = headless::render(&notice, 520, 200);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(if use_apply {
                "namespace-colours-empty-apply-warning.png"
            } else {
                "namespace-colours-empty-return-warning.png"
            }),
            &pixels,
            520,
            200,
        )
        .unwrap();
        assert!(entry.window().is_visible());
        assert!(notice.window().is_visible());
        assert!(entry.get_child_open());
        assert!(std::ptr::eq(
            entry.window(),
            bound
                .options_colour_child
                .borrow()
                .as_ref()
                .unwrap()
                .window()
        ));
        assert_eq!(entry.get_text(), "");
        assert_eq!(labels(&options), original_rows);
        assert_eq!(
            store.read::<NamespaceColours>(settings::get).unwrap(),
            before
        );
        entry.invoke_name_entered("blocked while warning".into());
        entry.invoke_blank_submitted();
        options.invoke_apply();
        assert!(bound.options.borrow().is_some());
        assert_eq!(windows.count(), count + 1);
        assert_eq!(labels(&options), original_rows);
        notice.dispatch_event(WindowEvent::WindowActiveChanged(true));
        key(&notice, Key::Return);
        assert!(!notice.window().is_visible());
        assert!(entry.window().is_visible());
        assert!(!entry.get_child_open());
        assert!(std::ptr::eq(
            entry.window(),
            bound
                .options_colour_child
                .borrow()
                .as_ref()
                .unwrap()
                .window()
        ));
        let retained_pixels = headless::render(&native, 520, 200);
        if use_apply {
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("namespace-colours-empty-retained-entry.png"),
                &retained_pixels,
                520,
                200,
            )
            .unwrap();
        }
        native.dispatch_event(WindowEvent::WindowActiveChanged(true));
        // Restore focus through the real re-enabled LineEdit after either path.
        let input = entry.get_name_input_frame();
        assert!(input.w > 0.0 && input.h > 0.0);
        let position =
            slint::LogicalPosition::new(input.x + input.w / 2.0, input.y + input.h / 2.0);
        native.dispatch_event(WindowEvent::PointerMoved { position });
        native.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        native.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        key(&native, accepted_text);
        assert_eq!(entry.get_text(), accepted_text);
        key(&native, Key::Return);
        assert!(bound.options_colour_child.borrow().is_none());
        assert!(!entry.window().is_visible());
        assert_ne!(labels(&options), original_rows);
        let added = recorded["rows_after"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                !recorded["rows_before"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|before| before["namespace"] == row["namespace"])
            })
            .unwrap();
        assert!(
            labels(&options)
                .as_array()
                .unwrap()
                .contains(&json!(namespace_label(&added["namespace"])))
        );
        assert_eq!(
            store.read::<NamespaceColours>(settings::get).unwrap(),
            before
        );
        options.invoke_cancel();
        let reopened = open(&ui, &bound);
        assert_eq!(labels(&reopened), original_rows);
        reopened.invoke_cancel();
    }
    // Raw whitespace reaches the namespace handler, unlike exact empty input.
    let whitespace = fixture["add_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["route"] == "whitespace")
        .unwrap();
    let options = open(&ui, &bound);
    let original_rows = labels(&options);
    options.invoke_namespace_colour_action("add".into());
    let entry = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let native = windows.get(windows.count() - 1).unwrap();
    headless::render(&native, 520, 200);
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    key(&native, whitespace["input"].as_str().unwrap());
    assert_eq!(
        entry.get_text(),
        whitespace["entered_text"].as_str().unwrap()
    );
    key(&native, Key::Return);
    assert!(!entry.window().is_visible());
    let notice = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(notice.get_notice_only());
    assert_eq!(
        notice.get_message(),
        whitespace["warning"]["message"].as_str().unwrap()
    );
    assert_eq!(
        notice.get_window_title(),
        whitespace["warning"]["title"].as_str().unwrap()
    );
    assert_eq!(
        json!([notice.get_notice_ok_label().as_str()]),
        whitespace["warning"]["button_labels"]
    );
    assert_eq!(labels(&options), original_rows);
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before
    );
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 520, 200);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("namespace-colours-whitespace-warning.png"),
        &pixels,
        520,
        200,
    )
    .unwrap();
    key(&native, Key::Return);
    assert!(bound.options_colour_child.borrow().is_none());
    options.invoke_cancel();
    // Cancelling the parent also retires the nested warning and retained input.
    let options = open(&ui, &bound);
    options.invoke_namespace_colour_action("add".into());
    let retired = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let native = windows.get(windows.count() - 1).unwrap();
    retired.hide().unwrap();
    let count = windows.count();
    retired.invoke_blank_submitted();
    assert_eq!(windows.count(), count, "hidden entry cannot open a warning");
    assert!(!retired.get_child_open());
    retired.show().unwrap();
    headless::render(&native, 520, 200);
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    key(&native, Key::Return);
    assert!(retired.get_child_open());
    let retired_notice = windows.get(windows.count() - 1).unwrap();
    headless::render(&retired_notice, 520, 200);
    options.invoke_cancel();
    assert!(!retired.window().is_visible());
    assert!(!retired_notice.window().is_visible());
    assert!(bound.options_colour_child.borrow().is_none());
    let successor_options = open(&ui, &bound);
    successor_options.invoke_namespace_colour_action("add".into());
    let successor = bound
        .options_colour_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let count = windows.count();
    retired.invoke_blank_submitted();
    retired.invoke_name_entered("retired blank owner".into());
    retired.invoke_cancelled();
    key(&retired_notice, Key::Return);
    assert_eq!(windows.count(), count);
    assert!(successor.window().is_visible());
    assert!(!successor.get_child_open());
    assert!(std::ptr::eq(
        successor.window(),
        bound
            .options_colour_child
            .borrow()
            .as_ref()
            .unwrap()
            .window()
    ));
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before
    );
    successor_options.invoke_cancel();
    ui.hide().unwrap();
}
