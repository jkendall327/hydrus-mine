//! The PopupPanel width and API notification controls, using the shared staged Options editor.
use super::{Page, boxed, check, int, kept_check};

pub(super) fn page() -> Page {
    Page {
        name: "popup notifications",
        items: vec![boxed(
            "popup window toaster",
            vec![
                int(
                    "Approximate max width of popup messages (in characters): ",
                    (16, 256),
                    |s| s.popup_width.characters,
                    |s, v| s.popup_width.characters = v,
                ),
                check(
                    "BUGFIX: Force this width as the fixed width for all popup messages: ",
                    |s| s.popup_width.fixed,
                    |s, v| s.popup_width.fixed = v,
                ),
                kept_check(
                    "Freeze the popup toaster when mouse is on another display: ",
                    "freeze_message_manager_when_mouse_on_other_monitor",
                ),
                check(
                    "Freeze the popup toaster when the main gui is minimised: ",
                    |s| s.popup_freeze.minimized,
                    |s, value| s.popup_freeze.minimized = value,
                ),
                check(
                    "Make a short-lived popup on cookie/header updates through the Client API: ",
                    |s| s.api_update_toasts.enabled,
                    |s, value| s.api_update_toasts.enabled = value,
                ),
            ],
        )],
    }
}
