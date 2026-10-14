//! "By default, stop searching once this many files are found:" set in the real
//! options window, and the places that read it, compared with the reference
//! (`oracle/fixtures/gallery_file_limit.json`): a new gallery downloader page's
//! limit, and the first-sync limit of a subscription added through
//! network > subscriptions (`min( 100, limit )`, 100 with no limit).

use slint::{ComponentHandle as _, Model as _};

use super::options_system_consumers::{add_example_downloader, client, new_page, row_in};

// leaf: audit-options-downloading-gallery-downloader-by-default-stop-searching-once-this-many-files-are-found
#[test]
fn the_default_file_limit_is_the_recorded_limit_of_new_pages_and_the_first_sync_of_new_subscriptions()
 {
    let fixture = hydrus_testkit::fixture_json("gallery_file_limit.json");
    let client = client();
    add_example_downloader(&client.store);
    let label = "By default, stop searching once this many files are found:";
    assert_eq!(
        fixture["default"].as_i64().unwrap(),
        {
            let window = client.options("downloading");
            let (_, found) = row_in(&window, "gallery downloader", label);
            assert_eq!(
                (found.kind, found.minimum, found.maximum),
                (3, 1, 1_000_000)
            );
            assert_eq!(found.none_phrase, "no limit");
            i64::from(found.number)
        },
        "the row starts on the reference's default"
    );

    for case in fixture["cases"].as_array().unwrap() {
        let limit = case["limit"].as_i64();
        let window = client.options("downloading");
        let (i, _) = row_in(&window, "gallery downloader", label);
        match limit {
            Some(n) => {
                window.invoke_none_toggled(i, false);
                window.invoke_number_edited(i, i32::try_from(n).unwrap());
            }
            None => window.invoke_none_toggled(i, true),
        }
        window.invoke_apply();

        // a new gallery downloader page
        new_page(&client.ui, true);
        let data = client.ui.get_gallery_data();
        let page = (!data.no_limit).then_some(i64::from(data.file_limit));
        assert_eq!(page, case["gallery page"].as_i64(), "{case}");

        // a new subscription, as the add button makes it
        let list = crate::subscriptions::open_dialog(&client.ui, &client.bound);
        list.invoke_add();
        let chooser = client
            .bound
            .subscription_gallery
            .borrow()
            .as_ref()
            .expect("the downloader chooser")
            .clone_strong();
        assert!(chooser.get_galleries().row_count() > 0);
        chooser.set_selected(0);
        chooser.invoke_accept_clicked();
        let editor = client
            .bound
            .edit_subscription
            .borrow()
            .as_ref()
            .expect("the edit subscription dialog")
            .clone_strong();
        assert_eq!(
            Some(i64::from(editor.get_initial_limit())),
            case["new subscription initial"].as_i64(),
            "{case}"
        );
        assert_eq!(
            Some(i64::from(editor.get_periodic_limit())),
            case["new subscription periodic"].as_i64(),
            "{case}"
        );
        editor.invoke_cancel();
        list.invoke_cancel();
    }
}
