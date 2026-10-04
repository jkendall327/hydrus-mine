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

#[test]
fn matcher_favourites_manager_saves_global_choices_and_parent_cancel_closes_it() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = hydrus_gui::string_processor_window::Slots::default();
    let matcher = StringMatch {
        kind: MatchKind::Regex(PyRegex::new("a+")),
        example: "a".into(),
        ..StringMatch::default()
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
        ..StringMatch::default()
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

    use hydrus_core::url::strings::{Conversion, StringConverter};
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
