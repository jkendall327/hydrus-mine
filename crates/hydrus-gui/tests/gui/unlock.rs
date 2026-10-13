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
// leaf: audit-options-unlock-cancel
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

    // cancelling closes the window and opens nothing
    let opened = Rc::new(Cell::new(false));
    let window = unlock_window(lock, {
        let opened = opened.clone();
        move || opened.set(true)
    })
    .unwrap();
    window.show().unwrap();
    window.invoke_cancelled();
    assert!(!window.window().is_visible());
    assert!(!opened.get());

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
