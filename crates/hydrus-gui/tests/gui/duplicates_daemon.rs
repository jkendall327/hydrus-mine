//! The two duplicates workers of `hydrus serve`, one turn at a time through
//! the code the daemon runs (`hydrus_duplicates::daemon`): the similar-files
//! search and the auto-resolution rules hold or work according to the
//! sidebar's idle-time and normal-time switches and the idle state the GUI
//! published, and the work they do is seen in the same numbers the sidebar
//! shows. (The daemon's loop only sleeps for the rest a turn asks for.)

use std::collections::BTreeMap;
use std::time::Duration;

use slint::Model as _;

use hydrus_duplicates::NoShuffle;
use hydrus_duplicates::daemon::{Turn, auto_resolution_turn, similar_files_turn};
use hydrus_search::Clock;
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self, PairStatus};
use hydrus_store::idle_state;
use hydrus_store::similar;

use crate::duplicates_lane_page::opened;

const NOW: i64 = 1_000_000;

fn publish(store: &Store, idle: bool) {
    idle_state::publish(store.dir(), idle, NOW).unwrap();
}

fn not_yet_searched(store: &Store) -> usize {
    store
        .read(similar::search_status_counts)
        .unwrap()
        .get(&None)
        .copied()
        .unwrap_or(0)
}

fn potential_pairs(store: &Store) -> i64 {
    store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM potential_pairs", [], |r| r.get(0))?))
        .unwrap()
}

// leaf: audit-media-preparation-scheduling
#[test]
fn the_search_worker_holds_or_works_by_the_switches_and_the_published_idle_state() {
    let o = opened();
    let (ui, store) = (&o.ui, &o.store);
    let pairs = potential_pairs(store);
    assert!(pairs > 0);

    // every file to be searched again
    let forget = || {
        ui.invoke_duplicates_action("delete pairs".into(), 0, false, false);
        ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        assert_eq!(potential_pairs(store), 0);
        assert!(not_yet_searched(store) > 0);
    };
    let searched_all = || {
        assert_eq!(not_yet_searched(store), 0);
        assert_eq!(potential_pairs(store), pairs);
    };
    let held = |idle_published: bool, at: i64| {
        publish(store, idle_published);
        let before = not_yet_searched(store);
        let tick = similar_files_turn(store, at);
        assert_eq!(tick.turn, Turn::Held, "idle {idle_published} at {at}");
        assert_eq!(tick.rest, Duration::from_secs(10));
        assert_eq!(not_yet_searched(store), before, "a held turn searched");
        assert_eq!(potential_pairs(store), 0);
    };
    let works = |idle_published: bool| {
        publish(store, idle_published);
        let tick = similar_files_turn(store, NOW);
        let Turn::Worked(searched) = tick.turn else {
            panic!("idle {idle_published}: {:?}", tick.turn)
        };
        assert!(searched.files > 0);
        searched_all();
    };

    // both switches on (the default): either state works
    forget();
    works(true);
    forget();
    works(false);

    // idle time only: the sidebar's normal-time switch off
    ui.invoke_duplicates_action("search during active".into(), 0, false, false);
    assert!(!ui.get_duplicates().search_during_active);
    forget();
    held(false, NOW);
    // (a published state is fresh for a while, then it reads as not idle)
    held(true, NOW + idle_state::FRESH_MS + 1);
    works(true);

    // normal time only
    ui.invoke_duplicates_action("search during active".into(), 0, false, false);
    ui.invoke_duplicates_action("search during idle".into(), 0, false, false);
    assert!(ui.get_duplicates().search_during_active && !ui.get_duplicates().search_during_idle);
    forget();
    held(true, NOW);
    works(false);

    // neither
    ui.invoke_duplicates_action("search during active".into(), 0, false, false);
    assert!(!ui.get_duplicates().search_during_active);
    forget();
    held(true, NOW);
    held(false, NOW);
}

fn counts(store: &Store, rule: i64) -> BTreeMap<PairStatus, u64> {
    store.read(|c| auto::counts(c, rule)).unwrap()
}

// leaf: audit-media-rule-sidebar-scheduling
#[test]
fn the_rules_worker_holds_or_works_by_the_switches_and_the_published_idle_state() {
    let _windows = hydrus_gui::headless::init();
    let o = crate::auto_resolution_review::opened();
    let (ui, store) = (&o.ui, &o.store);
    let rules = store.read(auto::rules).unwrap();
    let (rule, _) = rules.iter().find(|(_, r)| !r.paused).unwrap().clone();
    let name = rules.iter().find(|(id, _)| *id == rule).unwrap().1.name.clone();

    // select the rule and reset its search: its pairs are to be searched
    let rows = ui.get_duplicates_rules();
    let row = (0..rows.row_count())
        .find(|&r| rows.row_data(r).unwrap().cells.iter().next().is_some_and(|c| c == name.as_str()))
        .expect("the rule is listed");
    let waiting = |s: &Store| counts(s, rule)[&PairStatus::NotSearched];
    let forget = || {
        ui.invoke_duplicates_action("rule".into(), i32::try_from(row).unwrap(), false, false);
        ui.invoke_duplicates_action("reset search".into(), 0, false, false);
        ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        assert!(waiting(store) > 0);
        // (the selection stays for the next reset, so click it off)
        ui.invoke_duplicates_action("rule".into(), i32::try_from(row).unwrap(), true, false);
    };
    let turn = |idle_published: bool, at: i64| {
        publish(store, idle_published);
        auto_resolution_turn(store, at, &mut NoShuffle, &Clock::system())
    };
    let held = |idle_published: bool, at: i64| {
        let before = counts(store, rule);
        let tick = turn(idle_published, at);
        assert_eq!(tick.turn, Turn::Held, "idle {idle_published} at {at}");
        assert_eq!(tick.rest, Duration::from_secs(10));
        assert_eq!(counts(store, rule), before, "a held turn worked");
    };
    let works = |idle_published: bool| {
        let before = waiting(store);
        let tick = turn(idle_published, NOW);
        let Turn::Worked(done) = tick.turn else {
            panic!("idle {idle_published}: {:?}", tick.turn)
        };
        assert!(done.searched > 0, "{done:?}");
        assert!(waiting(store) < before);
    };

    // both switches on (the default): either state works
    forget();
    works(true);
    forget();
    works(false);

    // idle time only: the sidebar's normal-time switch off
    ui.invoke_duplicates_action("rules during active".into(), 0, false, false);
    assert!(!ui.get_duplicates().rules_during_active);
    forget();
    held(false, NOW);
    held(true, NOW + idle_state::FRESH_MS + 1);
    works(true);

    // normal time only
    ui.invoke_duplicates_action("rules during active".into(), 0, false, false);
    ui.invoke_duplicates_action("rules during idle".into(), 0, false, false);
    assert!(ui.get_duplicates().rules_during_active && !ui.get_duplicates().rules_during_idle);
    forget();
    held(true, NOW);
    works(false);
}
