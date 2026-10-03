//! A duplicates page's sidebar tabs on a store with potential pairs: the
//! preparation tab's numbers as the search distance changes, working
//! hard, deleting every pair (asking first), and the auto-resolution tab's
//! rules, paused and reset. (What the tabs say is tested against the
//! reference's in hydrus-gui-model's tests.)

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::duplicates::auto::{self, OperationMode, Rule, RuleAction};
use hydrus_store::sessions::{self, LAST_SESSION};
use hydrus_store::settings;
use hydrus_store::similar::SimilarFilesSettings;

use crate::duplicate_filter::{my_files, store_with_pairs};

fn rows(ui: &MainWindow) -> Vec<Vec<String>> {
    let rows = ui.get_duplicates_rules();
    (0..rows.row_count())
        .map(|r| {
            let row = rows.row_data(r).unwrap();
            (0..row.cells.row_count())
                .map(|c| row.cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn the_sidebar_tabs_show_and_change_the_search_and_the_rules() {
    let windows = headless::init();
    let (_dir, store) = store_with_pairs();
    let (_, key) = my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let duplicates_search = DuplicatesSearch {
        search_1: search.clone(),
        search_2: search,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
    };
    let rule = Rule {
        name: "jpegs over pngs".into(),
        paused: false,
        mode: OperationMode::SemiAutomatic,
        max_pending_pairs: None,
        search: duplicates_search.clone(),
        comparators: Vec::new(),
        action: RuleAction::Better,
        delete_a: false,
        delete_b: true,
        custom_merge: None,
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(duplicates_search),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 0)?;
            auto::add_rule(ctx.conn(), &rule, None)?;
            Ok(())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    assert!(ui.get_can_filter());

    // searched at its distance, 4: nothing to do
    let data = ui.get_duplicates();
    assert!(
        data.eligible.ends_with(" eligible files in the system."),
        "{}",
        data.eligible
    );
    assert!(!data.eligible.starts_with("0 "));
    assert_eq!(
        data.searched,
        "All potential duplicates found at this distance."
    );
    assert_eq!(data.preparation_name, "preparation");
    assert_eq!(
        (data.distance, data.distance_label.as_str()),
        (4, "similar")
    );
    assert!(!data.can_start);

    // at 8, none searched yet: the work can be started
    ui.invoke_duplicates_action("distance".into(), 8, false, false);
    let data = ui.get_duplicates();
    assert_eq!(data.distance_label, "speculative");
    assert_eq!(data.searched, "Have not yet searched at this distance.");
    assert_eq!(data.preparation_name, "preparation (0% done)");
    assert!(data.can_start && !data.working_hard);
    ui.invoke_duplicates_action("work hard".into(), 1, false, false);
    assert!(ui.get_duplicates().working_hard);
    let written: SimilarFilesSettings = store.read(settings::get).unwrap();
    assert!(written.work_hard && written.search_distance == 8);
    ui.invoke_duplicates_action("distance".into(), 6, false, false);
    assert_eq!(ui.get_duplicates().distance_label, "custom");
    ui.invoke_duplicates_action("search during idle".into(), 0, false, false);
    assert!(!ui.get_duplicates().search_during_idle);

    // (a screenshot, kept in the target directory)
    slint::ComponentHandle::show(&ui).unwrap();
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("duplicates_page.png"), &pixels, 1100, 700).unwrap();

    // the rule, paused and played
    let shown = rows(&ui);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0][0], "jpegs over pngs");
    assert!(!ui.get_duplicates().any_rule_selected);
    ui.invoke_duplicates_action("rule".into(), 0, false, false);
    assert!(ui.get_duplicates().any_rule_selected);
    ui.invoke_duplicates_action("pause rules".into(), 0, false, false);
    assert_eq!(rows(&ui)[0][2], "paused");
    ui.invoke_duplicates_action("pause rules".into(), 0, false, false);
    assert_ne!(rows(&ui)[0][2], "paused");
    // a reset asks first
    ui.invoke_duplicates_action("reset search".into(), 0, false, false);
    let data = ui.get_duplicates();
    assert!(data.asking);
    assert!(
        data.asking_message
            .starts_with("This will command the database to re-search these 1 rules.")
    );
    ui.invoke_duplicates_action("chosen".into(), 0, false, false);
    assert!(!ui.get_duplicates().asking);

    // every pair deleted, asking first: all to search again
    ui.invoke_duplicates_action("distance".into(), 4, false, false);
    ui.invoke_duplicates_action("delete pairs".into(), 0, false, false);
    assert!(
        ui.get_duplicates()
            .asking_message
            .starts_with("ADVANCED TOOL")
    );
    ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
    assert_eq!(
        ui.get_duplicates().searched,
        "All potential duplicates found at this distance."
    );
    ui.invoke_duplicates_action("delete pairs".into(), 0, false, false);
    ui.invoke_duplicates_action("chosen".into(), 0, false, false);
    assert_eq!(
        ui.get_duplicates().searched,
        "Have not yet searched at this distance."
    );
    let pairs: i64 = store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM potential_pairs", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(pairs, 0);
}
