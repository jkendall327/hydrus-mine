//! The lock password: asked for before the client opens, again after a
//! wrong one, and cancelling opens nothing. Replays
//! `oracle/fixtures/text_entry_checks.json` (`oracle/record_text_entry_checks.py`):
//! the reference's password dialog warns about a blank entry and stays open,
//! and its boot asks again, saying nothing, after a wrong password.

use std::cell::Cell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use hydrus_core::lock::LockPassword;
use hydrus_gui::{headless, unlock_window};

use crate::common::widgets;

// leaf: audit-options-unlock-password
#[test]
fn only_the_password_unlocks_the_client() {
    let recorded = hydrus_testkit::fixture_json("text_entry_checks.json");
    let windows = headless::init();
    let lock = LockPassword::new("hunter2");

    let opened = Rc::new(Cell::new(false));
    let window = unlock_window(lock.clone(), {
        let opened = opened.clone();
        move || opened.set(true)
    })
    .unwrap();
    window.show().unwrap();
    let pixels = headless::render(&windows.get(0).unwrap(), 360, 140);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("unlock.png"), &pixels, 360, 140).unwrap();
    let win = window.window();
    widgets::lay_out(win, 360.0, 140.0);

    // the reference's password dialog: blank warns and stays open
    let blank = recorded["dialogs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["dialog"] == "password" && c["typed"] == "")
        .unwrap();
    assert!(blank["still_open"].as_bool().unwrap());
    let warning = blank["shown"][0]["args"][0].as_str().unwrap();
    assert!(widgets::shows(win, "Enter your password."));
    assert!(!widgets::shows(win, warning));
    widgets::click(win, "ok");
    assert!(widgets::shows(win, warning));
    assert!(!opened.get());
    assert!(window.window().is_visible());

    // only whitespace is accepted by the reference's dialog (no warning): it
    // is checked as a password, here a wrong one, and asked for again
    let spaces = recorded["dialogs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["dialog"] == "password" && c["typed"] == "  ")
        .unwrap();
    assert!(spaces["accepted"].as_bool().unwrap());
    assert!(spaces["shown"].as_array().unwrap().is_empty());
    window.invoke_entered(spaces["value"].as_str().unwrap().into());
    assert!(!widgets::shows(win, warning));
    assert!(!opened.get());
    assert!(window.window().is_visible(), "asked again");

    // the recorded boot: each answer, and the same question asked again
    // after a wrong one with nothing else said
    let events = recorded["password_gate"]["events"].as_array().unwrap();
    let (last, wrong) = events.split_last().unwrap();
    for event in wrong {
        assert_eq!(event["asked"], "Enter your password.");
        window.invoke_entered(event["answered"].as_str().unwrap().into());
        assert!(!opened.get());
        assert!(window.window().is_visible(), "asked again");
        assert!(widgets::shows(win, event["asked"].as_str().unwrap()));
        assert!(!widgets::shows(win, warning), "nothing said");
    }
    assert!(recorded["password_gate"]["booted"].as_bool().unwrap());
    window.invoke_entered(last["answered"].as_str().unwrap().into());
    assert!(opened.get());
    assert!(!window.window().is_visible());

    // and a password of only whitespace is a password
    let opened = Rc::new(Cell::new(false));
    let window = unlock_window(LockPassword::new(spaces["value"].as_str().unwrap()), {
        let opened = opened.clone();
        move || opened.set(true)
    })
    .unwrap();
    window.show().unwrap();
    window.invoke_entered(spaces["value"].as_str().unwrap().into());
    assert!(opened.get());
}

// leaf: audit-options-unlock-cancel
#[test]
fn cancelling_the_password_dialog_opens_nothing_and_the_process_says_why() {
    // the reference's boot with its real password dialog's cancel button
    // clicked (`oracle/record_text_entry_checks.py`): the same question and
    // arguments as the password gate asks, `CancelledException` out of the
    // dialog, `DBCredentialsException` out of the boot, nothing else shown,
    // and the client never booted
    let recorded = hydrus_testkit::fixture_json("text_entry_checks.json");
    let cancel = &recorded["password_cancel"];
    assert_eq!(cancel["booted"], false);
    assert_eq!(cancel["boot_raised"], "DBCredentialsException");
    let events = cancel["events"].as_array().unwrap();
    assert_eq!(events.len(), 1, "asked once and nothing said");
    assert_eq!(events[0]["raised"], "CancelledException");
    let gate = &recorded["password_gate"]["events"][0];
    assert_eq!(events[0]["asked"], gate["asked"]);
    assert_eq!(events[0]["kwargs"], gate["kwargs"]);
    let windows = headless::init();
    let opened = Rc::new(Cell::new(false));
    let window = unlock_window(LockPassword::new("hunter2"), {
        let opened = opened.clone();
        move || opened.set(true)
    })
    .unwrap();
    window.show().unwrap();
    headless::render(&windows.get(0).unwrap(), 360, 140);
    let win = window.window();
    widgets::lay_out(win, 360.0, 140.0);
    assert!(widgets::shows(win, events[0]["asked"].as_str().unwrap()));
    // (blank not allowed, as `allow_blank: False`: it warns, the window stays)
    assert_eq!(events[0]["kwargs"]["allow_blank"], false);
    widgets::click(win, "ok");
    assert!(window.window().is_visible());
    // the dialog's cancel button, pressed; and its window gone
    widgets::click(win, "cancel");
    assert!(!window.window().is_visible());
    assert!(!opened.get(), "the client does not open");

    // and then the process: the running marker is cleared (so the next start
    // is not taken for a crash) and it ends saying the client is locked
    let dir = tempfile::tempdir().unwrap();
    let run = hydrus_gui::session_startup::Run::begin(dir.path()).unwrap();
    let error = hydrus_gui::finish_locked(run).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the client is locked, and its password wasn't entered"
    );
    assert!(
        !hydrus_gui::session_startup::Run::begin(dir.path())
            .unwrap()
            .bad(),
        "the marker was cleared"
    );
}
