//! The review window's pending pairs tab against the reference's, replaying
//! `oracle/fixtures/auto_resolution_pending.json`
//! (`oracle/record_auto_resolution_pending.py`: the real `ReviewActionsPanel`
//! on a rule with ten pairs waiting for approval): the sample size box and its
//! "fetch all" phrase, refreshing at several fetch limits, select all, the
//! question asked before approving more than five pairs, the progress texts
//! the buttons show as the work goes four at a time, what the tab holds
//! after (the popup the reference publishes four seconds in, which a job
//! done before then never shows, is not replayed: only the buttons are).

use serde_json::Value;
use slint::Model as _;

use crate::auto_resolution_review::{listed, opened_from, review, settle};
use hydrus_store::duplicates::auto;

fn pairs(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_owned(),
                p[1].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn rows_of(window: &hydrus_gui::AutoResolutionReviewWindow) -> usize {
    window.get_rows().row_count()
}

// leaf: audit-media-review-pending
// leaf: audit-media-review-progress
#[test]
fn pending_pairs_are_fetched_selected_approved_and_denied_as_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_pending.json");
    let o = opened_from("auto_resolution_pending");
    let name = recorded["rule"].as_str().unwrap();
    let rule = o
        .store
        .read(auto::rules)
        .unwrap()
        .into_iter()
        .find(|(_, r)| r.name == name)
        .unwrap()
        .0;
    let window = review(&o.ui, &o.bound, name);

    // as it opens
    let start = &recorded["start"];
    assert_eq!(window.get_label(), start["label"].as_str().unwrap());
    // (the reference lists the table in whatever order it scans it, which
    // follows how the rows were inserted; ours is by the pair's groups. The
    // same ten pairs, and a sample of a limit is as many of them.)
    let sorted = |mut pairs: Vec<(String, String)>| {
        pairs.sort();
        pairs
    };
    assert_eq!(
        sorted(listed(&o.store, rule, "pending", &o.hex)),
        sorted(pairs(&start["rows"]))
    );
    assert_eq!(rows_of(&window), start["rows"].as_array().unwrap().len());
    let fetch = &recorded["fetch_box"];
    assert_eq!(
        i64::from(window.get_fetch_limit()),
        fetch["value"].as_i64().unwrap()
    );
    assert!(!window.get_fetch_all());

    // each fetch limit, refreshed
    for case in recorded["fetches"].as_array().unwrap() {
        match case["limit"].as_i64() {
            Some(limit) => {
                window.set_fetch_all(false);
                window.set_fetch_limit(limit as i32);
            }
            None => window.set_fetch_all(true),
        }
        window.invoke_fetch_changed();
        window.invoke_refresh();
        settle(&window);
        assert_eq!(
            window.get_label(),
            case["label"].as_str().unwrap(),
            "{}",
            case["limit"]
        );
        assert_eq!(rows_of(&window), case["rows"].as_array().unwrap().len());
    }
    // (the fetch box keeps its limit; "fetch all" is the none phrase)
    window.set_fetch_all(true);
    window.invoke_fetch_changed();
    window.invoke_refresh();
    settle(&window);

    // select all
    assert!(window.get_can_select_all());
    window.invoke_select_all();
    let selected = (0..rows_of(&window))
        .filter(|&i| window.get_rows().row_data(i).unwrap().selected)
        .count();
    assert_eq!(
        selected as i64,
        recorded["select_all"]["selected"].as_i64().unwrap()
    );
    assert!(window.get_can_act());

    // the steps
    for step in recorded["steps"].as_array().unwrap() {
        let approve = step["button"] == "approve";
        let asked = step["asked"].as_array().unwrap();
        let declined = asked.first().is_some_and(|q| q["pressed"] == "no");
        let recorded_texts: Vec<String> = step["button_texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .filter(|t| t != "approve" && t != "deny")
            .collect();
        // Slow the store's writer, so that the work is seen at each chunk: a
        // job that holds it for a moment (the window looks at the work every
        // tenth of a second), again and again, between the work's
        // own writes.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let gate = (!declined).then(|| {
            let (store, stop) = (o.store.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Acquire) {
                    store
                        .write(|_| {
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            Ok(())
                        })
                        .unwrap();
                }
            })
        });
        // the rows selected, and the button
        window.invoke_tab_chosen(0);
        // The same pairs the reference had selected (its list is in the order
        // it scans the table in; ours is by group).
        let native = listed(&o.store, rule, "pending", &o.hex);
        let theirs = pairs(&step["before_rows"]);
        let rows: Vec<usize> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let at = r.as_u64().unwrap() as usize;
                // (a pair already gone from the database but still on the
                // window's list, as the reference's stale list had it, is
                // taken by its place: the lists then have the same pairs)
                native
                    .iter()
                    .position(|p| *p == theirs[at])
                    .unwrap_or_else(|| {
                        // only a press that goes ahead can have such rows
                        assert!(
                            !declined,
                            "a pair of the reference's list is missing from ours: {} {} row {at} of {}",
                        step["button"],
                        step["answer"],
                        native.len()
                        );
                        at
                    })
            })
            .collect();
        for (n, row) in rows.iter().enumerate() {
            window.invoke_row_clicked(*row as i32, n > 0, false);
        }
        if approve {
            window.invoke_approve();
        } else {
            window.invoke_deny();
        }
        match asked.as_slice() {
            [] => assert!(!window.get_asking(), "{step}"),
            [question] => {
                assert!(window.get_asking(), "{step}");
                assert_eq!(
                    window.get_asking_message(),
                    question["message"].as_str().unwrap()
                );
                if declined {
                    window.invoke_cancelled();
                } else {
                    window.invoke_chosen(0);
                }
            }
            _ => panic!("one question at most"),
        }
        // the progress shown on the button as the work goes, chunk by chunk
        let button = |window: &hydrus_gui::AutoResolutionReviewWindow| {
            if approve {
                window.get_approve_text()
            } else {
                window.get_deny_text()
            }
            .to_string()
        };
        let mut texts: Vec<String> = Vec::new();
        if gate.is_some() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                slint::platform::update_timers_and_animations();
                let text = button(&window);
                if texts.last() != Some(&text) && text != "approve" && text != "deny" {
                    texts.push(text);
                }
                if !texts.is_empty() && !window.get_working() {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "{texts:?}");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(gate) = gate {
            gate.join().unwrap();
        }
        settle(&window);
        assert_eq!(texts, recorded_texts, "the button's progress");
        assert_eq!(
            button(&window),
            if approve { "approve" } else { "deny" },
            "back to its label"
        );
        assert_eq!(
            window.get_label(),
            step["after"]["label"].as_str().unwrap(),
            "{step}"
        );
        // what the database holds afterwards, as the reference's own fetch
        // after the step has it (a pair between files an approval has put in
        // one group is gone)
        let left = listed(&o.store, rule, "pending", &o.hex);
        assert_eq!(
            sorted(left),
            sorted(pairs(&step["after"]["database_pending"])),
            "{step}"
        );
    }
}
