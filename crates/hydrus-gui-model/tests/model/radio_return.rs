//! Actual GUI checkbox staging and independent concurrent setting saves.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    radio_return::{self, RadioReturn},
    settings,
};
const LABEL: &str = "Force that hitting Enter/Return on radio button lists triggers a dialog ok: ";
fn draft(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let gui = editor
        .page_names()
        .iter()
        .position(|name| *name == "gui")
        .unwrap();
    editor.show_page(gui);
    let rows = editor.rows();
    let row = rows
        .iter()
        .position(|row| matches!(row,Row::Opt{option,..} if option.label==LABEL))
        .unwrap();
    let iso = rows
        .iter()
        .position(
            |row| matches!(row,Row::Opt{option,..} if option.label.starts_with("Prefer ISO time")),
        )
        .unwrap();
    assert_eq!(row, iso + 1, "recorded GUI/misc checkbox order");
    (editor, row)
}
#[test]
fn cancelled_staging_changed_save_and_unchanged_concurrent_apply_match_actual_options() {
    let fixture = hydrus_testkit::fixture_json("radio_return.json");
    let source = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        store.read(radio_return::load).unwrap().force_dialog_ok,
        fixture["loaded"].as_bool().unwrap()
    );
    let (mut cancelled, row) = draft(&store);
    cancelled.check(row, false);
    assert!(store.read(radio_return::load).unwrap().force_dialog_ok);
    drop(cancelled);
    assert!(store.read(radio_return::load).unwrap().force_dialog_ok);
    let (mut accepted, row) = draft(&store);
    accepted.check(row, false);
    store
        .write(|c| {
            let mut gui: settings::GuiSettings = settings::get(c.conn())?;
            gui.application_display_name = "Concurrent identity".into();
            settings::set(c.conn(), &gui)
        })
        .unwrap();
    let (after, before, errors) = accepted.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store.write(move |c| after.save(c.conn(), &before)).unwrap();
    assert!(!store.read(radio_return::load).unwrap().force_dialog_ok);
    assert_eq!(
        store
            .read(settings::get::<settings::GuiSettings>)
            .unwrap()
            .application_display_name,
        "Concurrent identity"
    );
    let (unchanged, _) = draft(&store);
    store
        .write(|c| settings::set(c.conn(), &RadioReturn::default()))
        .unwrap();
    let (after, before, errors) = unchanged.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store.write(move |c| after.save(c.conn(), &before)).unwrap();
    assert!(
        store.read(radio_return::load).unwrap().force_dialog_ok,
        "unchanged dialog must preserve a newer saved radio preference"
    );
    assert!(
        Store::open(directory.path())
            .unwrap()
            .read(radio_return::load)
            .unwrap()
            .force_dialog_ok
    );
}
