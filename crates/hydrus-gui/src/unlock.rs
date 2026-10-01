//! The lock password, asked for before the client opens, as the reference's
//! `InitView` asks for it.

use std::cell::Cell;

use slint::ComponentHandle as _;

use hydrus_core::lock::LockPassword;

use crate::UnlockWindow;

/// A window asking for `lock`'s password. Once the right one is entered,
/// `unlocked` runs (opening the client, whose window should be shown before
/// this one closes, or the event loop ends with no windows) and the window
/// closes; a wrong one is said to be wrong and asked for again. Cancelling
/// closes the window without unlocking.
pub fn unlock_window(
    lock: LockPassword,
    unlocked: impl FnOnce() + 'static,
) -> Result<UnlockWindow, slint::PlatformError> {
    let window = UnlockWindow::new()?;
    let unlocked = Cell::new(Some(unlocked));
    let weak = window.as_weak();
    window.on_entered(move |password| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if lock.accepts(&password) {
            if let Some(unlocked) = unlocked.take() {
                unlocked();
            }
            let _ = window.hide();
        } else {
            window.set_wrong(true);
        }
    });
    let weak = window.as_weak();
    window.on_cancelled(move || {
        if let Some(window) = weak.upgrade() {
            let _ = window.hide();
        }
    });
    Ok(window)
}
