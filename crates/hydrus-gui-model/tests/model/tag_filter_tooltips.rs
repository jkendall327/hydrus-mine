//! The "tag filter" button's label and tooltip, and the tag filter editor's
//! "show other panels" tooltip, against the reference's `TagFilterButton`.
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::tag_filter_editor::{SHOW_OTHER_PANELS_TOOLTIP, button_label};

/// Qt wraps a tooltip at 80 characters (`WrapToolTip`); Slint wraps its own.
fn unwrapped(text: &str) -> String {
    text.replace('\n', " ")
}

#[test]
fn tag_filter_buttons_label_and_tooltip_follow_the_recording() {
    let f = hydrus_testkit::fixture_json("tag_filter_tooltips.json");
    let buttons = f["buttons"].as_array().unwrap();
    assert_eq!(buttons.len(), 40);
    for b in buttons {
        let (blacklist_only, prefix, language) = match b["config"].as_str().unwrap() {
            "migration_taken" => (false, "tags taken: ", true),
            "migration_left" => (false, "left: ", true),
            "migration_right" => (false, "right: ", true),
            "display_shown" => (false, "tags shown: ", true),
            "string_match" => (false, "", false),
            "import_get_tags" => (false, "adding: ", true),
            "import_blacklist" => (true, "", false),
            "api_permitted" => (false, "permitted tags: ", false),
            other => panic!("{other}"),
        };
        let mut filter = TagFilter::new();
        for rule in b["rules"].as_array().unwrap() {
            filter = filter.with_rule(
                rule[0].as_str().unwrap(),
                if rule[1] == "black" {
                    FilterRule::Blacklist
                } else {
                    FilterRule::Whitelist
                },
            );
        }
        let (label, tooltip) = button_label(&filter, blacklist_only, prefix, language);
        assert_eq!(label, b["text"].as_str().unwrap(), "{b}");
        assert_eq!(tooltip, unwrapped(b["tooltip"].as_str().unwrap()), "{b}");
    }
    assert_eq!(
        SHOW_OTHER_PANELS_TOOLTIP,
        unwrapped(f["show_other_panels"].as_str().unwrap())
    );
}
