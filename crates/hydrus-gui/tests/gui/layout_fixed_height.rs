//! Fixed-height controls (buttons, combo boxes, single-line inputs) keep their
//! height when a window is given far more room than its rows want: spare
//! height goes to lists or to nothing, never into the controls. Each window
//! the frame tests can open is sized well past its preferred size and its
//! item tree walked.

use std::ops::ControlFlow;

use i_slint_core::item_tree::{ItemRc, ParentItemTraversalMode};
use i_slint_core::items::AccessibleRole;
use i_slint_core::window::WindowInner;

use hydrus_gui::headless;

use crate::frame_geometry::{Client, windows};

/// The tallest a control may be: a std control is about 32 logical pixels.
const LIMIT: f32 = 48.0;

/// Controls of the window taller than `LIMIT`.
fn too_tall(window: &slint::Window) -> Vec<String> {
    let inner = WindowInner::from_pub(window);
    let mut found = Vec::new();
    ItemRc::new_root(inner.component()).visit_descendants(|item| {
        let role = item.accessible_role();
        let measured = match role {
            AccessibleRole::Button | AccessibleRole::Combobox => Some(item.clone()),
            AccessibleRole::TextInput => item.parent_item(ParentItemTraversalMode::StopAtPopups),
            _ => None,
        };
        if let Some(measured) = measured {
            let height = measured.geometry().height();
            if item.is_visible() && height > LIMIT {
                let label = item
                    .accessible_string_property(
                        i_slint_core::accessibility::AccessibleStringProperty::Label,
                    )
                    .unwrap_or_default();
                found.push(format!("{role:?} {label:?} is {height} tall"));
            }
        }
        ControlFlow::<()>::Continue(())
    });
    found
}

#[test]
fn no_button_combo_box_or_input_grows_with_spare_window_height() {
    let _windows = headless::init();
    let mut failures = Vec::new();
    for (key, open, _accept) in windows() {
        let client = Client::new();
        let shown = open(&client);
        let window = shown.win();
        // a window is 0 until laid out: lay it out, then at twice that
        crate::common::widgets::lay_out(window, 700.0, 500.0);
        crate::common::widgets::settle();
        let size = window.size().to_logical(window.scale_factor());
        window.set_size(slint::LogicalSize::new(size.width * 2.0, size.height * 2.0));
        crate::common::widgets::settle();
        for line in too_tall(window) {
            failures.push(format!("{key}: {line}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
