//! Replay real raw-name editor acceptance and independently saved activation.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    settings::{self, GuiSettings, TagSearchActivation},
};

fn gui(editor: &mut Editor) {
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "gui")
        .unwrap();
    editor.show_page(page);
}
fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Opt { option,.. } if option.label == label))
        .unwrap()
}

#[test]
fn constructor_cancel_unchanged_apply_and_reopen_match_raw_qt_names() {
    let fixture = hydrus_testkit::fixture_json("main_window_identity.json");
    for case in fixture["names"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let raw = case["raw"].as_str().unwrap().to_owned();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiSettings {
                        application_display_name: raw,
                        confirm_exit: false,
                    },
                )
            })
            .unwrap();
        let mut editor = Editor::new(store.read(Settings::load).unwrap());
        gui(&mut editor);
        let at = row(&editor, "Application display name: ");
        if let Some(text) = case["typed"].as_str() {
            editor.text(at, text);
        }
        if case["apply"].as_bool().unwrap() {
            let (after, before, problems) = editor.applied();
            assert!(problems.is_empty());
            let before = before.clone();
            store
                .write(move |ctx| after.save(ctx.conn(), &before))
                .unwrap();
        }
        drop(editor);
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .read(settings::get::<GuiSettings>)
                .unwrap()
                .application_display_name,
            case["reopened"].as_str().unwrap()
        );
    }
}

#[test]
fn implicit_name_normalisation_and_other_fields_preserve_concurrent_updates() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: String::new(),
                    confirm_exit: false,
                },
            )
        })
        .unwrap();
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    gui(&mut editor);
    editor.check(
        row(
            &editor,
            "Switch to main window when creating new file search page from media viewer: ",
        ),
        true,
    );
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: "external name".into(),
                    confirm_exit: true,
                },
            )
        })
        .unwrap();
    let (after, before, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(after.gui.application_display_name, "hydrus client");
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved: GuiSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.application_display_name, "external name");
    assert!(saved.confirm_exit);
    assert!(
        store
            .read(settings::get::<TagSearchActivation>)
            .unwrap()
            .activate_main
    );
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    gui(&mut editor);
    editor.text(row(&editor, "Application display name: "), "explicit name");
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: "newer name".into(),
                    confirm_exit: false,
                },
            )
        })
        .unwrap();
    let (after, before, _) = editor.applied();
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved: GuiSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.application_display_name, "explicit name");
    assert!(!saved.confirm_exit);
}
