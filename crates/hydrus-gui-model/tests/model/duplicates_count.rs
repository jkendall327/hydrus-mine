//! The potential duplicate search's pair count against the reference's,
//! recorded by `oracle/record_potential_duplicates_count.py`: the real panel
//! on a scripted space, one block let through at a time. Each scenario's
//! steps are replayed on the worker the window uses, the same blocks let
//! through at the same moments, and the label, pause state, counts and
//! signals after every step must be the reference's.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde_json::Value;

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::DuplicatesPage;
use hydrus_search::FileSearchContext;
use hydrus_gui_model::duplicates_count::{
    BLOCK_GUIDELINE, Gate, Handle, Options, Signals, Snapshot, Source, Waiting, relative_error_at_95,
};
use hydrus_gui_model::duplicates_filtering::{distance_enabled, second_search_shown};

type Row = (i64, i64, u32);

const TIMEOUT: Duration = Duration::from_secs(20);

/// The scripted space: pair `i` is `(2i, 2i + 1, distance)`.
fn space_rows(space: &Value) -> Vec<Row> {
    let n = space["n"].as_i64().unwrap();
    (0..n)
        .map(|i| {
            let distance = if let Some(every) = space.get("rare_every") {
                if i % every.as_i64().unwrap() == 0 {
                    0
                } else {
                    space["far"].as_u64().unwrap() as u32
                }
            } else {
                ((i * space["mult"].as_i64().unwrap()) % space["modulus"].as_i64().unwrap()) as u32
            };
            (2 * i, 2 * i + 1, distance)
        })
        .collect()
}

/// The recording's database stand-in: a pair is a hit within the distance.
struct Scripted {
    rows: Vec<Row>,
    distance: Arc<AtomicU32>,
}

impl Source<Row> for Scripted {
    fn space(&mut self) -> Result<Vec<Row>, String> {
        Ok(self.rows.clone())
    }

    fn hits(&mut self, _: u64, rows: &[Row]) -> Result<usize, String> {
        let distance = self.distance.load(Ordering::SeqCst);
        Ok(rows.iter().filter(|r| r.2 <= distance).count())
    }
}

fn settle(handle: &Handle<Row>) {
    assert!(handle.wait_settled(TIMEOUT), "the count did not settle");
}

/// Let a waiting fetch or block through and wait for it to be done.
fn release(gate: &Gate, handle: &Handle<Row>, block: bool, seconds: f64) -> bool {
    let epoch = handle.epoch();
    let released = if block {
        gate.release_block(seconds)
    } else {
        gate.release_space()
    };
    if !released {
        return false;
    }
    let start = std::time::Instant::now();
    while handle.epoch() == epoch {
        assert!(start.elapsed() < TIMEOUT, "the released work did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    settle(handle);
    true
}

fn delta(now: Signals, before: Signals) -> Value {
    serde_json::json!({
        "restarted": now.restarted - before.restarted,
        "has_pairs": now.has_pairs - before.has_pairs,
        "no_pairs": now.no_pairs - before.no_pairs,
    })
}

fn check(recorded: &Value, shot: &Snapshot, before: Signals, what: &str) {
    assert_eq!(shot.label, recorded["label"].as_str().unwrap(), "{what}");
    assert_eq!(shot.tooltip, recorded["tooltip"].as_str().unwrap(), "{what}");
    assert_eq!(shot.paused, recorded["paused"], "{what}");
    assert_eq!(shot.matches as u64, recorded["matches"].as_u64().unwrap(), "{what}");
    assert_eq!(shot.searched as u64, recorded["searched"].as_u64().unwrap(), "{what}");
    assert_eq!(shot.in_space as u64, recorded["in_space"].as_u64().unwrap(), "{what}");
    // (`valueChanged` tells the panel's owner that the search was edited:
    // the three edits below)
    let mut signals = recorded["signals"].clone();
    let changed = signals.as_object_mut().unwrap().remove("changed").unwrap();
    let edited = ["distance", "kind", "pixel"].contains(&recorded["step"]["do"].as_str().unwrap());
    assert_eq!(changed, u64::from(edited), "{what}");
    assert_eq!(delta(shot.signals, before), signals, "{what}");
}

// leaf: audit-media-duplicate-search-count
#[test]
fn the_count_goes_as_the_references_does_step_by_step() {
    let recorded = hydrus_testkit::fixture_json("potential_duplicates_count.json");
    let defaults = &recorded["defaults"];
    let mut scenarios = 0;
    for scenario in recorded["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let rows = space_rows(&recorded["spaces"][scenario["space"].as_str().unwrap()]);
        let block_size = scenario["block_size"].as_u64().unwrap() as usize;
        let option = |key: &str| {
            scenario["options"]
                .get(key)
                .and_then(Value::as_bool)
                .unwrap_or_else(|| defaults[key].as_bool().unwrap() && key != "starts_paused")
        };
        let mut options = Options {
            stops_to_estimate: option("stops_to_estimate"),
            file_search_optimisation: option("file_search_optimisation"),
        };
        let mut starts_paused = option("starts_paused");
        let distance = Arc::new(AtomicU32::new(4));
        let gate = Gate::new(block_size);
        let mut page_search = DuplicatesSearch {
            search_1: FileSearchContext::default(),
            search_2: FileSearchContext::default(),
            kind: PairSearchKind::OneFileMatchesOneSearch,
            pixel_duplicates: PixelDuplicates::Allowed,
            max_hamming_distance: 4,
        };
        let mut handle: Option<Handle<Row>> = None;
        let mut before = Signals::default();
        for recorded_step in scenario["steps"].as_array().unwrap() {
            let step = &recorded_step["step"];
            let what = format!("{name}: {step}");
            let value = step["value"].as_i64();
            if step["do"] == "show" {
                handle = Some(Handle::start(
                    block_size,
                    starts_paused,
                    false,
                    options,
                    Scripted {
                        rows: rows.clone(),
                        distance: Arc::clone(&distance),
                    },
                    Some(Arc::clone(&gate)),
                ));
                settle(handle.as_ref().unwrap());
            } else {
                let h = handle.as_ref().expect("shown first");
                // (a block under way when the search is dropped finishes)
                let under_way = gate.waiting() == Some(Waiting::Block);
                match step["do"].as_str().unwrap() {
                    "space" | "block" => {
                        let seconds = step["seconds"].as_f64().unwrap_or(0.0);
                        let released = release(&gate, h, step["do"] == "block", seconds);
                        assert_eq!(released, recorded_step["released"], "{what}");
                    }
                    "pause" | "play" => {
                        h.pause_play();
                        settle(h);
                    }
                    "refresh" => {
                        h.refresh();
                        settle(h);
                        if under_way {
                            release(&gate, h, true, 0.0);
                        }
                    }
                    "distance" => {
                        distance.store(value.unwrap() as u32, Ordering::SeqCst);
                        page_search.max_hamming_distance = value.unwrap() as u32;
                        h.search_changed(false);
                        settle(h);
                        if under_way {
                            release(&gate, h, true, 0.0);
                        }
                    }
                    "kind" => {
                        page_search.kind = [
                            PairSearchKind::OneFileMatchesOneSearch,
                            PairSearchKind::BothFilesMatchOneSearch,
                            PairSearchKind::BothFilesMatchDifferentSearches,
                        ][value.unwrap() as usize];
                        h.search_changed(false);
                        settle(h);
                        if under_way {
                            release(&gate, h, true, 0.0);
                        }
                    }
                    "pixel" => {
                        page_search.pixel_duplicates = [
                            PixelDuplicates::Required,
                            PixelDuplicates::Allowed,
                            PixelDuplicates::Excluded,
                        ][value.unwrap() as usize];
                        h.search_changed(false);
                        settle(h);
                        if under_way {
                            release(&gate, h, true, 0.0);
                        }
                    }
                    "cog" => {
                        // (ticking an item of the cog: the option as the
                        // reference then holds it)
                        let recorded_value = recorded_step["option"].as_bool().unwrap();
                        match step["value"].as_str().unwrap() {
                            "stops_to_estimate" => options.stops_to_estimate = recorded_value,
                            "file_search_optimisation" => {
                                options.file_search_optimisation = recorded_value;
                            }
                            _ => starts_paused = recorded_value,
                        }
                        h.set_options(options);
                        settle(h);
                    }
                    other => panic!("unknown step {other}"),
                }
            }
            let h = handle.as_ref().unwrap();
            let shot = h.snapshot();
            check(recorded_step, &shot, before, &what);
            before = shot.signals;
            // (the spin box and the second search follow the kind and the
            // pixel preference, as the page does)
            let page = DuplicatesPage::new(page_search.clone());
            assert_eq!(
                distance_enabled(&page),
                recorded_step["distance_enabled"].as_bool().unwrap(),
                "{what}"
            );
            if step["do"] == "kind" {
                assert_eq!(
                    second_search_shown(&page),
                    recorded_step["second_search_shown"].as_bool().unwrap(),
                    "{what}"
                );
            }
        }
        gate.free();
        drop(handle);
        scenarios += 1;
    }
    assert_eq!(scenarios, recorded["scenarios"].as_array().unwrap().len());
}

// leaf: audit-media-duplicate-search-count
#[test]
fn the_cog_offers_the_references_three_items_with_its_tooltips() {
    use hydrus_gui_model::duplicates_count::{
        COG_FILE_SEARCH_OPTIMISATION, COG_STARTS_PAUSED, COG_STOPS_TO_ESTIMATE,
    };
    let recorded = hydrus_testkit::fixture_json("potential_duplicates_count.json");
    let scenario = recorded["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cog_items")
        .unwrap();
    let items: Vec<(&str, &str)> = scenario["cog_menu"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i.get("separator").is_none())
        .map(|i| {
            (
                i["title"].as_str().unwrap(),
                i["description"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            COG_STARTS_PAUSED,
            COG_STOPS_TO_ESTIMATE,
            COG_FILE_SEARCH_OPTIMISATION
        ]
    );
    assert_eq!(BLOCK_GUIDELINE, 4000);
}

#[test]
fn the_estimate_is_good_enough_when_its_95_percent_interval_is_within_2_5_percent() {
    // (the Wilson interval with the finite population correction: a sample
    // that is all of the space has no error; none found has an unbounded one)
    assert!((relative_error_at_95(10, 100, 100)).abs() < f64::EPSILON);
    assert!(relative_error_at_95(0, 50, 100).is_infinite());
    assert!((relative_error_at_95(5, 2, 100) - 1.0).abs() < f64::EPSILON);
    assert!(relative_error_at_95(3231, 8400, 30000) < 0.025);
    assert!(relative_error_at_95(1540, 4000, 30000) > 0.025);
}
