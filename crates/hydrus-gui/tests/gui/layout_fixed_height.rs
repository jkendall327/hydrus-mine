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
use slint::ComponentHandle as _;

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
            AccessibleRole::TextInput
                if item
                    .downcast::<i_slint_core::items::TextInput>()
                    .is_some_and(|input| input.as_pin_ref().single_line()) =>
            {
                item.parent_item(ParentItemTraversalMode::StopAtPopups)
            }
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

type Rect = (f32, f32, f32, f32);

/// The visible buttons and combo boxes of the window, as (what, label, rect
/// in the window).
fn controls(window: &slint::Window) -> Vec<(String, String, Rect)> {
    let inner = WindowInner::from_pub(window);
    let mut boxes = Vec::new();
    ItemRc::new_root(inner.component()).visit_descendants(|item| {
        if matches!(
            item.accessible_role(),
            AccessibleRole::Button | AccessibleRole::Combobox
        ) && item.is_visible()
        {
            let at = item.map_to_window(i_slint_core::lengths::LogicalPoint::zero());
            let size = item.geometry().size;
            let label = item
                .accessible_string_property(
                    i_slint_core::accessibility::AccessibleStringProperty::Label,
                )
                .unwrap_or_default();
            boxes.push((
                format!("{:?}", item.accessible_role()),
                label.to_string(),
                (at.x, at.y, at.x + size.width, at.y + size.height),
            ));
        }
        ControlFlow::<()>::Continue(())
    });
    boxes
}

/// Controls of the window that overlap another control without containing it.
fn overlapping(window: &slint::Window) -> Vec<String> {
    let boxes = controls(window);
    let mut found = Vec::new();
    for (i, (a_role, a_label, a)) in boxes.iter().enumerate() {
        for (b_role, b_label, b) in &boxes[i + 1..] {
            let (w, h) = (a.2.min(b.2) - a.0.max(b.0), a.3.min(b.3) - a.1.max(b.1));
            let contains = |o: &Rect, i: &Rect| {
                o.0 <= i.0 + 0.5 && o.1 <= i.1 + 0.5 && o.2 >= i.2 - 0.5 && o.3 >= i.3 - 0.5
            };
            if w > 1.0 && h > 1.0 && !contains(a, b) && !contains(b, a) {
                found.push(format!(
                    "{a_role} {a_label:?} {a:?} overlaps {b_role} {b_label:?} {b:?}"
                ));
            }
        }
    }
    found
}

/// Controls whose right edge is past `right` (clipped by their container).
fn past(window: &slint::Window, right: f32) -> Vec<String> {
    controls(window)
        .into_iter()
        .filter(|(_, _, rect)| rect.0 < right && rect.2 > right + 0.5)
        .map(|(role, label, rect)| format!("{role} {label:?} {rect:?} runs past {right}"))
        .collect()
}

#[test]
fn the_sort_row_and_the_controls_of_the_sidebar_do_not_overlap_at_any_sidebar_width() {
    let _windows = headless::init();
    let client = Client::new();
    let window = client.ui.window();
    crate::common::widgets::lay_out(window, 1400.0, 900.0);
    let mut failures = Vec::new();
    for width in [120.0, 200.0, 300.0, 400.0, 700.0] {
        client.ui.set_sidebar_requested_width(width);
        crate::common::widgets::settle();
        // (the sidebar starts at the window's left, inside its padding)
        let sidebar = client.ui.get_sidebar_actual_width();
        for line in overlapping(window)
            .into_iter()
            .chain(too_tall(window))
            .chain(past(window, sidebar))
        {
            failures.push(format!("sidebar {width} (laid out {sidebar}): {line}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
        for line in too_tall(window).into_iter().chain(overlapping(window)) {
            failures.push(format!("{key}: {line}"));
        }
        if key.starts_with("manage_tags") {
            // the tag input keeps its own height: the tab combo box is the
            // lowest combo box, and the row of buttons that closes the
            // dialog follows it closely; the tag list takes the spare height
            let all = controls(window);
            let combo_bottom = all
                .iter()
                .filter(|c| c.0 == "Combobox")
                .map(|c| c.2.3)
                .fold(0.0, f32::max);
            let below = all
                .iter()
                .filter(|c| c.0 == "Button" && ["apply", "close"].contains(&c.1.as_str()))
                .map(|c| c.2.1)
                .fold(f32::MAX, f32::min);
            assert!(
                below - combo_bottom < 40.0,
                "{key}: {} blank pixels between the input and the buttons below it",
                below - combo_bottom
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
