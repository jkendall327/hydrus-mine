//! The duplicates page's pair count, driven as a user would: it fetches the
//! pairs, counts them a block at a time off the UI thread, and the play/pause
//! and refresh buttons, the cog and any change to the search drive it, as in
//! the reference (`oracle/record_potential_duplicates_count.py`; the whole
//! recording is replayed against the same worker in hydrus-gui-model's tests).
//! A test gate holds each fetch and block until the test lets it through, so
//! the count can be read between two blocks.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use slint::Model as _;

use hydrus_gui::MainWindow;
use hydrus_gui_model::duplicates_count::{Gate, Waiting};
use hydrus_gui_model::duplicates_filtering as model;
use hydrus_store::settings::{self, PotentialPairsCountOptions};

use crate::duplicates_lane_page::{Opened, opened, opened_with};

fn count_of(ui: &MainWindow) -> String {
    ui.get_duplicates_filtering().count.to_string()
}

fn spin_until(what: &str, mut ready: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !ready() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
}

/// Wait for the count line to be `expected`.
fn await_count(ui: &MainWindow, expected: &str) {
    let until = Instant::now() + Duration::from_secs(20);
    while count_of(ui) != expected {
        assert!(
            Instant::now() < until,
            "the count never said {expected:?}; it says {:?}",
            count_of(ui)
        );
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
}

/// Let the waiting fetch or block through, once one waits.
fn release(gate: &Gate) {
    spin_until("nothing waited at the gate", || gate.release(0.0));
}

fn page_search(o: &Opened) -> hydrus_core::duplicates::DuplicatesSearch {
    o.bound
        .current
        .borrow()
        .borrow()
        .duplicates()
        .unwrap()
        .search
        .clone()
}

/// The pairs of the page's search in the order they are searched, and which
/// of them the search finds (by the one-shot search the page used to count
/// with, not the worker's).
fn space_and_hits(o: &Opened) -> (usize, Vec<bool>) {
    let snapshot = o.store.snapshot();
    let query =
        hydrus_duplicates::potentials::PotentialsQuery::from_search(&snapshot, &page_search(o))
            .unwrap();
    let space = o.store.read(|c| query.space(c, &snapshot)).unwrap();
    let found: HashSet<(i64, i64)> = o
        .store
        .read(|c| {
            Ok(query
                .with_search(c, &snapshot, |s| {
                    hydrus_store::duplicates::potential_pairs(c, &snapshot, s)
                })?
                .unwrap())
        })
        .unwrap()
        .into_iter()
        .map(|p| p.groups)
        .collect();
    let hits = space.iter().map(|r| found.contains(&r.groups)).collect();
    (space.len(), hits)
}

fn searching(searched: usize, space: usize, matches: usize) -> String {
    format!("{searched}/{space} pairs searched; {matches} match\u{2026}")
}

// leaf: audit-media-duplicate-search-count
#[test]
fn the_pair_count_goes_a_block_at_a_time_and_the_buttons_and_search_changes_drive_it() {
    let gate = Gate::new(3);
    gate.install_here();
    let o = opened();
    let (ui, store) = (&o.ui, &o.store);
    let (space, hits) = space_and_hits(&o);
    assert!(
        space > 6,
        "the store needs several blocks of pairs ({space})"
    );
    let matches = |n: usize| hits[..n].iter().filter(|h| **h).count();
    let (total, found) = model::count(store, &{
        let page = o.bound.current.borrow();
        let page = page.borrow();
        page.duplicates().unwrap().clone()
    })
    .unwrap();
    assert_eq!((total, found), (space, matches(space)));

    // the page asks for its pairs as it opens
    await_count(ui, "initialising\u{2026}");
    assert!(!ui.get_duplicates_filtering().count_paused);
    // (the reference shows nothing new when the pairs arrive, until the
    // first block has been searched)
    release(&gate);
    release(&gate);
    await_count(ui, &searching(3, space, matches(3)));

    // pausing lets the block under way finish, then stops
    spin_until("no block was under way to pause", || {
        gate.waiting() == Some(Waiting::Block)
    });
    ui.invoke_duplicates_filtering_action("pause count".into(), 0);
    assert!(ui.get_duplicates_filtering().count_paused);
    release(&gate);
    await_count(ui, &searching(6, space, matches(6)));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!gate.release(0.0), "a paused count searched another block");
    assert_eq!(count_of(ui), searching(6, space, matches(6)));

    // playing goes on to the end
    ui.invoke_duplicates_filtering_action("pause count".into(), 0);
    assert!(!ui.get_duplicates_filtering().count_paused);
    let done = model::count_text(space, matches(space));
    spin_until("the count never finished", || {
        gate.release(0.0);
        count_of(ui) == done
    });
    assert_eq!(done, model::count_text(total, found));

    // refresh fetches the pairs again and counts them again
    ui.invoke_duplicates_filtering_action("refresh count".into(), 0);
    await_count(ui, "initialising\u{2026}");
    spin_until("the count never finished again", || {
        gate.release(0.0);
        count_of(ui) == done
    });

    // a change to the search counts it again over the same pairs: the
    // largest distance finds every pair
    ui.invoke_duplicates_filtering_action("distance".into(), 64);
    let (_, wider) = space_and_hits(&o);
    assert_eq!(page_search(&o).max_hamming_distance, 64);
    let wider_found = wider.iter().filter(|h| **h).count();
    assert!(wider_found >= found);
    let wide = model::count_text(space, wider_found);
    // (the reference keeps the old text until the new count publishes, so the
    // text alone can't show a recount when the pairs found are the same: the
    // finished count waits at the gate again once it starts over)
    spin_until("the changed search was not counted again", || {
        gate.waiting().is_some()
    });
    spin_until("the wider search never finished", || {
        gate.release(0.0);
        count_of(ui) == wide
    });
    gate.free();
}

// leaf: audit-media-duplicate-search-count
#[test]
fn the_cog_keeps_its_three_options_and_new_counts_start_paused_when_asked() {
    let gate = Gate::new(3);
    gate.install_here();
    let o = opened_with(|store| {
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &PotentialPairsCountOptions {
                        starts_paused: true,
                        ..PotentialPairsCountOptions::default()
                    },
                )
            })
            .unwrap();
    });
    let (ui, store) = (&o.ui, &o.store);
    let filtering = ui.get_duplicates_filtering();
    let titles: Vec<String> = filtering.count_cog.iter().map(|t| t.to_string()).collect();
    assert_eq!(
        titles,
        [
            "start new potential duplicate pair search panels paused",
            "optimisation: try to state an estimate of final count rather than counting everything",
            "optimisation: allow single slow search optimisation when seeing low hit-rate",
        ]
    );
    assert_eq!(
        filtering.count_ticks.iter().collect::<Vec<_>>(),
        [true, true, true]
    );

    // a count that starts paused fetches its pairs, then waits to be played
    let (space, hits) = space_and_hits(&o);
    await_count(ui, "initialising\u{2026}");
    assert!(ui.get_duplicates_filtering().count_paused);
    release(&gate);
    await_count(ui, &searching(0, space, 0));
    assert!(!gate.release(0.0));
    ui.invoke_duplicates_filtering_action("pause count".into(), 0);
    release(&gate);
    await_count(
        ui,
        &searching(3, space, hits[..3].iter().filter(|h| **h).count()),
    );

    // each item of the cog flips its stored option
    let stored = || {
        store
            .read(settings::get::<PotentialPairsCountOptions>)
            .unwrap()
    };
    for (item, read) in [
        (
            0,
            (|o: PotentialPairsCountOptions| o.starts_paused) as fn(_) -> bool,
        ),
        (1, |o| o.stops_to_estimate),
        (2, |o| o.file_search_optimisation),
    ] {
        let before = read(stored());
        ui.invoke_duplicates_filtering_action("count option".into(), item);
        assert_eq!(read(stored()), !before, "item {item}");
        assert_eq!(
            ui.get_duplicates_filtering()
                .count_ticks
                .row_data(item as usize),
            Some(!before)
        );
        ui.invoke_duplicates_filtering_action("count option".into(), item);
        assert_eq!(read(stored()), before, "item {item} again");
    }
    gate.free();
}

// leaf: audit-media-duplicate-search-count
#[test]
fn a_page_counts_its_pairs_by_itself_to_the_same_numbers_as_the_one_shot_search() {
    let o = opened();
    let (total, found) = model::count(&o.store, &{
        let page = o.bound.current.borrow();
        let page = page.borrow();
        page.duplicates().unwrap().clone()
    })
    .unwrap();
    assert!(total > 0);
    await_count(&o.ui, &model::count_text(total, found));
}
