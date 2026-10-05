//! Actual Options, owned Manage Notes input/cog and existing viewer clipboard.
use hydrus_gui::{
    Bound, MainWindow, ManageNotesWindow, OptionsWindow, Pages, SearchPage, bind, headless,
};
use hydrus_store::{
    Store,
    settings::{self, NotePreferences},
};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn values(preferences: &NotePreferences) -> serde_json::Value {
    serde_json::json!([
        preferences.copy_all,
        preferences.copy_json,
        preferences.start_at_end,
        preferences.hover_text_only
    ])
}
fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "notes")
        .unwrap();
    window.set_page(i32::try_from(page).unwrap());
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window
}
fn edit_options(window: &OptionsWindow, fixture: &serde_json::Value, values: &serde_json::Value) {
    for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
        let row = window
            .get_rows()
            .iter()
            .position(|row| row.label == label.as_str().unwrap())
            .unwrap();
        window.invoke_check_toggled(
            i32::try_from(row).unwrap(),
            values[index].as_bool().unwrap(),
        );
    }
}
fn open(ui: &MainWindow, bound: &Bound, index: i32) -> ManageNotesWindow {
    ui.invoke_thumbnail_clicked(index, false, false);
    ui.invoke_thumbnail_menu_requested(index);
    let item = ui
        .get_thumbnail_menu()
        .manage
        .iter()
        .find(|item| item.label.starts_with("notes"))
        .unwrap();
    ui.invoke_menu_chosen(item.id);
    bound.manage_notes.borrow().as_ref().unwrap().clone_strong()
}
fn select(window: &ManageNotesWindow, name: &str) {
    let index = window
        .get_names()
        .iter()
        .position(|item| item == name)
        .unwrap();
    window.invoke_tab_chosen(i32::try_from(index).unwrap());
}
fn cog(window: &ManageNotesWindow, index: usize, desired: bool) {
    window.invoke_cog_opened();
    if window.get_cog_checks().row_data(index).unwrap() != desired {
        window.invoke_cog_chosen(i32::try_from(index).unwrap());
    }
}
fn byte_offset(text: &str, qt_offset: u64) -> i32 {
    let mut utf16 = 0;
    for (byte, character) in text.char_indices() {
        if utf16 == qt_offset {
            return i32::try_from(byte).unwrap();
        }
        utf16 += character.len_utf16() as u64;
    }
    assert_eq!(utf16, qt_offset);
    i32::try_from(text.len()).unwrap()
}
fn cancel(window: &ManageNotesWindow) {
    window.invoke_cancel();
    if window.get_asking() {
        window.invoke_chosen(0);
    }
}

#[test]
fn options_cog_cursor_copy_and_live_hover_replay_with_owned_cancel_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("notes_preferences.json");
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let file = files[0];
    let original: BTreeMap<String, String> =
        serde_json::from_value(fixture["notes"].clone()).unwrap();
    let written = original.clone();
    store
        .write_content(move |writer| {
            for (name, text) in &written {
                writer.set_note(file, name, text)?;
            }
            Ok(())
        })
        .unwrap();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let pasted = fixture["future_paste"].as_str().unwrap().to_owned();
    hydrus_gui::set_paster(move || pasted.clone());
    assert_eq!(
        values(&store.read(settings::get::<NotePreferences>).unwrap()),
        fixture["initial"]
    );
    for event in fixture["options"].as_array().unwrap() {
        let before: NotePreferences = store.read(settings::get).unwrap();
        let draft = options(&ui, &bound);
        edit_options(&draft, &fixture, &event["input"]);
        assert_eq!(
            store.read(settings::get::<NotePreferences>).unwrap(),
            before
        );
        draft.invoke_cancel();
        draft.invoke_apply();
        assert_eq!(
            store.read(settings::get::<NotePreferences>).unwrap(),
            before
        );
        let accepted = options(&ui, &bound);
        edit_options(&accepted, &fixture, &event["input"]);
        accepted.invoke_apply();
        let reopened = Store::open(store.dir()).unwrap();
        assert_eq!(
            values(&reopened.read(settings::get::<NotePreferences>).unwrap()),
            event["round_trip"]
        );
        let shown = open(&ui, &bound, 0);
        let expected = if event["input"][0].as_bool().unwrap() {
            shown.get_text().len()
        } else {
            0
        };
        assert_eq!(shown.get_cursor(), i32::try_from(expected).unwrap());
        cancel(&shown);
    }
    let dialog = open(&ui, &bound, 0);
    // Replay actual cog mouse routes after another owner changes saved options.
    // Right-click opens neither an embedded nor a separate popup; left-click reads
    // the current store before showing the menu, rather than its opening snapshot.
    // CI's former whole-frame comparison differed only in the cog's hover/focus
    // paint (35 x 32 pixels), so inspect Slint's actual popup lifecycle instead.
    let drawn = windows.get(windows.count() - 1).unwrap();
    let _ = headless::render(&drawn, 640, 420);
    for event in fixture["cog_openings"].as_array().unwrap() {
        let start_at_end = event["preferences"][2].as_bool().unwrap();
        store
            .write(move |writer| {
                let mut preferences: NotePreferences = settings::get(writer.conn())?;
                preferences.start_at_end = start_at_end;
                settings::set(writer.conn(), &preferences)
            })
            .unwrap();
        let position =
            slint::LogicalPosition::new(dialog.get_cog_x() + 16.0, dialog.get_cog_y() + 14.0);
        dialog
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        let _ = headless::render(&drawn, 640, 420);
        let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(dialog.window());
        assert!(
            inner.active_popups().is_empty(),
            "previous cog popup is closed"
        );
        let before_windows = windows.count();
        let button = match event["button"].as_str().unwrap() {
            "right" => PointerEventButton::Right,
            "left" => PointerEventButton::Left,
            other => panic!("unrecorded cog button: {other}"),
        };
        dialog
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        if button == PointerEventButton::Right {
            assert!(
                inner.active_popups().is_empty(),
                "right-button down opens no popup"
            );
        }
        dialog
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        if event["menus"].as_array().unwrap().is_empty() {
            assert_eq!(
                windows.count(),
                before_windows,
                "no right-click popup window"
            );
            assert!(
                inner.active_popups().is_empty(),
                "right-button release opens no embedded popup"
            );
        } else {
            assert!(
                !inner.active_popups().is_empty(),
                "left-button activation opens the actual cog popup"
            );
            assert_eq!(
                serde_json::json!(dialog.get_cog_checks().iter().collect::<Vec<_>>()),
                event["menus"][0]
            );
            dialog.window().dispatch_event(WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });
            dialog.window().dispatch_event(WindowEvent::KeyReleased {
                text: slint::platform::Key::Escape.into(),
            });
            assert!(
                inner.active_popups().is_empty(),
                "Escape closes the cog popup"
            );
        }
        assert_eq!(
            values(&store.read(settings::get::<NotePreferences>).unwrap()),
            event["preferences"]
        );
    }
    store
        .write(|writer| {
            let mut preferences: NotePreferences = settings::get(writer.conn())?;
            preferences.start_at_end = true;
            settings::set(writer.conn(), &preferences)
        })
        .unwrap();
    dialog.invoke_cog_opened();
    for recorded in fixture["initial_cursors"].as_array().unwrap() {
        select(&dialog, recorded["name"].as_str().unwrap());
        assert_eq!(dialog.get_text(), recorded["text"].as_str().unwrap());
        assert_eq!(
            dialog.get_cursor(),
            byte_offset(
                recorded["text"].as_str().unwrap(),
                recorded["cursor"].as_u64().unwrap()
            )
        );
    }
    select(&dialog, "note2");
    dialog.invoke_select_text(1, 1);
    select(&dialog, "note10");
    select(&dialog, "note2");
    assert_eq!(dialog.get_cursor(), 1);
    cog(&dialog, 2, false);
    assert_eq!(dialog.get_cursor(), 1, "cog changes future controls only");
    dialog.invoke_paste();
    assert_eq!(
        dialog.get_notice(),
        fixture["paste_notice"].as_str().unwrap()
    );
    assert_eq!(
        dialog.get_text(),
        fixture["future_cursor"]["text"].as_str().unwrap()
    );
    assert_eq!(dialog.get_cursor(), 0);
    let _ = headless::render(&drawn, 640, 420);
    dialog.invoke_focus_note();
    dialog
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: "X".into() });
    dialog
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: "X".into() });
    assert_eq!(
        dialog.get_text(),
        fixture["inserted"]["text"].as_str().unwrap()
    );
    assert_eq!(dialog.get_cursor(), 1);
    for event in fixture["copies"].as_array().unwrap() {
        select(&dialog, event["current"].as_str().unwrap());
        cog(&dialog, 0, event["all"].as_bool().unwrap());
        cog(&dialog, 1, event["json"].as_bool().unwrap());
        let before = copied.borrow().len();
        dialog.invoke_copy();
        assert_eq!(
            serde_json::json!(&copied.borrow()[before..]),
            event["clipboard"]
        );
        assert_eq!(dialog.get_notice(), event["notice"][0].as_str().unwrap());
    }
    cog(&dialog, 3, true);
    // A pending rename blocks global writes and parent Apply.
    dialog.invoke_rename();
    let before: NotePreferences = store.read(settings::get).unwrap();
    dialog.invoke_cog_chosen(0);
    dialog.invoke_apply();
    assert_eq!(
        store.read(settings::get::<NotePreferences>).unwrap(),
        before
    );
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::notes(conn, file))
            .unwrap(),
        original
    );
    dialog.invoke_cancelled();
    // An alternate target cannot orphan the live draft or redirect its file.
    let same = open(&ui, &bound, 1);
    assert_eq!(
        same.get_names().iter().collect::<Vec<_>>(),
        dialog.get_names().iter().collect::<Vec<_>>()
    );
    cancel(&dialog);
    assert!(bound.manage_notes.borrow().is_none());
    assert_eq!(
        values(&store.read(settings::get::<NotePreferences>).unwrap()),
        fixture["after_cancel"]
    );
    let successor = open(&ui, &bound, 1);
    dialog.invoke_cog_chosen(0);
    dialog.invoke_apply();
    dialog.invoke_cancel();
    assert!(
        bound.manage_notes.borrow().is_some(),
        "retired owner cannot cancel successor"
    );
    assert_eq!(
        store.read(settings::get::<NotePreferences>).unwrap(),
        before
    );
    cancel(&successor);
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(|conn| hydrus_store::media::notes(conn, file))
            .unwrap(),
        original
    );
    let reopened_notes = open(&ui, &bound, 0);
    assert_eq!(reopened_notes.get_cursor(), 0);
    cancel(&reopened_notes);

    // The same already-open viewer reads a new preference at each middle press.
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    let _ = headless::render(&drawn, 800, 600);
    let mut found = None;
    for y in (viewer.get_notes_hover_y() as i32 + 6
        ..(viewer.get_notes_hover_y() + viewer.get_notes_hover_height()) as i32)
        .step_by(4)
    {
        let position = slint::LogicalPosition::new(720.0, y as f32);
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        let before = copied.borrow().len();
        viewer.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Middle,
        });
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Middle,
            });
        if copied
            .borrow()
            .get(before)
            .is_some_and(|text| text == fixture["hover"][1]["clipboard"][0].as_str().unwrap())
        {
            found = Some(position);
            break;
        }
    }
    let position = found.expect("actual note2 hover TouchArea copies the body");
    assert!(
        viewer.get_notes_showing(),
        "child input retains the visible note hover"
    );
    let accepted = options(&ui, &bound);
    edit_options(&accepted, &fixture, &serde_json::json!([false, false]));
    accepted.invoke_apply();
    let before = copied.borrow().len();
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Middle,
    });
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Middle,
        });
    assert_eq!(
        serde_json::json!(&copied.borrow()[before..]),
        fixture["hover"][0]["clipboard"]
    );
    let pixels = headless::render(&drawn, 800, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("note-hover-copy.png"),
        &pixels,
        800,
        600,
    )
    .unwrap();
    viewer.invoke_close_requested();
    let before = copied.borrow().len();
    viewer.invoke_note_copy_requested(2);
    assert_eq!(
        copied.borrow().len(),
        before,
        "hidden retired viewer cannot copy"
    );
    assert_eq!(
        Store::open(store.dir())
            .unwrap()
            .read(|conn| hydrus_store::media::notes(conn, file))
            .unwrap(),
        original
    );
    ui.hide().unwrap();
}
