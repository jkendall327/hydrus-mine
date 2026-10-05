//! The Database menu's maintenance entries against
//! `oracle/record_database_maintenance.py`'s recording of the basic client:
//! their questions and buttons, the service choices, and the popups an
//! accepted run sends.
use hydrus_gui_model::database_maintenance::{
    Answer, Asking, Job, WHICH_SERVICE, run, service_choices,
};
use hydrus_gui_model::main_menu::{Command, Entry, Facts, menubar};
use hydrus_store::Store;
use serde_json::Value;

fn basic() -> (tempfile::TempDir, tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (legacy, dir, store)
}

fn events(fixture: &Value, label: &str) -> Vec<Value> {
    fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["label"] == label)
        .cloned()
        .collect()
}

#[test]
fn questions_buttons_and_service_choices_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("database_maintenance.json");
    let (_legacy, _dir, store) = basic();
    for job in Job::ALL {
        let recorded = events(&fixture, job.label());
        assert!(!recorded.is_empty(), "{job:?} was recorded");
        let refused = &recorded[0];
        let asked = &refused["asked"][0];
        match job.asking() {
            Asking::YesNo { yes, no } => {
                assert_eq!(asked["kind"], "yes_no");
                assert_eq!(asked["message"], job.question(), "{job:?}");
                assert_eq!(
                    (
                        asked["yes_label"].as_str().unwrap(),
                        asked["no_label"].as_str().unwrap()
                    ),
                    (yes, no)
                );
            }
            Asking::YesYesNo { yes, no } => {
                assert_eq!(asked["message"], job.question());
                let recorded: Vec<&str> = asked["yes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|y| y[0].as_str().unwrap())
                    .collect();
                assert_eq!(recorded, yes);
                assert_eq!(asked["no_label"], no);
            }
            Asking::Buttons { title, choices } => {
                assert_eq!(asked["message"], job.question());
                assert_eq!(asked["title"], title);
                let recorded: Vec<&str> = asked["choices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c[0].as_str().unwrap())
                    .collect();
                assert_eq!(
                    recorded,
                    choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>()
                );
            }
        }
        // refusing asks nothing more and writes nothing
        assert_eq!(refused["asked"].as_array().unwrap().len(), 1);
        assert!(refused["writes"].as_array().unwrap().is_empty());
        let accepted = &recorded[1];
        let chooser = accepted["asked"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["title"] == WHICH_SERVICE);
        assert_eq!(chooser.is_some(), job.chooses_service(), "{job:?}");
        if let Some(chooser) = chooser {
            let offered: Vec<(String, String)> = service_choices(&store, job)
                .into_iter()
                .map(|(name, _, tip)| (name, tip))
                .collect();
            let recorded: Vec<(String, String)> = chooser["choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    (
                        c[0].as_str().unwrap().to_owned(),
                        c[2].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            assert_eq!(offered, recorded);
        }
    }
}

#[test]
fn accepted_runs_send_the_reference_s_popups() {
    let fixture = hydrus_testkit::fixture_json("database_maintenance.json");
    for job in Job::ALL {
        let (_legacy, _dir, store) = basic();
        let accepted = &events(&fixture, job.label())[1];
        let outcome = run(&store, job, &Answer::default(), 1_000).unwrap();
        let shown: Vec<(Option<String>, Option<String>)> = store
            .read(|conn| hydrus_store::popups::all(conn, 1_000))
            .unwrap()
            .into_iter()
            .map(|p| (p.status_title, p.status_text_1))
            .collect();
        let recorded: Vec<(Option<String>, Option<String>)> = accepted["popups"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| !p["dismissed"].as_bool().unwrap())
            .map(|p| {
                (
                    p["title"].as_str().map(str::to_owned),
                    p["text_1"].as_str().map(str::to_owned),
                )
            })
            .collect();
        if job == Job::TablesUsingDefinitions {
            // the reference's v688 read fails (NotImplementedError); hydrus-rs
            // lists its own tables
            assert_eq!(accepted["read_error"], "DBException");
            assert!(
                outcome
                    .clipboard
                    .unwrap()
                    .lines()
                    .any(|l| l == "file_urls,hash_id")
            );
            assert!(
                shown[0]
                    .1
                    .as_ref()
                    .unwrap()
                    .ends_with("table and column pairs sent to clipboard.")
            );
            continue;
        }
        assert_eq!(shown, recorded, "{job:?}");
    }
}

#[test]
fn the_menus_enable_the_jobs_hydrus_rs_runs() {
    fn find<'a>(entries: &'a [Entry], label: &str) -> Option<&'a Entry> {
        entries.iter().find_map(|e| match e {
            Entry::Menu { entries, .. } => find(entries, label),
            other if other.label() == label => Some(other),
            _ => None,
        })
    }
    let menus = menubar(&Facts::default());
    for job in Job::ALL {
        let entry = find(&menus, &format!("{}\u{2026}", job.label())).unwrap();
        assert!(entry.usable(), "{job:?}");
        assert!(
            matches!(entry, Entry::Item { command: Some(Command::DatabaseMaintenance(j)), .. } if *j == job)
        );
    }
    assert!(!find(&menus, "review vacuum data\u{2026}").unwrap().usable());
}
