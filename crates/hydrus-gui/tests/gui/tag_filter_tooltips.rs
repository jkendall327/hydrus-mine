//! The tooltip of the tag filter editor's "show other panels" button: the
//! reference's text, bound to the button's `Tooltip`. (The headless test
//! platform does not display Slint's tooltip popups, so what is tested is the
//! text each control hands to its `Tooltip`; the other tag filter buttons'
//! tooltips are asserted where their windows are tested: tag migration, tag
//! display, the import options' tag filtering, and the model's
//! `tag_filter_tooltips` replay of the reference's button text.)
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui::{headless, tag_filter_window};
use hydrus_store::Store;
use std::{cell::RefCell, rc::Rc, sync::Arc};

// leaf: audit-shared-tag-tooltips
#[test]
fn show_other_panels_carries_the_references_tooltip() {
    let f = hydrus_testkit::fixture_json("tag_filter_tooltips.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let _windows = headless::init();
    store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let slot = Rc::new(RefCell::new(None));
    let w = tag_filter_window::open(
        &store,
        &TagFilter::new().with_rule("goblin", FilterRule::Blacklist),
        true,
        "blacklist",
        "",
        &slot,
        Rc::new(|_| {}),
    )
    .unwrap();
    assert!(w.get_show_other_panels());
    // (Qt wraps its tooltips at 80 characters; Slint wraps its own)
    assert_eq!(
        w.get_show_other_panels_tooltip().replace('\n', " "),
        f["show_other_panels"].as_str().unwrap().replace('\n', " ")
    );
}
