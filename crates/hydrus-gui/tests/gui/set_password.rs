//! The actual SetPassword owner and persisted Store, including retained callbacks.
use hydrus_core::lock::LockPassword;
use hydrus_gui::{SessionDialog, headless, set_password_window};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::sync::Arc;

fn stored(store: &Store) -> LockPassword {
    store.read(hydrus_store::settings::get).unwrap()
}

fn seed(store: &Store, lock: &LockPassword) {
    let lock = lock.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &lock))
        .unwrap();
}

fn dialog(store: &Arc<Store>, slot: &set_password_window::Slot) -> SessionDialog {
    set_password_window::open(store, slot).unwrap();
    let window = slot.borrow().as_ref().unwrap().clone_strong();
    assert!(window.window().is_visible());
    window
}

fn retained_attempt(window: &SessionDialog) {
    window.invoke_answered(true);
    window.invoke_name_entered("replacement".into());
    window.invoke_name_entered("replacement".into());
}

#[test]
fn actual_password_dialogs_persist_the_six_recorded_outcomes() {
    let _windows = headless::init();
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("set_password.json");
    let before = LockPassword {
        sha256: Some(fixture["before_hash"].as_str().unwrap().to_owned()),
    };
    let texts: [&[Option<&str>]; 6] = [
        &[None],
        &[Some("")],
        &[Some("")],
        &[Some("hunter2"), Some("hunter2")],
        &[Some("hunter2"), Some("hunter3")],
        &[Some("hunter2"), None],
    ];
    for (event, script) in fixture["events"].as_array().unwrap().iter().zip(texts) {
        seed(&store, &before);
        let slot = set_password_window::Slot::default();
        let window = dialog(&store, &slot);
        let mut text = script.iter();
        for ask in event["asked"].as_array().unwrap() {
            assert_eq!(
                window.get_message().as_str(),
                ask["message"].as_str().unwrap()
            );
            match ask["kind"].as_str().unwrap() {
                "text" => {
                    assert!(window.get_asking_name());
                    assert!(!window.get_notice_only());
                    assert_eq!(
                        window.get_window_title().as_str(),
                        ask["title"].as_str().unwrap()
                    );
                    match text.next().unwrap() {
                        Some(value) => window.invoke_name_entered((*value).into()),
                        None => window.invoke_cancelled(),
                    }
                }
                "yes_no" => {
                    assert!(!window.get_asking_name());
                    assert!(!window.get_notice_only());
                    assert_eq!(
                        window.get_yes_label().as_str(),
                        ask["yes_label"].as_str().unwrap()
                    );
                    assert_eq!(
                        window.get_no_label().as_str(),
                        ask["no_label"].as_str().unwrap()
                    );
                    window.invoke_answered(event["yes"].as_bool().unwrap());
                }
                "critical" => {
                    assert!(window.get_notice_only());
                    assert_eq!(
                        window.get_window_title().as_str(),
                        ask["title"].as_str().unwrap()
                    );
                    window.invoke_cancelled();
                }
                other => panic!("unexpected recorded question {other}"),
            }
        }
        assert!(!window.window().is_visible());
        let expected = event["stored"].as_str();
        assert_eq!(stored(&store).sha256.as_deref(), expected);
        // Every outcome survives opening a new Store instance.
        let reopened = Store::open(home.path()).unwrap();
        assert_eq!(stored(&reopened).sha256.as_deref(), expected);
        // Completed/cancelled owners cannot change that result if retained and shown.
        window.show().unwrap();
        retained_attempt(&window);
        assert_eq!(stored(&reopened).sha256.as_deref(), expected);
        window.hide().unwrap();
    }
}

#[test]
fn cancelled_and_closed_password_owners_cannot_clear_or_replace_the_lock() {
    let _windows = headless::init();
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let before = LockPassword::new("original");
    seed(&store, &before);
    let slot = set_password_window::Slot::default();
    let cancelled = dialog(&store, &slot);
    cancelled.invoke_name_entered("".into());
    cancelled.invoke_cancelled();
    retained_attempt(&cancelled);
    cancelled.show().unwrap();
    retained_attempt(&cancelled);
    assert_eq!(stored(&store), before);
    cancelled.hide().unwrap();

    let closed = dialog(&store, &slot);
    closed.invoke_name_entered("replacement".into());
    closed
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!closed.window().is_visible());
    closed.show().unwrap();
    retained_attempt(&closed);
    assert_eq!(stored(&store), before);
    closed.hide().unwrap();

    // An actual current clear confirmation remains effective.
    let current = dialog(&store, &slot);
    current.invoke_answered(true); // no clear question has been asked yet
    assert_eq!(stored(&store), before);
    current.invoke_name_entered("".into());
    current.invoke_name_entered("replacement".into()); // wrong callback for this step
    assert_eq!(
        current.get_message().as_str(),
        "Clear any existing password?"
    );
    current.invoke_answered(true);
    assert_eq!(stored(&store), LockPassword::default());
    assert!(!current.window().is_visible());
    assert_eq!(
        stored(&Store::open(home.path()).unwrap()),
        LockPassword::default()
    );
}

#[test]
fn hidden_replaced_and_dropped_password_slots_cannot_mutate_a_successor() {
    let _windows = headless::init();
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let before = LockPassword::new("original");
    seed(&store, &before);
    let slot = set_password_window::Slot::default();
    let old = dialog(&store, &slot);
    old.invoke_name_entered("".into());
    old.hide().unwrap();
    retained_attempt(&old);
    assert_eq!(stored(&store), before);

    let current = dialog(&store, &slot);
    old.show().unwrap();
    retained_attempt(&old);
    assert_eq!(
        current.get_message().as_str(),
        hydrus_gui_model::set_password::FIRST
    );
    assert_eq!(stored(&store), before);
    old.hide().unwrap();
    current.invoke_name_entered("successor".into());
    current.invoke_name_entered("successor".into());
    let saved = LockPassword::new("successor");
    assert_eq!(stored(&store), saved);
    retained_attempt(&old);
    retained_attempt(&current);
    assert_eq!(stored(&store), saved);

    let abandoned = dialog(&store, &slot);
    abandoned.invoke_name_entered("".into());
    drop(slot);
    retained_attempt(&abandoned);
    assert_eq!(stored(&store), saved);
    abandoned.hide().unwrap();
    assert_eq!(stored(&Store::open(home.path()).unwrap()), saved);
}
