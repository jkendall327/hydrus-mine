//! Options > shortcuts' custom sets (add, edit, delete), "restore defaults"
//! for the built-in sets, and the mouse half of the command editor, against
//! `ShortcutsPanel` and `oracle/record_shortcut_capture.py`.
use hydrus_core::shortcuts::Settings;
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_gui_model::shortcut_sets as sets;
use slint::{ComponentHandle as _, Model as _};

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|row| row.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "shortcuts")
        .unwrap() as i32;
    window.set_page(page);
    window.invoke_page_chosen(page);
    window
}

fn cells(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<(String, String)> {
    rows.iter()
        .map(|row| {
            (
                row.cells.row_data(0).unwrap().to_string(),
                row.cells.row_data(1).unwrap().to_string(),
            )
        })
        .collect()
}
fn custom(parent: &OptionsWindow) -> Vec<(String, String)> {
    cells(&parent.get_shortcut_custom_rows())
}
fn reserved_count(parent: &OptionsWindow, name: &str) -> String {
    let label = sets::pretty_name(name);
    cells(&parent.get_shortcut_reserved_rows())
        .into_iter()
        .find(|(pretty, _)| pretty == label)
        .unwrap()
        .1
}
fn pair(name: &str, count: &str) -> (String, String) {
    (name.to_owned(), count.to_owned())
}

/// Add a captured key to the open set editor.
fn add_key(sets_window: &hydrus_gui::ShortcutSetWindow, key: &str) {
    sets_window.invoke_action("add".into());
    let command = hydrus_gui::shortcut_windows::last_command().unwrap();
    command.invoke_key_capture(key.into(), 0);
    command.invoke_apply();
}
/// Add a custom set called `name` holding one key for each of `keys`.
fn add_custom(parent: &OptionsWindow, name: &str, keys: &[&str]) {
    parent.invoke_shortcut_set_action("add".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert!(window.get_name_enabled());
    window.set_set_name(name.into());
    for key in keys {
        add_key(&window, key);
    }
    window.invoke_apply();
    assert!(!parent.get_shortcuts_child_open());
}
fn chooser() -> hydrus_gui::ChoiceButtonsWindow {
    let chooser = hydrus_gui::shortcut_windows::last_chooser().unwrap();
    assert!(chooser.window().is_visible());
    chooser
}
fn choices(chooser: &hydrus_gui::ChoiceButtonsWindow) -> Vec<String> {
    chooser
        .get_choices()
        .iter()
        .map(|c| c.to_string())
        .collect()
}

// leaf: audit-options-shortcuts-custom-user-sets-add
// leaf: audit-options-shortcuts-custom-user-sets-edit
// leaf: audit-options-shortcuts-custom-user-sets-delete
#[test]
fn custom_sets_are_added_edited_renamed_deleted_and_saved_only_by_options_ok() {
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    assert!(custom(&parent).is_empty());

    // add: the editor starts on "new shortcuts", with a free name and no
    // description; a cancelled editor adds nothing
    parent.invoke_shortcut_set_action("add".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert_eq!(window.get_set_name(), sets::NEW_NAME);
    assert!(window.get_name_enabled());
    assert_eq!(window.get_description(), "");
    assert_eq!(window.get_rows().row_count(), 0);
    assert!(parent.get_shortcuts_child_open());
    window.invoke_cancel();
    assert!(custom(&parent).is_empty());
    assert!(!parent.get_shortcuts_child_open());

    add_custom(&parent, "rating keys", &["q", "w"]);
    assert_eq!(custom(&parent), [pair("rating keys", "2")]);
    // a second set of the same name is named apart, as `GetNonDupeName` does
    add_custom(&parent, "rating keys", &[]);
    assert_eq!(
        custom(&parent),
        [pair("rating keys", "2"), pair("rating keys (1)", "0")]
    );
    // a custom set cannot take a built-in name
    add_custom(&parent, "main_gui", &["e"]);
    assert_eq!(
        custom(&parent),
        [
            pair("main_gui (1)", "1"),
            pair("rating keys", "2"),
            pair("rating keys (1)", "0")
        ]
    );
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial,
        "nothing is saved before Options is OKed"
    );

    // edit: the editor opens on the selected set, its name editable; renaming
    // onto another set's name is named apart, onto its own is kept
    parent.invoke_shortcut_set_clicked(true, 1, false, false);
    assert!(parent.get_shortcut_custom_selected());
    parent.invoke_shortcut_set_action("edit-custom".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert_eq!(window.get_set_name(), "rating keys");
    assert_eq!(window.get_rows().row_count(), 2);
    window.set_set_name("main_gui (1)".into());
    window.invoke_selected_row(0);
    window.invoke_action("remove".into());
    assert!(window.get_remove_question());
    window.invoke_remove_chosen(true);
    window.invoke_apply();
    assert_eq!(
        custom(&parent),
        [
            pair("main_gui (1)", "1"),
            pair("main_gui (1) (1)", "1"),
            pair("rating keys (1)", "0")
        ]
    );
    // activating a row (double-click) edits it too; own name stays
    parent.invoke_shortcut_set_activated(true, 2);
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert_eq!(window.get_set_name(), "rating keys (1)");
    window.invoke_apply();
    assert_eq!(custom(&parent)[2], pair("rating keys (1)", "0"));

    // delete: asks, and a "no" keeps them; ctrl-click selects more than one
    parent.invoke_shortcut_set_clicked(true, 0, false, false);
    parent.invoke_shortcut_set_clicked(true, 2, true, false);
    parent.invoke_shortcut_set_action("delete".into());
    let question = chooser();
    assert_eq!(question.get_message(), sets::DELETE_QUESTION);
    assert_eq!(choices(&question), ["yes"]);
    assert_eq!(question.get_no_label(), "no");
    question.invoke_cancelled();
    assert_eq!(custom(&parent).len(), 3);
    parent.invoke_shortcut_set_action("delete".into());
    chooser().invoke_chosen(0);
    assert_eq!(custom(&parent), [pair("main_gui (1) (1)", "1")]);
    assert!(!parent.get_shortcut_custom_selected());

    // Cancel discards everything; OK saves it
    parent.invoke_cancel();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial
    );
    let parent = options(&ui, &bound);
    assert!(custom(&parent).is_empty());
    add_custom(&parent, "kept", &["r"]);
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    assert_eq!(saved.sets["kept"].len(), 1);
    assert_eq!(saved.sets.len(), initial.sets.len() + 1);
    let parent = options(&ui, &bound);
    assert_eq!(custom(&parent), [pair("kept", "1")]);
    parent.invoke_shortcut_set_clicked(true, 0, false, false);
    parent.invoke_shortcut_set_action("delete".into());
    chooser().invoke_chosen(0);
    parent.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial,
        "deleting the saved set and OK removes it"
    );
}

// leaf: audit-options-shortcuts-built-in-hydrus-shortcut-sets-restore-defaults
#[test]
fn restore_defaults_asks_which_set_then_confirms_or_says_it_was_missing() {
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    // a client whose "global" set has gone missing
    let mut damaged = initial.clone();
    damaged.sets.remove("global");
    store
        .write({
            let damaged = damaged.clone();
            move |ctx| hydrus_store::settings::set(ctx.conn(), &damaged)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    let original = reserved_count(&parent, "main_gui");
    assert_eq!(parent.get_shortcut_reserved_rows().row_count(), 10);

    // alter main_gui through its editor
    parent.invoke_shortcut_set_clicked(
        false,
        cells(&parent.get_shortcut_reserved_rows())
            .iter()
            .position(|(name, _)| name == "the main window")
            .unwrap() as i32,
        false,
        false,
    );
    parent.invoke_shortcut_set_action("edit-reserved".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    add_key(&window, "x");
    window.invoke_apply();
    assert_ne!(reserved_count(&parent, "main_gui"), original);

    let pick = |name: &str| {
        parent.invoke_shortcut_set_action("restore".into());
        let which = chooser();
        assert_eq!(which.get_message(), sets::RESTORE_TITLE);
        assert_eq!(which.get_no_label(), "cancel");
        let names = choices(&which);
        assert_eq!(names, sets::default_names());
        which.invoke_chosen(names.iter().position(|n| n == name).unwrap() as i32);
    };
    // cancelling the choice does nothing
    parent.invoke_shortcut_set_action("restore".into());
    chooser().invoke_cancelled();
    assert_ne!(reserved_count(&parent, "main_gui"), original);

    // an existing set asks first; "no" keeps the edits, "yes" wipes them
    pick("main_gui");
    let sure = chooser();
    assert_eq!(
        sure.get_message(),
        "Are you certain you want to restore the defaults for \"main_gui\"? Any custom shortcuts you have set will be wiped."
    );
    assert_eq!(choices(&sure), ["yes"]);
    sure.invoke_cancelled();
    assert_ne!(reserved_count(&parent, "main_gui"), original);
    pick("main_gui");
    chooser().invoke_chosen(0);
    assert_eq!(reserved_count(&parent, "main_gui"), original);

    // a missing set is restored after being told so
    pick("global");
    let told = chooser();
    assert_eq!(
        told.get_message(),
        "It looks like your client was missing the \"global\" shortcut set! It will now be restored."
    );
    assert!(choices(&told).is_empty());
    assert_eq!(parent.get_shortcut_reserved_rows().row_count(), 10);
    told.invoke_cancelled();
    assert_eq!(parent.get_shortcut_reserved_rows().row_count(), 11);
    assert_eq!(reserved_count(&parent, "global"), "1");

    // nothing is saved until OK, and then it is the reference's defaults
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        damaged
    );
    parent.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial
    );
}

fn bits(fixture: &serde_json::Value, flags: u64) -> i32 {
    ["Control", "Alt", "Shift", "Keypad", "GroupSwitch", "Meta"]
        .iter()
        .enumerate()
        .fold(0, |out, (i, key)| {
            out | if flags & fixture["modifier_masks"][*key].as_u64().unwrap() != 0 {
                1 << i
            } else {
                0
            }
        })
}

// leaf: audit-options-nested-shortcuts-command-mouse
#[test]
fn mouse_capture_replays_the_references_buttons_press_release_double_click_and_wheel() {
    let fixture = hydrus_testkit::fixture_json("shortcut_capture.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["kind"] == "mouse")
    {
        let primary = case["primary_labels"].as_bool().unwrap_or(false);
        let parent = options(&ui, &bound);
        parent.invoke_shortcuts_policy(true, primary);
        // any viewer set takes mouse gestures
        let row = cells(&parent.get_shortcut_reserved_rows())
            .iter()
            .position(|(name, _)| name == sets::pretty_name("media_viewer"))
            .unwrap();
        parent.invoke_shortcut_set_clicked(false, row as i32, false, false);
        parent.invoke_shortcut_set_action("edit-reserved".into());
        let window = hydrus_gui::shortcut_windows::last_set().unwrap();
        window.invoke_action("add".into());
        let command = hydrus_gui::shortcut_windows::last_command().unwrap();
        assert_eq!(command.get_mode(), 0, "the keyboard half is first");
        let mut last = String::new();
        for step in case["steps"].as_array().unwrap() {
            let flags = bits(&fixture, step["modifiers"].as_u64().unwrap());
            match step["kind"].as_str().unwrap() {
                "choice" => command.invoke_release_changed(step["delta"].as_i64().unwrap() as i32),
                "wheel" => {
                    command.invoke_wheel_capture(step["delta"].as_i64().unwrap() as f32, flags);
                }
                "horizontal" => command.invoke_wheel_capture(0.0, flags),
                kind => {
                    let key = match step["button"].as_u64().unwrap() {
                        1 => 0,
                        2 => 1,
                        4 => 2,
                        8 => 7,
                        16 => 8,
                        32 => 9,
                        other => panic!("recorded button {other}"),
                    };
                    let press = match kind {
                        "press" => 0,
                        "release" => 1,
                        "double" => 2,
                        other => panic!("{other}"),
                    };
                    command.invoke_mouse_capture(key, press, flags);
                }
            }
            assert_eq!(command.get_mode(), 1, "{}", step["label"]);
            assert_eq!(
                command.get_release_enabled(),
                step["press_release_enabled"],
                "{}",
                step["label"]
            );
            assert_eq!(
                command.get_release_choice(),
                step["choice"].as_i64().unwrap() as i32,
                "{}",
                step["label"]
            );
            if !cfg!(target_os = "macos") {
                assert_eq!(command.get_mouse_text(), step["text"].as_str().unwrap());
            }
            last = command.get_mouse_text().to_string();
        }
        // OK puts the mouse gesture, rather than the keyboard one, in the set
        command.invoke_apply();
        let rows: Vec<String> = window
            .get_rows()
            .iter()
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect();
        assert_eq!(rows.last().unwrap(), &last);
        window.invoke_cancel();
        parent.invoke_cancel();
    }
}

// leaf: audit-options-nested-shortcuts-command-mouse
#[test]
fn mouse_events_on_the_capture_box_set_the_gesture_and_a_second_click_makes_it_a_double_click() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    let row = cells(&parent.get_shortcut_reserved_rows())
        .iter()
        .position(|(name, _)| name == sets::pretty_name("media_viewer"))
        .unwrap();
    parent.invoke_shortcut_set_clicked(false, row as i32, false, false);
    parent.invoke_shortcut_set_action("edit-reserved".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    window.invoke_action("add".into());
    let command = hydrus_gui::shortcut_windows::last_command().unwrap();
    let position = slint::LogicalPosition::new(
        command.get_capture_x() / 1.0 + 10.0,
        command.get_capture_y() / 1.0 + 10.0,
    );
    let click = |button| {
        command
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        command
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    };
    command
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    click(PointerEventButton::Right);
    assert_eq!(command.get_mode(), 1);
    assert_eq!(command.get_mouse_text(), "right-click");
    click(PointerEventButton::Middle);
    assert_eq!(command.get_mouse_text(), "middle-click");
    click(PointerEventButton::Left);
    assert_eq!(command.get_mouse_text(), "left-click");
    // the second quick click is a double-click
    click(PointerEventButton::Left);
    assert_eq!(command.get_mouse_text(), "double left-click");
    assert!(!command.get_release_enabled());
    // the wheel scrolls up and down
    command
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position,
            delta_x: 0.0,
            delta_y: -120.0,
        });
    assert_eq!(command.get_mouse_text(), "scroll down");
    command.invoke_cancel();
    window.invoke_cancel();
    parent.invoke_cancel();
}
