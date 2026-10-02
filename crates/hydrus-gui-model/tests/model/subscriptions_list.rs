//! The manage subscriptions dialog's lists against the reference's rows,
//! recorded by `oracle/record_subscriptions_list.py`: subscriptions of
//! queries in every state (working, paused, dead, never checked, checking
//! now; delayed, paused, with no downloader, with no queries), each row as
//! the reference writes it at the recorded time; and the dialog's buttons
//! on them as the reference's list acted, step by step.

use serde_json::Value as Json;

use hydrus_core::subscriptions::{CheckerOptions, QueryState, SeedTime, SubscriptionSettings};
use hydrus_gui_model::subscriptions_dialog::{
    CheckNow, DELETE_QUESTION, DialogQuery, SELECT_MESSAGE, Subscriptions,
};
use hydrus_gui_model::subscriptions_list::{
    QueryFacts, ShortSummary, SubscriptionFacts, query_row, subscription_row,
};
use hydrus_store::queues::{SeedStatus, StatusCounts};

fn status(code: i64) -> SeedStatus {
    match code {
        0 => SeedStatus::Unknown,
        1 => SeedStatus::SuccessfulAndNew,
        2 => SeedStatus::SuccessfulButRedundant,
        3 => SeedStatus::Deleted,
        4 => SeedStatus::Error,
        7 => SeedStatus::Vetoed,
        8 => SeedStatus::Skipped,
        other => panic!("status {other}"),
    }
}

fn checker(v: &Json) -> CheckerOptions {
    CheckerOptions {
        intended_files_per_check: v[0].as_f64().unwrap(),
        never_faster_than: v[1].as_i64().unwrap(),
        never_slower_than: v[2].as_i64().unwrap(),
        death_file_velocity: (v[3][0].as_i64().unwrap(), v[3][1].as_i64().unwrap()),
    }
}

/// A recorded query, as the dialog would read it: its file log, its
/// state after the client synced it, and its velocity by our checker.
fn query(recorded: &Json, checker: &CheckerOptions, now: i64) -> QueryFacts {
    let q = &recorded["query"];
    let after = &recorded["after_sync"];
    let mut files = StatusCounts::new();
    let mut times = Vec::new();
    for seed in q["seeds"].as_array().unwrap() {
        *files.entry(status(seed[0].as_i64().unwrap())).or_default() += 1;
        times.push(SeedTime {
            created: now - seed[1].as_i64().unwrap(),
            source_time: seed[2].as_i64().map(|ago| now - ago),
        });
    }
    let last_check_time = q["last_check"].as_i64().map_or(0, |ago| now - ago);
    QueryFacts {
        query_text: q["text"].as_str().unwrap().into(),
        display_name: q["display"].as_str().map(Into::into),
        paused: after["paused"].as_bool().unwrap(),
        dead: after["dead"].as_bool().unwrap(),
        check_now: q["check_now"].as_bool().unwrap(),
        last_check_time,
        next_check_time: after["next_check_time"].as_i64().unwrap(),
        files,
        latest_added: times.iter().map(|t| t.created).max().unwrap_or(0),
        velocity: checker.pretty_velocity(&times, last_check_time, false),
        additional_tags: String::new(),
    }
}

#[test]
fn the_lists_rows_are_the_references() {
    let recorded = hydrus_testkit::fixture_json("subscriptions_list.json");
    let now = recorded["now"].as_i64().unwrap();
    let mut seen_queries = 0;
    for sub in recorded["subscriptions"].as_array().unwrap() {
        let case = &sub["subscription"];
        let checker = checker(&case["checker"]);
        let queries: Vec<QueryFacts> = sub["queries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| query(q, &checker, now))
            .collect();
        for (facts, q) in queries.iter().zip(sub["queries"].as_array().unwrap()) {
            let ours = query_row(facts, now, ShortSummary::default());
            assert_eq!(serde_json::json!(ours), q["row"], "{}", facts.query_text);
            seen_queries += 1;
        }
        let (no_work_until, reason) = match case["delay"].as_array() {
            Some(d) => (
                now + d[0].as_i64().unwrap(),
                d[1].as_str().unwrap().to_owned(),
            ),
            None => (0, String::new()),
        };
        let facts = SubscriptionFacts {
            name: case["name"].as_str().unwrap().into(),
            gug_name: case["gug"].as_str().unwrap().into(),
            paused: case["paused"].as_bool().unwrap(),
            no_work_until,
            no_work_until_reason: reason,
            queries,
            import_options: String::new(),
        };
        let ours = subscription_row(&facts, now, ShortSummary::default());
        assert_eq!(serde_json::json!(ours), sub["row"], "{}", facts.name);
    }
    assert_eq!(seen_queries, 7);
}

/// A recorded subscription as the dialog would load it.
fn dialog_subscription(
    recorded: &Json,
    now: i64,
) -> (Option<i64>, String, SubscriptionSettings, Vec<DialogQuery>) {
    let case = &recorded["subscription"];
    let checker = checker(&case["checker"]);
    let (no_work_until, reason) = match case["delay"].as_array() {
        Some(d) => (
            now + d[0].as_i64().unwrap(),
            d[1].as_str().unwrap().to_owned(),
        ),
        None => (0, String::new()),
    };
    let settings = SubscriptionSettings {
        gug_name: case["gug"].as_str().unwrap().into(),
        checker: checker.clone(),
        paused: case["paused"].as_bool().unwrap(),
        no_work_until,
        no_work_until_reason: reason,
        ..SubscriptionSettings::default()
    };
    let queries = recorded["queries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| {
            let facts = query(q, &checker, now);
            let mut state = QueryState::new(facts.query_text.clone());
            state.display_name.clone_from(&facts.display_name);
            state.paused = facts.paused;
            state.dead = facts.dead;
            state.check_now = facts.check_now;
            state.last_check_time = facts.last_check_time;
            state.next_check_time = facts.next_check_time;
            let seed_times = q["query"]["seeds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|seed| SeedTime {
                    created: now - seed[1].as_i64().unwrap(),
                    source_time: seed[2].as_i64().map(|ago| now - ago),
                })
                .collect();
            DialogQuery {
                queue: None,
                state,
                files: facts.files,
                seed_times,
            }
        })
        .collect();
    (
        None,
        case["name"].as_str().unwrap().into(),
        settings,
        queries,
    )
}

#[test]
fn the_dialogs_buttons_act_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("subscriptions_list.json");
    let now = recorded["now"].as_i64().unwrap();
    let mut dialog = Subscriptions::new(
        recorded["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| dialog_subscription(s, now))
            .collect(),
    );
    for (i, step) in recorded["actions"].as_array().unwrap().iter().enumerate() {
        let at = format!("step {i} ({})", step["do"]);
        // the step's rows selected: a click, then ctrl+clicks
        let names: Vec<&str> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        dialog.selection.select_only(None);
        for (n, name) in names.iter().enumerate() {
            let order = dialog.order(now);
            let row = order
                .iter()
                .position(|&k| dialog.get(k).unwrap().name == *name)
                .unwrap();
            dialog.click(now, row, n > 0, false);
        }
        let mut asked = step["asked"].as_array().unwrap().iter();
        match step["do"].as_str().unwrap() {
            "select" => {}
            "pause/resume" => dialog.pause_resume(now),
            "scrub delays" => dialog.scrub_delays(now),
            "select subscriptions" => {
                let q = asked.next().unwrap();
                assert_eq!(q["message"], SELECT_MESSAGE, "{at}");
                dialog.select_by_text(now, q["answer"].as_str().unwrap());
            }
            "delete" => {
                let q = asked.next().unwrap();
                assert_eq!(q["message"], DELETE_QUESTION, "{at}");
                if q["answer"].as_bool().unwrap() {
                    dialog.delete_selected(now);
                }
            }
            "check queries now" => {
                let mut check = CheckNow::new(&dialog, now);
                let mut cancelled = false;
                while let Some(choice) = check.next(&dialog) {
                    let q = asked
                        .next()
                        .unwrap_or_else(|| panic!("{at}: asked {choice:?}"));
                    assert_eq!(q["title"], choice.title.as_str(), "{at}");
                    assert_eq!(q["message"], choice.message.as_str(), "{at}");
                    assert_eq!(q["choices"], serde_json::json!(choice.choices), "{at}");
                    let Some(index) = q["answer"].as_u64() else {
                        cancelled = true;
                        break;
                    };
                    check.answer(usize::try_from(index).unwrap());
                }
                if !cancelled {
                    check.apply(&mut dialog);
                }
            }
            other => panic!("{other}"),
        }
        assert!(asked.next().is_none(), "{at}: more was asked");
        // after it: what is selected, what the buttons can do, and every
        // subscription's state
        let mut selected: Vec<String> = dialog
            .selected(now)
            .into_iter()
            .map(|k| dialog.get(k).unwrap().name.clone())
            .collect();
        selected.sort();
        assert_eq!(serde_json::json!(selected), step["selected"], "{at}");
        assert_eq!(dialog.can_check_now(now), step["can_check_now"], "{at}");
        assert_eq!(dialog.can_reset(now), step["can_reset"], "{at}");
        assert_eq!(
            dialog.can_scrub_delays(now),
            step["can_scrub_delays"],
            "{at}"
        );
        let ours: Vec<Json> = dialog
            .subscriptions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "paused": s.settings.paused,
                    "no_work_until": s.settings.no_work_until,
                    "queries": s.queries.iter().map(|q| serde_json::json!({
                        "text": q.state.query_text,
                        "check_now": q.state.check_now,
                        "paused": q.state.paused,
                        "dead": q.state.dead,
                        "next_check_time": q.state.next_check_time,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        assert_eq!(serde_json::json!(ours), step["subscriptions"], "{at}");
    }
}

#[test]
fn the_list_sorts_by_the_column_chosen() {
    let recorded = hydrus_testkit::fixture_json("subscriptions_list.json");
    let now = recorded["now"].as_i64().unwrap();
    let mut dialog = Subscriptions::new(
        recorded["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| dialog_subscription(s, now))
            .collect(),
    );
    let names = |dialog: &Subscriptions| -> Vec<String> {
        dialog
            .rows(now, ShortSummary::default())
            .into_iter()
            .map(|r| r.1[0].clone())
            .collect()
    };
    // by name at first, casefolded
    assert_eq!(
        names(&dialog),
        [
            "artist one",
            "Empty sub",
            "no downloader",
            "paused and delayed"
        ]
    );
    // by status: (queries, paused, dead)
    dialog.sort = (2, true);
    assert_eq!(
        names(&dialog),
        [
            "Empty sub",
            "no downloader",
            "paused and delayed",
            "artist one"
        ]
    );
    // by items, the other way: (total, done)
    dialog.sort = (6, false);
    assert_eq!(
        names(&dialog),
        [
            "artist one",
            "paused and delayed",
            "no downloader",
            "Empty sub"
        ]
    );
    // by paused, then by name
    dialog.sort = (7, true);
    assert_eq!(
        names(&dialog),
        [
            "artist one",
            "Empty sub",
            "no downloader",
            "paused and delayed"
        ]
    );
    // the selection follows its subscriptions through a sort
    dialog.sort = (0, true);
    dialog.click(now, 1, false, false);
    dialog.sort = (0, false);
    let rows = dialog.rows(now, ShortSummary::default());
    assert!(rows[2].2 && rows.iter().filter(|r| r.2).count() == 1);
    assert_eq!(rows[2].1[0], "Empty sub");
}
