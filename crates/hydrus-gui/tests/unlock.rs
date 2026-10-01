//! The lock password: asked for before the client opens, again after a
//! wrong one, and cancelling opens nothing.

use std::cell::Cell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use hydrus_core::lock::LockPassword;
use hydrus_gui::{headless, unlock_window};

#[test]
fn only_the_password_unlocks_the_client() {
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

    window.invoke_entered("hunter".into());
    assert!(window.get_wrong());
    assert!(!opened.get());
    assert!(window.window().is_visible(), "asked again");
    window.invoke_entered("hunter2".into());
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
}
