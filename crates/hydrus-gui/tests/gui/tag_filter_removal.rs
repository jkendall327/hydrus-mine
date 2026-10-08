//! The tag filter editor's list removal: a selection asks "Remove all
//! selected?" first (`ClientGUITagFilter._SimpleDeleteBlacklistButton` and
//! friends), and a double-click takes the selected entries out without asking
//! (`ListBoxTagsFilter._Activate`).
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui::{TagFilterWindow, headless, tag_filter_window};
use hydrus_store::Store;
use slint::Model as _;
use std::{cell::RefCell, rc::Rc, sync::Arc};

const BLACKLIST: i32 = 1;
const EXCLUDE: i32 = 2;

fn rule_set(filter: &TagFilter) -> Vec<(String, bool)> {
    let mut rules: Vec<_> = filter
        .rules()
        .map(|(s, r)| (s.to_owned(), r == FilterRule::Blacklist))
        .collect();
    rules.sort();
    rules
}

fn rows(w: &TagFilterWindow, list: i32) -> Vec<String> {
    let model = match list {
        BLACKLIST => w.get_black_rows(),
        EXCLUDE => w.get_exclude_rows(),
        _ => w.get_white_rows(),
    };
    model
        .iter()
        .map(|row| row.cells.row_data(0).unwrap().to_string())
        .collect()
}

// leaf: audit-shared-tag-removal
#[test]
fn removing_selected_entries_asks_first_and_double_click_removes_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let _windows = headless::init();
    let slot = Rc::new(RefCell::new(None));
    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let w = tag_filter_window::open(
        &store,
        &TagFilter::new(),
        false,
        "filter",
        "",
        &slot,
        Rc::new(move |f| *output.borrow_mut() = Some(f)),
    )
    .unwrap();
    // The simple blacklist and the advanced exclude list show the same
    // blacklisted slices.
    w.invoke_typed(BLACKLIST, "creator:".into());
    w.invoke_typed(BLACKLIST, "series:".into());
    w.invoke_typed(EXCLUDE, "character:".into());
    let all = ["'character' tags", "'creator' tags", "'series' tags"];
    assert_eq!(rows(&w, BLACKLIST), all);
    assert_eq!(rows(&w, EXCLUDE), all);

    // Nothing selected: the remove button does nothing, and asks nothing.
    w.invoke_remove(BLACKLIST);
    assert!(!w.get_asking());

    // Select the first row, remove: asked; "no" keeps it.
    w.invoke_row_clicked(BLACKLIST, 0, false, false);
    assert!(w.get_black_rows().row_data(0).unwrap().selected);
    w.invoke_remove(BLACKLIST);
    assert!(w.get_asking());
    assert_eq!(w.get_asking_message(), "Remove all selected?");
    assert_eq!(
        w.get_asking_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        ["yes", "no"]
    );
    w.invoke_chosen(1);
    assert!(!w.get_asking());
    assert_eq!(rows(&w, BLACKLIST), all);

    // "yes" removes only the selected entry.
    w.invoke_remove(BLACKLIST);
    w.invoke_chosen(0);
    assert!(!w.get_asking());
    assert_eq!(rows(&w, BLACKLIST), &all[1..]);

    // Double-click takes the selection out without asking; with several rows
    // selected, all of them go.
    w.invoke_row_clicked(BLACKLIST, 0, false, false);
    w.invoke_row_clicked(BLACKLIST, 1, true, false);
    w.invoke_row_activated(BLACKLIST, 1);
    assert!(!w.get_asking());
    assert!(rows(&w, BLACKLIST).is_empty());

    // The advanced exclude list asks the same question.
    w.invoke_typed(EXCLUDE, "title:".into());
    assert_eq!(rows(&w, EXCLUDE), ["'title' tags"]);
    w.invoke_row_clicked(EXCLUDE, 0, false, false);
    w.invoke_remove(EXCLUDE);
    assert_eq!(w.get_asking_message(), "Remove all selected?");
    w.invoke_chosen(0);
    assert!(rows(&w, EXCLUDE).is_empty());

    w.invoke_apply();
    assert!(rule_set(&result.borrow().clone().unwrap()).is_empty());
}
