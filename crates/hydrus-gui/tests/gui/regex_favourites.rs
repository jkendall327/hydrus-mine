//! Rendered favourite editor staging, validation, persistence and stale handles.
use crate::subscriptions::store;
use hydrus_core::url::strings::{MatchKind, PyRegex, StringMatch};
use hydrus_gui::{headless, regex_favourites_window};
use hydrus_gui_model::regex_favourites::RegexFavourites;
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

#[test]
fn favourite_window_stages_advisory_invalid_rows_and_cancels_retained_handles() {
    let windows = headless::init();
    let slot = regex_favourites_window::Slot::default();
    let accepted = Rc::new(RefCell::new(None));
    let initial = RegexFavourites(vec![("a+".into(), "letters".into())]);
    let apply: regex_favourites_window::Applied = Rc::new({
        let accepted = accepted.clone();
        move |value| {
            *accepted.borrow_mut() = Some(value);
            Ok(())
        }
    });
    let window = regex_favourites_window::open(&initial, &slot, apply.clone()).unwrap();
    window.invoke_action("add".into());
    window.set_phrase("[".into());
    window.invoke_changed();
    assert!(!window.get_valid());
    window.set_description("fragment".into());
    window.invoke_action("save-row".into());
    assert_eq!(window.get_rows().row_count(), 2);
    assert!(accepted.borrow().is_none());
    window.invoke_action("add".into());
    window.set_phrase("[".into());
    window.set_description("fragment".into());
    window.invoke_action("save-row".into());
    assert!(window.get_editing());
    assert_eq!(
        window.get_error(),
        "That regex and description are already in the list!"
    );
    window.invoke_action("cancel-row".into());
    let pixels = headless::render(&windows.get(0).unwrap(), 780, 500);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("regex_favourites.png"),
        &pixels,
        780,
        500,
    )
    .unwrap();
    window.invoke_action("cancel".into());
    let fresh = regex_favourites_window::open(&initial, &slot, apply).unwrap();
    window.invoke_action("apply".into());
    assert!(accepted.borrow().is_none());
    assert!(regex_favourites_window::has_open(&slot));
    fresh.invoke_action("apply".into());
    assert_eq!(*accepted.borrow(), Some(initial));
    assert!(!regex_favourites_window::has_open(&slot));
}

// leaf: audit-network-matcher-favourites
#[test]
fn matcher_favourites_manager_saves_global_choices_and_parent_cancel_closes_it() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = hydrus_gui::string_processor_window::Slots::default();
    let matcher = StringMatch {
        kind: MatchKind::Regex(PyRegex::new("a+")),
        example: "a".into(),
        ..StringMatch::any()
    };
    hydrus_gui::string_processor_window::open_match(&store, &matcher, &slots, Rc::new(|_| {}));
    let step = slots.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_manage_favourites();
    let manager = slots.favourites.borrow().as_ref().unwrap().clone_strong();
    assert!(step.get_child_open());
    manager.invoke_action("add".into());
    manager.set_phrase("new-regex".into());
    manager.set_description("new favourite".into());
    manager.invoke_action("save-row".into());
    manager.invoke_action("apply".into());
    assert!(!step.get_child_open());
    assert!(
        store
            .read(hydrus_store::regex_favourites::load)
            .unwrap()
            .0
            .contains(&("new-regex".into(), "new favourite".into()))
    );
    assert!(
        (0..step.get_favourites().row_count())
            .any(|i| step.get_favourites().row_data(i).unwrap() == "new favourite")
    );
    step.invoke_manage_favourites();
    let old = slots.favourites.borrow().as_ref().unwrap().clone_strong();
    step.invoke_cancel();
    assert!(!old.window().is_visible());
    old.invoke_action("apply".into());
    assert!(!slots.has_open());
}

#[test]
fn shared_regex_menus_copy_without_changing_text() {
    use hydrus_core::url::strings::{Conversion, StringConverter};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = hydrus_gui::string_processor_window::Slots::default();
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let matcher = StringMatch {
        kind: MatchKind::Regex(PyRegex::new("a+")),
        example: "a".into(),
        ..StringMatch::any()
    };
    hydrus_gui::string_processor_window::open_match(&store, &matcher, &slots, Rc::new(|_| {}));
    let step = slots.step.borrow().as_ref().unwrap().clone_strong();
    for index in 0..step.get_regex_components().row_count() {
        step.invoke_regex_tool(1, i32::try_from(index).unwrap());
    }
    assert_eq!(step.get_match_regex(), "a+");
    let expected = hydrus_gui_model::regex_favourites::regex_tools(1)
        .into_iter()
        .map(|row| row.1)
        .collect::<Vec<_>>();
    assert_eq!(*copied.borrow(), expected);
    step.invoke_regex_tool(-1, 0);
    step.invoke_regex_tool(1, 9999);
    step.invoke_cancel();
    let count = copied.borrow().len();
    step.invoke_regex_tool(1, 0);
    assert_eq!(copied.borrow().len(), count);

    let converter = hydrus_gui::string_processor_window::open_converter(
        &StringConverter {
            example: "a".into(),
            conversions: vec![Conversion::RegexSub {
                pattern: PyRegex::new("a"),
                replacement: "b".into(),
            }],
        },
        None,
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_edit();
    let conversion = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    for index in 0..conversion.get_regex_groups().row_count() {
        conversion.invoke_regex_tool(2, i32::try_from(index).unwrap());
    }
    assert_eq!(conversion.get_pattern(), "a");
    assert_eq!(conversion.get_replacement(), "b");
    assert_eq!(
        &copied.borrow()[count..],
        &hydrus_gui_model::regex_favourites::regex_tools(2)
            .into_iter()
            .map(|row| row.1)
            .collect::<Vec<_>>()
    );
    converter.invoke_cancel();
    let count = copied.borrow().len();
    conversion.invoke_regex_tool(2, 0);
    assert_eq!(copied.borrow().len(), count);
}

#[test]
fn last_conversion_child_acceptance_survives_parent_cancel_and_fresh_slots() {
    use hydrus_core::url::strings::{ProcessingStep, StringConverter};
    use hydrus_gui::string_processor_window::{Slots, open_converter};
    use hydrus_store::string_conversion::load;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("string_conversion_preference.json");
    for step in fixture["steps"].as_array().unwrap() {
        let slots = Slots::default();
        slots.set_store(&store);
        let window = open_converter(
            &StringConverter::default(),
            Some("example".into()),
            &slots,
            Rc::new(|_| panic!("Parent Cancel must not accept its draft")),
        )
        .unwrap();
        window.invoke_add();
        let child = slots.conversion.borrow().as_ref().unwrap().clone_strong();
        let types = child.get_types();
        let kind = usize::try_from(child.get_kind()).unwrap();
        assert_eq!(
            types.row_data(kind).unwrap(),
            step["opened"]["type"].as_str().unwrap()
        );
        assert_eq!(child.get_text(), step["opened"]["text"].as_str().unwrap());
        if let Some(label) = step["do"]["type"].as_str() {
            let index = (0..types.row_count())
                .position(|i| types.row_data(i).unwrap() == label)
                .unwrap();
            child.set_kind(i32::try_from(index).unwrap());
            child.invoke_changed();
        }
        if let Some(text) = step["do"]["text"].as_str() {
            child.set_text(text.into());
            child.invoke_changed();
        }
        if step["do"]["accept"] == true {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert!(slots.conversion.borrow().is_none());
        window.invoke_cancel();
        let expected =
            hydrus_downloader_exchange::processing::decode_text(&step["saved"].to_string())
                .unwrap();
        let ProcessingStep::Convert(expected) = &expected[0] else {
            panic!("reference preference is a converter")
        };
        assert_eq!(
            store.read(load).unwrap().0.as_ref(),
            expected.conversions.first()
        );
    }
}

#[test]
fn failed_last_conversion_write_keeps_the_child_and_parent_draft_open() {
    use hydrus_core::url::strings::{Conversion, StringConverter};
    use hydrus_gui::string_processor_window::{Slots, open_converter};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = Slots::default();
    slots.set_store(&store);
    store.write_and_refresh(|ctx| {
        ctx.conn().execute_batch("CREATE TRIGGER refuse_last_conversion BEFORE INSERT ON settings WHEN NEW.key = 'last_used_string_conversion_step' BEGIN SELECT RAISE(ABORT, 'preference write blocked'); END;")?;
        Ok(())
    }).unwrap();
    let parent =
        open_converter(&StringConverter::default(), None, &slots, Rc::new(|_| {})).unwrap();
    parent.invoke_add();
    let child = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    child.set_text("new suffix".into());
    child.invoke_changed();
    child.invoke_apply();
    assert!(child.get_error().contains("preference write blocked"));
    assert!(slots.conversion.borrow().is_some());
    assert_eq!(parent.get_rows().row_count(), 0);
    store
        .write_and_refresh(|ctx| {
            ctx.conn()
                .execute_batch("DROP TRIGGER refuse_last_conversion")?;
            Ok(())
        })
        .unwrap();
    child.invoke_apply();
    assert!(slots.conversion.borrow().is_none());
    assert_eq!(parent.get_rows().row_count(), 1);
    parent.invoke_cancel();
    assert_eq!(
        store.read(hydrus_store::string_conversion::load).unwrap().0,
        Some(Conversion::Append("new suffix".into()))
    );
}

// leaf: audit-network-matcher-favourites
#[test]
fn matcher_favourite_popup_copies_refreshes_and_owns_persisted_management() {
    use hydrus_gui::{Clip, StringStepWindow, string_processor_window};
    use hydrus_store::{Store, settings};
    fn rows(step: &StringStepWindow) -> serde_json::Value {
        let pane = step.get_favourite_panes().row_data(0).unwrap();
        serde_json::json!(
            (0..pane.lines.row_count())
                .map(|i| {
                    let line = pane.lines.row_data(i).unwrap();
                    if line.kind == 2 {
                        serde_json::json!({"separator": true})
                    } else {
                        serde_json::json!({"label": line.label.to_string(), "enabled": line.usable})
                    }
                })
                .collect::<Vec<_>>()
        )
    }
    fn choose(step: &StringStepWindow, label: &str) {
        let pane = step.get_favourite_panes().row_data(0).unwrap();
        let index = (0..pane.lines.row_count())
            .find(|i| pane.lines.row_data(*i).unwrap().label == label)
            .unwrap();
        step.invoke_favourite_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    }
    let (_dirs, store) = store();
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("matcher_favourites.json");
    let initial = RegexFavourites(serde_json::from_value(fixture["initial"].clone()).unwrap());
    store
        .write(move |ctx| settings::set(ctx.conn(), &initial))
        .unwrap();
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |value| {
            if let Clip::Text(text) = value {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let slots = string_processor_window::Slots::default();
    let matcher = StringMatch {
        kind: MatchKind::Regex(PyRegex::new("original+")),
        example: "original".into(),
        ..StringMatch::any()
    };
    string_processor_window::open_match(
        &store,
        &matcher,
        &slots,
        Rc::new(|_| panic!("parent cancelled")),
    );
    let step = slots.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_favourite_menu(10.0, 10.0);
    assert_eq!(rows(&step), fixture["menus"][0]);
    step.invoke_favourite_line_clicked(0, 2, 0.0, 0.0, 0.0);
    assert!(copied.borrow().is_empty());
    assert_eq!(
        step.get_favourite_panes().row_count(),
        0,
        "instruction dismisses without a clipboard action"
    );
    step.invoke_favourite_menu(10.0, 10.0);
    choose(&step, "letters");
    step.invoke_favourite_menu(10.0, 10.0);
    choose(&step, "fragment");
    assert_eq!(
        serde_json::json!(*copied.borrow()),
        serde_json::json!(["a+", "["])
    );
    assert_eq!(
        step.get_match_regex(),
        fixture["unchanged_input"].as_str().unwrap()
    );
    for case in fixture["steps"].as_array().unwrap() {
        step.invoke_favourite_menu(10.0, 10.0);
        choose(&step, "manage favourites");
        let child = slots.favourites.borrow().as_ref().unwrap().clone_strong();
        assert!(step.get_child_open());
        child.invoke_action("add".into());
        child.set_phrase("z+".into());
        child.set_description("new choice".into());
        child.invoke_action("save-row".into());
        child.invoke_action(
            if case["accepted"] == true {
                "apply"
            } else {
                "cancel"
            }
            .into(),
        );
        assert!(!step.get_child_open());
        assert_eq!(
            serde_json::json!(store.read(hydrus_store::regex_favourites::load).unwrap().0),
            case["saved"]
        );
        assert_eq!(step.get_match_regex(), case["input"].as_str().unwrap());
    }
    step.invoke_favourite_menu(10.0, 10.0);
    assert_eq!(rows(&step), fixture["menus"][3]);
    choose(&step, "new choice");
    assert_eq!(copied.borrow().last().unwrap(), "z+");
    // An already-open input sees global favourites updated by another owner.
    let refreshed = RegexFavourites(vec![("external+".into(), "external change".into())]);
    let saved = refreshed.clone();
    store
        .write(move |ctx| settings::set(ctx.conn(), &saved))
        .unwrap();
    step.invoke_favourite_menu(10.0, 10.0);
    choose(&step, "external change");
    assert_eq!(copied.borrow().last().unwrap(), "external+");
    assert_eq!(step.get_match_regex(), "original+");
    step.invoke_favourite_menu(10.0, 10.0);
    step.invoke_favourite_dismissed();
    assert_eq!(step.get_favourite_panes().row_count(), 0);
    step.invoke_favourite_menu(10.0, 10.0);
    choose(&step, "manage favourites");
    let stale_child = slots.favourites.borrow().as_ref().unwrap().clone_strong();
    step.invoke_cancel();
    assert!(!stale_child.window().is_visible());
    stale_child.invoke_action("apply".into());
    let count = copied.borrow().len();
    step.invoke_favourite_menu(10.0, 10.0);
    step.invoke_favourite_line_clicked(0, 4, 0.0, 0.0, 0.0);
    assert_eq!(copied.borrow().len(), count);
    assert_eq!(
        store.read(hydrus_store::regex_favourites::load).unwrap(),
        refreshed
    );
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened.read(hydrus_store::regex_favourites::load).unwrap(),
        refreshed
    );
}
