//! Actual window-state contract, staged preference and independently live other keys.
use hydrus_gui_model::{
    options::{Editor, Row, Settings},
    popup_freeze::can_alter,
};
use hydrus_store::{
    Store,
    popup_freeze::{self, Preferences},
    settings,
};
const LABEL: &str = "Freeze the popup toaster when the main gui is minimised: ";
fn editor(store: &Store) -> (Editor, usize) {
    let mut e = Editor::new(store.read(Settings::load).unwrap());
    let page = e
        .page_names()
        .iter()
        .position(|p| *p == "popup notifications")
        .unwrap();
    e.show_page(page);
    let row = e
        .rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==LABEL))
        .unwrap();
    (e, row)
}
#[test]
fn real_qt_hidden_and_minimized_states_do_not_use_focus_and_unknown_is_explicit() {
    let fixture = hydrus_testkit::fixture_json("popup_freeze.json");
    for state in fixture["states"].as_array().unwrap() {
        assert_eq!(
            can_alter(
                state["hidden"].as_bool().unwrap(),
                Some(state["minimized"].as_bool().unwrap()),
                state["policy"].as_bool().unwrap()
            ),
            state["ok_to_alter"].as_bool().unwrap(),
            "{state}"
        );
    }
    assert!(
        can_alter(false, None, true),
        "Wayland unavailable state is not fabricated as focus/minimization"
    );
    assert!(
        !can_alter(true, None, false),
        "hidden is unconditional even when platform minimization is unavailable"
    );
    let states = fixture["states"].as_array().unwrap();
    let expired = states
        .iter()
        .find(|s| s["name"] == "expired_job_cached_while_frozen")
        .unwrap();
    assert!(expired["expiring_dismissed"].as_bool().unwrap());
    assert_eq!(expired["cards"][1]["text"], "expires while frozen");
    let restored = states
        .iter()
        .find(|s| s["name"] == "restore_reconciles_expired_and_pending")
        .unwrap();
    assert_eq!(
        restored["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["working changed while frozen", "queued while frozen"]
    );
}
#[test]
fn staged_cancel_save_reopen_and_concurrent_popup_fields_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("popup_freeze.json");
    assert_eq!(
        store.read(popup_freeze::load).unwrap().minimized,
        fixture["default"].as_bool().unwrap()
    );
    let (mut cancelled, row) = editor(&store);
    cancelled.check(row, true);
    drop(cancelled);
    assert!(!store.read(popup_freeze::load).unwrap().minimized);
    let (mut e, row) = editor(&store);
    e.check(row, true);
    store
        .write(|c| {
            settings::set(
                c.conn(),
                &hydrus_store::popup_width::PopupWidth {
                    characters: 32,
                    fixed: true,
                },
            )?;
            settings::set(
                c.conn(),
                &hydrus_store::api_update_toasts::Preferences { enabled: true },
            )
        })
        .unwrap();
    let (after, before, errors) = e.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store.write(move |c| after.save(c.conn(), &before)).unwrap();
    assert!(
        Store::open(dir.path())
            .unwrap()
            .read(popup_freeze::load)
            .unwrap()
            .minimized
    );
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::popup_width::PopupWidth>)
            .unwrap(),
        hydrus_store::popup_width::PopupWidth {
            characters: 32,
            fixed: true
        }
    );
    assert!(
        store
            .read(hydrus_store::api_update_toasts::load)
            .unwrap()
            .enabled
    );
    let (e, _) = editor(&store);
    store
        .write(|c| settings::set(c.conn(), &Preferences { minimized: false }))
        .unwrap();
    let (after, before, errors) = e.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store.write(move |c| after.save(c.conn(), &before)).unwrap();
    assert!(
        !store.read(popup_freeze::load).unwrap().minimized,
        "unchanged draft must preserve concurrent policy change"
    );
}
