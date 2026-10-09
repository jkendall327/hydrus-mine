//! The duplicates page, driven as a user would: the preparation tab's cog
//! (regenerate numbers/tree, resync pairs to local storage), the filtering
//! tab (sort, group mode, random group, quick set buttons), the five filter
//! decisions' relationships, and the rule preview's pair-list menu.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicate_filter::{my_files, store_with_pairs};

pub(crate) struct Opened {
    pub _windows: headless::Windows,
    pub _dir: tempfile::TempDir,
    pub store: Arc<Store>,
    pub ui: MainWindow,
    pub bound: Bound,
}

/// A store with potential pairs and a saved session of one duplicates page.
pub(crate) fn opened() -> Opened {
    opened_with(|_| {})
}

/// [`opened`], with the store prepared before the window is made.
pub(crate) fn opened_with(prepare: impl FnOnce(&Store)) -> Opened {
    let windows = headless::init();
    let (dir, store) = store_with_pairs();
    prepare(&store);
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
    Opened {
        _windows: windows,
        _dir: dir,
        store,
        ui,
        bound,
    }
}

fn potential_pairs(store: &Store) -> i64 {
    store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM potential_pairs", [], |r| r.get(0))?))
        .unwrap()
}

fn choices(ui: &MainWindow) -> Vec<String> {
    ui.get_duplicates()
        .asking_choices
        .iter()
        .map(|c| c.to_string())
        .collect()
}

// (numbers and tree regeneration are out of scope: the native store keeps no
// such caches, so only their questions are shown)
// leaf: audit-media-preparation-storage-resync
#[test]
fn the_preparation_cog_regenerates_numbers_and_tree_and_resyncs_pairs() {
    let Opened {
        store, ui, bound, ..
    } = opened();
    let _keep = &bound;
    let before = ui.get_duplicates();
    let pairs = potential_pairs(&store);
    assert!(pairs > 0);

    // "regenerate search tree": the reference's question and its two
    // buttons; forgetting it changes nothing
    ui.invoke_duplicates_action("regenerate tree".into(), 0, false, false);
    let data = ui.get_duplicates();
    assert!(data.asking);
    assert!(
        data.asking_message
            .starts_with("This will delete and then recreate the similar files search tree.")
    );
    assert!(
        data.asking_message
            .ends_with("If you do not have a specific reason to run this, it is pointless.")
    );
    assert_eq!(choices(&ui), ["do it", "forget it"]);
    ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
    assert!(!ui.get_duplicates().asking);
    // doing it: hydrus-rs builds its tree from the hashes for each search,
    // so the numbers and pairs are as they were (DIFFERENCES.md)
    ui.invoke_duplicates_action("regenerate tree".into(), 0, false, false);
    ui.invoke_duplicates_action("chosen".into(), 0, false, false);
    let after = ui.get_duplicates();
    assert!(!after.asking);
    assert_eq!(after.eligible, before.eligible);
    assert_eq!(after.searched, before.searched);
    assert_eq!(potential_pairs(&store), pairs);

    // "regenerate search numbers"
    ui.invoke_duplicates_action("regenerate numbers".into(), 0, false, false);
    let data = ui.get_duplicates();
    assert!(
        data.asking_message
            .starts_with("The store of how many files have been searched at each distance")
    );
    assert!(
        data.asking_message
            .ends_with("Correcting a miscount is the only purpose of this task.")
    );
    assert_eq!(choices(&ui), ["yes", "no"]);
    ui.invoke_duplicates_action("chosen".into(), 0, false, false);
    let after = ui.get_duplicates();
    assert_eq!(after.eligible, before.eligible);
    assert_eq!(after.searched, before.searched);
    assert_eq!(potential_pairs(&store), pairs);

    // "resync potential pairs to local storage": pairs of files that were
    // never in local storage are cleared out, the rest kept, and a popup
    // says how many files were
    store
        .write(|ctx| {
            ctx.conn().execute_batch(
                "INSERT INTO dup_groups (group_id, king_hash_id) VALUES (900001, 900001), (900002, 900002);
                 INSERT INTO potential_pairs (smaller_group_id, larger_group_id, distance) VALUES (900001, 900002, 0);
                 INSERT INTO similar_search_status (hash_id, searched_distance) VALUES (900001, 0);",
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(potential_pairs(&store), pairs + 1);
    ui.invoke_duplicates_action("resync pairs".into(), 0, false, false);
    let data = ui.get_duplicates();
    assert!(
        data.asking_message
            .starts_with("There was a time that pairs were not delisted")
    );
    ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
    assert_eq!(
        potential_pairs(&store),
        pairs + 1,
        "refused: nothing cleared"
    );
    ui.invoke_duplicates_action("resync pairs".into(), 0, false, false);
    ui.invoke_duplicates_action("chosen".into(), 0, false, false);
    assert_eq!(potential_pairs(&store), pairs);
    let popups = store
        .read(|c| hydrus_store::popups::all(c, i64::MAX / 4))
        .unwrap();
    let popup = popups
        .iter()
        .find(|p| {
            p.status_title.as_deref()
                == Some("resyncing potential pairs to hydrus local file storage")
        })
        .expect("the resync's popup");
    assert_eq!(
        popup.status_text_1.as_deref(),
        Some("Done! Pairs for 2 out-of-domain files cleared out.")
    );
}


// leaf: audit-media-filter-decisions
#[test]
fn each_filter_decision_writes_its_relationship_when_committed() {
    use hydrus_core::service::builtin_keys;
    use hydrus_duplicates::potentials::PotentialsQuery;
    use hydrus_gui::duplicate_filter::{Decision, DuplicateFilter, Step};
    use hydrus_store::duplicates::{FileScope, PairOrder, file_relationships};

    let (_dir, store) = store_with_pairs();
    let (id, key) = my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let query = PotentialsQuery {
        scope: FileScope::Domains {
            current: vec![id],
            deleted: Vec::new(),
        },
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
        search_1: search.clone(),
        search_2: search,
    };
    let mut filter = DuplicateFilter::new(
        Arc::clone(&store),
        query,
        PairOrder::MaxFilesize,
        false,
        false,
    )
    .unwrap();
    assert_eq!(filter.load_batch().unwrap(), Step::Showing);
    let decisions = [
        ("better, delete the other", Decision::BETTER_DELETE_OTHER),
        ("better, keep both", Decision::BETTER_KEEP_BOTH),
        ("same quality", Decision::SAME_QUALITY),
        ("alternates", Decision::ALTERNATES),
        ("false positive", Decision::FALSE_POSITIVE),
    ];
    let mut made = Vec::new();
    for (name, decision) in decisions {
        let pair = filter.current().expect("a pair to decide on");
        made.push((name, pair));
        assert_eq!(filter.decide(decision).unwrap(), Step::Showing, "{name}");
    }
    // nothing is written until the batch is committed
    let local = store
        .snapshot()
        .services
        .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)
        .unwrap()
        .id;
    let scope = FileScope::Domains {
        current: vec![id],
        deleted: Vec::new(),
    };
    let relations = |a| {
        store
            .read(|c| file_relationships(c, &scope, local, a))
            .unwrap()
    };
    assert!(relations(made[2].1.0).duplicates.is_empty());
    filter.commit_pending().unwrap();
    // better: the first file is king; delete removes the second only then
    let in_files = |f| {
        store
            .read(|c| hydrus_store::media::current_in(c, id, &[f]))
            .unwrap()
            .contains(&f)
    };
    let (a, b) = made[0].1;
    assert_eq!(relations(a).king, a);
    assert!(in_files(a) && !in_files(b));
    let (a, b) = made[1].1;
    assert_eq!(relations(b).king, a);
    assert!(in_files(a) && in_files(b), "keeping both");
    let (a, b) = made[2].1;
    assert!(relations(a).duplicates.contains(&b), "same quality");
    let (a, b) = made[3].1;
    let r = relations(a);
    assert!(
        !r.duplicates.contains(&b) && r.alternates.contains(&b),
        "alternates"
    );
    let (a, b) = made[4].1;
    let r = relations(a);
    assert!(r.false_positives.contains(&b), "false positive");
}

// leaf: audit-media-review-pending
#[test]
fn pending_pairs_are_sampled_refreshed_selected_and_approved() {
    use crate::auto_resolution_review::{listed, opened, review, settle};
    let _windows = headless::init();
    let o = opened();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let name = recorded["reviewed"].as_str().unwrap();
    let rules = o.store.read(hydrus_store::duplicates::auto::rules).unwrap();
    let rule = rules.iter().find(|(_, r)| r.name == name).unwrap().0;
    let window = review(&o.ui, &o.bound, name);
    assert_eq!(window.get_label(), "Found 2 pairs.");
    // "only sample this many" (default 250) takes effect when refreshed
    assert_eq!(window.get_fetch_limit(), 250);
    window.set_fetch_limit(1);
    window.invoke_fetch_changed();
    assert_eq!(window.get_label(), "Found 2 pairs.");
    window.invoke_refresh();
    assert_eq!(window.get_label(), "Found 1 pairs.");
    assert_eq!(window.get_rows().row_count(), 1);
    // "fetch all"
    window.set_fetch_all(true);
    window.invoke_fetch_changed();
    window.invoke_refresh();
    assert_eq!(window.get_label(), "Found 2 pairs.");
    assert_eq!(window.get_rows().row_count(), 2);
    assert!(!window.get_can_act());
    // select all, then approve every pair
    assert!(window.get_can_select_all());
    let actioned_before = listed(&o.store, rule, "actioned", &o.hex).len();
    window.invoke_select_all();
    assert!(
        (0..2).all(|i| window.get_rows().row_data(i).unwrap().selected),
        "both rows selected"
    );
    assert!(window.get_can_act());
    window.invoke_approve();
    settle(&window);
    assert_eq!(window.get_label(), "Found 0 pairs.");
    assert!(listed(&o.store, rule, "pending", &o.hex).is_empty());
    assert_eq!(
        listed(&o.store, rule, "actioned", &o.hex).len(),
        actioned_before + 2
    );
    window.invoke_tab_chosen(1);
    assert_eq!(
        window.get_label(),
        format!("Found {} pairs.", actioned_before + 2)
    );
}

/// Let the preview test every pair it fetched.
fn tested(rule: &hydrus_gui::AutoResolutionRuleWindow) {
    for _ in 0..3000 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        if rule.get_preview_search_label().contains("pairs searched")
            && rule.get_preview_to_test_label().is_empty()
        {
            return;
        }
    }
    panic!("the preview never finished");
}

// leaf: audit-media-preview-context
#[test]
fn preview_pair_lists_offer_show_selected_pairs_in_a_new_page() {
    let Opened {
        ui, bound, store, ..
    } = opened();
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
    rule.set_tab(3);
    rule.invoke_preview_shown();
    tested(&rule);
    let passed = rule.get_preview_pass_rows().row_count();
    assert!(passed >= 2, "{passed} passing pairs");

    // nothing selected: no menu
    assert_eq!(rule.get_preview_pass_menu(), "");
    // one row, then two (control-click), each with the reference's label
    rule.invoke_preview_pass_clicked(0, false, false);
    assert_eq!(
        rule.get_preview_pass_menu(),
        "show selected row in a new page"
    );
    rule.invoke_preview_pass_clicked(1, true, false);
    assert_eq!(rule.get_preview_pass_menu(), "show 2 rows in a new page");
    // the other list's menu is its own
    assert_eq!(rule.get_preview_fail_menu(), "");
    let pages_before = bound.pages.borrow().open_pages().len();
    rule.invoke_preview_pass_show();
    assert_eq!(bound.pages.borrow().open_pages().len(), pages_before + 1);
    let mut shown = bound.current.borrow().borrow().files();
    shown.sort();
    shown.dedup();
    // both pairs' files (a pair's two files, twice over), whatever was shown
    assert!(shown.len() >= 3 && shown.len() <= 4, "{shown:?}");
    let _ = store;
}

pub(crate) fn rule_window(bound: &Bound) -> hydrus_gui::AutoResolutionRuleWindow {
    bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor is open")
        .clone_strong()
}

pub(crate) fn newest_comparator(bound: &Bound) -> hydrus_gui::ComparatorWindow {
    bound
        .auto_resolution
        .comparators
        .borrow()
        .last()
        .map(|(_, w)| w.clone_strong())
        .expect("a comparator editor is open")
}

pub(crate) fn kind_index(window: &hydrus_gui::AutoResolutionRuleWindow, name: &str) -> i32 {
    window
        .get_comparator_kinds()
        .iter()
        .position(|k| k == name)
        .unwrap_or_else(|| panic!("no comparator kind {name:?}")) as i32
}

pub(crate) fn list_of(bound: &Bound) -> hydrus_gui::AutoResolutionRulesWindow {
    bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .expect("the rules list is open")
        .clone_strong()
}

// leaf: audit-media-comparator-hardcoded, audit-media-comparator-relative, audit-media-comparator-visual, audit-media-comparator-and
#[test]
fn edited_comparators_are_accepted_and_persisted_with_their_summaries() {
    use hydrus_core::search::comparable::Comparable;
    use hydrus_core::search::number::NumberOp;
    use hydrus_gui_model::auto_resolution_rules::{
        PROPERTIES, comparator_summary, operator_choices, visual_text,
    };
    use hydrus_store::duplicates::auto::{self, Comparator, LookingAt, OneFileTest};

    let Opened {
        ui, bound, store, ..
    } = opened();
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_add();
    let rule = rule_window(&bound);
    rule.set_name("comparators".into());
    rule.invoke_changed();
    let context = hydrus_search::TextContext::default();
    let summary_of = |c: &Comparator| comparator_summary(c, &context);

    // one-file hardcoded: target B, the non-progressive test, together
    rule.set_comparator_kind(kind_index(&rule, "test A or B using other file info"));
    rule.invoke_comparator_add();
    let hardcoded = newest_comparator(&bound);
    assert_eq!(
        hardcoded.get_window_title(),
        "edit one-file hardcoded comparator"
    );
    assert_eq!(
        hardcoded
            .get_looking_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        ["A will match", "B will match", "either will match"]
    );
    hardcoded.set_looking(1);
    hardcoded.set_test(1);
    hardcoded.invoke_changed();
    let expected = Comparator::OneFileHardcoded {
        looking_at: LookingAt::B,
        test: OneFileTest::JpegIsNotProgressive,
    };
    assert_eq!(hardcoded.get_summary(), summary_of(&expected));
    hardcoded.invoke_apply();

    // relative: width, within 15% of 2.5x B plus a delta of 3
    rule.set_comparator_kind(kind_index(&rule, "test A against B using file info"));
    rule.invoke_comparator_add();
    let relative = newest_comparator(&bound);
    let width = PROPERTIES
        .iter()
        .position(|p| *p == Comparable::Width)
        .unwrap();
    relative.set_property(width as i32);
    relative.invoke_changed();
    let approx = operator_choices(Comparable::Width)
        .iter()
        .position(|o| matches!(o, NumberOp::ApproxPercent { .. }))
        .unwrap();
    relative.set_operator(approx as i32);
    relative.invoke_changed();
    assert!(relative.get_approximate());
    relative.set_range(15);
    relative.set_multiplier("2.5".into());
    relative.set_delta(3);
    relative.invoke_changed();
    assert_eq!(relative.get_errors(), "");
    // a multiplier that is no number is refused before applying
    relative.set_multiplier("lots".into());
    relative.invoke_changed();
    assert_eq!(relative.get_errors(), "The multiplier is not a number.");
    relative.set_multiplier("2.5".into());
    relative.invoke_changed();
    relative.invoke_apply();

    // visual duplicates: the top confidence
    rule.set_comparator_kind(kind_index(&rule, "test if A and B are visual duplicates"));
    rule.invoke_comparator_add();
    let visual = newest_comparator(&bound);
    assert_eq!(
        visual
            .get_visual_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        [visual_text(60), visual_text(85), visual_text(100)]
    );
    visual.set_visual(2);
    visual.invoke_changed();
    assert_eq!(
        visual.get_summary(),
        summary_of(&Comparator::VisualDuplicates { confidence: 100 })
    );
    visual.invoke_apply();

    // AND: two children added in nested editors, then one deleted
    rule.set_comparator_kind(kind_index(&rule, "AND Comparator"));
    rule.invoke_comparator_add();
    let and = newest_comparator(&bound);
    assert_eq!(and.get_window_title(), "edit AND comparator");
    and.set_sub_kind(kind_index_sub(
        &and,
        "test if A and B are visual duplicates",
    ));
    and.invoke_sub_add();
    let child = newest_comparator(&bound);
    child.set_visual(0);
    child.invoke_changed();
    child.invoke_apply();
    and.set_sub_kind(kind_index_sub(&and, "test A or B using other file info"));
    and.invoke_sub_add();
    let child = newest_comparator(&bound);
    child.set_looking(2);
    child.set_test(0);
    child.invoke_changed();
    child.invoke_apply();
    assert_eq!(and.get_subs().row_count(), 2);
    and.invoke_sub_clicked(0);
    and.invoke_sub_delete();
    assert_eq!(and.get_subs().row_count(), 1);
    and.invoke_apply();

    assert_eq!(rule.get_errors(), "");
    rule.invoke_apply();
    assert!(bound.auto_resolution.rule.borrow().is_none(), "applied");
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let mine = &written
        .iter()
        .find(|(_, r)| r.name == "comparators")
        .unwrap()
        .1;
    assert_eq!(mine.comparators.len(), 4);
    assert_eq!(mine.comparators[0], expected);
    let Comparator::RelativeFileInfo {
        property,
        test,
        multiplier,
        delta,
    } = &mine.comparators[1]
    else {
        panic!("{:?}", mine.comparators[1])
    };
    assert_eq!(*property, Comparable::Width);
    assert_eq!(test.op, NumberOp::ApproxPercent { percent: 15 });
    assert!((multiplier - 2.5).abs() < 1e-9 && *delta == 3);
    assert_eq!(
        mine.comparators[2],
        Comparator::VisualDuplicates { confidence: 100 }
    );
    assert_eq!(
        mine.comparators[3],
        Comparator::And(vec![Comparator::OneFileHardcoded {
            looking_at: LookingAt::Either,
            test: OneFileTest::JpegIsProgressive,
        }])
    );

    // reopened, the accepted values are shown again
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    let row = (0..list.get_rows().row_count())
        .position(|r| {
            list.get_rows()
                .row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                == "comparators"
        })
        .unwrap();
    list.invoke_row_clicked(row as i32, false, false);
    list.invoke_edit();
    let rule = rule_window(&bound);
    rule.invoke_comparator_clicked(1);
    rule.invoke_comparator_edit();
    let relative = newest_comparator(&bound);
    assert_eq!(relative.get_multiplier(), "2.50");
    assert_eq!(relative.get_delta(), 3);
    assert_eq!(relative.get_range(), 15);
    assert_eq!(relative.get_property(), width as i32);
}

pub(crate) fn kind_index_sub(window: &hydrus_gui::ComparatorWindow, name: &str) -> i32 {
    window
        .get_sub_kinds()
        .iter()
        .position(|k| k == name)
        .unwrap_or_else(|| panic!("no comparator kind {name:?}")) as i32
}

fn pair_counts(store: &Store, rule: i64) -> std::collections::BTreeMap<i64, i64> {
    store
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT status, COUNT(*) FROM dup_auto_pairs WHERE rule_id = ? GROUP BY status",
            )?;
            Ok(stmt
                .query_map([rule], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap()
}

fn rule_rows(ui: &MainWindow) -> Vec<Vec<String>> {
    let rows = ui.get_duplicates_rules();
    (0..rows.row_count())
        .map(|r| {
            let cells = rows.row_data(r).unwrap().cells;
            cells.iter().map(|c| c.to_string()).collect()
        })
        .collect()
}

// leaf: audit-media-rule-sidebar-progress, audit-media-rule-sidebar-resets
#[test]
fn the_rules_list_selects_pauses_shows_progress_and_resets_queues() {
    use hydrus_gui_model::duplicates_page::{Reset, reset_question, rule_progress};
    use hydrus_store::duplicates::auto::{self, PairStatus};

    let _windows = headless::init();
    let o = crate::auto_resolution_review::opened();
    let (ui, store) = (&o.ui, &o.store);
    let rules = store.read(auto::rules).unwrap();
    assert!(rules.len() >= 2);

    // progress text from the live counts of each rule
    let shown = rule_rows(ui);
    assert_eq!(shown.len(), rules.len());
    for (name, progress) in shown.iter().map(|r| (&r[0], &r[1])) {
        let (id, _) = rules.iter().find(|(_, r)| &r.name == name).unwrap();
        let counts = store.read(|c| auto::counts(c, *id)).unwrap();
        assert_eq!(*progress, rule_progress(&counts), "{name}");
    }

    // click selects one row; ctrl-click adds, shift-click ranges
    assert!(!ui.get_duplicates().any_rule_selected);
    ui.invoke_duplicates_action("rule".into(), 0, false, false);
    ui.invoke_duplicates_action("rule".into(), 1, true, false);
    let selected = |ui: &MainWindow| -> Vec<bool> {
        let rows = ui.get_duplicates_rules();
        (0..rows.row_count())
            .map(|r| rows.row_data(r).unwrap().selected)
            .collect()
    };
    assert_eq!(&selected(ui)[..2], &[true, true]);
    ui.invoke_duplicates_action("rule".into(), 1, false, false);
    assert_eq!(&selected(ui)[..2], &[false, true]);

    // pausing flips every selected rule (by its stable id)
    let before: Vec<String> = rule_rows(ui).iter().map(|r| r[2].clone()).collect();
    assert_ne!(before[1], "paused");
    ui.invoke_duplicates_action("pause rules".into(), 0, false, false);
    let paused = rule_rows(ui);
    assert_eq!(paused[1][2], "paused");
    let name = paused[1][0].clone();
    assert!(
        store
            .read(auto::rules)
            .unwrap()
            .iter()
            .any(|(_, r)| r.name == name && r.paused)
    );
    assert_eq!(paused[0][2], before[0], "the unselected rule is left alone");
    ui.invoke_duplicates_action("pause rules".into(), 0, false, false);
    assert_ne!(rule_rows(ui)[1][2], "paused");

    // resets, of the selected rule: asked first (naming how many rules),
    // refused or accepted
    let rule = rules
        .iter()
        .find(|(_, r)| r.name == rule_rows(ui)[1][0])
        .unwrap()
        .0;
    let all: i64 = pair_counts(store, rule).values().sum();
    let pairs_in = |status: PairStatus| {
        pair_counts(store, rule)
            .get(&i64::from(status.code()))
            .copied()
            .unwrap_or(0)
    };
    // give it something of each kind to move
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE dup_auto_pairs SET status = ? WHERE rule_id = ? AND (smaller_group_id, larger_group_id) IN
                 (SELECT smaller_group_id, larger_group_id FROM dup_auto_pairs
                  WHERE rule_id = ? AND status = ? LIMIT 1)",
                rusqlite::params![PairStatus::Denied.code(), rule, rule, PairStatus::ReadyToAction.code()],
            )?;
            Ok(())
        })
        .unwrap();
    for (action, which) in [
        ("reset test", Reset::Test),
        ("reset search", Reset::Search),
        ("reset denied", Reset::Denied),
    ] {
        let denied = pairs_in(PairStatus::Denied);
        let actioned = pairs_in(PairStatus::Actioned);
        ui.invoke_duplicates_action(action.into(), 0, false, false);
        assert!(ui.get_duplicates().asking, "{action}");
        assert_eq!(ui.get_duplicates().asking_message, reset_question(which, 1));
        ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
        assert_eq!(pair_counts(store, rule).values().sum::<i64>(), all);
        ui.invoke_duplicates_action(action.into(), 0, false, false);
        ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        match which {
            Reset::Test => {
                assert_eq!(pairs_in(PairStatus::FailedTest), 0);
                assert_eq!(pairs_in(PairStatus::ReadyToAction), 0);
                assert_eq!(pairs_in(PairStatus::Denied), denied, "denials stay");
            }
            Reset::Search => {
                assert_eq!(pairs_in(PairStatus::MatchesSearchNotTested), 0);
                assert_eq!(pairs_in(PairStatus::DoesNotMatchSearch), 0);
                assert_eq!(pairs_in(PairStatus::Denied), denied, "denials stay");
                assert_eq!(pairs_in(PairStatus::Actioned), actioned);
            }
            Reset::Denied => {
                assert_eq!(pairs_in(PairStatus::Denied), 0);
                assert!(pairs_in(PairStatus::NotSearched) >= denied);
            }
        }
        assert_eq!(pair_counts(store, rule).values().sum::<i64>(), all);
    }
    assert!(pairs_in(PairStatus::NotSearched) > 0);

    // with none selected, a reset asks about every rule
    ui.invoke_duplicates_action("rule".into(), 1, true, false);
    assert!(!ui.get_duplicates().any_rule_selected);
    ui.invoke_duplicates_action("reset search".into(), 0, false, false);
    assert_eq!(
        ui.get_duplicates().asking_message,
        reset_question(Reset::Search, rules.len())
    );
}

fn edit_row(bound: &Bound, name: &str) -> hydrus_gui::AutoResolutionRuleWindow {
    let list = list_of(bound);
    let rows = list.get_rows();
    let row = (0..rows.row_count())
        .position(|r| rows.row_data(r).unwrap().cells.row_data(0).unwrap() == name)
        .unwrap_or_else(|| panic!("no rule {name:?}"));
    list.invoke_row_clicked(row as i32, false, false);
    list.invoke_edit();
    rule_window(bound)
}

// leaf: audit-media-rules-list, audit-media-rule-identity, audit-media-duplicate-search-kind, audit-media-duplicate-search-pixels, audit-media-duplicate-search-predicates
#[test]
fn rule_identity_and_search_choices_are_staged_then_persisted_on_apply() {
    use hydrus_core::duplicates::PairSearchKind as K;
    use hydrus_store::duplicates::auto::{self, OperationMode};
    let Opened {
        ui, bound, store, ..
    } = opened();

    // staged: a list cancelled writes nothing
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(1);
    assert_eq!(list.get_rows().row_count(), 2);
    assert!(store.read(auto::rules).unwrap().is_empty());
    list.invoke_cancel();
    assert!(store.read(auto::rules).unwrap().is_empty());
    assert!(bound.auto_resolution.list.borrow().is_none());

    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(1);
    let names: Vec<String> = (0..2)
        .map(|r| {
            list.get_rows()
                .row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect();

    // identity and the search kind, pixel preference, distance and the two
    // predicate lists, each of the three kinds in turn
    let rounds = [
        (2, 0, 6, "system:inbox", "system:archive"),
        (1, 2, 0, "system:filetype is image", "system:archive"),
        (0, 1, 12, "system:everything", "system:inbox"),
    ];
    let mut name = names[0].clone();
    for (round, (kind, pixel, distance, s1, s2)) in rounds.into_iter().enumerate() {
        let rule = edit_row(&bound, &name);
        assert_eq!(
            rule.get_pair_search_choices()
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>(),
            [
                "at least one file matches the search",
                "both files match the search",
                "the two files match different searches",
            ]
        );
        assert_eq!(
            rule.get_pixel_choices()
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>(),
            [
                "must be pixel dupes",
                "can be pixel dupes",
                "must not be pixel dupes"
            ]
        );
        rule.set_name(format!("mine {round}").as_str().into());
        rule.set_paused(round % 2 == 0);
        rule.set_operation((round % 2) as i32);
        rule.set_pair_search(kind);
        rule.set_pixel(pixel);
        rule.set_distance(distance);
        rule.set_search_1(s1.into());
        rule.set_search_2(s2.into());
        rule.invoke_changed();
        assert_eq!(rule.get_errors(), "");
        // a predicate that isn't one is refused where it is, and the editor
        // stays open
        rule.set_search_1("system:nonsense here".into());
        rule.invoke_changed();
        assert!(
            rule.get_errors().starts_with("Problem with the search:"),
            "{}",
            rule.get_errors()
        );
        rule.invoke_apply();
        assert!(bound.auto_resolution.rule.borrow().is_some(), "refused");
        rule.set_search_1(s1.into());
        rule.invoke_changed();
        rule.invoke_apply();
        assert!(bound.auto_resolution.rule.borrow().is_none());
        name = format!("mine {round}");
    }
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    assert_eq!(written.len(), 2);
    let mine = &written.iter().find(|(_, r)| r.name == "mine 2").unwrap().1;
    // round 2: paused (even), semi-automatic, kind 0, must-not... pixel 1
    assert!(mine.paused);
    assert_eq!(mine.mode, OperationMode::SemiAutomatic);
    assert_eq!(
        mine.search.kind,
        hydrus_core::duplicates::PairSearchKind::OneFileMatchesOneSearch
    );
    assert_eq!(mine.search.pixel_duplicates, PixelDuplicates::Allowed);
    assert_eq!(mine.search.max_hamming_distance, 12);
    assert_eq!(mine.search.search_1.predicates.len(), 1);
    assert_eq!(mine.search.search_2.predicates.len(), 1);

    // reopened: all three kinds were chosen in turn; check the first two by
    // writing and reading each again
    for (kind, pixel, distance) in [(2, 0, 6), (1, 2, 0)] {
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let rule = edit_row(&bound, "mine 2");
        rule.set_pair_search(kind);
        rule.set_pixel(pixel);
        rule.set_distance(distance);
        rule.set_operation(kind % 2);
        rule.set_paused(kind == 1);
        rule.invoke_changed();
        rule.invoke_apply();
        list_of(&bound).invoke_apply();
        let written = store.read(auto::rules).unwrap();
        let mine = &written.iter().find(|(_, r)| r.name == "mine 2").unwrap().1;
        assert_eq!(
            mine.search.kind,
            [
                K::OneFileMatchesOneSearch,
                K::BothFilesMatchOneSearch,
                K::BothFilesMatchDifferentSearches
            ][kind as usize]
        );
        assert_eq!(
            mine.search.pixel_duplicates,
            [
                PixelDuplicates::Required,
                PixelDuplicates::Allowed,
                PixelDuplicates::Excluded
            ][pixel as usize]
        );
        assert_eq!(mine.search.max_hamming_distance, distance as u32);
        assert_eq!(
            mine.mode,
            [OperationMode::SemiAutomatic, OperationMode::FullyAutomatic][(kind % 2) as usize]
        );
        assert_eq!(mine.paused, kind == 1);
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let rule = edit_row(&bound, "mine 2");
        assert_eq!(
            (
                rule.get_pair_search(),
                rule.get_pixel(),
                rule.get_distance()
            ),
            (kind, pixel, distance)
        );
        assert_eq!(
            (rule.get_operation(), rule.get_paused()),
            (kind % 2, kind == 1)
        );
        rule.invoke_cancel();
        list_of(&bound).invoke_cancel();
    }

    // the maximum pending queue, and the mode that goes with it
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let rule = edit_row(&bound, "mine 2");
    rule.set_operation(0);
    rule.set_no_pending_limit(false);
    rule.set_pending_limit(77);
    rule.invoke_changed();
    rule.invoke_apply();
    // a name another rule has (casefolded) is made unique, as the reference's
    // SetNonDupeName does
    let other = names[1].clone();
    let rule = edit_row(&bound, "mine 2");
    rule.set_name(other.to_uppercase().as_str().into());
    rule.invoke_changed();
    rule.invoke_apply();
    list_of(&bound).invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let mine = written
        .iter()
        .map(|(_, r)| r)
        .find(|r| r.name != other)
        .unwrap();
    assert_eq!(mine.max_pending_pairs, Some(77));
    assert_eq!(mine.mode, OperationMode::SemiAutomatic);
    assert_eq!(mine.name, format!("{} (1)", other.to_uppercase()));
}

// leaf: audit-media-preparation-work-hard
#[test]
fn working_hard_is_started_and_stopped_and_needs_something_left_to_search() {
    use hydrus_store::similar::SimilarFilesSettings;
    let Opened {
        store, ui, bound, ..
    } = opened();
    let _keep = &bound;
    let written = || -> SimilarFilesSettings { store.read(hydrus_store::settings::get).unwrap() };
    // everything searched at the stored distance: nothing to start
    assert!(!ui.get_duplicates().can_start);
    ui.invoke_duplicates_action("work hard".into(), 1, false, false);
    assert!(written().work_hard, "stored all the same");
    assert!(!ui.get_duplicates().working_hard, "but nothing to work on");
    ui.invoke_duplicates_action("work hard".into(), 0, false, false);
    assert!(!written().work_hard);
    // a wider distance has files left to search
    ui.invoke_duplicates_action("distance".into(), 8, false, false);
    assert!(ui.get_duplicates().can_start && !ui.get_duplicates().working_hard);
    ui.invoke_duplicates_action("work hard".into(), 1, false, false);
    assert!(ui.get_duplicates().working_hard && written().work_hard);
    // and stopped again
    ui.invoke_duplicates_action("work hard".into(), 0, false, false);
    assert!(!ui.get_duplicates().working_hard && !written().work_hard);
}

// leaf: audit-media-comparator-conditional
#[test]
fn a_metadata_conditional_comparators_target_and_predicate_lines_are_persisted() {
    use hydrus_store::duplicates::auto::{self, Comparator, LookingAt};
    let Opened {
        ui, bound, store, ..
    } = opened();
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_add();
    let rule = rule_window(&bound);
    rule.set_comparator_kind(kind_index(&rule, "test A or B using search terms"));
    rule.invoke_comparator_add();
    let c = newest_comparator(&bound);
    assert_eq!(
        c.get_looking_choices()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        [
            "A will match these",
            "B will match these",
            "either will match these"
        ]
    );
    // a line that is no predicate is refused, with the target unchanged
    c.set_predicates("system:inbox\nsystem:nonsense here".into());
    c.invoke_changed();
    assert!(!c.get_errors().is_empty());
    // the target B, two predicate lines
    c.set_looking(1);
    c.set_predicates("system:inbox\nsystem:filetype is image".into());
    c.invoke_changed();
    assert_eq!(c.get_errors(), "");
    assert!(
        c.get_summary().starts_with("B will match: "),
        "{}",
        c.get_summary()
    );
    c.invoke_apply();
    rule.invoke_apply();
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let Comparator::OneFileMetadata {
        looking_at,
        predicates,
    } = &written[0].1.comparators[0]
    else {
        panic!("{:?}", written[0].1.comparators)
    };
    assert_eq!(*looking_at, LookingAt::B);
    assert_eq!(predicates.len(), 2);
    // reopened, the lines are as they were
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let rule = edit_row(&bound, "new rule");
    rule.invoke_comparator_clicked(0);
    rule.invoke_comparator_edit();
    let c = newest_comparator(&bound);
    assert_eq!(c.get_looking(), 1);
    assert_eq!(c.get_predicates().lines().count(), 2);
}

// (not tagged audit-media-preparation-scheduling / audit-media-rule-sidebar-scheduling:
// this replays the daemon's decision, not the daemon; see the review in
// docs/rust/notes/impl-small-areas.md)
#[test]
fn the_idle_and_normal_time_switches_gate_the_search_and_the_rules_by_the_published_idle_state() {
    use hydrus_store::duplicates::auto::AutoResolutionSettings;
    use hydrus_store::idle_state;
    use hydrus_store::similar::SimilarFilesSettings;
    let Opened {
        _dir,
        _windows,
        store,
        ui,
        bound,
    } = opened();
    let _keep = &bound;
    let now = 1_000_000;
    // the daemon's decision: the stored switches, by the state the GUI published
    let allowed = |idle_published: bool| -> [bool; 2] {
        idle_state::publish(store.dir(), idle_published, now).unwrap();
        let idle = idle_state::is_idle(store.dir(), now);
        assert_eq!(idle, idle_published);
        let similar: SimilarFilesSettings = store.read(hydrus_store::settings::get).unwrap();
        let auto: AutoResolutionSettings = store.read(hydrus_store::settings::get).unwrap();
        // (the daemon also lets "work hard" through, off here)
        [similar.pace(idle).allowed, auto.pace(idle).allowed]
    };
    assert_eq!([allowed(true), allowed(false)], [[true; 2], [true; 2]]);

    // idle only: the sidebar's normal-time switches off
    ui.invoke_duplicates_action("search during active".into(), 0, false, false);
    ui.invoke_duplicates_action("rules during active".into(), 0, false, false);
    let data = ui.get_duplicates();
    assert!(!data.search_during_active && !data.rules_during_active);
    assert!(data.search_during_idle && data.rules_during_idle);
    assert_eq!([allowed(true), allowed(false)], [[true; 2], [false; 2]]);

    // normal time only
    ui.invoke_duplicates_action("search during active".into(), 0, false, false);
    ui.invoke_duplicates_action("rules during active".into(), 0, false, false);
    ui.invoke_duplicates_action("search during idle".into(), 0, false, false);
    ui.invoke_duplicates_action("rules during idle".into(), 0, false, false);
    assert_eq!([allowed(true), allowed(false)], [[false; 2], [true; 2]]);

    // each independent of the other
    ui.invoke_duplicates_action("rules during idle".into(), 0, false, false);
    assert_eq!([allowed(true), allowed(false)], [[false, true], [true; 2]]);

    // a stale published state reads as not idle
    idle_state::publish(store.dir(), true, now).unwrap();
    assert!(!idle_state::is_idle(
        store.dir(),
        now + idle_state::FRESH_MS + 1
    ));
}

// leaf: audit-media-rules-exchange
#[test]
fn rules_and_comparators_are_exported_imported_and_duplicated_whole() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let Opened { ui, bound, .. } = opened();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let pasted: Rc<RefCell<String>> = Rc::default();
    hydrus_gui::set_paster({
        let pasted = pasted.clone();
        move || pasted.borrow().clone()
    });
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    let names = |list: &hydrus_gui::AutoResolutionRulesWindow| -> Vec<String> {
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
    };
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    assert_eq!(names(&list).len(), 1);
    let first = names(&list)[0].clone();

    // export to the clipboard, then import it: a second, whole copy
    list.invoke_row_clicked(0, false, false);
    list.invoke_exchange(0);
    let text = copied.borrow().last().cloned().expect("on the clipboard");
    *pasted.borrow_mut() = text;
    list.invoke_exchange(3);
    let after = names(&list);
    assert_eq!(after.len(), 2);
    assert_ne!(
        after[0], after[1],
        "named apart from the rule already there"
    );
    assert!(after[1].to_lowercase().starts_with(&first.to_lowercase()));
    // duplicate: another
    list.invoke_row_clicked(0, false, false);
    list.invoke_exchange(6);
    assert_eq!(names(&list).len(), 3);
    // something that is not a rule is refused and adds nothing
    *pasted.borrow_mut() = "not a rule".into();
    list.invoke_exchange(3);
    assert_eq!(names(&list).len(), 3);

    // the comparators of a rule: export, import, duplicate
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor opens")
        .clone_strong();
    let count = || rule.get_comparators().row_count();
    let start = count();
    assert!(start >= 1);
    rule.invoke_comparator_clicked(0);
    rule.invoke_comparator_exchange(4);
    assert_eq!(count(), start + 1);
    assert_eq!(
        rule.get_comparators().row_data(0),
        rule.get_comparators().row_data(start),
        "the whole comparator is copied"
    );
    rule.invoke_comparator_clicked(0);
    rule.invoke_comparator_exchange(0);
    let text = copied.borrow().last().cloned().unwrap();
    assert!(text.contains("comparators"));
    *pasted.borrow_mut() = text.clone();
    rule.invoke_comparator_exchange(2);
    assert_eq!(count(), start + 2);
    *pasted.borrow_mut() = "nonsense".into();
    rule.invoke_comparator_exchange(2);
    assert_eq!(count(), start + 2);
}

// The pngs carry hydrus-rs JSON (DIFFERENCES.md): no reference png is replayed.
// leaf: audit-media-rules-exchange
#[test]
fn rules_and_comparators_go_out_and_come_in_as_pngs() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let Opened { ui, bound, .. } = opened();
    let temp = tempfile::tempdir().unwrap();
    let picked: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::default();
    hydrus_gui::set_picker({
        let picked = picked.clone();
        move |_, _| picked.borrow().clone()
    });
    let names = |list: &hydrus_gui::AutoResolutionRulesWindow| -> Vec<String> {
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
    };
    let export = |path: &std::path::Path| {
        let window = hydrus_gui::png_export_window::last().expect("the png export opens");
        window.set_path(path.to_string_lossy().as_ref().into());
        window.set_png_title("rules".into());
        window.invoke_action("update".into());
        window.invoke_action("export".into());
        assert!(window.get_done(), "{}", window.get_error());
        window.invoke_action("close".into());
    };
    let said = || {
        let window = hydrus_gui::message_window().expect("a message is shown");
        let said = (
            window.get_window_title().to_string(),
            window.get_message().to_string(),
        );
        window.invoke_cancelled();
        said
    };

    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    let first = names(&list)[0].clone();

    // rules: export the selected one to a png, import it again
    list.invoke_row_clicked(0, false, false);
    list.invoke_exchange(2);
    let rules_png = temp.path().join("rule.png");
    export(&rules_png);
    assert!(rules_png.exists());
    *picked.borrow_mut() = vec![rules_png];
    list.invoke_exchange(5);
    let after = names(&list);
    assert_eq!(after.len(), 2);
    assert_ne!(
        after[0], after[1],
        "named apart from the rule already there"
    );
    assert!(after[1].to_lowercase().starts_with(&first.to_lowercase()));
    assert_eq!(said(), ("Information".into(), "1 objects added!".into()));
    // a png that carries no payload: the file wording, not the clipboard's
    let bad = temp.path().join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    *picked.borrow_mut() = vec![bad];
    list.invoke_exchange(5);
    assert_eq!(names(&list).len(), 2);
    let (title, message) = said();
    assert_eq!(title, "Problem importing!");
    assert!(!message.is_empty());
    assert_eq!(
        message,
        hydrus_downloader_exchange::text_png::decode(b"not a png")
            .unwrap_err()
            .to_string()
    );

    // comparators of a rule
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor opens")
        .clone_strong();
    let count = || rule.get_comparators().row_count();
    let start = count();
    rule.invoke_comparator_clicked(0);
    rule.invoke_comparator_exchange(1);
    let comparator_png = temp.path().join("comparator.png");
    export(&comparator_png);
    *picked.borrow_mut() = vec![comparator_png];
    rule.invoke_comparator_exchange(3);
    assert_eq!(count(), start + 1);
    assert_eq!(
        rule.get_comparators().row_data(0),
        rule.get_comparators().row_data(start),
        "the whole comparator comes back"
    );
    assert_eq!(said().1, "1 objects added!");
}
