//! The real Options capture children feed staged settings and existing executors.
use hydrus_core::shortcuts::{Gesture, Settings};
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
fn add(
    parent: &OptionsWindow,
    set: i32,
) -> (
    hydrus_gui::ShortcutSetWindow,
    hydrus_gui::ShortcutCommandWindow,
) {
    parent.invoke_shortcuts_clicked();
    let sets = hydrus_gui::shortcut_windows::last_set().unwrap();
    sets.set_set_index(set);
    sets.invoke_set_changed(set);
    sets.invoke_action("add".into());
    (sets, hydrus_gui::shortcut_windows::last_command().unwrap())
}
#[test]
fn keyboard_capture_applies_through_owned_set_then_options_and_saved_main_executor() {
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let before = bound.current.borrow().results().to_vec();
    assert!(before.len() > 1);
    let parent = options(&ui, &bound);
    parent.set_shortcuts_merge_numpad(false);
    parent.invoke_shortcuts_policy(false, true);
    let (sets, command) = add(&parent, 0);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 700, 330);
    assert!(!pixels.is_empty());
    // Dispatch real widget keys, rather than calling its capture callback directly.
    use slint::platform::{Key, WindowEvent};
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
    assert!(
        store
            .read(hydrus_store::settings::get::<Settings>)
            .unwrap()
            .sets["main_gui"]
            .is_empty()
    );
    command.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), 1);
    parent.invoke_apply();
    assert!(bound.options.borrow().is_some());
    sets.invoke_apply();
    assert!(
        store
            .read(hydrus_store::settings::get::<Settings>)
            .unwrap()
            .sets["main_gui"]
            .is_empty()
    );
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    assert!(!saved.merge_numpad);
    assert!(saved.primary_labels);
    assert_eq!(
        saved.sets["main_gui"][0].gesture,
        Gesture::new(0, 113, 0, 5)
    );
    ui.invoke_flip_synchronised();
    ui.invoke_search_edited("system:archive".into());
    ui.invoke_search_accepted();
    assert_eq!(bound.current.borrow().results(), before);
    assert!(ui.invoke_shortcut_key("q".into(), 5));
    assert!(bound.current.borrow().synchronised());
    assert!(bound.current.borrow().results().len() < before.len());
    let parent = options(&ui, &bound);
    parent.invoke_shortcuts_clicked();
    let reopened = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert_eq!(reopened.get_rows().row_count(), 1);
    reopened.invoke_selected_row(0);
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
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    let (sets, child) = add(&parent, 0);
    child.invoke_key_capture("x".into(), 1);
    child.invoke_cancel();
    child.invoke_key_capture("y".into(), 1);
    child.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), 0);
    sets.invoke_action("add".into());
    let next = hydrus_gui::shortcut_windows::last_command().unwrap();
    next.invoke_key_capture("a".into(), 0);
    next.invoke_apply();
    assert_eq!(sets.get_rows().row_count(), 1);
    sets.invoke_cancel();
    sets.invoke_apply();
    assert!(!parent.get_shortcuts_child_open());
    let (successor, child) = add(&parent, 0);
    assert_eq!(successor.get_rows().row_count(), 0);
    child.invoke_key_capture("b".into(), 0);
    parent.invoke_cancel();
    child.invoke_apply();
    successor.invoke_apply();
    parent.invoke_shortcuts_clicked();
    let fresh = options(&ui, &bound);
    fresh.invoke_apply();
    assert!(
        store
            .read(hydrus_store::settings::get::<Settings>)
            .unwrap()
            .sets
            .values()
            .all(Vec::is_empty)
    );
}
#[test]
fn saved_mouse_and_keyboard_captures_feed_existing_viewer_navigation_and_guard_closed_owners() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert!(bound.current.borrow().results().len() > 1);
    let parent = options(&ui, &bound);
    let (sets, mouse) = add(&parent, 1);
    mouse.invoke_mouse_capture(1, 0, 1);
    assert_eq!(mouse.get_mode(), 1);
    mouse.set_command_index(1);
    mouse.invoke_apply();
    sets.invoke_action("add".into());
    let key = hydrus_gui::shortcut_windows::last_command().unwrap();
    key.invoke_key_capture("v".into(), 0);
    key.set_command_index(2);
    key.invoke_apply();
    sets.invoke_action("add".into());
    let wheel = hydrus_gui::shortcut_windows::last_command().unwrap();
    wheel.invoke_wheel_capture(-120.0, 0);
    assert!(!wheel.get_release_enabled());
    wheel.set_command_index(1);
    wheel.invoke_apply();
    sets.invoke_apply();
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    assert_eq!(saved.sets["media_viewer"].len(), 3);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let count = bound.current.borrow().results().len();
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
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let parent = options(&ui, &bound);
    let (sets, command) = add(&parent, 0);
    // Apply the untouched reference F7/default refresh command.
    command.invoke_apply();
    sets.invoke_apply();
    parent.invoke_apply();
    let saved = store.read(hydrus_store::settings::get::<Settings>).unwrap();
    assert_eq!(saved.sets["main_gui"][0].gesture, Gesture::default());
    assert_eq!(saved.sets["main_gui"][0].action, 78);
    store
        .write(|ctx| {
            let mut settings: hydrus_store::settings::GuiSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.confirm_exit = true;
            hydrus_store::settings::set(ctx.conn(), &settings)
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
    assert!(!bound.current.borrow().synchronised());
    assert!(ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(bound.current.borrow().synchronised());
    ui.invoke_flip_synchronised();
    assert!(!bound.current.borrow().synchronised());
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(ui.window().is_visible());
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    ui.show().unwrap();
    assert!(!ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(!bound.current.borrow().synchronised());
    assert_eq!(
        store.read(hydrus_store::settings::get::<Settings>).unwrap(),
        saved
    );
    let reopened = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_flip_synchronised();
    assert!(!reopened.current.borrow().synchronised());
    assert!(ui.invoke_shortcut_key(slint::platform::Key::F7.into(), 0));
    assert!(reopened.current.borrow().synchronised());
    assert!(!bound.current.borrow().synchronised());
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
