//! The tag suggestions page against the reference's `TagSuggestionsPanel`:
//! the controls of its related and recent tabs, and what the manage tags
//! window does with them. Read from the reference's source (there is no Qt in
//! the sandbox to record with). The page is not a whole leaf's worth: the
//! file lookup tab's controls and the durations of the related search have no
//! native consumer (see DIFFERENCES.md).

use slint::ComponentHandle as _;

use hydrus_store::related_tags::Settings as Related;
use hydrus_store::settings::TagSuggestionSettings;

use crate::options_gui_support::{Client, box_of, row, show_page};

const SHOW_RELATED: &str = "Show related tags: ";
const QUICK: &str = "Initial/Quick search duration (ms): ";
const MEDIUM: &str = "Medium search duration (ms): ";
const THOROUGH: &str = "Thorough search duration (ms): ";
const CONCURRENCE: &str = "Tag concurrence threshold %: ";
const RECENT: &str = "number of recent tags to show: ";
const LOOKUP: &str = "Show file lookup scripts on single-file manage tags windows: ";

#[test]
fn the_related_recent_and_file_lookup_tabs_have_the_references_controls_and_defaults() {
    let client = Client::basic();
    let options = client.open_options();
    show_page(&options, "tag suggestions");
    for (label, tab) in [
        (SHOW_RELATED, "related"),
        (QUICK, "related"),
        (MEDIUM, "related"),
        (THOROUGH, "related"),
        (CONCURRENCE, "related"),
        (LOOKUP, "file lookup scripts"),
        (RECENT, "recent"),
    ] {
        assert_eq!(box_of(&options, label), tab, "{label}");
    }
    let number = |label| {
        let (_, r) = row(&options, label);
        (r.kind, r.minimum, r.maximum, r.number)
    };
    assert_eq!(number(QUICK), (2, 50, 60_000, 250));
    assert_eq!(number(MEDIUM), (2, 50, 60_000, 2_000));
    assert_eq!(number(THOROUGH), (2, 50, 60_000, 6_000));
    assert_eq!(number(CONCURRENCE), (2, 1, 100, 6));
    assert!(row(&options, SHOW_RELATED).1.checked);
    assert!(!row(&options, LOOKUP).1.checked);
    let (_, recent) = row(&options, RECENT);
    assert_eq!(recent.number, 20);
    assert!(!recent.is_none, "20 recent tags by default");
}

#[test]
fn showing_related_tags_and_the_recent_count_reach_the_manage_tags_window() {
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let tabs = |client: &Client| {
        client.ui.invoke_select_all();
        client.ui.invoke_manage_tags_selected();
        let manage = client
            .bound
            .manage_tags
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let shown = (
            manage.get_related_tags_enabled(),
            manage.get_recent_tags_enabled(),
        );
        manage.invoke_cancel();
        shown
    };
    assert_eq!(tabs(&client), (true, true));

    let options = client.open_options();
    show_page(&options, "tag suggestions");
    options.invoke_check_toggled(row(&options, SHOW_RELATED).0, false);
    options.invoke_number_edited(row(&options, CONCURRENCE).0, 40);
    options.invoke_number_edited(row(&options, QUICK).0, 10);
    options.invoke_none_toggled(row(&options, RECENT).0, true);
    options.invoke_apply();
    let related = client.setting::<Related>();
    assert!(!related.enabled);
    assert_eq!(related.concurrence_percent, 40);
    assert_eq!(related.durations_ms[0], 50, "the spin box's lowest");
    assert_eq!(client.setting::<TagSuggestionSettings>().recent_limit, None);
    // the weights the page already edited are not lost with the rest
    assert_eq!(
        related.weights,
        hydrus_store::related_tags::Weights::default()
    );
    assert_eq!(tabs(&client), (false, false), "no related, no recent");

    let options = client.open_options();
    show_page(&options, "tag suggestions");
    options.invoke_check_toggled(row(&options, SHOW_RELATED).0, true);
    options.invoke_none_toggled(row(&options, RECENT).0, false);
    options.invoke_number_edited(row(&options, RECENT).0, 7);
    options.invoke_apply();
    assert_eq!(
        client.setting::<TagSuggestionSettings>().recent_limit,
        Some(7)
    );
    assert!(client.setting::<Related>().enabled);
    assert_eq!(tabs(&client), (true, true));
}
