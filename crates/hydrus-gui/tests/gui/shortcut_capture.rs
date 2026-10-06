//! The real Options capture children feed staged settings and existing executors.
use hydrus_core::shortcuts::{Binding, Gesture, Settings};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use slint::{ComponentHandle as _, Model as _};
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|row| row.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, index as i32, 0.0, 0.0, 0.0);
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
fn edit_set(parent: &OptionsWindow, name: &str) -> hydrus_gui::ShortcutSetWindow {
    let label = hydrus_gui_model::shortcut_sets::pretty_name(name);
    let row = parent
        .get_shortcut_reserved_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == label)
        .unwrap();
    parent.invoke_shortcut_set_clicked(false, i32::try_from(row).unwrap(), false, false);
    assert!(parent.get_shortcut_reserved_selected());
    parent.invoke_shortcut_set_action("edit-reserved".into());
    let window = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert!(window.window().is_visible());
    assert_eq!(window.get_set_name().as_str(), name);
    window
}
fn choose_command(window: &hydrus_gui::ShortcutCommandWindow, label: &str) {
    let index = window
        .get_commands()
        .iter()
        .position(|command| command.as_str() == label)
        .unwrap();
    window.set_command_index(i32::try_from(index).unwrap());
}
fn with_added(before: &Settings, set: &str, additions: &[Binding]) -> Settings {
    let mut expected = before.clone();
    let bindings = expected.sets.get_mut(set).unwrap();
    for addition in additions {
        assert!(
            bindings
                .iter()
                .all(|binding| binding.gesture != addition.gesture),
            "the test gesture must add to, rather than replace, an imported binding"
        );
        bindings.push(addition.clone());
    }
    expected
}
fn simple_binding(gesture: Gesture, action: i32) -> Binding {
    Binding {
        gesture,
        action,
        text: None,
        content: None,
    }
}
fn add(
    parent: &OptionsWindow,
    set: &str,
) -> (
    hydrus_gui::ShortcutSetWindow,
    hydrus_gui::ShortcutCommandWindow,
) {
    let sets = edit_set(parent, set);
    sets.invoke_action("add".into());
    (sets, hydrus_gui::shortcut_windows::last_command().unwrap())
}
#[test]
fn keyboard_capture_applies_through_owned_set_then_options_and_saved_main_executor() {
    use slint::platform::{Key, WindowEvent};
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let before = bound.current.borrow().borrow().results().to_vec();
    assert!(before.len() > 1);
    let original_count = initial.sets["main_gui"].len();
    assert!(
        original_count > 0,
        "preserve the imported built-in defaults"
    );
    let parent = options(&ui, &bound);
    parent.set_shortcuts_merge_numpad(false);
    parent.invoke_shortcuts_policy(false, true);
    let (sets, command) = add(&parent, "main_gui");
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 700, 330);
    assert!(!pixels.is_empty());
    // Dispatch real widget keys, rather than calling its capture callback directly.
    for text in [Key::Control.into(), Key::Shift.into(), "q".into()] {
        command
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text });
    }
    for text in ["q".into(), Key::Shift.into(), Key::Control.into()] {
        command
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
    assert_eq!(
        command.get_keyboard_text(),
        if cfg!(target_os = "macos") {
            "command+shift+q"
        } else {
            "ctrl+shift+q"
        }
    );
    parent.invoke_apply();
    sets.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial,
        "child/set Apply must not publish the outer Options draft"
    );
    command.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), original_count + 1);
    parent.invoke_apply();
    assert!(bound.options.borrow().is_some());
    sets.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial,
        "child/set Apply must not publish the outer Options draft"
    );
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    assert!(!saved.merge_numpad);
    assert!(saved.primary_labels);
    let captured = simple_binding(Gesture::new(0, 113, 0, 5), 78);
    let mut expected = with_added(&initial, "main_gui", std::slice::from_ref(&captured));
    expected.merge_numpad = false;
    expected.primary_labels = true;
    assert_eq!(
        saved, expected,
        "all imported bindings survive the new capture"
    );
    assert_eq!(
        saved.sets["main_gui"]
            .iter()
            .find(|binding| binding.gesture == captured.gesture)
            .unwrap(),
        &captured
    );
    ui.invoke_flip_synchronised();
    ui.invoke_search_edited("system:archive".into());
    ui.invoke_search_accepted();
    assert_eq!(bound.current.borrow().borrow().results(), before);
    assert!(ui.invoke_shortcut_key("q".into(), 5));
    assert!(bound.current.borrow().borrow().synchronised());
    assert!(bound.current.borrow().borrow().results().len() < before.len());
    let parent = options(&ui, &bound);
    let reopened = edit_set(&parent, "main_gui");
    assert_eq!(
        reopened.get_rows().row_count(),
        saved.sets["main_gui"].len()
    );
    let label = captured.gesture.text(saved.primary_labels);
    let row = reopened
        .get_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == label.as_str())
        .unwrap();
    reopened.invoke_selected_row(i32::try_from(row).unwrap());
    reopened.invoke_action("edit".into());
    let edit = hydrus_gui::shortcut_windows::last_command().unwrap();
    assert_eq!(edit.get_keyboard_text(), command.get_keyboard_text());
    edit.invoke_key_capture("z".into(), 0);
    parent.invoke_cancel();
    edit.invoke_apply();
    reopened.invoke_apply();
    parent.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        saved
    );
}
#[test]
fn command_and_set_cancel_reject_stale_children_and_preserve_successor_drafts() {
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let original_count = initial.sets["main_gui"].len();
    assert!(
        original_count > 0,
        "the cancelled draft starts with imported defaults"
    );
    let parent = options(&ui, &bound);
    let (sets, child) = add(&parent, "main_gui");
    child.invoke_key_capture("x".into(), 1);
    child.invoke_cancel();
    child.invoke_key_capture("y".into(), 1);
    child.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), original_count);
    sets.invoke_action("add".into());
    let next = hydrus_gui::shortcut_windows::last_command().unwrap();
    next.invoke_key_capture("a".into(), 0);
    next.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), original_count + 1);
    sets.invoke_cancel();
    sets.invoke_apply();
    assert!(!parent.get_shortcuts_child_open());
    let (successor, child) = add(&parent, "main_gui");
    assert_eq!(successor.get_rows().row_count(), original_count);
    child.invoke_key_capture("b".into(), 0);
    parent.invoke_cancel();
    child.invoke_apply();
    successor.invoke_apply();
    parent.invoke_shortcut_set_action("edit-reserved".into());
    assert!(!parent.get_shortcuts_child_open());
    let fresh = options(&ui, &bound);
    fresh.invoke_apply();
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        initial,
        "Cancel and stale callbacks preserve every imported set and capture policy"
    );
}
#[test]
fn saved_mouse_and_keyboard_captures_feed_existing_viewer_navigation_and_guard_closed_owners() {
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert!(bound.current.borrow().borrow().results().len() > 1);
    let parent = options(&ui, &bound);
    let (sets, mouse) = add(&parent, "media_viewer");
    mouse.invoke_mouse_capture(1, 0, 1);
    assert_eq!(mouse.get_mode(), 1);
    choose_command(&mouse, "media navigation: next");
    mouse.invoke_apply();
    sets.invoke_action("add".into());
    let key = hydrus_gui::shortcut_windows::last_command().unwrap();
    key.invoke_key_capture("v".into(), 0);
    choose_command(&key, "media navigation: previous");
    key.invoke_apply();
    sets.invoke_action("add".into());
    let wheel = hydrus_gui::shortcut_windows::last_command().unwrap();
    wheel.invoke_wheel_capture(-120.0, 0);
    assert!(!wheel.get_release_enabled());
    choose_command(&wheel, "media navigation: next");
    wheel.invoke_apply();
    sets.invoke_apply();
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let additions = [
        simple_binding(Gesture::new(1, 1, 0, 1), 99),
        simple_binding(Gesture::new(0, 118, 0, 0), 100),
        simple_binding(Gesture::new(1, 4, 0, 0), 99),
    ];
    assert_eq!(saved, with_added(&initial, "media_viewer", &additions));
    for addition in &additions {
        assert_eq!(
            saved.sets["media_viewer"]
                .iter()
                .find(|binding| binding.gesture == addition.gesture)
                .unwrap(),
            addition
        );
    }
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let count = bound.current.borrow().borrow().results().len();
    assert_eq!(viewer.get_caption(), format!("1/{count}"));
    assert!(viewer.invoke_shortcut_mouse(1, 0, 1));
    assert_eq!(viewer.get_caption(), format!("2/{count}"));
    assert!(viewer.invoke_shortcut_key("v".into(), 0));
    assert_eq!(viewer.get_caption(), format!("1/{count}"));
    assert!(viewer.invoke_shortcut_wheel(-120.0, 0));
    assert_eq!(viewer.get_caption(), format!("2/{count}"));
    viewer.invoke_close_requested();
    ui.invoke_thumbnail_activated(0);
    let successor = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(!viewer.invoke_shortcut_mouse(1, 0, 1));
    assert!(!viewer.invoke_shortcut_key("v".into(), 0));
    viewer.show().unwrap();
    let retired_caption = viewer.get_caption();
    assert!(!viewer.invoke_shortcut_mouse(1, 0, 1));
    assert!(!viewer.invoke_shortcut_key("v".into(), 0));
    assert!(!viewer.invoke_shortcut_wheel(-120.0, 0));
    assert!(!viewer.invoke_shortcut_wheel(-30.0, 0));
    assert_eq!(viewer.get_caption(), retired_caption);
    assert_eq!(successor.get_caption(), format!("1/{count}"));
    assert!(successor.invoke_shortcut_key("v".into(), 0));
    assert_eq!(successor.get_caption(), format!("{count}/{count}"));
    viewer.hide().unwrap();
    successor.invoke_close_requested();
}

#[test]
fn accepted_main_close_retires_saved_shortcuts_even_after_show_and_fresh_binding_reopens() {
    let (_dirs, store) = crate::subscriptions::store();
    let initial = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    let (sets, command) = add(&parent, "main_gui");
    // Apply the untouched reference F7/default refresh command.
    command.invoke_apply();
    sets.invoke_apply();
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    let added = simple_binding(Gesture::default(), 78);
    assert_eq!(
        saved,
        with_added(&initial, "main_gui", std::slice::from_ref(&added))
    );
    assert_eq!(
        saved.sets["main_gui"]
            .iter()
            .find(|binding| binding.gesture == added.gesture)
            .unwrap(),
        &added
    );
    store
        .write(|ctx| {
            let mut settings: hydrus_store::settings::GuiSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.confirm_exit = true;
            hydrus_store::settings::set(ctx.conn(), &settings)?;
            // Exercise the outer exit question without a second maintenance prompt.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(ui.window().is_visible());
    assert!(
        ui.get_question()
            .starts_with("Are you sure you want to exit the client?")
    );
    ui.invoke_answer(false);
    ui.invoke_flip_synchronised();
    assert!(!bound.current.borrow().borrow().synchronised());
    assert!(ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(bound.current.borrow().borrow().synchronised());
    ui.invoke_flip_synchronised();
    assert!(!bound.current.borrow().borrow().synchronised());
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(ui.window().is_visible());
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    ui.show().unwrap();
    assert!(!ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(!bound.current.borrow().borrow().synchronised());
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        saved
    );
    let reopened = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_flip_synchronised();
    assert!(!reopened.current.borrow().borrow().synchronised());
    assert!(ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(reopened.current.borrow().borrow().synchronised());
    assert!(!bound.current.borrow().borrow().synchronised());
}
#[test]
fn raw_backend_key_location_and_modifier_identity_reach_the_same_capture_contract() {
    use slint::winit_030::winit::keyboard::{Key, KeyLocation, NamedKey};
    let numeric = Key::Character("7".into());
    let arrow = Key::Named(NamedKey::ArrowLeft);
    let (kind, key, bits) =
        hydrus_gui::shortcut_input::key(&numeric, KeyLocation::Numpad, 1).unwrap();
    assert_eq!(
        Gesture::keyboard(kind, key, bits, true).unwrap(),
        Gesture::new(0, 55, 0, 9)
    );
    let (kind, key, bits) =
        hydrus_gui::shortcut_input::key(&arrow, KeyLocation::Numpad, 4).unwrap();
    assert_eq!(
        Gesture::keyboard(kind, key, bits, true).unwrap(),
        Gesture::new(2, 11, 0, 4)
    );
    assert_eq!(
        Gesture::keyboard(kind, key, bits, false).unwrap(),
        Gesture::new(2, 11, 0, 12)
    );
    assert_eq!(
        hydrus_gui::shortcut_input::key(&Key::Named(NamedKey::Enter), KeyLocation::Numpad, 0),
        Some((2, 4, 8))
    );
    let (kind, key, bits) =
        hydrus_gui::shortcut_input::key(&Key::Character("ß".into()), KeyLocation::Standard, 16)
            .unwrap();
    assert_eq!(
        Gesture::keyboard(kind, key, bits, true).unwrap(),
        Gesture::new(0, 115, 0, 16)
    );
    assert!(
        hydrus_gui::shortcut_input::key(&Key::Dead(Some('^')), KeyLocation::Standard, 0).is_none()
    );
}
