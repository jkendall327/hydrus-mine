//! Options controls for reference settings that hydrus-rs keeps and round-trips
//! without a native consumer (the Qt locale for integers, the media viewer's
//! rescue padding, the toaster's other-display freeze, the number of recent
//! petition reasons): each is where and what the reference's panel has it, starts
//! on the reference's default, is staged until OK, and survives reopening.
use crate::options_gui_support::{Client, box_of, row, show_page};

/// One check or number control: its page, box, label and default.
struct Control {
    page: &'static str,
    boxed: &'static str,
    label: &'static str,
}
const CHECKS: [Control; 3] = [
    Control {
        page: "gui",
        boxed: "misc",
        label: "TEST: Use your locale for integer rendering: ",
    },
    Control {
        page: "gui",
        boxed: "frame locations",
        label: "DEBUG: When rescuing resizing-to-media media viewer, add top-left safety padding:",
    },
    Control {
        page: "popup notifications",
        boxed: "popup window toaster",
        label: "Freeze the popup toaster when mouse is on another display: ",
    },
];

fn checked(client: &Client, control: &Control) -> bool {
    let options = client.open_options();
    show_page(&options, control.page);
    let value = row(&options, control.label).1.checked;
    options.invoke_cancel();
    value
}

// (not tagged: these settings are kept and staged as the reference does, but
// nothing in hydrus-rs reads them yet, so their leaves stay open)
#[test]
fn the_reference_s_unused_switches_are_checkboxes_off_by_default_staged_and_kept() {
    let client = Client::basic();
    for control in &CHECKS {
        let options = client.open_options();
        show_page(&options, control.page);
        let (at, shown) = row(&options, control.label);
        assert_eq!(shown.kind, 1, "{}: a checkbox", control.label);
        assert!(!shown.checked, "{}: off in the reference", control.label);
        assert_eq!(box_of(&options, control.label), control.boxed);
        // Cancel discards the change
        options.invoke_check_toggled(at, true);
        options.invoke_cancel();
        assert!(!checked(&client, control), "{}", control.label);
        // OK keeps it, on reopening and again after another OK of the page
        let options = client.open_options();
        show_page(&options, control.page);
        options.invoke_check_toggled(row(&options, control.label).0, true);
        options.invoke_apply();
        assert!(checked(&client, control), "{}", control.label);
    }
    // each is independent of the others
    for control in &CHECKS {
        assert!(checked(&client, control));
        let options = client.open_options();
        show_page(&options, control.page);
        options.invoke_check_toggled(row(&options, control.label).0, false);
        options.invoke_apply();
        assert!(!checked(&client, control));
        for other in CHECKS.iter().filter(|c| c.label != control.label) {
            assert!(checked(&client, other), "{}", other.label);
        }
        let options = client.open_options();
        show_page(&options, control.page);
        options.invoke_check_toggled(row(&options, control.label).0, true);
        options.invoke_apply();
    }
}

#[test]
fn the_number_of_recent_petition_reasons_is_a_spin_box_from_0_to_100_that_defaults_to_5() {
    let client = Client::basic();
    let label = "Number of recent petition reasons to remember in dialogs: ";
    let value = |client: &Client| {
        let options = client.open_options();
        show_page(&options, "tag editing");
        let number = row(&options, label).1.number;
        options.invoke_cancel();
        number
    };
    let options = client.open_options();
    show_page(&options, "tag editing");
    let (at, shown) = row(&options, label);
    assert_eq!(
        (shown.kind, shown.minimum, shown.maximum, shown.number),
        (2, 0, 100, 5)
    );
    assert_eq!(box_of(&options, label), "tag dialogs");
    options.invoke_number_edited(at, 12);
    options.invoke_cancel();
    assert_eq!(value(&client), 5, "staged until OK");
    for (typed, kept) in [(12, 12), (0, 0), (100, 100), (250, 100), (-3, 0)] {
        let options = client.open_options();
        show_page(&options, "tag editing");
        options.invoke_number_edited(row(&options, label).0, typed);
        options.invoke_apply();
        assert_eq!(value(&client), kept, "typed {typed}");
    }
}
