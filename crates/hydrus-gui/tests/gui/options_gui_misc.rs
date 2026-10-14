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

// leaf: audit-options-file-sort-collect-file-sort-update-default-file-sort-every-time-a-new-sort-is-manually-chosen
#[test]
fn choosing_a_sort_updates_the_default_as_the_reference_s_sort_control_does() {
    // `oracle/record_default_sort_update.py`: for the option off and on, a sort type is
    // chosen and then the order flipped; what the recording keeps is whether the default
    // sort then equals the page's (and what it was before), and what a new page opens with.
    let recorded = hydrus_testkit::fixture_json("default_sort_update.json");
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let page = client.bound.current.borrow().clone();
    let start = client.setting::<SortSettings>();
    for case in recorded["cases"].as_array().unwrap() {
        let save = case["save"].as_bool().unwrap();
        // each case starts from the same sort on the page and as the default
        page.borrow_mut()
            .set_sort_type(start.default_sort.by.clone());
        page.borrow_mut()
            .set_sort_order(if start.default_sort.ascending {
                hydrus_search::SortOrder::Ascending
            } else {
                hydrus_search::SortOrder::Descending
            });
        {
            let start = start.clone();
            client
                .store
                .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &start))
                .unwrap();
        }
        let options = client.open_options();
        show_page(&options, "file sort/collect");
        let label = "Update default file sort every time a new sort is manually chosen: ";
        let (at, shown) = row(&options, label);
        if shown.checked != save {
            options.invoke_check_toggled(at, save);
        }
        options.invoke_apply();
        let default_before = client.setting::<SortSettings>().default_sort;
        let step_names = ["type", "flip order"];
        for (n, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let before_sort = page.borrow().sort().clone();
            if n == 0 {
                // "number of tags", found by its label in the page's choices
                let choices =
                    hydrus_gui_model::sort::page_choices(&client.store, &page.borrow().sort().by);
                let tags = choices
                    .iter()
                    .position(|c| c.name.ends_with("number of tags"))
                    .expect("the sort type list has number of tags");
                client.ui.invoke_sort_chosen(i32::try_from(tags).unwrap());
            } else {
                let ascending = page.borrow().sort().ascending;
                client.ui.invoke_order_chosen(i32::from(ascending));
            }
            let default = client.setting::<SortSettings>().default_sort;
            let page_sort = page.borrow().sort().clone();
            assert_ne!(page_sort, before_sort, "the page's sort really changed");
            let recorded_equal = step["page_sort"] == step["default"];
            assert_eq!(
                default == page_sort,
                recorded_equal,
                "{save} {}",
                step_names[n]
            );
            let recorded_changed = step["default"] != case["default_before"];
            assert_eq!(
                default != default_before,
                recorded_changed,
                "{save} {}",
                step_names[n]
            );
        }
        // a new page opens with the default sort, which is the last chosen only if saved
        let new_sort = hydrus_gui::SearchPage::new(client.store.clone())
            .sort()
            .clone();
        let page_sort = page.borrow().sort().clone();
        let recorded_new_is_chosen = case["new_page_sort"] == case["steps"][1]["page_sort"];
        assert_eq!(new_sort == page_sort, recorded_new_is_chosen, "{save}");
    }
}
