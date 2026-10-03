//! The manage subscriptions dialog's other buttons against the
//! reference's, recorded by `oracle/record_subscriptions_buttons.py`:
//! retry ignored and failed, reset, lowercase, overwrite checker options
//! and downloader, separate (in half, and whole) and merge, step by step,
//! with what each asked and every subscription after it.

use serde_json::{Value as Json, json};

use hydrus_core::subscriptions::{QueryState, SeedTime, SubscriptionSettings};
use hydrus_gui_model::edit_subscription::RetryIgnored;
use hydrus_gui_model::subscriptions_dialog::{
    DialogQuery, LOWERCASE_QUESTION, MERGE_PRIMARY, MERGE_QUESTION, Picked, RESET_QUESTION,
    SEPARATE_CHOICES, SEPARATE_MERGED_CHOICES, SEPARATE_MERGED_NAME, SEPARATE_MERGED_QUESTION,
    SEPARATE_NAME, SEPARATE_PICK, SEPARATE_QUESTION, Separate, Subscriptions, added_message,
    picked,
};
use hydrus_store::queues::StatusCounts;

use crate::subscriptions_list::{checker, status};

/// A recorded subscription as the dialog loads it, its queries' states as
/// the recording started.
fn subscription(
    case: &Json,
    start: &Json,
    now: i64,
) -> (Option<i64>, String, SubscriptionSettings, Vec<DialogQuery>) {
    let settings = SubscriptionSettings {
        gug_name: case["gug"].as_str().unwrap().into(),
        checker: checker(&case["checker"]),
        paused: case["paused"].as_bool().unwrap(),
        ..SubscriptionSettings::default()
    };
    let queries = case["queries"]
        .as_array()
        .unwrap()
        .iter()
        .zip(start["queries"].as_array().unwrap())
        .map(|(q, at)| {
            let mut files = StatusCounts::new();
            let mut seed_times = Vec::new();
            let mut ignored_notes = Vec::new();
            for seed in q["seeds"].as_array().unwrap() {
                let s = status(seed[0].as_i64().unwrap());
                *files.entry(s).or_default() += 1;
                if s == hydrus_store::queues::SeedStatus::Vetoed {
                    ignored_notes.push(String::new());
                }
                seed_times.push(SeedTime {
                    created: now - seed[1].as_i64().unwrap(),
                    source_time: seed[2].as_i64().map(|ago| now - ago),
                });
            }
            let mut state = QueryState::new(q["text"].as_str().unwrap());
            state.display_name = q["display"].as_str().map(Into::into);
            state.paused = at["paused"].as_bool().unwrap();
            state.dead = at["dead"].as_bool().unwrap();
            state.check_now = at["check_now"].as_bool().unwrap();
            state.last_check_time = at["last_check_time"].as_i64().unwrap();
            state.next_check_time = at["next_check_time"].as_i64().unwrap();
            DialogQuery {
                files,
                seed_times,
                ignored_notes,
                ..DialogQuery::new(state)
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

/// Every subscription, by name, as the recording writes them.
fn state(dialog: &Subscriptions) -> Json {
    let mut subs: Vec<_> = dialog.subscriptions.iter().collect();
    subs.sort_by(|a, b| a.name.cmp(&b.name));
    Json::Array(
        subs.iter()
            .map(|s| {
                let c = &s.settings.checker;
                json!({
                    "name": s.name,
                    "gug": s.settings.gug_name,
                    "paused": s.settings.paused,
                    "checker": [
                        c.intended_files_per_check,
                        c.never_faster_than,
                        c.never_slower_than,
                        [c.death_file_velocity.0, c.death_file_velocity.1],
                    ],
                    "queries": s.queries.iter().map(|q| {
                        let files: serde_json::Map<String, Json> = q
                            .files
                            .iter()
                            .map(|(st, n)| (st.code().to_string(), json!(n)))
                            .collect();
                        json!({
                            "text": q.state.query_text,
                            "display": q.state.display_name,
                            "paused": q.state.paused,
                            "check_now": q.state.check_now,
                            "dead": q.state.dead,
                            "last_check_time": q.state.last_check_time,
                            "next_check_time": q.state.next_check_time,
                            "files": files,
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The recording's checker numbers (intended files as a float, as ours).
fn normalised(mut v: Json) -> Json {
    for s in v.as_array_mut().unwrap() {
        let c = &mut s["checker"][0];
        *c = json!(c.as_f64().unwrap());
    }
    v
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_other_buttons_act_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("subscriptions_buttons.json");
    let now = recorded["now"].as_i64().unwrap();
    let list = hydrus_testkit::fixture_json("subscriptions_list.json");
    let mut cases: Vec<Json> = list["subscriptions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["subscription"].clone())
        .collect();
    cases.extend(recorded["extra"].as_array().unwrap().iter().cloned());
    let start = &recorded["start"]["subscriptions"];
    let loaded = cases
        .iter()
        .map(|case| {
            let at = start
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == case["name"])
                .unwrap();
            subscription(case, at, now)
        })
        .collect();
    let mut dialog = Subscriptions::new(loaded);
    assert_eq!(state(&dialog), normalised(start.clone()));

    for (i, step) in recorded["actions"].as_array().unwrap().iter().enumerate() {
        let at = format!("step {i} ({})", step["do"]);
        let names: Vec<&str> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let keys: Vec<u64> = dialog
            .subscriptions
            .iter()
            .filter(|s| names.contains(&s.name.as_str()))
            .map(|s| s.key)
            .collect();
        dialog.selection.select_many(&keys);
        let asked = step["asked"].as_array().unwrap();
        match step["do"].as_str().unwrap() {
            "retry ignored" => {
                let q = &asked[0];
                let labels: Vec<&str> = RetryIgnored::CHOICES.iter().map(|c| c.1).collect();
                assert_eq!(q["choices"], json!(labels), "{at}");
                let which = RetryIgnored::CHOICES[q["answer"].as_u64().unwrap() as usize].0;
                dialog.retry_ignored_selected(now, which);
            }
            "retry failed" => dialog.retry_failed_selected(now),
            "reset" => {
                assert_eq!(asked[0]["message"], RESET_QUESTION, "{at}");
                if asked[0]["answer"].as_bool().unwrap() {
                    dialog.reset_selected(now);
                }
            }
            "lowercase" => {
                assert_eq!(asked[0]["message"], LOWERCASE_QUESTION, "{at}");
                if asked[0]["answer"].as_bool().unwrap() {
                    dialog.lowercase_selected(now);
                }
            }
            "overwrite checker options" => {
                let new = checker(&asked[0]["answer"]);
                dialog.set_checker_selected(now, &new);
            }
            "overwrite downloader" => {
                let first = dialog.selected(now)[0];
                assert_eq!(
                    asked[0]["current"],
                    dialog.get(first).unwrap().settings.gug_name.as_str(),
                    "{at}"
                );
                let name = asked[0]["answer"].as_str().unwrap();
                dialog.set_downloader_selected(now, "", name);
            }
            "separate" => {
                let key = dialog.selected(now)[0];
                let original = dialog.get(key).unwrap().clone();
                let mut n = 0;
                let mut how = Separate::Whole;
                if asked[0]["kind"] == "yes/yes/no" {
                    assert_eq!(asked[0]["message"], SEPARATE_QUESTION, "{at}");
                    assert_eq!(asked[0]["yeses"], json!(SEPARATE_CHOICES), "{at}");
                    n = 1;
                    how = match asked[0]["answer"].as_u64().unwrap() {
                        0 => Separate::Half,
                        1 => Separate::Whole,
                        _ => {
                            // the queries by name, none ticked
                            let pick = &asked[1];
                            assert_eq!(pick["title"], SEPARATE_PICK, "{at}");
                            let names: Vec<&str> = original
                                .queries
                                .iter()
                                .map(|q| q.state.human_name())
                                .collect();
                            assert_eq!(pick["choices"], json!(names), "{at}");
                            assert_eq!(pick["checked"], json!([]), "{at}");
                            let ticked: Vec<usize> = (0..names.len())
                                .filter(|&i| {
                                    pick["answer"] == "*"
                                        || pick["answer"]
                                            .as_array()
                                            .unwrap()
                                            .contains(&json!(names[i]))
                                })
                                .collect();
                            n = 2;
                            match picked(ticked.len(), names.len()) {
                                Picked::All => Separate::Whole,
                                Picked::Several => {
                                    let q = &asked[2];
                                    assert_eq!(q["message"], SEPARATE_MERGED_QUESTION, "{at}");
                                    assert_eq!(q["yeses"], json!(SEPARATE_MERGED_CHOICES), "{at}");
                                    assert_eq!(q["no"], "forget it", "{at}");
                                    n = 3;
                                    Separate::Part {
                                        queries: ticked,
                                        merged: q["answer"] == 0,
                                    }
                                }
                                Picked::Few => Separate::Part {
                                    queries: ticked,
                                    merged: false,
                                },
                            }
                        }
                    };
                }
                // a name (but for halves), defaulting to the original's
                let base = if how == Separate::Half {
                    Some(String::new())
                } else {
                    let message = if matches!(how, Separate::Part { merged: true, .. }) {
                        SEPARATE_MERGED_NAME
                    } else {
                        SEPARATE_NAME
                    };
                    assert_eq!(asked[n]["message"], message, "{at}");
                    assert_eq!(asked[n]["default"], original.name.as_str(), "{at}");
                    asked[n]["answer"].as_str().map(str::to_owned)
                };
                // (cancelled, nothing happens)
                if let Some(base) = base {
                    dialog.separate(now, &how, &base);
                }
            }
            "duplicate" => {
                let made = dialog.duplicate_selected(now);
                assert_eq!(asked[0]["kind"], "information", "{at}");
                assert_eq!(
                    asked[0]["message"],
                    added_message(made.len()).as_str(),
                    "{at}"
                );
            }
            "merge" => {
                assert_eq!(asked[0]["message"], MERGE_QUESTION, "{at}");
                let groups = dialog.merge_groups(now);
                assert_eq!(groups.len(), 1, "{at}");
                let group = &groups[0];
                let q = &asked[1];
                assert_eq!(q["title"], MERGE_PRIMARY, "{at}");
                let choices: Vec<&str> = group
                    .iter()
                    .map(|&k| dialog.get(k).unwrap().name.as_str())
                    .collect();
                assert_eq!(q["choices"], json!(choices), "{at}");
                let primary = group[q["answer"].as_u64().unwrap() as usize];
                let primary_name = dialog.get(primary).unwrap().name.clone();
                let message = format!(
                    "{primary_name} was able to merge {} other subscriptions. If you wish to change its name, do so here.",
                    group.len() - 1
                );
                assert_eq!(asked[2]["message"], message.as_str(), "{at}");
                // (a cancelled name keeps the primary's)
                let name = asked[2]["answer"]
                    .as_str()
                    .map_or(primary_name, str::to_owned);
                dialog.merge(primary, group, &name);
            }
            other => panic!("{other}"),
        }
        assert_eq!(
            state(&dialog),
            normalised(step["subscriptions"].clone()),
            "{at}"
        );
        let mut selected: Vec<String> = dialog
            .selected(now)
            .iter()
            .map(|&k| dialog.get(k).unwrap().name.clone())
            .collect();
        selected.sort();
        // (the reference's merge selects nothing after; ours keeps the
        // primary)
        if step["do"] != "merge" {
            assert_eq!(json!(selected), step["selected"], "{at}");
            let can = &step["can"];
            assert_eq!(can["lowercase"], dialog.can_lowercase(now), "{at}");
            assert_eq!(can["merge"], dialog.can_merge(now), "{at}");
            assert_eq!(can["separate"], dialog.can_separate(now), "{at}");
            assert_eq!(can["reset"], dialog.can_reset(now), "{at}");
            assert_eq!(can["retry_failed"], dialog.can_retry_failed(now), "{at}");
            assert_eq!(can["retry_ignored"], dialog.can_retry_ignored(now), "{at}");
        }
    }
}
