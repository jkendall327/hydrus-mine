//! Actual tree cursor and disclosure events do not activate notebook pages.
#[path = "../../../hydrus-gui-model/tests/support/notebook_tree.rs"]
mod support;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{
    Store, sessions,
    settings::{self, TabAlignment, TabPresentationSettings},
};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
fn click(native: &slint::platform::software_renderer::MinimalSoftwareWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    native.dispatch_event(WindowEvent::PointerMoved { position });
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}
fn press(native: &slint::platform::software_renderer::MinimalSoftwareWindow, key: Key) {
    native.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    native.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}
#[test]
fn real_tree_mouse_and_keys_replay_cursor_activation_and_preserved_child_expansion() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = support::source();
    let saved = session.clone();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 1)?;
            settings::set(
                ctx.conn(),
                &TabPresentationSettings {
                    tree_alignment: Some(TabAlignment::Left),
                    hide_navigation_tabs: true,
                    ..TabPresentationSettings::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    ui.invoke_page_tree_chosen(support::key(&session, "gamma").to_hex().into());
    let native = windows.get(0).unwrap();
    headless::render(&native, 900, 600);
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    ui.invoke_focus_page_tree();
    assert!(ui.get_page_tree_focused());
    let fixture = hydrus_testkit::fixture_json("notebook_tree.json");
    for step in fixture["steps"].as_array().unwrap() {
        let label = step["action"].as_str().unwrap();
        if let Some(action) = support::action(label) {
            let key = match action {
                0 => Some(Key::UpArrow),
                1 => Some(Key::DownArrow),
                2 => Some(Key::LeftArrow),
                3 => Some(Key::RightArrow),
                4 => Some(Key::Home),
                5 => Some(Key::End),
                6 => Some(Key::Return),
                _ => None,
            };
            if let Some(key) = key {
                ui.invoke_focus_page_tree();
                press(&native, key);
            } else {
                ui.invoke_page_tree_navigate(action);
            }
        } else {
            match label {
                "click beta" => {
                    let rows = ui.get_page_tree();
                    let index = (0..rows.row_count())
                        .find(|&i| rows.row_data(i).unwrap().name == "beta")
                        .unwrap();
                    click(
                        &native,
                        ui.get_page_tree_x() + 110.0,
                        ui.get_page_tree_y() + 28.0 + 26.0 * index as f32 + 13.0,
                    );
                }
                "select inner" => {
                    ui.invoke_page_tree_selected(support::key(&session, "inner").to_hex().into());
                }
                "collapse alpha" => {
                    ui.invoke_page_tree_selected(support::key(&session, "alpha").to_hex().into());
                    ui.invoke_focus_page_tree();
                    press(&native, Key::LeftArrow);
                }
                "expand inner" => {
                    ui.invoke_page_tree_toggled(support::key(&session, "inner").to_hex().into());
                }
                "click inner disclosure" => {
                    let rows = ui.get_page_tree();
                    let index = (0..rows.row_count())
                        .find(|&i| rows.row_data(i).unwrap().name == "inner")
                        .unwrap();
                    click(
                        &native,
                        ui.get_page_tree_x() + 30.0,
                        ui.get_page_tree_y() + 28.0 + 26.0 * index as f32 + 13.0,
                    );
                }
                "collapse parent retains inner expansion" | "expand parent" => {
                    ui.invoke_page_tree_toggled(support::key(&session, "alpha").to_hex().into());
                }
                "double click beta" => {
                    ui.invoke_page_tree_selected(support::key(&session, "beta").to_hex().into());
                    ui.invoke_page_tree_chosen(support::key(&session, "beta").to_hex().into());
                }
                "show gamma reveals ancestors" => {
                    ui.invoke_page_tree_chosen(support::key(&session, "gamma").to_hex().into());
                }
                "double click inner" => {
                    ui.invoke_page_tree_selected(support::key(&session, "inner").to_hex().into());
                    ui.invoke_page_tree_toggled(support::key(&session, "inner").to_hex().into());
                    ui.invoke_page_tree_chosen(support::key(&session, "inner").to_hex().into());
                }
                "initial" => {}
                _ => panic!("unknown action {label}"),
            }
        }
        headless::render(&native, 900, 600);
        let rows = ui.get_page_tree();
        assert_eq!(
            serde_json::json!(
                (0..rows.row_count())
                    .map(|i| rows.row_data(i).unwrap().name.to_string())
                    .collect::<Vec<_>>()
            ),
            step["visible"],
            "{label}"
        );
        let cursor = (0..rows.row_count()).find_map(|i| {
            let row = rows.row_data(i).unwrap();
            row.selected.then(|| row.name.to_string())
        });
        let expected_visible = step["visible"]
            .as_array()
            .unwrap()
            .contains(&step["current"]);
        assert_eq!(
            cursor,
            expected_visible.then(|| step["current"].as_str().unwrap().to_owned()),
            "{label}"
        );
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["shown"].as_str().unwrap(),
            "{label}"
        );
        assert_eq!(bound.pages.borrow().session(), &session, "{label}");
    }
    let before = bound.pages.borrow().shown().key;
    ui.invoke_page_tree_selected(hydrus_core::pages::PageKey::random().to_hex().into());
    ui.invoke_page_tree_toggled("invalid".into());
    assert_eq!(bound.pages.borrow().shown().key, before);
    let pixels = headless::render_snapshot(&native, 900, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("notebook_tree.png"),
        &pixels,
        900,
        600,
    )
    .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<TabPresentationSettings>)
            .unwrap()
            .tree_alignment,
        Some(TabAlignment::Left)
    );
}
