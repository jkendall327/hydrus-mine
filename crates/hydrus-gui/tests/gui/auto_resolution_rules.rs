//! The duplicates auto-resolution rules editor, opened by the duplicates
//! page's "edit rules": a suggested rule added, a new rule edited (refused
//! while "better" has no comparator telling A from B; a search comparator
//! and an OR of a pair test added), one deleted, and "apply" writing them.
//! What the windows say is tested against the reference's in
//! hydrus-gui-model's tests.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{AutoResolutionRulesWindow, MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::duplicates::auto::{self, Comparator, LookingAt, PairTest};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicate_filter::{my_files, store_with_pairs};

fn names(list: &AutoResolutionRulesWindow) -> Vec<String> {
    let rows = list.get_rows();
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

fn strings(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn rules_are_added_edited_deleted_and_written() {
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
        .expect("it opens")
        .clone_strong();
    assert!(
        list.get_warning()
            .starts_with("THIS IS AN ADVANCED SYSTEM.")
    );
    assert!(names(&list).is_empty());

    // a suggested rule
    list.invoke_add_suggested();
    assert!(list.get_choosing_suggested());
    assert_eq!(
        strings(&list.get_suggestions())[0],
        "pixel-perfect jpegs vs pngs"
    );
    list.invoke_suggested_chosen(0);
    assert_eq!(names(&list), ["pixel-perfect jpegs vs pngs"]);

    // a new rule: refused without a comparator telling A from B
    list.invoke_add();
    let rule = || {
        bound
            .auto_resolution
            .rule
            .borrow()
            .as_ref()
            .expect("the editor is open")
            .clone_strong()
    };
    assert_eq!(rule().get_window_title(), "edit rule");
    assert_eq!(rule().get_name(), "new rule");
    assert_eq!(
        rule().get_search_1(),
        "system:filetype is image\nsystem:height > 128\nsystem:width > 128"
    );
    rule().invoke_apply();
    assert!(
        rule()
            .get_errors()
            .starts_with("Hey, you have the action set to")
    );
    // a search comparator for A
    rule().set_comparator_kind(0);
    rule().invoke_comparator_add();
    let comparator = || {
        bound
            .auto_resolution
            .comparators
            .borrow()
            .last()
            .map(|(_, w)| w.clone_strong())
            .expect("a comparator editor is open")
    };
    assert_eq!(
        comparator().get_window_title(),
        "edit one-file metadata conditional comparator"
    );
    comparator().set_predicates("system:inbox".into());
    comparator().invoke_changed();
    assert_eq!(comparator().get_summary(), "A will match: system:inbox");
    comparator().invoke_apply();
    assert!(bound.auto_resolution.comparators.borrow().is_empty());
    assert_eq!(
        strings(&rule().get_comparators()),
        ["A will match: system:inbox"]
    );
    // an OR, of a pair test (which has no editor of its own)
    let or = strings(&rule().get_comparator_kinds())
        .iter()
        .position(|k| k == "OR Comparator")
        .unwrap();
    rule().set_comparator_kind(i32::try_from(or).unwrap());
    rule().invoke_comparator_add();
    assert_eq!(comparator().get_window_title(), "edit OR comparator");
    comparator().set_sub_kind(4);
    comparator().invoke_sub_add();
    assert_eq!(
        strings(&comparator().get_subs()),
        ["A and B have the same filetype"]
    );
    comparator().invoke_apply();
    // (screenshots, kept in the target directory: the comparison tab, and
    // a relative comparator's editor)
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    rule().set_tab(1);
    let pixels = headless::render(&windows.get(2).unwrap(), 900, 700);
    headless::save_png(&shots.join("auto_resolution_rule.png"), &pixels, 900, 700).unwrap();
    rule().set_comparator_kind(2);
    rule().invoke_comparator_add();
    assert_eq!(comparator().get_window_title(), "edit relative comparator");
    comparator().set_multiplier("1.5".into());
    comparator().invoke_changed();
    assert_eq!(
        comparator().get_summary(),
        "A has \"system:filesize\" > 1.50x B"
    );
    let pixels = headless::render(&windows.get(5).unwrap(), 640, 420);
    headless::save_png(
        &shots.join("auto_resolution_comparator.png"),
        &pixels,
        640,
        420,
    )
    .unwrap();
    comparator().invoke_cancel();
    rule().set_name("mine".into());
    rule().invoke_changed();
    rule().invoke_apply();
    assert!(bound.auto_resolution.rule.borrow().is_none());
    assert_eq!(names(&list), ["mine", "pixel-perfect jpegs vs pngs"]);

    // the suggested one deleted, asked first
    list.invoke_row_clicked(1, false, false);
    list.invoke_delete();
    assert_eq!(list.get_asking_message(), "Remove all selected?");
    list.invoke_chosen(0);
    assert_eq!(names(&list), ["mine"]);

    // written
    list.invoke_apply();
    assert!(bound.auto_resolution.list.borrow().is_none());
    let written = store.read(auto::rules).unwrap();
    assert_eq!(written.len(), 1);
    let (_, mine) = &written[0];
    assert_eq!(mine.name, "mine");
    assert!(matches!(
        mine.comparators[0],
        Comparator::OneFileMetadata {
            looking_at: LookingAt::A,
            ..
        }
    ));
    assert_eq!(
        mine.comparators[1],
        Comparator::Or(vec![Comparator::Pair(PairTest::FiletypeSame)])
    );
}

#[test]
fn a_rules_searches_location_is_chosen() {
    let _windows = headless::init();
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
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(rule.get_location(), "combined local file domains");
    // my files and the trash, in the locations list
    rule.invoke_edit_location();
    let locations = bound
        .auto_resolution
        .locations
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the list opens");
    let ticks = locations.get_ticks();
    for label in ["my files", "trash"] {
        let at = (0..ticks.row_count())
            .position(|i| ticks.row_data(i).unwrap().label == label)
            .unwrap();
        locations.invoke_toggled(i32::try_from(at).unwrap(), true);
    }
    locations.invoke_apply();
    assert_eq!(rule.get_location(), "my files, trash");
    rule.invoke_apply();
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let search = &written[0].1.search;
    assert_eq!(search.search_1.location.current().len(), 2);
    assert_eq!(search.search_1.location, search.search_2.location);
}
