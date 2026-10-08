//! The remembered/default frame editor of the gui page against the
//! reference's `EditFrameLocationPanel`: what its remember switches,
//! optional sizes and places, and start-maximised/fullscreen switches do to a
//! window as it next opens.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::windows::WindowSettings;
use hydrus_gui::FrameLocationWindow;

use crate::options_gui_support::{Client, show_page};

fn index(window: &hydrus_gui::OptionsWindow, name: &str) -> i32 {
    let rows = window.get_frame_rows();
    (0..rows.row_count())
        .position(|i| rows.row_data(i).unwrap().cells.row_data(0).unwrap() == name)
        .unwrap_or_else(|| panic!("frame {name:?}")) as i32
}

/// The media viewer's frame, edited by `change` in the editor child and
/// applied through the options window.
fn edit_media_viewer(client: &Client, change: &dyn Fn(&FrameLocationWindow)) {
    let options = client.open_options();
    show_page(&options, "gui");
    options.invoke_frame_clicked(index(&options, "media_viewer"), false, false);
    options.invoke_frame_action("edit".into());
    let child = client
        .bound
        .options_frame_child
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    assert_eq!(
        child.get_message(),
        "Setting frame location info for media_viewer."
    );
    change(&child);
    child.invoke_apply();
    options.invoke_apply();
}

/// A viewer opened now, and its window as it opened.
fn opened_viewer(client: &Client) -> ((f32, f32), bool, bool) {
    client.ui.invoke_thumbnail_activated(0);
    let viewer = client
        .bound
        .viewer
        .borrow()
        .as_ref()
        .expect("the viewer opens")
        .clone_strong();
    let window = viewer.window();
    let size = window.size().to_logical(window.scale_factor());
    let opened = (
        (size.width, size.height),
        window.is_maximized(),
        window.is_fullscreen(),
    );
    viewer.invoke_close_requested();
    opened
}

// leaf: audit-options-nested-frame-location-remember
// leaf: audit-options-nested-frame-location-state
#[test]
fn the_frame_editor_s_switches_decide_how_the_media_viewer_next_opens() {
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    // the reference's defaults for the media viewer: remembered 640x480,
    // opening maximised
    let defaults = client.setting::<WindowSettings>().media_viewer;
    assert!(defaults.remember_size && defaults.maximised);

    // remember the size, and open neither maximised nor fullscreen
    edit_media_viewer(&client, &|child| {
        child.set_remember_size(true);
        child.set_size_none(false);
        child.set_last_width(700);
        child.set_last_height(500);
        child.set_maximised(false);
        child.set_fullscreen(false);
    });
    let saved = client.setting::<WindowSettings>().media_viewer;
    assert_eq!(
        (
            saved.remember_size,
            saved.last_size,
            saved.maximised,
            saved.fullscreen
        ),
        (true, Some((700, 500)), false, false)
    );
    assert_eq!(opened_viewer(&client), ((700.0, 500.0), false, false));

    // another size is the one opened
    edit_media_viewer(&client, &|child| {
        child.set_last_width(820);
        child.set_last_height(610);
    });
    assert_eq!(opened_viewer(&client).0, (820.0, 610.0));

    // not remembering the size: the saved one is kept but not used
    edit_media_viewer(&client, &|child| child.set_remember_size(false));
    let saved = client.setting::<WindowSettings>().media_viewer;
    assert_eq!(
        (saved.remember_size, saved.last_size),
        (false, Some((820, 610)))
    );
    assert_ne!(opened_viewer(&client).0, (820.0, 610.0));

    // start maximised; then fullscreen
    edit_media_viewer(&client, &|child| child.set_maximised(true));
    assert!(client.setting::<WindowSettings>().media_viewer.maximised);
    let (_, maximised, fullscreen) = opened_viewer(&client);
    assert!(maximised && !fullscreen);
    edit_media_viewer(&client, &|child| {
        child.set_maximised(false);
        child.set_fullscreen(true);
    });
    let (_, maximised, fullscreen) = opened_viewer(&client);
    assert!(!maximised && fullscreen);
}
