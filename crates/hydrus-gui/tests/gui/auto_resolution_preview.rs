//! The duplicates auto-resolution rule editor's "preview" tab: the rule as
//! edited searched for its pairs and each tested, off the UI thread, those
//! that pass listed with what would be done to them and those that fail
//! apart; a rule that can't be had says why.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{AutoResolutionRuleWindow, Bound, MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicate_filter::{my_files, store_with_pairs};

/// Let the preview work until it has tested every pair it fetched.
fn tested(rule: &AutoResolutionRuleWindow) {
    for _ in 0..3000 {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        let label = rule.get_preview_search_label();
        if label.contains("pairs searched") && rule.get_preview_to_test_label().is_empty() {
            return;
        }
    }
    panic!(
        "the preview never finished: {} / {}",
        rule.get_preview_search_label(),
        rule.get_preview_to_test_label()
    );
}

fn editor(bound: &Bound) -> AutoResolutionRuleWindow {
    bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the editor is open")
        .clone_strong()
}

#[test]
fn a_rules_pairs_are_previewed() {
    let windows = headless::init();
    let (_dir, store) = store_with_pairs();
    let (_, key) = my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();

    // a new rule can't be had: "better" with nothing telling A from B
    list.invoke_add();
    let rule = editor(&bound);
    rule.set_tab(3);
    rule.invoke_preview_shown();
    assert!(
        rule.get_preview_search_label()
            .starts_with("Problem fetching the current rule! Hey, you have the action set to"),
        "{}",
        rule.get_preview_search_label()
    );
    rule.invoke_cancel();

    // a suggested rule: its pairs fetched and tested
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = editor(&bound);
    rule.set_tab(3);
    rule.invoke_preview_shown();
    tested(&rule);
    let label = rule.get_preview_search_label().to_string();
    let matched: usize = label
        .rsplit("; ")
        .next()
        .and_then(|m| m.strip_suffix(" matched"))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{label}"));
    assert!(matched > 0, "{label}");
    let (passed, failed) = (
        rule.get_preview_pass_rows().row_count(),
        rule.get_preview_fail_rows().row_count(),
    );
    assert_eq!(passed + failed, matched);
    assert!(passed > 0, "pixel-perfect jpegs and pngs pass");
    let text = rule
        .get_preview_pass_rows()
        .row_data(0)
        .unwrap()
        .text
        .to_string();
    assert!(
        text.starts_with("this way around\nset as duplicates--A better\nA:\n")
            || text.starts_with("either way around\nset as duplicates--A better\nA:\n"),
        "{text}"
    );
    assert!(
        rule.get_preview_pass_label()
            .starts_with(&format!("{passed} pairs - "))
    );
    // a pair double-clicked: the duplicate filter, on the passing pairs
    // from that one round to the start
    rule.invoke_preview_pass_activated(i32::try_from(passed - 1).unwrap());
    assert_eq!(rule.get_errors(), "");
    let filter = bound
        .auto_resolution
        .preview_filter
        .borrow()
        .as_ref()
        .expect("the filter opened")
        .clone_strong();
    assert!(
        filter
            .get_index_text()
            .starts_with(&format!("File One - 1/{passed}")),
        "{}",
        filter.get_index_text()
    );
    filter.hide().unwrap();
    // a smaller sample, fetched again
    rule.set_preview_fetch_limit(1);
    rule.invoke_preview_fetch_changed();
    tested(&rule);
    assert_eq!(
        rule.get_preview_pass_rows().row_count() + rule.get_preview_fail_rows().row_count(),
        1
    );
    // (the newest window)
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    rule.set_preview_fetch_all(true);
    rule.invoke_preview_fetch_changed();
    tested(&rule);
    let pixels = headless::render(&windows.get(last).unwrap(), 900, 800);
    let shot =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("auto_resolution_preview.png");
    headless::save_png(&shot, &pixels, 900, 800).unwrap();
}
