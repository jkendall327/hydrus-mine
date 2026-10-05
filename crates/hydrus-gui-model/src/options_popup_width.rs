//! The two PopupPanel width controls, using the shared staged Options editor.
use super::{Page, boxed, check, int};

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
            ],
        )],
    }
}
