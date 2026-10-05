//! Actual tab selection/menu callbacks feed real page queries and owned write drafts.
use hydrus_core::{
    ServiceKey,
    search::context::{FileSearchContext, LocationContext, TagContext},
};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, WindowEvent},
};
use std::{cell::RefCell, rc::Rc};

fn choose_menu(window: &MainWindow, path: &[&str]) {
    for (pane, label) in path.iter().enumerate() {
        let lines = window.get_tag_menu_panes().row_data(pane).unwrap().lines;
        let line = lines.iter().position(|row| row.label == *label).unwrap();
        window.invoke_tag_menu_clicked(
            i32::try_from(pane).unwrap(),
            i32::try_from(line).unwrap(),
            100.0,
            50.0,
            10.0,
        );
    }
}
fn snapshot(window: &MainWindow, bound: &hydrus_gui::Bound, event: &Value) {
    let current = bound.current.borrow().clone();
    let current = current.borrow();
    let rows: Vec<_> = current
        .autocomplete()
        .suggestions()
        .iter()
        .map(|row| row.predicate.clone())
        .collect();
    let selected: Vec<_> = current
        .autocomplete()
        .selected_suggestions()
        .iter()
        .map(|row| row.predicate.clone())
        .collect();
    assert_eq!(json!(rows), event["rows"], "{event}");
    assert_eq!(json!(selected), event["selected"], "{event}");
    assert_eq!(json!(current.predicates()), event["predicates"], "{event}");
    assert_eq!(json!(window.get_autocomplete_tab()), event["tab"]);
    assert_eq!(window.get_search_text(), event["text"].as_str().unwrap());
    let mut favourites: settings::FavouriteTags = current.store().read(settings::get).unwrap();
    favourites.0.sort();
    assert_eq!(json!(favourites.0), event["favourites"]);
}

#[test]
fn read_batches_favourite_questions_write_drafts_and_owner_boundaries_replay_qt() {
    let fixture = hydrus_testkit::fixture_json("autocomplete_tab_selection.json");
    let (_dirs, store, key) = super::read_autocomplete::seeded(&fixture);
    store
        .write(|ctx| {
            let mut settings: settings::FileSearchSettings = settings::get(ctx.conn())?;
            settings.float_autocomplete = true;
            settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES,
        )),
        tags: TagContext::new(key.clone(), true, true),
        predicates: Vec::new(),
    };
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store.clone(),
            context,
            false,
            None,
            Vec::new(),
        )),
    );
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    for event in fixture["events"].as_array().unwrap() {
        let action = event["action"].as_str().unwrap();
        match action {
            "favourites" | "return_favourites" => ui.invoke_autocomplete_tab_chosen(1),
            "hit" => {
                let index: i32 = event["index"].as_i64().unwrap().try_into().unwrap();
                let ctrl = event["ctrl"].as_bool().unwrap();
                let shift = event["shift"].as_bool().unwrap();
                if event["tab"] == 1 && index == 2 && ctrl && !shift {
                    let native = windows.get(0).unwrap();
                    headless::render(&native, 1100, 750);
                    assert!(ui.get_autocomplete_overlay_visible());
                    native.dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Control.into(),
                    });
                    let position = slint::LogicalPosition::new(
                        ui.get_read_results_x() + 10.0,
                        ui.get_read_results_y() + 55.0,
                    );
                    native.dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: slint::platform::PointerEventButton::Left,
                    });
                    native.dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: slint::platform::PointerEventButton::Left,
                    });
                    native.dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Control.into(),
                    });
                } else {
                    ui.invoke_suggestion_selection_clicked(index, ctrl, shift);
                }
            }
            "activate_favourites" | "activate_children" => ui.invoke_suggestions_activated(false),
            "clear_search" => {
                let count = bound.current.borrow().borrow().predicates().len();
                for index in (0..count).rev() {
                    ui.invoke_remove_predicate(i32::try_from(index).unwrap());
                }
            }
            "deselect" => {
                assert!(ui.invoke_suggestions_deselected());
            }
            "select_all" => ui.invoke_suggestions_select_all(),
            "shift_activate_favourites" => {
                let before = bound.pages.borrow().predicate_history();
                ui.invoke_suggestions_activated(true);
                let current = bound.current.borrow().clone();
                let mut terms: Vec<_> = current
                    .borrow()
                    .or_terms()
                    .unwrap()
                    .iter()
                    .map(|predicate| {
                        let hydrus_core::search::predicate::Predicate::Tag { tag, .. } = predicate
                        else {
                            panic!("tag OR expected");
                        };
                        tag.as_str().to_owned()
                    })
                    .collect();
                terms.sort();
                assert_eq!(json!(terms), event["or_terms"]);
                assert_eq!(bound.pages.borrow().predicate_history(), before);
            }
            "cancel_or" => ui.invoke_search_or_action(2),
            "remove_favourite" => {
                let index = bound
                    .current
                    .borrow()
                    .borrow()
                    .autocomplete()
                    .suggestions()
                    .iter()
                    .position(|row| row.predicate == "parity:tabs root")
                    .unwrap();
                ui.invoke_suggestion_context_menu(i32::try_from(index).unwrap(), 20.0, 30.0);
                choose_menu(&ui, &["favourites", event["label"].as_str().unwrap()]);
                assert_eq!(
                    json!([ui.get_tag_menu_question().as_str()]),
                    event["questions"]
                );
                ui.invoke_tag_menu_answered(event["yes"].as_bool().unwrap());
            }
            "children" => {
                bound.current.borrow().borrow_mut().add_predicates(&[
                    hydrus_core::search::predicate::Predicate::Tag {
                        tag: hydrus_core::Tag::new("parity:tabs root").unwrap(),
                        inclusive: true,
                    },
                ]);
                ui.invoke_autocomplete_tab_chosen(2);
            }
            "add_child_favourite" => {
                ui.invoke_suggestion_context_menu(0, 20.0, 30.0);
                choose_menu(&ui, &["favourites", event["label"].as_str().unwrap()]);
                assert!(ui.get_tag_menu_question().is_empty());
            }
            "return_children" => ui.invoke_autocomplete_tab_chosen(2),
            "reopen" => {
                let reopened = Store::open(store.dir()).unwrap();
                let mut input = hydrus_gui_model::autocomplete::Autocomplete::new(reopened);
                input.set_tab(hydrus_gui_model::write_autocomplete::Tab::Favourites);
                assert_eq!(
                    json!(
                        input
                            .suggestions()
                            .iter()
                            .map(|row| row.predicate.clone())
                            .collect::<Vec<_>>()
                    ),
                    event["rows"]
                );
                continue;
            }
            "write_activate" => continue,
            other => panic!("unrecorded action {other}"),
        }
        snapshot(&ui, &bound, event);
        if action == "activate_children" {
            ui.invoke_refresh_page();
            assert_eq!(
                json!(bound.current.borrow().borrow().results().len()),
                event["query_count"]
            );
            ui.invoke_flip_synchronised();
        }
        if action == "select_all" || action == "children" {
            let pixels = headless::render(&windows.get(0).unwrap(), 1100, 750);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("autocomplete_tab_{action}.png")),
                &pixels,
                1100,
                750,
            )
            .unwrap();
        }
    }
    // Concurrent writes survive a delayed favourite answer; a hidden owner cannot write.
    let active_query = bound.current.borrow().borrow().predicates();
    ui.invoke_search_edited("caller draft".into());
    ui.invoke_autocomplete_tab_chosen(1);
    ui.invoke_suggestion_context_menu(0, 20.0, 30.0);
    choose_menu(
        &ui,
        &["favourites", "remove \"parity:tabs alpha\" from favourites"],
    );
    store
        .write(|ctx| {
            let mut favourites: settings::FavouriteTags = settings::get(ctx.conn())?;
            favourites.0.push("parity:concurrent favourite".into());
            settings::set(ctx.conn(), &favourites)
        })
        .unwrap();
    ui.invoke_tag_menu_answered(true);
    assert_eq!(ui.get_search_text(), "caller draft");
    assert_eq!(bound.current.borrow().borrow().predicates(), active_query);
    assert!(
        store
            .read(settings::get::<settings::FavouriteTags>)
            .unwrap()
            .0
            .contains(&"parity:concurrent favourite".into())
    );
    let before = store
        .read(settings::get::<settings::FavouriteTags>)
        .unwrap()
        .0;
    ui.invoke_suggestion_context_menu(0, 20.0, 30.0);
    choose_menu(
        &ui,
        &[
            "favourites",
            "remove \"parity:concurrent favourite\" from favourites",
        ],
    );
    let query = bound.current.borrow().borrow().predicates();
    ui.hide().unwrap();
    ui.invoke_tag_menu_answered(true);
    ui.invoke_suggestions_activated(false);
    assert_eq!(
        store
            .read(settings::get::<settings::FavouriteTags>)
            .unwrap()
            .0,
        before
    );
    assert_eq!(bound.current.borrow().borrow().predicates(), query);
    ui.show().unwrap();
    // Lock before repaint: rendered flags may lag, but the live model rejects the write.
    ui.invoke_suggestion_context_menu(0, 20.0, 30.0);
    choose_menu(
        &ui,
        &[
            "favourites",
            "remove \"parity:concurrent favourite\" from favourites",
        ],
    );
    bound.current.borrow().borrow_mut().lock_search();
    assert!(!ui.get_search_locked());
    ui.invoke_tag_menu_answered(true);
    assert_eq!(
        store
            .read(settings::get::<settings::FavouriteTags>)
            .unwrap()
            .0,
        before
    );
    bound.current.borrow().borrow_mut().unlock();
    ui.invoke_autocomplete_tab_chosen(1);
    // A retained question belongs to the opening page, even while the frame stays visible.
    ui.invoke_suggestion_context_menu(0, 20.0, 30.0);
    choose_menu(
        &ui,
        &[
            "favourites",
            "remove \"parity:concurrent favourite\" from favourites",
        ],
    );
    (bound.open_page)(&hydrus_gui_model::page_chooser::NewPage::Search {
        domain: ServiceKey::new(hydrus_core::service::builtin_keys::MY_FILES),
        name: "successor search".into(),
    });
    ui.invoke_tag_menu_answered(true);
    assert_eq!(
        store
            .read(settings::get::<settings::FavouriteTags>)
            .unwrap()
            .0,
        before
    );
    assert!(bound.current.borrow().borrow().predicates().is_empty());
    // The same write tabs stage additions in their captured child, then Cancel retires it.
    for event in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["action"] == "write_activate")
    {
        // Restore the exact recorded favourites before this independent write replay.
        let favourites: Vec<String> = serde_json::from_value(
            fixture["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|event| event["action"] == "return_favourites")
                .unwrap()["favourites"]
                .clone(),
        )
        .unwrap();
        store
            .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteTags(favourites)))
            .unwrap();
        let slot = hydrus_gui::write_tag_window::Slot::default();
        let applied = Rc::new(RefCell::new(Vec::new()));
        let child = hydrus_gui::write_tag_window::open(
            &store,
            key.clone(),
            &["parity:tabs root".into()],
            "synthetic tag tab draft",
            &slot,
            Rc::new({
                let applied = applied.clone();
                move |tags| *applied.borrow_mut() = tags
            }),
            Rc::new(|| {}),
        )
        .unwrap();
        child.invoke_tab_chosen(event["tab"].as_i64().unwrap().try_into().unwrap());
        for (selected, tag) in event["selected"].as_array().unwrap().iter().enumerate() {
            let physical = child
                .get_suggestions()
                .iter()
                .position(|row| row.text.starts_with(tag.as_str().unwrap()))
                .unwrap();
            child.invoke_selection_clicked(i32::try_from(physical).unwrap(), selected > 0, false);
        }
        child.invoke_entered();
        let mut tags: Vec<_> = child
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        tags.sort();
        let mut expected: Vec<String> = serde_json::from_value(event["entered"].clone()).unwrap();
        expected.push("parity:tabs root".into());
        expected.sort();
        assert_eq!(tags, expected);
        child.invoke_cancel();
        child.invoke_apply();
        assert!(applied.borrow().is_empty());
        assert!(slot.borrow().is_none());
    }
    // External accepted settings refresh both live panes, preserving each caller's draft.
    ui.invoke_search_edited("main retained input".into());
    ui.invoke_autocomplete_tab_chosen(1);
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "live tab refresh",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    child.invoke_edited("write retained input".into());
    child.invoke_tab_chosen(1);
    bound.current.borrow().borrow_mut().lock_search();
    let locked_query = bound.current.borrow().borrow().predicates();
    store
        .write(|ctx| {
            let mut favourites: settings::FavouriteTags = settings::get(ctx.conn())?;
            favourites.0.push("parity:live external".into());
            settings::set(ctx.conn(), &favourites)
        })
        .unwrap();
    // The detached owner proves the notification ran while the main page was locked.
    for _ in 0..100 {
        slint::platform::update_timers_and_animations();
        if child
            .get_suggestions()
            .iter()
            .any(|row| row.text == "parity:live external")
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        child
            .get_suggestions()
            .iter()
            .any(|row| row.text == "parity:live external")
    );
    assert!(
        !ui.get_suggestions()
            .iter()
            .any(|row| row.text == "parity:live external")
    );
    assert_eq!(bound.current.borrow().borrow().predicates(), locked_query);
    bound.current.borrow().borrow_mut().unlock();
    // No tab switch or further settings write: unlocking must consume the deferred revision.
    let contains_live = || {
        ui.get_suggestions()
            .iter()
            .any(|row| row.text == "parity:live external")
            && child
                .get_suggestions()
                .iter()
                .any(|row| row.text == "parity:live external")
    };
    for _ in 0..100 {
        slint::platform::update_timers_and_animations();
        if contains_live() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(contains_live());
    assert_eq!(ui.get_search_text(), "main retained input");
    assert_eq!(child.get_text(), "write retained input");
    child.invoke_cancel();
}

fn tag_shape(predicates: &[hydrus_core::search::predicate::Predicate]) -> Value {
    use hydrus_core::search::predicate::Predicate;
    let mut values: Vec<_> = predicates
        .iter()
        .map(|predicate| match predicate {
            Predicate::Tag {
                tag,
                inclusive: true,
            } => json!({"kind": "tag", "value": tag.as_str()}),
            Predicate::Or(children) => {
                let mut tags: Vec<_> = children
                    .iter()
                    .map(|child| {
                        let Predicate::Tag {
                            tag,
                            inclusive: true,
                        } = child
                        else {
                            panic!("literal pane must broadcast inclusive tags, got {child:?}");
                        };
                        tag.as_str().to_owned()
                    })
                    .collect();
                tags.sort();
                json!({"kind": "or", "value": tags})
            }
            other => panic!("literal pane must not parse tag text as search syntax: {other:?}"),
        })
        .collect();
    values.sort_by_key(|value| value["value"].to_string());
    json!(values)
}

#[test]
fn literal_batches_and_empty_activation_preserve_typed_tags_query_and_or_history() {
    let fixture = hydrus_testkit::fixture_json("autocomplete_tab_selection.json");
    let (_dirs, store, key) = super::read_autocomplete::seeded(&fixture);
    let tags: Vec<String> =
        serde_json::from_value(fixture["literal_cases"][0]["tags"].clone()).unwrap();
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteTags(tags)))
        .unwrap();
    headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store,
            FileSearchContext {
                location: LocationContext::single(ServiceKey::new(
                    hydrus_core::service::builtin_keys::MY_FILES,
                )),
                tags: TagContext::new(key, true, true),
                predicates: Vec::new(),
            },
            false,
            None,
            Vec::new(),
        )),
    );
    for case in fixture["literal_cases"].as_array().unwrap() {
        let count = bound.current.borrow().borrow().predicates().len();
        for index in (0..count).rev() {
            ui.invoke_remove_predicate(i32::try_from(index).unwrap());
        }
        ui.invoke_autocomplete_tab_chosen(1);
        ui.invoke_suggestions_deselected();
        let empty_history = bound.pages.borrow().predicate_history();
        for shift in [false, true] {
            ui.invoke_suggestions_activated(shift);
            assert!(bound.current.borrow().borrow().predicates().is_empty());
            assert!(bound.current.borrow().borrow().or_terms().is_none());
            assert_eq!(bound.pages.borrow().predicate_history(), empty_history);
        }
        ui.invoke_suggestions_select_all();
        ui.invoke_suggestions_activated(case["shift"].as_bool().unwrap());
        let page = bound.current.borrow().clone();
        assert_eq!(
            tag_shape(&page.borrow().favourite_to_save().unwrap().search.predicates),
            case["active"]
        );
        assert_eq!(
            tag_shape(page.borrow().or_terms().unwrap_or(&[])),
            case["draft"]
        );
        ui.invoke_search_edited("empty batch retained input".into());
        assert!(ui.invoke_suggestions_deselected());
        for attempt in case["empty_attempts"].as_array().unwrap() {
            let history = bound.pages.borrow().predicate_history();
            ui.invoke_suggestions_activated(attempt["shift"].as_bool().unwrap());
            assert_eq!(attempt["activated"], false);
            assert_eq!(
                tag_shape(&page.borrow().favourite_to_save().unwrap().search.predicates),
                attempt["active"]
            );
            assert_eq!(
                tag_shape(page.borrow().or_terms().unwrap_or(&[])),
                attempt["draft"]
            );
            assert_eq!(bound.pages.borrow().predicate_history(), history);
            assert_eq!(ui.get_search_text(), "empty batch retained input");
        }
        ui.invoke_suggestions_select_all();
        if case["shift"] == true {
            ui.invoke_suggestions_activated(false);
        }
        assert_eq!(
            tag_shape(&page.borrow().favourite_to_save().unwrap().search.predicates),
            case["committed"]
        );
        ui.invoke_refresh_page();
        assert_eq!(json!(page.borrow().results().len()), case["query_count"]);
        ui.invoke_flip_synchronised();
    }
}
