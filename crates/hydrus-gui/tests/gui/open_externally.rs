//! Owned Options routing, actual launch consumers and staged persistent defaults.
#[cfg(unix)]
use hydrus_core::external_calls::{Parameter, Process, Rule};
use hydrus_core::{
    Mime,
    external_calls::{ActualCall, Callable, Manager, Pipeline},
    open_externally::{CallRef, Routing},
};
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};
// Drive the reference's selected reserved-set Edit button, not the retired
// shortcuts-clicked callback. Also use this sender for blocked-child attempts.
fn request_global_shortcuts(parent: &OptionsWindow) {
    let label = hydrus_gui_model::shortcut_sets::pretty_name("global");
    let row = parent
        .get_shortcut_reserved_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == label)
        .unwrap();
    parent.invoke_shortcut_set_clicked(false, i32::try_from(row).unwrap(), false, false);
    parent.invoke_shortcut_set_action("edit-reserved".into());
}
fn seed(store: &Store) -> Manager {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let manager = Manager {
        calls: fixture["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut call = Callable::new(value["name"].as_str().unwrap());
                call.key =
                    [u8::from_str_radix(&value["key"].as_str().unwrap()[..2], 16).unwrap(); 32];
                call.pipeline = if value["pipeline"] == 1 {
                    Pipeline::File
                } else {
                    Pipeline::Url
                };
                call.call = if call.pipeline == Pipeline::File {
                    ActualCall::DefaultFile
                } else {
                    ActualCall::DefaultUrl
                };
                if call.key == [1; 32] {
                    call.name = "Zulu URL".into();
                }
                call
            })
            .collect(),
    };
    let routing = Routing {
        urls: vec![(&manager.calls[4]).into()],
        files: std::collections::BTreeMap::from([(
            Mime::GeneralFile,
            vec![(&manager.calls[5]).into()],
        )]),
    };
    let saved = manager.clone();
    store
        .write(move |ctx| {
            settings::set(ctx.conn(), &saved)?;
            settings::set(ctx.conn(), &routing)
        })
        .unwrap();
    manager
}
fn open(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&index| lines.row_data(index).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let index = (0..pages.row_count())
        .find(|&index| pages.row_data(index).unwrap().text == "open externally")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    assert_eq!(
        window
            .get_rows()
            .iter()
            .filter(|row| row.kind == 34)
            .count(),
        1,
        "routing renders its own table once"
    );
    assert!(
        window.get_rows().iter().all(|row| row.kind != 31),
        "routing does not render the unrelated namespace colour editor"
    );
    window
}
fn choice(bound: &Bound) -> hydrus_gui::ExternalRoutingChoiceWindow {
    bound
        .options_open_externally
        .choice
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn choose(bound: &Bound, name: &str) {
    let window = choice(bound);
    let names = window.get_choices();
    let index = (0..names.row_count())
        .find(|&index| names.row_data(index).unwrap() == name)
        .unwrap();
    window.invoke_chosen(i32::try_from(index).unwrap());
}
fn urls(window: &OptionsWindow) -> Vec<String> {
    let rows = window.get_routing_url_rows();
    (0..rows.row_count())
        .map(|index| {
            rows.row_data(index)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}
// The adapter is retained when its actual owner is constructed. Advance real
// timers and animations; never force child flags or retry an input to get a hit.
fn routing_pixels(
    adapter: &slint::platform::software_renderer::MinimalSoftwareWindow,
    window: &slint::Window,
    size: (u32, u32),
) -> Vec<u8> {
    use std::time::{Duration, Instant};
    let started = Instant::now();
    let mut previous = None;
    loop {
        let pixels = headless::render(adapter, size.0, size.1);
        if started.elapsed() >= Duration::from_millis(35)
            && !window.has_active_animations()
            && previous.as_ref() == Some(&pixels)
        {
            assert!(window.is_visible(), "capture the actual live routing owner");
            return pixels;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "routing render did not settle"
        );
        previous = Some(pixels);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn routing_capture(
    adapter: &slint::platform::software_renderer::MinimalSoftwareWindow,
    window: &slint::Window,
    name: &str,
    size: (u32, u32),
) {
    let pixels = routing_pixels(adapter, window, size);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        size.0,
        size.1,
    )
    .unwrap();
}
fn choice_labels(window: &hydrus_gui::ExternalRoutingChoiceWindow) -> Vec<String> {
    window
        .get_choices()
        .iter()
        .map(|label| label.to_string())
        .collect()
}
fn assert_recorded_choice(
    window: &hydrus_gui::ExternalRoutingChoiceWindow,
    fixture: &Value,
    index: usize,
) {
    let recorded = &fixture["chooser"][index];
    assert!(window.window().is_visible());
    assert_eq!(
        window.get_window_title(),
        recorded["title"].as_str().unwrap()
    );
    assert_eq!(
        choice_labels(window),
        recorded["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for item in recorded["choices"].as_array().unwrap() {
        assert_eq!(
            window.get_choice_description(),
            item["tooltip"].as_str().unwrap()
        );
    }
    assert_eq!(recorded["kwargs"]["allow_insta_one_item_select"], false);
}
fn routing_saved(store: &Store) -> (Routing, Manager) {
    store
        .read(|conn| {
            Ok((
                settings::get::<Routing>(conn)?,
                settings::get::<Manager>(conn)?,
            ))
        })
        .unwrap()
}
fn routing_draft(window: &OptionsWindow) -> Value {
    let rows = |rows: slint::ModelRc<hydrus_gui::TableRow>| {
        rows.iter()
            .map(|row| {
                json!({
                    "cells": row.cells.iter().map(|cell| cell.to_string()).collect::<Vec<_>>(),
                    "selected": row.selected,
                })
            })
            .collect::<Vec<_>>()
    };
    json!({
        "urls": rows(window.get_routing_url_rows()),
        "files": rows(window.get_routing_file_rows()),
        "url_selected": window.get_routing_url_selected(),
        "file_single": window.get_routing_file_single(),
        "file_delete": window.get_routing_file_delete(),
        "child_open": window.get_routing_child_open(),
        "prepared": window.get_routing_prepared(),
        "error": window.get_routing_error().to_string(),
        "page": window.get_page(),
    })
}
// These coordinates are supported only at 960x800, scale 1, on the unscrolled
// open-externally page. The new exact chooser must prove the physical hit.
fn physical_url_choice(
    window: &OptionsWindow,
    adapter: &slint::platform::software_renderer::MinimalSoftwareWindow,
    bound: &Bound,
    edit: bool,
) -> hydrus_gui::ExternalRoutingChoiceWindow {
    use slint::platform::{PointerEventButton, WindowEvent};
    assert!(!bound.options_open_externally.has_open());
    routing_pixels(adapter, window.window(), (960, 800));
    assert!(!window.get_routing_child_open());
    assert_eq!(window.window().scale_factor().to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        window.get_options_scroll_y().abs().to_bits(),
        0.0_f32.to_bits()
    );
    assert!(window.get_search_text().is_empty());
    assert_eq!(window.get_matches().row_count(), 0);
    let position = slint::LogicalPosition::new(if edit { 767.0 } else { 400.0 }, 339.0);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    assert!(bound.options_open_externally.question.borrow().is_none());
    assert!(bound.options_open_externally.files.borrow().is_none());
    let child = choice(bound);
    assert!(
        child.window().is_visible(),
        "physical URL button must admit its chooser"
    );
    child
}
fn pending_url_chooser_retirement(ui: &MainWindow, bound: &Bound, store: &Store) {
    let persisted = routing_saved(store);
    for action in ["add", "edit"] {
        let old = open(ui, bound);
        if action == "edit" {
            old.invoke_routing_url_action("add".into());
            choose(bound, "alpha URL");
        }
        let selected = i32::from(action == "edit");
        old.invoke_routing_url_clicked(selected, false, false);
        old.invoke_routing_url_action(action.into());
        let retired = choice(bound);
        assert!(
            retired.window().is_visible(),
            "retire a genuinely pending chooser"
        );
        old.invoke_cancel();
        assert!(!retired.window().is_visible());
        assert!(bound.options_open_externally.choice.borrow().is_none());
        let current = open(ui, bound);
        if action == "edit" {
            current.invoke_routing_url_action("add".into());
            choose(bound, "alpha URL");
        }
        current.invoke_routing_url_clicked(selected, false, false);
        current.invoke_routing_url_action(action.into());
        let child = choice(bound);
        let draft = routing_draft(&current);
        let labels = choice_labels(&child);
        assert_eq!(
            labels,
            if action == "edit" {
                vec!["Default OS URL Launch"]
            } else {
                vec!["Default OS URL Launch", "alpha URL"]
            }
        );
        let before_urls = urls(&current);
        let before_files = file_rows(&current);
        // Both forms of temporary invisibility reject chosen without retiring
        // the current chooser. Re-show permits its later positive answer.
        child.hide().unwrap();
        child.invoke_chosen(0);
        assert_eq!(routing_draft(&current), draft);
        assert!(std::ptr::eq(choice(bound).window(), child.window()));
        child.show().unwrap();
        current.hide().unwrap();
        child.invoke_chosen(0);
        assert_eq!(routing_draft(&current), draft);
        assert!(std::ptr::eq(choice(bound).window(), child.window()));
        current.show().unwrap();
        for attempt in 0..4 {
            match attempt {
                0 => retired.invoke_chosen(0),
                1 => retired.invoke_cancel(),
                2 => old.invoke_routing_url_action(action.into()),
                _ => {
                    old.invoke_apply();
                    old.invoke_cancel();
                }
            }
            assert_eq!(routing_draft(&current), draft);
            assert_eq!(choice_labels(&child), labels);
            assert_eq!(routing_saved(store), persisted);
            assert!(current.window().is_visible() && child.window().is_visible());
            assert!(std::ptr::eq(choice(bound).window(), child.window()));
            assert!(std::ptr::eq(
                bound.options.borrow().as_ref().unwrap().window(),
                current.window()
            ));
            assert!(bound.options_open_externally.question.borrow().is_none());
            assert!(bound.options_open_externally.files.borrow().is_none());
        }
        let mut expected_urls = before_urls;
        if action == "add" {
            expected_urls.push("Default OS URL Launch".into());
        } else {
            expected_urls[1] = "Default OS URL Launch".into();
        }
        child.invoke_chosen(0);
        assert!(!child.window().is_visible());
        assert!(bound.options_open_externally.choice.borrow().is_none());
        assert_eq!(
            urls(&current),
            expected_urls,
            "positive answer changes only this queue position"
        );
        assert_eq!(file_rows(&current), before_files);
        assert_eq!(
            routing_saved(store),
            persisted,
            "the accepted choice is still staged"
        );
        current.invoke_cancel();
        assert_eq!(
            routing_saved(store),
            persisted,
            "parent Cancel discards the current draft"
        );
    }
}
fn expected(fixture: &Value, event: usize) -> Vec<String> {
    fixture["events"][event]["urls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["name"].as_str().unwrap().to_owned())
        .collect()
}
fn file_rows(window: &OptionsWindow) -> Value {
    let rows = window.get_routing_file_rows();
    json!(
        (0..rows.row_count())
            .map(|index| {
                let cells = rows.row_data(index).unwrap().cells;
                vec![
                    cells.row_data(0).unwrap().to_string(),
                    cells.row_data(1).unwrap().to_string(),
                ]
            })
            .collect::<Vec<_>>()
    )
}
fn expected_files(fixture: &Value, event: usize) -> Value {
    json!(
        fixture["events"][event]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["display"].clone())
            .collect::<Vec<_>>()
    )
}
fn files(bound: &Bound) -> hydrus_gui::OpenFileCallsWindow {
    bound
        .options_open_externally
        .files
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn add_nested(bound: &Bound, name: &str) {
    files(bound).invoke_action("add".into());
    choose(bound, name);
}
#[test]
fn shortcut_and_routing_children_exclude_each_other_and_cancel_staged_changes() {
    let _headless_windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    seed(&store);
    let saved = || {
        store
            .read(|conn| {
                Ok((
                    settings::get::<Routing>(conn)?,
                    settings::get::<Manager>(conn)?,
                    settings::get::<hydrus_core::shortcuts::Settings>(conn)?,
                ))
            })
            .unwrap()
    };
    let before = saved();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    let routing_page = window.get_page();
    let shortcuts_page = i32::try_from(
        window
            .get_pages()
            .iter()
            .position(|page| page.text == "shortcuts")
            .unwrap(),
    )
    .unwrap();
    window.invoke_page_chosen(shortcuts_page);
    request_global_shortcuts(&window);
    let shortcut_child = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert!(shortcut_child.window().is_visible());
    assert_eq!(shortcut_child.get_set_name().as_str(), "global");
    // StandardListView's two-way current-item binding writes page first.
    window.set_page(routing_page);
    window.invoke_page_chosen(routing_page);
    assert_eq!(
        window.get_page(),
        shortcuts_page,
        "shortcut child owns parent page navigation"
    );
    window.invoke_search_edited("open externally".into());
    let match_index = i32::try_from(
        window
            .get_matches()
            .iter()
            .position(|label| label.contains("open externally"))
            .unwrap(),
    )
    .unwrap();
    window.invoke_search_chosen(match_index);
    assert_eq!(
        window.get_page(),
        shortcuts_page,
        "search cannot bypass the child guard"
    );
    window.invoke_routing_url_action("add".into());
    assert!(!bound.options_open_externally.has_open());
    window.invoke_apply();
    assert!(bound.options.borrow().is_some());
    assert_eq!(saved(), before);
    shortcut_child.invoke_cancel();
    window.invoke_page_chosen(routing_page);
    assert_eq!(window.get_page(), routing_page);
    window.invoke_routing_url_action("add".into());
    let routing_child = choice(&bound);
    request_global_shortcuts(&window);
    assert!(
        !shortcut_child.window().is_visible(),
        "routing child cannot reopen a shortcut child"
    );
    assert!(
        hydrus_gui::shortcut_windows::last_set().is_none_or(|child| !child.window().is_visible())
    );
    window.invoke_shortcuts_policy(!before.2.merge_numpad, !before.2.primary_labels);
    window.set_page(shortcuts_page);
    window.invoke_page_chosen(shortcuts_page);
    assert_eq!(window.get_page(), routing_page);
    window.invoke_apply();
    assert!(bound.options.borrow().is_some());
    routing_child.invoke_cancel();
    window.invoke_apply();
    assert!(bound.options.borrow().is_none());
    assert_eq!(
        saved(),
        before,
        "blocked policy edits and child-free Apply retain saved settings"
    );
    let successor = open(&ui, &bound);
    successor.invoke_page_chosen(shortcuts_page);
    request_global_shortcuts(&successor);
    let child = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert_eq!(child.get_set_name().as_str(), "global");
    successor.invoke_cancel();
    assert!(!child.window().is_visible());
    child.invoke_apply();
    routing_child.invoke_chosen(0);
    assert_eq!(saved(), before, "parent Cancel retires both child families");
}
// leaf: audit-options-nested-open-file-call-list-add-edit
// leaf: audit-options-nested-open-file-call-list-choose
// leaf: audit-options-open-externally-single-file-calls-add
// leaf: audit-options-open-externally-single-file-calls-delete
// leaf: audit-options-open-externally-single-file-calls-edit
// leaf: audit-options-open-externally-url-calls-add
// leaf: audit-options-open-externally-url-calls-edit
#[test]
fn options_routes_replay_qt_owned_choosers_cancel_order_apply_reopen_and_retirement() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let manager = seed(&store);
    let before = store.read(settings::get::<Routing>).unwrap();
    let full_before = routing_saved(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    let options_native = windows.get(windows.count() - 1).unwrap();
    assert_eq!(urls(&window), expected(&fixture, 0));
    assert_eq!(file_rows(&window), expected_files(&fixture, 0));
    let physical = physical_url_choice(&window, &options_native, &bound, false);
    assert_recorded_choice(&physical, &fixture, 0);
    physical.invoke_cancel();
    assert_eq!(routing_saved(&store), full_before);
    window.invoke_routing_url_action("add".into());
    let cancelled = choice(&bound);
    assert_eq!(
        cancelled.get_window_title(),
        fixture["chooser"][0]["title"].as_str().unwrap()
    );
    assert_eq!(cancelled.get_choice_description(), "Select this call.");
    assert_recorded_choice(&cancelled, &fixture, 0);
    let cancelled_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &cancelled_native,
        cancelled.window(),
        "url-add-chooser.png",
        (520, 440),
    );
    cancelled.invoke_cancel();
    cancelled.invoke_chosen(0);
    assert_eq!(urls(&window), expected(&fixture, 1));
    for (name, event) in [("Zulu URL", 2), ("alpha URL", 3)] {
        window.invoke_routing_url_action("add".into());
        let active = choice(&bound);
        assert_recorded_choice(&active, &fixture, event - 1);
        if name == "alpha URL" {
            let native = windows.get(windows.count() - 1).unwrap();
            routing_capture(
                &native,
                active.window(),
                "url-add-single-choice.png",
                (520, 440),
            );
            assert_eq!(
                choice(&bound).get_choices().row_count(),
                1,
                "Qt shows even a single remaining choice"
            );
        }
        choose(&bound, name);
        assert_eq!(urls(&window), expected(&fixture, event));
    }
    assert!(!window.get_routing_url_selected());
    assert!(
        window
            .get_routing_url_rows()
            .iter()
            .all(|row| !row.selected)
    );
    assert_eq!(routing_saved(&store), full_before);
    routing_capture(
        &options_native,
        window.window(),
        "url-add-draft.png",
        (960, 800),
    );
    window.invoke_routing_url_clicked(0, false, false);
    window.invoke_routing_url_action("down".into());
    assert_eq!(urls(&window), expected(&fixture, 4));
    window.invoke_routing_url_action("edit".into());
    let notice = bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(notice.get_window_title(), "Information");
    assert_eq!(
        notice.get_message(),
        fixture["notices"][0].as_str().unwrap()
    );
    assert_eq!(notice.get_notice_ok_label(), "OK");
    let notice_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &notice_native,
        notice.window(),
        "url-exhausted-information.png",
        (520, 200),
    );
    window.invoke_apply();
    assert!(bound.options.borrow().is_some());
    window.hide().unwrap();
    notice.invoke_cancelled();
    assert!(bound.options_open_externally.question.borrow().is_some());
    window.show().unwrap();
    notice.invoke_cancelled();
    window.invoke_routing_url_action("delete".into());
    let question = bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        question.get_message(),
        fixture["notices"][1].as_str().unwrap()
    );
    question.invoke_answered(true);
    assert_eq!(urls(&window), expected(&fixture, 6));
    window.invoke_routing_url_clicked(0, false, false);
    let physical = physical_url_choice(&window, &options_native, &bound, true);
    assert_recorded_choice(&physical, &fixture, 3);
    assert_eq!(urls(&window), expected(&fixture, 6));
    // The duplicate callback is blocked by this live child. Accept the
    // physically admitted child below: replacement, rather than append,
    // distinguishes Edit from the adjacent Add button with identical choices.
    window.invoke_routing_url_action("edit".into());
    let edited = choice(&bound);
    assert!(std::ptr::eq(edited.window(), physical.window()));
    assert_recorded_choice(&edited, &fixture, 3);
    let edited_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &edited_native,
        edited.window(),
        "url-edit-single-choice.png",
        (520, 440),
    );
    choose(&bound, "Default OS URL Launch");
    assert_eq!(urls(&window), expected(&fixture, 7));
    // MIME chooser preserves the recorded groups+searchable order (not label sorting).
    for accepted in [false, true] {
        window.invoke_routing_file_action("add".into());
        let mime_choice = choice(&bound);
        assert_eq!(mime_choice.get_window_title(), "which filetype?");
        let labels = (0..mime_choice.get_choices().row_count())
            .map(|index| {
                mime_choice
                    .get_choices()
                    .row_data(index)
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            fixture["mime_choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value["label"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        choose(&bound, "image/png");
        assert_eq!(files(&bound).get_window_title(), "edit calls");
        add_nested(&bound, "File one 日本");
        if accepted {
            add_nested(&bound, "File two");
            // Edit excludes both current rows, even the row being replaced.
            files(&bound).invoke_clicked(0, false, false);
            files(&bound).invoke_action("edit".into());
            assert_eq!(choice(&bound).get_choices().row_count(), 1);
            choose(&bound, "Default OS File Launch");
            files(&bound).invoke_action("edit".into());
            assert_eq!(choice(&bound).get_choices().row_count(), 1);
            choose(&bound, "File one 日本");
            files(&bound).invoke_clicked(0, false, false);
            files(&bound).invoke_action("down".into());
            files(&bound).invoke_apply();
            assert_eq!(file_rows(&window), expected_files(&fixture, 9));
        } else {
            files(&bound).invoke_cancel();
            assert_eq!(file_rows(&window), expected_files(&fixture, 8));
        }
    }
    for accepted in [false, true] {
        window.invoke_routing_file_action("edit".into());
        assert_eq!(files(&bound).get_window_title(), "edit launch path");
        add_nested(&bound, "Default OS File Launch");
        files(&bound).invoke_clicked(0, false, false);
        files(&bound).invoke_action("down".into());
        window.invoke_apply();
        assert!(bound.options.borrow().is_some());
        if accepted {
            files(&bound).invoke_apply();
            assert_eq!(file_rows(&window), expected_files(&fixture, 11));
        } else {
            files(&bound).invoke_cancel();
            assert_eq!(file_rows(&window), expected_files(&fixture, 10));
        }
    }
    window.invoke_routing_file_action("edit".into());
    files(&bound).invoke_clicked(0, false, false);
    files(&bound).invoke_action("edit".into());
    let exhausted = bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        exhausted.get_message(),
        hydrus_gui_model::open_externally::exhausted(Pipeline::File)
    );
    exhausted.invoke_cancelled();
    files(&bound).invoke_action("delete".into());
    let removal = bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(removal.get_message(), "Remove 1 selected?");
    removal.invoke_answered(false);
    assert_eq!(files(&bound).get_rows().row_count(), 3);
    files(&bound).invoke_action("delete".into());
    let removal = bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    removal.invoke_answered(true);
    assert_eq!(files(&bound).get_rows().row_count(), 2);
    let page = window.get_page();
    window.invoke_page_chosen(0);
    assert_eq!(window.get_page(), page);
    files(&bound).invoke_cancel();
    assert_eq!(file_rows(&window), expected_files(&fixture, 11));
    window.invoke_routing_file_clicked(0, true, false);
    assert!(!window.get_routing_file_delete());
    window.invoke_routing_file_action("delete".into());
    assert!(bound.options_open_externally.question.borrow().is_none());
    assert_eq!(window.get_routing_file_rows().row_count(), 2);
    assert_eq!(
        store.read(settings::get::<Routing>).unwrap(),
        before,
        "accepted children stay staged"
    );
    let pixels = headless::render(&options_native, 960, 800);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("open-externally-routing.png"),
        &pixels,
        960,
        800,
    )
    .unwrap();
    window.invoke_cancel();
    assert_eq!(store.read(settings::get::<Routing>).unwrap(), before);
    assert_eq!(routing_saved(&store), full_before);
    let window = open(&ui, &bound);
    window.invoke_routing_url_clicked(0, false, false);
    window.invoke_routing_url_action("edit".into());
    choose(&bound, "Zulu URL");
    window.invoke_routing_file_action("add".into());
    choose(&bound, "image/png");
    add_nested(&bound, "File one 日本");
    files(&bound).invoke_apply();
    let staged_urls = urls(&window);
    let staged_files = file_rows(&window);
    assert_eq!(routing_saved(&store), full_before);
    window.invoke_apply();
    let saved = store.read(settings::get::<Routing>).unwrap();
    assert_eq!(saved.urls[0].key, manager.calls[0].key);
    assert_eq!(saved.files[&Mime::ImagePng][0].key, manager.calls[2].key);
    let mut expected_saved = before.clone();
    expected_saved.urls = vec![(&manager.calls[0]).into()];
    expected_saved
        .files
        .insert(Mime::ImagePng, vec![(&manager.calls[2]).into()]);
    assert_eq!(routing_saved(&store), (expected_saved, manager.clone()));
    let window = open(&ui, &bound);
    let reopened_native = windows.get(windows.count() - 1).unwrap();
    assert_eq!(urls(&window), staged_urls);
    assert_eq!(file_rows(&window), staged_files);
    assert!(
        window
            .get_routing_url_rows()
            .iter()
            .all(|row| !row.selected)
    );
    routing_capture(
        &reopened_native,
        window.window(),
        "url-reopened-saved.png",
        (960, 800),
    );
    assert_eq!(urls(&window)[0], "Zulu URL");
    window.invoke_routing_url_action("add".into());
    let retired = choice(&bound);
    window.invoke_cancel();
    assert!(!retired.window().is_visible());
    let successor = open(&ui, &bound);
    successor.invoke_routing_url_action("add".into());
    retired.invoke_chosen(0);
    retired.invoke_cancel();
    assert!(bound.options_open_externally.choice.borrow().is_some());
    assert_eq!(urls(&successor)[0], "Zulu URL");
    successor.invoke_cancel();
    pending_url_chooser_retirement(&ui, &bound, &store);
}

#[test]
fn saved_routes_reach_main_and_live_viewer_os_fallback_and_missing_owned_notice() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let _headless_windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let manager = seed(&store);
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let ids = page.borrow().results().to_vec();
    let media = store
        .read(|conn| hydrus_store::media::load_basic(conn, &ids))
        .unwrap();
    let file = media
        .iter()
        .find(|media| {
            media
                .info
                .as_ref()
                .is_some_and(|info| info.mime == Mime::ImageJpeg)
        })
        .unwrap()
        .hash_id;
    let index = ids.iter().position(|id| *id == file).unwrap();
    page.borrow_mut().hit(Some(index), false, false);
    let path = hydrus_gui::thumbnail_menu::paths(&store, &[file])
        .pop()
        .unwrap();
    ui.invoke_open_externally();
    assert_eq!(launched.borrow().as_slice(), std::slice::from_ref(&path));
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_open_externally();
    assert_eq!(launched.borrow().len(), 2);
    // Live Options changes are read at the actual default-launch boundary.
    let missing = CallRef {
        key: [9; 32],
        name: "missing".into(),
    };
    let bad = Routing {
        urls: vec![missing.clone(), (&manager.calls[0]).into()],
        files: std::collections::BTreeMap::from([(Mime::GeneralFile, vec![missing])]),
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &bad))
        .unwrap();
    ui.invoke_open_externally();
    assert_eq!(launched.borrow().len(), 2);
    let notice = bound.external_launches.notice().unwrap();
    assert_eq!(notice.get_window_title(), "Information");
    assert!(
        notice
            .get_message()
            .starts_with("Sorry, could not open that file: When trying to open file")
    );
    assert!(notice.get_message().contains("did not exist!"));
    assert_eq!(notice.get_notice_ok_label(), "OK");
    notice.invoke_cancelled();
    let url = fixture["dispatch"][0]["inputs"]["2"].as_str().unwrap();
    assert!(!bound.external_launches.url(&store, url));
    let error = bound.external_launches.notice().unwrap();
    assert_eq!(error.get_message(), fixture["notices"][3].as_str().unwrap());
    error.invoke_cancelled();
    ui.hide().unwrap();
    assert!(!bound.external_launches.url(&store, url));
    assert!(bound.external_launches.notice().is_none());
    ui.show().unwrap();
    let empty = Routing::default();
    store
        .write(move |ctx| settings::set(ctx.conn(), &empty))
        .unwrap();
    assert!(bound.external_launches.url(&store, url));
    assert_eq!(launched.borrow().last().unwrap(), url);
    viewer.invoke_open_externally();
    assert_eq!(launched.borrow().last().unwrap(), &path);
    viewer.invoke_close_requested();
    viewer.invoke_open_externally();
    assert_eq!(launched.borrow().len(), 4);
    // A retained physical path is insufficient once current local membership leaves.
    store
        .write_content(move |w| w.delete_files(w.roles().local_file_storage, &[file], None))
        .unwrap();
    assert_eq!(hydrus_gui::thumbnail_menu::paths(&store, &[file]), [path]);
    assert!(!bound.external_launches.file(&store, file));
    assert_eq!(
        bound.external_launches.notice().unwrap().get_message(),
        "Sorry, could not open that file: This file is not local--it cannot be opened!"
    );
    // Rebinding the same visible window permanently retires its prior owner and notice.
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert!(bound.external_launches.notice().is_none());
    assert!(!bound.external_launches.url(&store, url));
    assert!(successor.external_launches.url(&store, url));
    // Client-close rejection preserves the new owner; acceptance retires it permanently.
    let mut gui_settings = store.read(settings::get::<settings::GuiSettings>).unwrap();
    gui_settings.confirm_exit = true;
    store
        .write(move |ctx| {
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui_settings)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(successor.external_launches.url(&store, url));
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    ui.show().unwrap();
    assert!(!successor.external_launches.url(&store, url));
    assert!(!bound.external_launches.url(&store, url));
    assert_eq!(launched.borrow().len(), 6);
    hydrus_gui::set_launcher(|_| {});
}

#[test]
fn live_local_membership_restore_reaches_the_same_launcher_after_store_reopen() {
    let _windows = headless::init();
    let (directories, store) = super::subscriptions::store();
    store
        .write(|ctx| settings::set(ctx.conn(), &Routing::default()))
        .unwrap();
    let file = store
        .read(|conn| {
            let services = hydrus_store::services::ServiceRegistry::load(conn)?;
            let local = hydrus_store::content::DomainRoles::new(&services)?.local_file_storage;
            Ok(conn.query_row(
                "SELECT f.hash_id FROM files f JOIN file_domain_current d USING(hash_id)
                 WHERE d.service_id=?1 ORDER BY f.hash_id LIMIT 1",
                [local],
                |row| row.get::<_, hydrus_core::HashId>(0),
            )?)
        })
        .unwrap();
    let path = hydrus_gui::thumbnail_menu::paths(&store, &[file])
        .pop()
        .unwrap();
    let observed = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let observed = observed.clone();
        move |target| observed.borrow_mut().push(target.to_owned())
    });
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let launcher = hydrus_gui::open_externally_launch::Launcher::new(Rc::new({
        let weak = ui.as_weak();
        move || {
            weak.upgrade()
                .is_some_and(|owner| owner.window().is_visible())
        }
    }));
    assert!(launcher.file(&store, file));
    assert_eq!(observed.borrow().as_slice(), std::slice::from_ref(&path));
    store
        .write_content(move |writer| {
            writer.delete_files(writer.roles().local_file_storage, &[file], None)
        })
        .unwrap();
    assert_eq!(
        hydrus_gui::thumbnail_menu::paths(&store, &[file]),
        std::slice::from_ref(&path)
    );
    assert!(!launcher.file(&store, file));
    assert_eq!(observed.borrow().as_slice(), std::slice::from_ref(&path));
    let notice = launcher.notice().unwrap();
    assert_eq!(
        notice.get_message(),
        "Sorry, could not open that file: This file is not local--it cannot be opened!"
    );
    notice.invoke_cancelled();
    store
        .write_content(move |writer| writer.add_files(writer.roles().local[0], &[(file, None)]))
        .unwrap();
    let reopened = Store::open(directories[1].path()).unwrap();
    assert!(
        launcher.file(&reopened, file),
        "restored persisted membership is read by the existing owner"
    );
    assert_eq!(observed.borrow().as_slice(), [path.clone(), path]);
    launcher.cancel();
    ui.hide().unwrap();
    ui.show().unwrap();
    assert!(!launcher.file(&reopened, file));
    assert_eq!(
        observed.borrow().len(),
        2,
        "a retired owner cannot launch the restored file"
    );
    hydrus_gui::set_launcher(|_| {});
}

#[cfg(unix)]
fn capture_process(output: &std::path::Path, parameters: &[Parameter], template: &str) -> Process {
    Process {
        executable: "/bin/sh".into(),
        arguments: vec![
            "-c".into(),
            "printf '%s' \"$1\" > \"$2\"".into(),
            "owned-routing-fixture".into(),
            template.into(),
            output.display().to_string(),
        ],
        rules: parameters.iter().copied().map(Rule::new).collect(),
        ..Process::default()
    }
}
#[cfg(unix)]
fn await_output(path: &std::path::Path, expected: &str) {
    let started = std::time::Instant::now();
    loop {
        slint::platform::update_timers_and_animations();
        if std::fs::read_to_string(path).is_ok_and(|value| value == expected) {
            return;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
#[cfg(unix)]
fn saved_registered_vectors_feed_actual_main_live_viewer_and_url_pipeline() {
    let _headless_windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let mut manager = seed(&store);
    let output = tempfile::tempdir().unwrap();
    let captured = output.path().join("owned Unicode routing.txt");
    manager.calls[2].call = ActualCall::Process(capture_process(
        &captured,
        &[
            Parameter::Path,
            Parameter::Uri,
            Parameter::Hash,
            Parameter::FileId,
        ],
        "%path%|%path_uri%|%hash%|%file_id%",
    ));
    manager.calls[0].call =
        ActualCall::Process(capture_process(&captured, &[Parameter::Url], "URL:%url%"));
    let routing = Routing {
        urls: vec![(&manager.calls[0]).into(), (&manager.calls[1]).into()],
        files: std::collections::BTreeMap::from([
            (Mime::GeneralFile, vec![(&manager.calls[3]).into()]),
            (
                Mime::GeneralImage,
                vec![(&manager.calls[2]).into(), (&manager.calls[3]).into()],
            ),
        ]),
    };
    let calls = manager.clone();
    store
        .write(move |ctx| {
            settings::set(ctx.conn(), &calls)?;
            settings::set(ctx.conn(), &routing)
        })
        .unwrap();
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let ids = page.borrow().results().to_vec();
    let media = store
        .read(|conn| hydrus_store::media::load_basic(conn, &ids))
        .unwrap();
    let jpeg = media
        .iter()
        .find(|media| {
            media
                .info
                .as_ref()
                .is_some_and(|info| info.mime == Mime::ImageJpeg)
        })
        .unwrap();
    let index = ids.iter().position(|id| *id == jpeg.hash_id).unwrap();
    page.borrow_mut().hit(Some(index), false, false);
    let path = hydrus_gui::thumbnail_menu::paths(&store, &[jpeg.hash_id])
        .pop()
        .unwrap();
    assert!(path.is_ascii() && !path.contains(' '));
    let file_uri = format!("file://{path}");
    let expected = format!(
        "{path}|{file_uri}|{}|{}",
        jpeg.hash.to_hex(),
        jpeg.hash_id.0
    );
    // Change the default through the UI, rather than merely consuming a
    // directly seeded first call. File two is an OS launch; File one is the
    // owned process below, so the observed routes cannot be confused.
    let original_order = routing_saved(&store);
    let mut reordered = original_order.clone();
    reordered
        .0
        .files
        .get_mut(&Mime::GeneralImage)
        .unwrap()
        .swap(0, 1);
    move_saved_image_queue(&ui, &bound, &store, true, 0, &reordered);
    assert!(!captured.exists());
    ui.invoke_open_externally();
    assert_eq!(launched.borrow().as_slice(), std::slice::from_ref(&path));
    assert!(
        !captured.exists(),
        "the changed first call takes only the OS route"
    );
    // Clear only after proving the complete first phase's observation. Restore
    // the process-backed first call through Up/Apply for all original checks.
    launched.borrow_mut().clear();
    move_saved_image_queue(&ui, &bound, &store, false, 1, &original_order);
    assert_eq!(routing_saved(&store), original_order);
    ui.invoke_open_externally();
    await_output(&captured, &expected);
    assert!(launched.borrow().is_empty());
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    std::fs::remove_file(&captured).unwrap();
    viewer.invoke_open_externally();
    await_output(&captured, &expected);
    assert!(launched.borrow().is_empty());
    let authored = "https://example.net/authored/漢?mode=fixture#anchor";
    assert!(bound.external_launches.url(&store, authored));
    await_output(&captured, &format!("URL:{authored}"));
    assert!(launched.borrow().is_empty());
    // Explicit empty JPEG row overrides image's registered process with OS default.
    let empty = Routing {
        urls: Vec::new(),
        files: std::collections::BTreeMap::from([
            (Mime::GeneralImage, vec![(&manager.calls[2]).into()]),
            (Mime::ImageJpeg, Vec::new()),
        ]),
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &empty))
        .unwrap();
    viewer.invoke_open_externally();
    assert_eq!(launched.borrow().as_slice(), [path]);
    viewer.invoke_close_requested();
    hydrus_gui::set_launcher(|_| {});
}

fn nested_rows(window: &hydrus_gui::OpenFileCallsWindow) -> Value {
    json!(
        window
            .get_rows()
            .iter()
            .map(|row| {
                json!({
                    "cells": row.cells.iter().map(|cell| cell.to_string()).collect::<Vec<_>>(),
                    "selected": row.selected,
                })
            })
            .collect::<Vec<_>>()
    )
}
fn nested_draft(window: &hydrus_gui::OpenFileCallsWindow) -> Value {
    json!({
        "rows": nested_rows(window),
        "selected": window.get_selected(),
        "child_open": window.get_child_open(),
        "title": window.get_window_title().to_string(),
    })
}
fn nested_names(window: &hydrus_gui::OpenFileCallsWindow) -> Vec<String> {
    window
        .get_rows()
        .iter()
        .map(|row| {
            assert_eq!(row.cells.row_count(), 1);
            row.cells.row_data(0).unwrap().to_string()
        })
        .collect()
}
fn recorded_nested_refs(fixture: &Value, index: usize) -> Vec<CallRef> {
    fixture["nested_checks"][index]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let hex = value["key"].as_str().unwrap();
            assert_eq!(hex.len(), 64);
            CallRef {
                key: std::array::from_fn(|index| {
                    u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap()
                }),
                name: value["name"].as_str().unwrap().to_owned(),
            }
        })
        .collect()
}
fn assert_nested_recording(
    window: &hydrus_gui::OpenFileCallsWindow,
    fixture: &Value,
    index: usize,
    selected: &[usize],
) {
    let values = recorded_nested_refs(fixture, index);
    assert_eq!(
        nested_names(window),
        values
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>(),
        "complete recorded nested state {}",
        fixture["nested_checks"][index]["action"]
    );
    assert_eq!(
        window
            .get_rows()
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.selected.then_some(index))
            .collect::<Vec<_>>(),
        selected
    );
    assert_eq!(window.get_selected(), !selected.is_empty());
    assert!(!window.get_child_open());
    assert!(window.window().is_visible());
}
fn file_question(bound: &Bound) -> hydrus_gui::SessionDialog {
    bound
        .options_open_externally
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn new_png_queue(parent: &OptionsWindow, bound: &Bound) -> hydrus_gui::OpenFileCallsWindow {
    parent.invoke_routing_file_action("add".into());
    choose(bound, "image/png");
    let child = files(bound);
    assert_eq!(child.get_window_title(), "edit calls");
    child
}
// These cases retain actual pending grandchildren, not merely callbacks after
// a completed answer. All old owners stay strongly held while a successor works.
fn pending_file_queue_retirement(ui: &MainWindow, bound: &Bound, store: &Store) {
    use slint::platform::WindowEvent;
    let persisted = routing_saved(store);
    for action in ["edit", "delete"] {
        let old = open(ui, bound);
        let retired_files = new_png_queue(&old, bound);
        add_nested(bound, "File one 日本");
        retired_files.invoke_clicked(0, false, false);
        retired_files.invoke_action(action.into());
        let retired_choice = bound
            .options_open_externally
            .choice
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let retired_question = bound
            .options_open_externally
            .question
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        assert!(retired_files.get_child_open());
        assert_eq!(retired_choice.is_some(), action == "edit");
        assert_eq!(retired_question.is_some(), action == "delete");
        old.invoke_cancel();
        assert!(!old.window().is_visible() && !retired_files.window().is_visible());
        if let Some(child) = &retired_choice {
            assert!(!child.window().is_visible());
        }
        if let Some(child) = &retired_question {
            assert!(!child.window().is_visible());
        }
        assert!(!bound.options_open_externally.has_open());
        assert_eq!(routing_saved(store), persisted);

        let current = open(ui, bound);
        let current_files = new_png_queue(&current, bound);
        add_nested(bound, "File one 日本");
        add_nested(bound, "File two");
        current_files.invoke_clicked(1, false, false);
        current_files.invoke_action(action.into());
        let current_choice = bound
            .options_open_externally
            .choice
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let current_question = bound
            .options_open_externally
            .question
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        assert_eq!(current_choice.is_some(), action == "edit");
        assert_eq!(current_question.is_some(), action == "delete");
        let parent_draft = routing_draft(&current);
        let child_draft = nested_draft(&current_files);
        assert!(current_files.get_child_open());
        if let Some(child) = &current_choice {
            assert_eq!(choice_labels(child), vec!["Default OS File Launch"]);
            let before_labels = choice_labels(child);
            child.hide().unwrap();
            child.invoke_chosen(0);
            assert_eq!(nested_draft(&current_files), child_draft);
            assert!(std::ptr::eq(choice(bound).window(), child.window()));
            child.show().unwrap();
            current_files.hide().unwrap();
            child.invoke_chosen(0);
            current_files.invoke_action("down".into());
            current_files.invoke_apply();
            assert_eq!(nested_draft(&current_files), child_draft);
            assert_eq!(choice_labels(child), before_labels);
            current_files.show().unwrap();
        }
        if let Some(child) = &current_question {
            assert_eq!(child.get_message(), "Remove 1 selected?");
            child.hide().unwrap();
            child.invoke_answered(true);
            assert_eq!(nested_draft(&current_files), child_draft);
            assert!(std::ptr::eq(file_question(bound).window(), child.window()));
            child.show().unwrap();
            current_files.hide().unwrap();
            child.invoke_answered(true);
            current_files.invoke_action("down".into());
            current_files.invoke_apply();
            assert_eq!(nested_draft(&current_files), child_draft);
            current_files.show().unwrap();
        }
        assert_eq!(routing_draft(&current), parent_draft);
        current_files.invoke_apply();
        current.invoke_apply();
        assert_eq!(routing_draft(&current), parent_draft);
        assert_eq!(nested_draft(&current_files), child_draft);
        for attempt in 0..5 {
            match attempt {
                0 => {
                    if let Some(child) = &retired_choice {
                        child.invoke_chosen(0);
                    }
                    if let Some(child) = &retired_question {
                        child.invoke_answered(true);
                    }
                }
                1 => {
                    if let Some(child) = &retired_choice {
                        child.invoke_cancel();
                    }
                    if let Some(child) = &retired_question {
                        child.invoke_answered(false);
                        child.invoke_cancelled();
                        child.invoke_force_close();
                    }
                }
                2 => {
                    retired_files.invoke_clicked(0, false, false);
                    retired_files.invoke_activated(0);
                    for action in ["add", "edit", "delete", "up", "down"] {
                        retired_files.invoke_action(action.into());
                    }
                    retired_files.invoke_apply();
                }
                3 => {
                    retired_files.invoke_cancel();
                    retired_files
                        .window()
                        .dispatch_event(WindowEvent::CloseRequested);
                    if let Some(child) = &retired_choice {
                        child.window().dispatch_event(WindowEvent::CloseRequested);
                    }
                    if let Some(child) = &retired_question {
                        child.window().dispatch_event(WindowEvent::CloseRequested);
                    }
                }
                _ => {
                    old.invoke_routing_file_action("add".into());
                    old.invoke_apply();
                    old.invoke_cancel();
                }
            }
            assert_eq!(routing_saved(store), persisted);
            assert_eq!(routing_draft(&current), parent_draft);
            assert_eq!(nested_draft(&current_files), child_draft);
            assert!(current.window().is_visible() && current_files.window().is_visible());
            assert!(std::ptr::eq(
                bound.options.borrow().as_ref().unwrap().window(),
                current.window()
            ));
            assert!(std::ptr::eq(files(bound).window(), current_files.window()));
            if let Some(child) = &current_choice {
                assert!(bound.options_open_externally.question.borrow().is_none());
                assert!(child.window().is_visible());
                assert!(std::ptr::eq(choice(bound).window(), child.window()));
                assert_eq!(choice_labels(child), vec!["Default OS File Launch"]);
                assert_eq!(child.get_window_title(), "select call to add");
                assert_eq!(child.get_choice_description(), "Select this call.");
            }
            if let Some(child) = &current_question {
                assert!(bound.options_open_externally.choice.borrow().is_none());
                assert!(child.window().is_visible());
                assert!(std::ptr::eq(file_question(bound).window(), child.window()));
                assert_eq!(child.get_message(), "Remove 1 selected?");
                assert_eq!(child.get_window_title(), "Question");
                assert!(!child.get_notice_only());
            }
        }
        let expected = if let Some(child) = &current_choice {
            child.invoke_chosen(0);
            assert!(!child.window().is_visible());
            vec!["File one 日本", "Default OS File Launch"]
        } else {
            let child = current_question.as_ref().unwrap();
            child.invoke_answered(true);
            assert!(!child.window().is_visible());
            vec!["File one 日本"]
        };
        assert_eq!(nested_names(&current_files), expected);
        assert_eq!(
            current_files
                .get_rows()
                .iter()
                .enumerate()
                .filter_map(|(index, row)| row.selected.then_some(index))
                .collect::<Vec<_>>(),
            if action == "edit" {
                vec![1]
            } else {
                Vec::new()
            },
            "Edit keeps its row selected; captured deletion removes that identity"
        );
        assert!(!current_files.get_child_open());
        assert!(bound.options_open_externally.choice.borrow().is_none());
        assert!(bound.options_open_externally.question.borrow().is_none());
        assert_eq!(
            routing_saved(store),
            persisted,
            "current positive answer is staged only"
        );
        current_files.invoke_apply();
        assert!(bound.options_open_externally.files.borrow().is_none());
        assert_eq!(
            file_rows(&current),
            json!([
                ["all files", "Default OS File Launch"],
                ["png", expected.join(", ")]
            ])
        );
        current.invoke_cancel();
        assert_eq!(routing_saved(store), persisted);
    }
}

#[test]
fn file_queue_replays_all_nested_reference_states_and_live_children() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let manager = seed(&store);
    let persisted = routing_saved(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    pending_file_queue_retirement(&ui, &bound, &store);
    let parent = open(&ui, &bound);
    let child = new_png_queue(&parent, &bound);
    let child_native = windows.get(windows.count() - 1).unwrap();
    child.invoke_action("add".into());
    let registered = choice(&bound);
    assert_recorded_choice(&registered, &fixture, 5);
    let registered_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &registered_native,
        registered.window(),
        "file-call-chooser-registered.png",
        (520, 440),
    );
    choose(&bound, "File one 日本");
    assert!(!child.get_selected(), "Add appends without selecting");
    child.invoke_action("add".into());
    assert_recorded_choice(&choice(&bound), &fixture, 8);
    choose(&bound, "File two");
    assert!(!child.get_selected());
    child.invoke_clicked(0, false, false);
    assert_nested_recording(&child, &fixture, 0, &[0]);
    routing_capture(
        &child_native,
        child.window(),
        "file-call-queue-populated.png",
        (580, 340),
    );
    child.invoke_action("edit".into());
    let cancelled = choice(&bound);
    assert_recorded_choice(&cancelled, &fixture, 9);
    cancelled.invoke_cancel();
    assert_nested_recording(&child, &fixture, 1, &[0]);
    child.invoke_action("edit".into());
    let single = choice(&bound);
    assert_recorded_choice(&single, &fixture, 9);
    let single_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &single_native,
        single.window(),
        "file-call-edit-single-choice.png",
        (520, 440),
    );
    choose(&bound, "Default OS File Launch");
    assert_nested_recording(&child, &fixture, 2, &[0]);
    child.invoke_action("edit".into());
    assert_recorded_choice(&choice(&bound), &fixture, 13);
    choose(&bound, "File one 日本");
    assert_nested_recording(&child, &fixture, 3, &[0]);
    child.invoke_action("down".into());
    assert_nested_recording(&child, &fixture, 4, &[1]);
    let down = nested_draft(&child);
    child.invoke_action("up".into());
    assert_nested_recording(&child, &fixture, 3, &[0]);
    child.invoke_action("down".into());
    assert_eq!(
        nested_draft(&child),
        down,
        "both arrows retain selection and complete order"
    );
    routing_capture(
        &child_native,
        child.window(),
        "file-call-queue-reordered.png",
        (580, 340),
    );
    child.invoke_action("delete".into());
    let removal = file_question(&bound);
    assert_eq!(removal.get_window_title(), "Question");
    assert_eq!(
        removal.get_message(),
        fixture["notices"][4].as_str().unwrap()
    );
    assert!(!removal.get_notice_only());
    let removal_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &removal_native,
        removal.window(),
        "file-call-remove-question.png",
        (520, 200),
    );
    removal.invoke_answered(false);
    assert_nested_recording(&child, &fixture, 5, &[1]);
    child.invoke_action("delete".into());
    file_question(&bound).invoke_answered(true);
    assert_nested_recording(&child, &fixture, 6, &[]);
    child.invoke_action("add".into());
    assert_recorded_choice(&choice(&bound), &fixture, 14);
    choose(&bound, "File one 日本");
    assert!(!child.get_selected());
    child.invoke_action("add".into());
    assert_recorded_choice(&choice(&bound), &fixture, 15);
    choose(&bound, "Default OS File Launch");
    assert!(!child.get_selected());
    child.invoke_clicked(0, false, false);
    child.invoke_action("edit".into());
    let exhausted = file_question(&bound);
    assert_eq!(exhausted.get_window_title(), "Information");
    assert_eq!(
        exhausted.get_message(),
        fixture["notices"][6].as_str().unwrap()
    );
    assert!(exhausted.get_notice_only());
    assert!(bound.options_open_externally.choice.borrow().is_none());
    let exhausted_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &exhausted_native,
        exhausted.window(),
        "file-call-exhausted-information.png",
        (520, 200),
    );
    exhausted.invoke_cancelled();
    assert_nested_recording(&child, &fixture, 7, &[0]);
    assert_eq!(
        routing_saved(&store),
        persisted,
        "all eight reference states are still detached"
    );
    child.invoke_apply();
    assert!(!child.window().is_visible());
    assert_eq!(
        file_rows(&parent),
        json!([
            ["all files", "Default OS File Launch"],
            ["png", "File two, File one 日本, Default OS File Launch"]
        ])
    );
    assert_eq!(routing_saved(&store), persisted);
    parent.invoke_apply();
    let mut expected = persisted.0.clone();
    expected
        .files
        .insert(Mime::ImagePng, recorded_nested_refs(&fixture, 7));
    assert_eq!(routing_saved(&store), (expected.clone(), manager.clone()));
    let reopened = open(&ui, &bound);
    reopened.invoke_routing_file_clicked(1, false, false);
    reopened.invoke_routing_file_action("edit".into());
    let restored = files(&bound);
    assert_nested_recording(&restored, &fixture, 7, &[]);
    assert_eq!(restored.get_window_title(), "edit launch path");
    restored.invoke_cancel();
    reopened.invoke_cancel();
    assert_eq!(routing_saved(&store), (expected, manager));
}

// Save through the actual nested editor, then reopen before the real launcher
// observes its first call. No settings reseed intervenes between these phases.
#[cfg(unix)]
fn move_saved_image_queue(
    ui: &MainWindow,
    bound: &Bound,
    store: &Store,
    down: bool,
    selected: usize,
    expected: &(Routing, Manager),
) {
    let before = routing_saved(store);
    let parent = open(ui, bound);
    let row = parent
        .get_routing_file_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == Mime::GeneralImage.human_name())
        .unwrap();
    parent.invoke_routing_file_clicked(i32::try_from(row).unwrap(), false, false);
    parent.invoke_routing_file_action("edit".into());
    let child = files(bound);
    assert_eq!(
        nested_names(&child),
        before.0.files[&Mime::GeneralImage]
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>()
    );
    child.invoke_clicked(i32::try_from(selected).unwrap(), false, false);
    child.invoke_action(if down { "down" } else { "up" }.into());
    let names = expected.0.files[&Mime::GeneralImage]
        .iter()
        .map(|value| value.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(nested_names(&child), names);
    assert_eq!(
        child.get_rows().iter().position(|row| row.selected),
        Some(if down { selected + 1 } else { selected - 1 })
    );
    assert_eq!(routing_saved(store), before);
    child.invoke_apply();
    assert!(!child.window().is_visible());
    assert_eq!(
        routing_saved(store),
        before,
        "child Apply still stages the reordered queue"
    );
    parent.invoke_apply();
    assert_eq!(
        &routing_saved(store),
        expected,
        "full stable CallRefs and Manager are saved by Options"
    );
    let reopened = open(ui, bound);
    let row = reopened
        .get_routing_file_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == Mime::GeneralImage.human_name())
        .unwrap();
    reopened.invoke_routing_file_clicked(i32::try_from(row).unwrap(), false, false);
    reopened.invoke_routing_file_action("edit".into());
    let restored = files(bound);
    assert_eq!(nested_names(&restored), names);
    assert!(!restored.get_selected());
    restored.invoke_cancel();
    reopened.invoke_cancel();
    assert_eq!(&routing_saved(store), expected);
}

fn select_mapping(parent: &OptionsWindow, mime: Mime) {
    let index = parent
        .get_routing_file_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == mime.human_name())
        .unwrap_or_else(|| {
            panic!(
                "missing MIME mapping {mime:?} (code {}, human {:?}, label {:?}); visible={}, draft={}",
                mime.code(),
                mime.human_name(),
                mime.mimetype(),
                parent.window().is_visible(),
                routing_draft(parent),
            )
        });
    parent.invoke_routing_file_clicked(i32::try_from(index).unwrap(), false, false);
    assert_eq!(
        parent
            .get_routing_file_rows()
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect::<Vec<_>>(),
        vec![mime.human_name()]
    );
}
fn assert_mime_chooser(
    child: &hydrus_gui::ExternalRoutingChoiceWindow,
    fixture: &Value,
    recorded: usize,
) {
    let reference = &fixture["chooser"][recorded];
    assert_eq!(
        child.get_window_title(),
        reference["title"].as_str().unwrap()
    );
    assert_eq!(reference["kwargs"]["sort_tuples"], false);
    assert_eq!(
        choice_labels(child),
        reference["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    assert!(child.get_choice_description().is_empty());
    assert!(child.window().is_visible());
}
fn mapping_refs(fixture: &Value, event: usize, before: &Routing) -> Routing {
    let mut expected = before.clone();
    expected.files = fixture["events"][event]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mime =
                Mime::from_code(u8::try_from(row["mime"].as_u64().unwrap()).unwrap()).unwrap();
            let refs = row["calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| {
                    let hex = value["key"].as_str().unwrap();
                    assert_eq!(hex.len(), 64);
                    CallRef {
                        key: std::array::from_fn(|index| {
                            u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap()
                        }),
                        name: value["name"].as_str().unwrap().to_owned(),
                    }
                })
                .collect();
            (mime, refs)
        })
        .collect();
    expected
}
fn stage_mapping(parent: &OptionsWindow, bound: &Bound, mime: Mime, names: &[&str]) {
    // Qt records distinct MIME codes with duplicate labels: animated GIF (3)
    // and static GIF (68) both say image/gif. Choose the recorded typed index,
    // not the first matching label, after excluding actual existing mappings.
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let used = parent
        .get_routing_file_rows()
        .iter()
        .map(|row| row.cells.row_data(0).unwrap().to_string())
        .collect::<Vec<_>>();
    let remaining = fixture["mime_choices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|value| {
            !used
                .iter()
                .any(|human| human == value["human"].as_str().unwrap())
        })
        .collect::<Vec<_>>();
    parent.invoke_routing_file_action("add".into());
    let chooser = choice(bound);
    assert_eq!(
        choice_labels(&chooser),
        remaining
            .iter()
            .map(|value| value["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    let index = remaining.iter().position(|value| value["mime"].as_u64() == Some(u64::from(mime.code()))).unwrap_or_else(|| {
        panic!("requested MIME {mime:?} (code {}) is not an eligible recorded choice; used={used:?}, remaining={remaining:?}", mime.code())
    });
    chooser.invoke_chosen(i32::try_from(index).unwrap());
    for name in names {
        add_nested(bound, name);
    }
    files(bound).invoke_apply();
    assert!(bound.options_open_externally.files.borrow().is_none());
    assert!(
        parent
            .get_routing_file_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap().as_str() == mime.human_name()),
        "accepted typed MIME {mime:?} must appear in the draft: {}",
        routing_draft(parent)
    );
}
// Outer MIME choices and captured-MIME questions use the same shared slots as
// nested queues, but need their own pending-owner and positive-successor proof.
fn pending_mime_mapping_retirement(ui: &MainWindow, bound: &Bound, store: &Store) {
    use slint::platform::WindowEvent;
    let persisted = routing_saved(store);
    for action in ["add", "delete"] {
        let old = open(ui, bound);
        if action == "delete" {
            stage_mapping(&old, bound, Mime::ImagePng, &["File one 日本"]);
            select_mapping(&old, Mime::ImagePng);
        }
        old.invoke_routing_file_action(action.into());
        let retired_choice = bound
            .options_open_externally
            .choice
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let retired_question = bound
            .options_open_externally
            .question
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        assert_eq!(retired_choice.is_some(), action == "add");
        assert_eq!(retired_question.is_some(), action == "delete");
        assert!(old.get_routing_child_open());
        if let Some(child) = &retired_choice {
            assert!(child.window().is_visible());
        }
        if let Some(child) = &retired_question {
            assert!(child.window().is_visible());
        }
        old.invoke_cancel();
        assert!(!old.window().is_visible());
        if let Some(child) = &retired_choice {
            assert!(!child.window().is_visible());
        }
        if let Some(child) = &retired_question {
            assert!(!child.window().is_visible());
        }
        assert!(!bound.options_open_externally.has_open());
        assert_eq!(routing_saved(store), persisted);

        let current = open(ui, bound);
        if action == "delete" {
            // A different captured MIME prevents an old PNG answer from being
            // mistaken for the current successor's GIF deletion.
            stage_mapping(&current, bound, Mime::ImageGif, &["File two"]);
            select_mapping(&current, Mime::ImageGif);
        }
        current.invoke_routing_file_action(action.into());
        let current_choice = bound
            .options_open_externally
            .choice
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let current_question = bound
            .options_open_externally
            .question
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        assert_eq!(current_choice.is_some(), action == "add");
        assert_eq!(current_question.is_some(), action == "delete");
        assert!(bound.options_open_externally.files.borrow().is_none());
        let draft = routing_draft(&current);
        let labels = current_choice.as_ref().map(choice_labels);
        if let Some(child) = &current_choice {
            child.hide().unwrap();
            child.invoke_chosen(0);
            assert_eq!(routing_draft(&current), draft);
            assert!(std::ptr::eq(choice(bound).window(), child.window()));
            child.show().unwrap();
            current.hide().unwrap();
            child.invoke_chosen(0);
            assert_eq!(routing_draft(&current), draft);
            current.show().unwrap();
        }
        if let Some(child) = &current_question {
            assert_eq!(child.get_message(), "Remove all selected?");
            child.hide().unwrap();
            child.invoke_answered(true);
            assert_eq!(routing_draft(&current), draft);
            assert!(std::ptr::eq(file_question(bound).window(), child.window()));
            child.show().unwrap();
            current.hide().unwrap();
            child.invoke_answered(true);
            assert_eq!(routing_draft(&current), draft);
            current.show().unwrap();
        }
        current.invoke_apply();
        assert_eq!(routing_draft(&current), draft);
        for attempt in 0..4 {
            match attempt {
                0 => {
                    if let Some(child) = &retired_choice {
                        child.invoke_chosen(0);
                    }
                    if let Some(child) = &retired_question {
                        child.invoke_answered(true);
                    }
                }
                1 => {
                    if let Some(child) = &retired_choice {
                        child.invoke_cancel();
                    }
                    if let Some(child) = &retired_question {
                        child.invoke_answered(false);
                        child.invoke_cancelled();
                        child.invoke_force_close();
                    }
                }
                2 => {
                    old.invoke_routing_file_clicked(0, false, false);
                    old.invoke_routing_file_activated(0);
                    for action in ["add", "edit", "delete"] {
                        old.invoke_routing_file_action(action.into());
                    }
                    old.invoke_apply();
                    old.invoke_cancel();
                }
                _ => {
                    old.window().dispatch_event(WindowEvent::CloseRequested);
                    if let Some(child) = &retired_choice {
                        child.window().dispatch_event(WindowEvent::CloseRequested);
                    }
                    if let Some(child) = &retired_question {
                        child.window().dispatch_event(WindowEvent::CloseRequested);
                    }
                }
            }
            assert_eq!(routing_saved(store), persisted);
            assert_eq!(routing_draft(&current), draft);
            assert!(current.window().is_visible());
            assert!(std::ptr::eq(
                bound.options.borrow().as_ref().unwrap().window(),
                current.window()
            ));
            assert!(bound.options_open_externally.files.borrow().is_none());
            if let Some(child) = &current_choice {
                assert!(bound.options_open_externally.question.borrow().is_none());
                assert!(child.window().is_visible());
                assert!(std::ptr::eq(choice(bound).window(), child.window()));
                assert_eq!(Some(choice_labels(child)), labels);
                assert_eq!(child.get_window_title(), "which filetype?");
                assert!(child.get_choice_description().is_empty());
            }
            if let Some(child) = &current_question {
                assert!(bound.options_open_externally.choice.borrow().is_none());
                assert!(child.window().is_visible());
                assert!(std::ptr::eq(file_question(bound).window(), child.window()));
                assert_eq!(child.get_message(), "Remove all selected?");
                assert_eq!(child.get_window_title(), "Question");
                assert!(!child.get_notice_only());
            }
        }
        if let Some(child) = &current_choice {
            choose(bound, "image/jpeg");
            assert!(!child.window().is_visible());
            let queue = files(bound);
            assert_eq!(queue.get_window_title(), "edit calls");
            assert!(nested_names(&queue).is_empty());
            add_nested(bound, "File two");
            queue.invoke_apply();
            assert_eq!(
                file_rows(&current),
                json!([
                    ["all files", "Default OS File Launch"],
                    ["jpeg", "File two"]
                ])
            );
        } else {
            let child = current_question.as_ref().unwrap();
            child.invoke_answered(true);
            assert!(!child.window().is_visible());
            assert_eq!(
                file_rows(&current),
                json!([["all files", "Default OS File Launch"]])
            );
            assert!(!current.get_routing_file_single() && !current.get_routing_file_delete());
        }
        assert!(!bound.options_open_externally.has_open());
        assert!(!current.get_routing_child_open());
        assert_eq!(
            routing_saved(store),
            persisted,
            "current answers remain staged"
        );
        current.invoke_cancel();
        assert_eq!(routing_saved(store), persisted);
    }
}

#[test]
fn outer_mime_mapping_reference_replay_saved_keys_delete_and_retired_children() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let manager = seed(&store);
    let persisted = routing_saved(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    pending_mime_mapping_retirement(&ui, &bound, &store);
    let parent = open(&ui, &bound);
    let parent_native = windows.get(windows.count() - 1).unwrap();
    let initial = routing_draft(&parent);
    parent.invoke_routing_file_action("add".into());
    let cancelled = choice(&bound);
    assert_mime_chooser(&cancelled, &fixture, 4);
    let mime_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &mime_native,
        cancelled.window(),
        "mime-add-chooser.png",
        (520, 440),
    );
    cancelled.invoke_cancel();
    assert_eq!(routing_draft(&parent), initial);
    assert_eq!(routing_saved(&store), persisted);
    for accepted in [false, true] {
        parent.invoke_routing_file_action("add".into());
        assert_mime_chooser(&choice(&bound), &fixture, if accepted { 6 } else { 4 });
        choose(&bound, "image/png");
        let child = files(&bound);
        let child_native = windows.get(windows.count() - 1).unwrap();
        assert_eq!(
            child.get_window_title(),
            fixture["nested"][usize::from(accepted)]["title"]
                .as_str()
                .unwrap()
        );
        assert!(nested_names(&child).is_empty());
        assert!(!child.get_selected() && !child.get_child_open());
        if !accepted {
            routing_capture(
                &child_native,
                child.window(),
                "mime-add-blank-calls.png",
                (580, 340),
            );
        }
        add_nested(&bound, "File one 日本");
        if accepted {
            add_nested(&bound, "File two");
            child.invoke_clicked(0, false, false);
            child.invoke_action("down".into());
        }
        assert_eq!(
            nested_names(&child),
            fixture["nested"][usize::from(accepted)]["before"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value["name"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        if accepted {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert_eq!(
            file_rows(&parent),
            expected_files(&fixture, if accepted { 9 } else { 8 })
        );
        assert_eq!(routing_saved(&store), persisted);
    }
    assert_eq!(
        parent
            .get_routing_file_rows()
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect::<Vec<_>>(),
        vec!["png"],
        "Add selects the sorted MIME identity"
    );
    for accepted in [false, true] {
        select_mapping(&parent, Mime::ImagePng);
        parent.invoke_routing_file_action("edit".into());
        let child = files(&bound);
        let child_native = windows.get(windows.count() - 1).unwrap();
        assert_eq!(
            child.get_window_title(),
            fixture["nested"][2 + usize::from(accepted)]["title"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            nested_names(&child),
            mapping_refs(&fixture, 9, &persisted.0).files[&Mime::ImagePng]
                .iter()
                .map(|value| value.name.clone())
                .collect::<Vec<_>>()
        );
        if !accepted {
            routing_capture(
                &child_native,
                child.window(),
                "mime-edit-existing-calls.png",
                (580, 340),
            );
        }
        add_nested(&bound, "Default OS File Launch");
        child.invoke_clicked(0, false, false);
        child.invoke_action("down".into());
        assert_eq!(
            nested_names(&child),
            fixture["nested"][2 + usize::from(accepted)]["before"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value["name"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        parent.invoke_apply();
        assert!(std::ptr::eq(
            bound.options.borrow().as_ref().unwrap().window(),
            parent.window()
        ));
        if accepted {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert_eq!(
            file_rows(&parent),
            expected_files(&fixture, if accepted { 11 } else { 10 })
        );
        assert_eq!(routing_saved(&store), persisted);
    }
    select_mapping(&parent, Mime::GeneralFile);
    assert_eq!(fixture["events"][12]["can_delete"], false);
    assert!(!parent.get_routing_file_delete());
    parent.invoke_routing_file_action("delete".into());
    assert!(bound.options_open_externally.question.borrow().is_none());
    assert_eq!(file_rows(&parent), expected_files(&fixture, 12));
    let png = parent
        .get_routing_file_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap() == "png")
        .unwrap();
    parent.invoke_routing_file_clicked(i32::try_from(png).unwrap(), true, false);
    assert_eq!(fixture["multi_general_protected"], true);
    assert!(!parent.get_routing_file_delete() && !parent.get_routing_file_single());
    parent.invoke_routing_file_action("delete".into());
    assert!(bound.options_open_externally.question.borrow().is_none());
    assert_eq!(file_rows(&parent), expected_files(&fixture, 12));
    select_mapping(&parent, Mime::ImagePng);
    let selected = routing_draft(&parent);
    parent.invoke_routing_file_action("delete".into());
    let removal = file_question(&bound);
    assert_eq!(removal.get_window_title(), "Question");
    assert_eq!(
        removal.get_message(),
        fixture["notices"][2].as_str().unwrap()
    );
    assert!(!removal.get_notice_only());
    let removal_native = windows.get(windows.count() - 1).unwrap();
    routing_capture(
        &removal_native,
        removal.window(),
        "mime-delete-question.png",
        (520, 200),
    );
    removal.invoke_answered(false);
    assert_eq!(
        routing_draft(&parent),
        selected,
        "No preserves complete rows and selection"
    );
    assert_eq!(routing_saved(&store), persisted);
    parent.invoke_routing_file_action("delete".into());
    file_question(&bound).invoke_answered(true);
    assert_eq!(file_rows(&parent), expected_files(&fixture, 13));
    assert!(!parent.get_routing_file_single() && !parent.get_routing_file_delete());
    assert_eq!(
        routing_saved(&store),
        persisted,
        "outer deletion remains staged"
    );
    routing_capture(
        &parent_native,
        parent.window(),
        "mime-delete-staged.png",
        (960, 800),
    );
    parent.invoke_cancel();
    assert_eq!(routing_saved(&store), persisted);

    // Persist the actual Add route first, then reopen and persist an actual
    // Edit. The full saved opaque keys must come from each accepted child,
    // rather than rebuilding the final Edit vector through a second Add.
    let parent = open(&ui, &bound);
    let child = new_png_queue(&parent, &bound);
    add_nested(&bound, "File one 日本");
    add_nested(&bound, "File two");
    child.invoke_clicked(0, false, false);
    child.invoke_action("down".into());
    child.invoke_apply();
    assert_eq!(file_rows(&parent), expected_files(&fixture, 9));
    assert_eq!(routing_saved(&store), persisted);
    parent.invoke_apply();
    let added = (mapping_refs(&fixture, 9, &persisted.0), manager.clone());
    assert_eq!(
        routing_saved(&store),
        added,
        "Add saves the complete recorded stable CallRefs"
    );
    let parent = open(&ui, &bound);
    assert_eq!(file_rows(&parent), expected_files(&fixture, 9));
    select_mapping(&parent, Mime::ImagePng);
    parent.invoke_routing_file_action("edit".into());
    let child = files(&bound);
    assert_eq!(child.get_window_title(), "edit launch path");
    assert_eq!(
        nested_names(&child),
        added.0.files[&Mime::ImagePng]
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>()
    );
    add_nested(&bound, "Default OS File Launch");
    child.invoke_clicked(0, false, false);
    child.invoke_action("down".into());
    child.invoke_apply();
    assert_eq!(file_rows(&parent), expected_files(&fixture, 11));
    assert_eq!(
        routing_saved(&store),
        added,
        "Edit remains staged until Options Apply"
    );
    parent.invoke_apply();
    let expected = (mapping_refs(&fixture, 11, &persisted.0), manager.clone());
    assert_eq!(
        routing_saved(&store),
        expected,
        "actual full stable CallRefs and Manager are persisted"
    );
    let reopened = open(&ui, &bound);
    let reopened_native = windows.get(windows.count() - 1).unwrap();
    assert_eq!(file_rows(&reopened), expected_files(&fixture, 11));
    select_mapping(&reopened, Mime::ImagePng);
    routing_capture(
        &reopened_native,
        reopened.window(),
        "mime-mapping-reopened.png",
        (960, 800),
    );
    let before_choice = routing_draft(&reopened);
    reopened.invoke_routing_file_action("add".into());
    let remaining = choice(&bound);
    // Unlike recorded chooser4/6 (both precede PNG insertion), used-PNG
    // exclusion here is grounded in Qt _AddMimeLaunch's source filtering.
    assert_eq!(
        choice_labels(&remaining),
        fixture["mime_choices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|value| value["mime"] != Mime::ImagePng.code())
            .map(|value| value["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    remaining.invoke_cancel();
    assert_eq!(routing_draft(&reopened), before_choice);
    reopened.invoke_routing_file_action("edit".into());
    let child = files(&bound);
    assert_eq!(
        nested_names(&child),
        expected.0.files[&Mime::ImagePng]
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>()
    );
    child.invoke_cancel();
    assert_eq!(routing_saved(&store), expected);
    reopened.invoke_routing_file_action("delete".into());
    let removal = file_question(&bound);
    assert_eq!(
        removal.get_message(),
        fixture["notices"][2].as_str().unwrap()
    );
    removal.invoke_answered(true);
    assert_eq!(file_rows(&reopened), expected_files(&fixture, 13));
    assert_eq!(routing_saved(&store), expected);
    reopened.invoke_apply();
    let deleted = (mapping_refs(&fixture, 13, &persisted.0), manager);
    assert_eq!(routing_saved(&store), deleted);
    let restored = open(&ui, &bound);
    assert_eq!(file_rows(&restored), expected_files(&fixture, 13));
    restored.invoke_cancel();
    assert_eq!(routing_saved(&store), deleted);
}
