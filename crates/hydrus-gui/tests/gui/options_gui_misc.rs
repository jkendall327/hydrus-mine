//! Single options of the gui, file sort/collect and ratings pages against
//! the reference's panels: the control as the reference shows it, and the
//! behaviour the saved value changes.

use slint::ComponentHandle as _;

use hydrus_core::pages::SortSettings;
use hydrus_core::windows::WindowSettings;

use crate::options_gui_support::{Client, box_of, row, show_page};

// leaf: audit-options-file-sort-collect-file-sort-update-default-file-sort-every-time-a-new-sort-is-manually-chosen
#[test]
fn a_chosen_sort_becomes_the_default_only_if_the_option_says_so() {
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let page = client.bound.current.borrow().clone();
    let original = client.setting::<SortSettings>().default_sort;
    // choose the other order than the page has
    let flip = |client: &Client| {
        let ascending = page.borrow().sort().ascending;
        client.ui.invoke_order_chosen(i32::from(ascending));
        assert_eq!(page.borrow().sort().ascending, !ascending);
    };

    let options = client.open_options();
    show_page(&options, "file sort/collect");
    let label = "Update default file sort every time a new sort is manually chosen: ";
    let (at, r) = row(&options, label);
    assert_eq!(
        (r.kind, r.checked),
        (1, false),
        "off in the reference's defaults"
    );
    assert_eq!(box_of(&options, label), "file sort");
    options.invoke_cancel();

    flip(&client);
    assert_eq!(
        client.setting::<SortSettings>().default_sort,
        original,
        "off: unchanged"
    );

    let options = client.open_options();
    show_page(&options, "file sort/collect");
    options.invoke_check_toggled(at, true);
    options.invoke_apply();
    assert!(client.setting::<SortSettings>().save_page_sort_on_change);
    // (a different sort type this time: the order flipped back would be the default again)
    client.ui.invoke_sort_chosen(1);
    let chosen = page.borrow().sort().clone();
    assert_ne!(chosen, original);
    assert_eq!(client.setting::<SortSettings>().default_sort, chosen);
    // new pages then open with it
    assert_eq!(
        hydrus_gui::SearchPage::new(client.store.clone()).sort(),
        &chosen
    );
}

// leaf: audit-options-gui-frame-locations-save-media-viewer-window-size-and-position-on-close
#[test]
fn the_media_viewer_keeps_its_window_geometry_on_close_only_if_the_option_says_so() {
    let client = Client::basic();
    let geometry = || client.setting::<WindowSettings>().media_viewer;
    let before = geometry();
    let window = client.ui.window();
    let state = hydrus_gui::windows::state(window);
    assert_ne!(Some(state.size), before.last_size, "a size to tell apart");

    let options = client.open_options();
    show_page(&options, "gui");
    let label = "Save media viewer window size and position on close: ";
    let (at, r) = row(&options, label);
    assert_eq!(
        (r.kind, r.checked),
        (1, false),
        "off in the reference's defaults"
    );
    assert_eq!(box_of(&options, label), "frame locations");
    options.invoke_cancel();

    // closing a viewer with it off keeps nothing
    hydrus_gui::windows::save_named(window, &client.store, "media_viewer");
    assert_eq!(geometry(), before);

    let options = client.open_options();
    show_page(&options, "gui");
    options.invoke_check_toggled(at, true);
    options.invoke_apply();
    assert!(
        client
            .setting::<WindowSettings>()
            .save_media_viewer_on_close
    );
    hydrus_gui::windows::save_named(window, &client.store, "media_viewer");
    assert_eq!(geometry().last_size, Some(state.size));
}
