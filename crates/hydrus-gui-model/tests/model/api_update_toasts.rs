//! Staged real preference, retained import and unrelated concurrent popup policy.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    api_update_toasts::{self, Preferences},
    settings,
};
const LABEL: &str = "Make a short-lived popup on cookie/header updates through the Client API: ";
fn open_editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|p| *p == "popup notifications")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(|row| matches!(row,Row::Opt {option,..} if option.label==LABEL))
        .unwrap();
    (editor, row)
}
// leaf: audit-options-popup-notifications-popup-window-toaster-make-a-short-lived-popup-on-cookie-header-updates-through-the-client-api
#[test]
fn all_recorded_apply_cancel_reopen_cases_and_concurrent_width_survive() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("api_update_toasts.json");
    assert!(!store.read(api_update_toasts::load).unwrap().enabled);
    for case in fixture["controls"].as_array().unwrap() {
        let before = case["before"].as_bool().unwrap();
        store
            .write(move |ctx| settings::set(ctx.conn(), &Preferences { enabled: before }))
            .unwrap();
        let (mut editor, row) = open_editor(&store);
        editor.check(row, case["typed"].as_bool().unwrap());
        if case["apply"] == true {
            let (after, before, errors) = editor.applied();
            assert!(errors.is_empty());
            let before = before.clone();
            store
                .write(move |ctx| after.save(ctx.conn(), &before))
                .unwrap();
        }
        assert_eq!(
            Store::open(directory.path())
                .unwrap()
                .read(api_update_toasts::load)
                .unwrap()
                .enabled,
            case["reopened"].as_bool().unwrap()
        );
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
        .unwrap();
    let (mut editor, row) = open_editor(&store);
    editor.check(row, true);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::popup_width::PopupWidth {
                    characters: 32,
                    fixed: true,
                },
            )
        })
        .unwrap();
    let (after, before, errors) = editor.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert!(store.read(api_update_toasts::load).unwrap().enabled);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::popup_width::PopupWidth>)
            .unwrap(),
        hydrus_store::popup_width::PopupWidth {
            characters: 32,
            fixed: true
        }
    );
    let (editor, _) = open_editor(&store);
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { enabled: false }))
        .unwrap();
    let (after, before, errors) = editor.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert!(
        !store.read(api_update_toasts::load).unwrap().enabled,
        "untouched draft preserves concurrent saved notification change"
    );
}
