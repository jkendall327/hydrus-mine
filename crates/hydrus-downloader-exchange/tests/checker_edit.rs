//! A checker options edit on a query's cached header, against the reference's
//! `Subscription.SetCheckerOptions` (`oracle/fixtures/subscription_checker_edit.json`,
//! `record_subscription_checker_edit.py`).
use hydrus_core::subscriptions::{CheckerOptions, SeedTime};
use hydrus_downloader_exchange::subscriptions::{self as exchange, CheckerEdit};
use serde_json::Value;

fn checker(value: &Value) -> CheckerOptions {
    // [52, 1, [intended, never faster, never slower, [files, seconds]]]
    let info = &value[2];
    CheckerOptions {
        intended_files_per_check: info[0].as_f64().unwrap(),
        never_faster_than: info[1].as_i64().unwrap(),
        never_slower_than: info[2].as_i64().unwrap(),
        death_file_velocity: (info[3][0].as_i64().unwrap(), info[3][1].as_i64().unwrap()),
    }
}

/// The header fields a checker edit can change, and the ones it must keep.
fn fields(header: &Value) -> Vec<Value> {
    [8, 9, 13, 14, 15, 16]
        .iter()
        .map(|i| header[2][*i].clone())
        .collect()
}

fn imported() -> (Value, exchange::Subscription) {
    let fixture = hydrus_testkit::fixture_json("subscription_checker_edit.json");
    let now = fixture["now"].as_i64().unwrap();
    let mut subs = exchange::decode_text_at(&fixture["source"].to_string(), now).unwrap();
    (fixture, subs.remove(0))
}

#[test]
fn an_unloaded_history_is_marked_unsynced_with_the_recalculate_words() {
    let (fixture, mut sub) = imported();
    let query = &mut sub.queries[0];
    let now = fixture["now"].as_i64().unwrap();
    exchange::apply_checker_edit(query, &CheckerEdit::Unsynced, now).unwrap();
    let header = query.reference_header.as_ref().unwrap();
    for name in ["dead", "alive"] {
        assert_eq!(
            fields(header),
            fields(&fixture["cases"][name]["unloaded"]),
            "{name}"
        );
    }
    // (the same words and flags the reference's list shows)
    assert_eq!(header[2][14], exchange::RECALCULATE_WORDS);
}

#[test]
fn a_loaded_history_is_recalculated_with_the_new_checker_options() {
    let (fixture, sub) = imported();
    let now = fixture["now"].as_i64().unwrap();
    for name in ["dead", "alive"] {
        let mut sub = sub.clone();
        let query = &mut sub.queries[0];
        let new = checker(&fixture["checkers"][name]);
        let log = query.log.as_ref().unwrap();
        let seeds: Vec<SeedTime> = log
            .file_seeds
            .iter()
            .map(|s| SeedTime {
                source_time: s.source_time,
                created: s.created,
            })
            .collect();
        let last = query.state.last_check_time;
        let edit = CheckerEdit::Synced {
            velocity: new.raw_current_velocity(&seeds, last),
            words: new.pretty_velocity(&seeds, last, false),
        };
        exchange::apply_checker_edit(query, &edit, now).unwrap();
        let recorded = &fixture["cases"][name]["loaded"];
        assert_eq!(
            fields(query.reference_header.as_ref().unwrap()),
            fields(recorded),
            "{name}"
        );
        // and the timing the dialog keeps in the query's state
        assert_eq!(new.is_dead(&seeds, last), recorded[2][7] == 1, "{name}");
        assert_eq!(
            new.next_check_time(&seeds, last, now),
            recorded[2][5],
            "{name}"
        );
    }
}
