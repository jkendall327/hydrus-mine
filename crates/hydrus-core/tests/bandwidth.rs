//! Bandwidth rules and usage tracking against the reference
//! (`oracle/dump_bandwidth.py`): random usage and time passing, and every
//! answer the reference gave along the way.

use serde_json::Value as Json;

use hydrus_core::bandwidth::{BandwidthType, GalleryTokenKind, Manager, Rule, Rules, Tracker};
use hydrus_core::network::NetworkContext;

fn fixture() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/bandwidth.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn rule(r: &Json) -> Rule {
    Rule::new(
        BandwidthType::from_code(r[0].as_i64().unwrap()).unwrap(),
        r[1].as_u64(),
        r[2].as_u64().unwrap(),
    )
}

fn context(c: &Json) -> NetworkContext {
    NetworkContext {
        kind: c[0].as_i64().unwrap(),
        data: c[1].as_str().unwrap_or_default().to_owned(),
    }
}

#[test]
fn trackers_count_and_rules_judge_as_the_reference_does() {
    let fixture = fixture();
    let cases = fixture["trackers"].as_array().unwrap();
    assert!(cases.len() > 100);
    for (n, case) in cases.iter().enumerate() {
        let rules: Vec<Rule> = case["rules"].as_array().unwrap().iter().map(rule).collect();
        let judge = Rules::new(rules.clone());
        let mut now = case["start"].as_i64().unwrap();
        let mut tracker = Tracker::new(now);
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            now += step["advance"].as_i64().unwrap();
            if let Some(n) = step["requests"].as_u64() {
                tracker.report_requests(n, now);
            }
            if let Some(n) = step["bytes"].as_u64() {
                tracker.report_data(n, now);
            }
            let expected = &step["answers"];
            let at = format!("case {n}, step {i}, rules {rules:?}");
            for (r, e) in rules.iter().zip(expected["rules"].as_array().unwrap()) {
                let usage = tracker.usage(r.kind, r.time_delta, now);
                let estimate = tracker.waiting_estimate(r.kind, r.time_delta, r.max_allowed, now);
                assert_eq!(
                    (usage, estimate),
                    (e[0].as_u64().unwrap(), e[1].as_u64().unwrap()),
                    "{at}: {r:?}"
                );
            }
            let ours = (
                judge.can_start_request(&mut tracker, now),
                judge.can_continue_download(&mut tracker, now),
                judge.can_do_work(&mut tracker, 1, 1_048_576, 30, now),
                judge.can_do_work(&mut tracker, 1, 1_048_576, 90, now),
                judge.waiting_estimate(&mut tracker, now),
            );
            let theirs = (
                expected["can_start"].as_bool().unwrap(),
                expected["can_continue"].as_bool().unwrap(),
                expected["can_do_work_30"].as_bool().unwrap(),
                expected["can_do_work_90"].as_bool().unwrap(),
                expected["waiting_estimate"].as_u64().unwrap(),
            );
            assert_eq!(ours, theirs, "{at}");
        }
        // what is kept is what the reference keeps
        let ours: Vec<Vec<(i64, u64)>> = tracker
            .to_counters()
            .into_iter()
            .map(|mut c| {
                c.sort_unstable();
                c
            })
            .collect();
        let theirs: Vec<Vec<(i64, u64)>> = case["counters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c.as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (p[0].as_i64().unwrap(), p[1].as_u64().unwrap()))
                    .collect()
            })
            .collect();
        assert_eq!(ours, theirs, "case {n}'s counters");
    }
}

#[test]
fn the_manager_starts_requests_as_the_reference_does() {
    let fixture = fixture();
    let cases = fixture["managers"].as_array().unwrap();
    assert!(!cases.is_empty());
    let (mut started, mut refused) = (0, 0);
    for (n, case) in cases.iter().enumerate() {
        let rules = case["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let rules = r[1].as_array().unwrap().iter().map(rule);
                (context(&r[0]), Rules::new(rules))
            })
            .collect();
        let mut manager = Manager::new(rules);
        let jobs: Vec<Vec<NetworkContext>> = case["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| j.as_array().unwrap().iter().map(context).collect())
            .collect();
        let mut now = case["start"].as_i64().unwrap();
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            now += step["advance"].as_i64().unwrap();
            let contexts = &jobs[usize::try_from(step["job"].as_u64().unwrap()).unwrap()];
            let at = format!("case {n}, step {i}");
            if let Some(expected) = step["started"].as_bool() {
                assert_eq!(
                    manager.try_to_start_request(contexts, now),
                    expected,
                    "{at}"
                );
                if expected {
                    started += 1;
                } else {
                    refused += 1;
                }
                let (estimate, whose) = manager.waiting_estimate_and_context(contexts, now);
                assert_eq!(
                    (estimate, whose),
                    (
                        step["estimate"][0].as_u64().unwrap(),
                        context(&step["estimate"][1])
                    ),
                    "{at}"
                );
                if let Some(bytes) = step["bytes"].as_u64() {
                    manager.report_data(contexts, bytes, now);
                }
            } else if let Some(expected) = step["can_do_work"].as_bool() {
                assert_eq!(manager.can_do_work(contexts, 90, now), expected, "{at}");
                assert_eq!(
                    manager.can_continue_download(contexts, now),
                    step["can_continue"].as_bool().unwrap(),
                    "{at}"
                );
            } else {
                let t = &step["token"];
                let kind = match t[0].as_str().unwrap() {
                    "download page" => GalleryTokenKind::DownloadPage,
                    "subscription" => GalleryTokenKind::Subscription,
                    _ => GalleryTokenKind::Watcher,
                };
                let result = manager.try_to_consume_gallery_token(
                    t[1].as_str().unwrap(),
                    kind,
                    t[2].as_i64().unwrap(),
                    now,
                );
                let expected = if t[3].as_bool().unwrap() {
                    Ok(())
                } else {
                    Err(t[4].as_i64().unwrap())
                };
                assert_eq!(result, expected, "{at}");
            }
        }
    }
    assert!(started > 100 && refused > 100, "{started} {refused}");
}
