//! The thumbnail menu's maintenance jobs chosen in the main window against
//! `oracle/fixtures/regen_questions.json` (recorded by
//! `oracle/record_regen_questions.py` from the reference's grid menu): the
//! jobs offered in the menu's order, each one's question (its wording, with
//! the number of files for the three that count them), its dialog's title and
//! buttons, and that "no" queues nothing.

use hydrus_core::HashId;
use hydrus_gui::thumbnail_maintenance_window;
use serde_json::Value;

use super::media_support::{rows, start_local};

// leaf: audit-media-context-missing-maintenance
#[test]
fn every_maintenance_job_asks_what_the_reference_asks() {
    let recorded = hydrus_testkit::fixture_json("regen_questions.json");
    let fixture = start_local();
    let hex = |files: &[HashId]| -> Vec<String> {
        let hashes = fixture
            .store
            .read(|c| hydrus_store::master::hashes(c, files))
            .unwrap();
        files.iter().map(|f| hashes[f].to_hex()).collect()
    };
    let names = hex(&fixture.results());
    let at = |hash: &str| -> i32 {
        i32::try_from(names.iter().position(|n| n == hash).unwrap()).unwrap()
    };
    let queued = || -> i64 {
        fixture
            .store
            .read(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM file_maintenance_jobs", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap()
    };
    let before = queued();
    let mut asked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let hashes: Vec<&str> = case["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap())
            .collect();
        let others: Vec<i32> = hashes[1..].iter().map(|h| at(h)).collect();
        fixture.select(at(hashes[0]), &others);
        assert_eq!(fixture.selected().len(), hashes.len());
        let menu = fixture.menu(-1);
        let jobs = rows(&menu.manage_maintenance);
        let theirs: Vec<&Value> = case["rows"].as_array().unwrap().iter().collect();
        assert_eq!(
            jobs.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
            theirs
                .iter()
                .map(|r| r["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            "the jobs for {} files",
            hashes.len()
        );
        for row in theirs {
            let label = row["label"].as_str().unwrap();
            let want = &row["asked"][0];
            let menu = fixture.menu(-1);
            // (the exact label: one is the start of another)
            let id = rows(&menu.manage_maintenance)
                .into_iter()
                .find(|(l, _)| l == label)
                .unwrap()
                .1;
            fixture.ui.invoke_menu_chosen(id);
            let question = thumbnail_maintenance_window::question().expect("a question");
            assert_eq!(
                question.get_message(),
                want["message"].as_str().unwrap(),
                "{label} ({})",
                hashes.len()
            );
            assert_eq!(question.get_window_title(), want["title"].as_str().unwrap());
            assert_eq!(question.get_yes_label(), want["yes"].as_str().unwrap());
            assert_eq!(question.get_no_label(), want["no"].as_str().unwrap());
            question.invoke_answered(false);
            asked += 1;
        }
    }
    assert_eq!(asked, 54);
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(queued(), before, "\"no\" queues nothing");
}
