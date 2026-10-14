//! The archive/delete filter driven through its window against
//! `oracle/fixtures/archive_delete_filter.json` (recorded by
//! `oracle/record_archive_delete_filter.py` from the reference's
//! `CanvasMediaListFilterArchiveDelete`): runs of keep, delete, skip and
//! back, then the finish question (its wording, its commit choices), each
//! of its answers and what each does to the files.

use std::sync::Arc;

use hydrus_core::{HashId, ServiceKey};
use hydrus_gui::{ArchiveDeleteWindow, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use crate::common::widgets;

struct Opened {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    files: Vec<HashId>,
    ui: MainWindow,
    bound: hydrus_gui::Bound,
}

fn open(recorded: &Value, all_local: bool) -> Opened {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hashes: Vec<hydrus_core::Sha256> = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse().unwrap())
        .collect();
    let files: Vec<HashId> = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            Ok(hashes.iter().map(|h| ids[h]).collect())
        })
        .unwrap();
    let marked = files.clone();
    store.write_content(move |w| w.inbox(&marked)).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::archive_delete_preferences::Preferences {
                    delay_multiple: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let location = if all_local {
        LocationContext::default()
    } else {
        LocationContext::single(ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
        ))
    };
    let page = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location,
            ..Default::default()
        },
        false,
        None,
        files.clone(),
    );
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page));
    Opened {
        _dir: dir,
        store,
        files,
        ui,
        bound,
    }
}

fn filter(o: &Opened) -> ArchiveDeleteWindow {
    o.ui.invoke_archive_delete_filter();
    o.bound
        .archive_delete
        .borrow()
        .as_ref()
        .expect("the filter opened")
        .clone_strong()
}

/// (in the inbox, in the trash)
fn state(store: &Store, id: HashId) -> (bool, bool) {
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
        .unwrap();
    let trash = snapshot.services.by_name("trash").unwrap().id;
    (
        batch.results[0].inbox,
        batch.results[0].current.iter().any(|c| c.service == trash),
    )
}

fn step(window: &ArchiveDeleteWindow, op: &str) {
    match op {
        "keep" => window.invoke_keep(),
        "delete" => window.invoke_delete(),
        "skip" => window.invoke_skip(),
        "back" => window.invoke_back(),
        other => panic!("{other}"),
    }
}

// leaf: audit-media-archive-decision
#[test]
fn keep_delete_skip_and_back_go_where_the_references_do() {
    let recorded = hydrus_testkit::fixture_json("archive_delete_filter.json");
    let _windows = headless::init();
    let o = open(&recorded, true);
    let window = filter(&o);
    let total = o.files.len();
    let steps = recorded["decisions"].as_array().unwrap();
    for (n, want) in steps.iter().enumerate() {
        let op = want["do"].as_str().unwrap();
        step(&window, op);
        let current = want["current"].as_u64().unwrap();
        // (a decision on the last file asks to finish; the recording answered
        // "back to filtering": the decision made on that file was dropped)
        if !window.get_question().is_empty() {
            window.invoke_resume();
        }
        assert_eq!(
            window.get_caption(),
            format!("{}/{total}", current + 1),
            "step {n} ({op})"
        );
    }
    // the decisions the reference holds at the end: kept 0 and 1, deleted 2,
    // skipped 3, the last undecided: committing does exactly that
    let last = &steps[steps.len() - 1];
    let ids = |key: &str| -> Vec<HashId> {
        last[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| o.files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect()
    };
    let (kept, deleted, skipped) = (ids("kept"), ids("deleted"), ids("skipped"));
    window.invoke_close_requested();
    window.invoke_commit_choice(0);
    for id in &kept {
        assert_eq!(state(&o.store, *id), (false, false), "kept: archived");
    }
    for id in &deleted {
        assert_eq!(state(&o.store, *id), (true, true), "deleted: in the trash");
    }
    for id in &skipped {
        assert_eq!(state(&o.store, *id), (true, false), "skipped: untouched");
    }
    assert_eq!(
        state(&o.store, o.files[4]),
        (true, false),
        "the last, undecided"
    );
}

// leaf: audit-media-archive-close
#[test]
fn the_finish_question_and_its_answers_are_the_references() {
    let recorded = hydrus_testkit::fixture_json("archive_delete_filter.json");
    let _windows = headless::init();
    let mut asked = 0;
    for case in recorded["closes"].as_array().unwrap() {
        let page = case["page"].as_str().unwrap();
        let plan = case["plan"].as_str().unwrap();
        let button = case["button"].as_str().unwrap();
        let pick = usize::try_from(case["pick"].as_u64().unwrap()).unwrap();
        let questions = case["questions"].as_array().unwrap();
        let multiple = questions.first().is_some_and(|q| {
            q["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| *b == "commit")
                .count()
                > 1
        });
        // (the second commit button is only there to press with several)
        if button == "commit" && pick == 1 && !multiple {
            continue;
        }
        // (nothing to ask: one run is enough)
        if questions.is_empty() && button != "commit" {
            continue;
        }
        let label = format!("{page} / {plan} / {button} {pick}");
        let o = open(&recorded, page == "all local files");
        let window = filter(&o);
        for op in case["ops"].as_array().unwrap() {
            step(&window, op.as_str().unwrap());
        }
        let was_asked = !window.get_question().is_empty();
        assert_eq!(
            was_asked,
            case["asked_by_the_window"].as_bool().unwrap(),
            "{label}"
        );
        if !was_asked {
            window.invoke_close_requested();
        }
        if questions.is_empty() {
            // no kept or deleted file: nothing to commit, and it closes
            assert!(o.bound.archive_delete.borrow().is_none(), "{label}: closes");
            for id in &o.files {
                assert_eq!(state(&o.store, *id), (true, false), "{label}");
            }
            continue;
        }
        asked += 1;
        let q = &questions[0];
        // the question: the kept line over each deletion choice, as the
        // reference's panel words them
        let mut shown: Vec<String> = Vec::new();
        let header = window.get_finish_header().to_string();
        if !header.is_empty() {
            shown.push(header);
        }
        shown.extend(window.get_commit_labels().iter().map(|l| l.to_string()));
        shown.push("-or-".into());
        assert_eq!(shown, strings(&q["labels"]), "{label}: the question");
        let kept = q["kept"].as_str().map_or(0, |k| {
            k.trim_start_matches("keep ").parse::<usize>().unwrap()
        });
        match button {
            "commit" => {
                let commit = &case["commits"][0];
                let choices = q["options"].as_array().unwrap().len().max(1);
                window.invoke_commit_choice(i32::try_from(pick.min(choices - 1)).unwrap());
                assert!(o.bound.archive_delete.borrow().is_none(), "{label}: closes");
                let (mut archived, mut trashed) = (0, 0);
                for id in &o.files {
                    match state(&o.store, *id) {
                        (false, false) => archived += 1,
                        (true, true) => trashed += 1,
                        _ => {}
                    }
                }
                assert_eq!(
                    archived,
                    commit["kept"].as_array().unwrap().len(),
                    "{label}: archived"
                );
                assert_eq!(
                    trashed,
                    commit["deleted"].as_array().unwrap().len(),
                    "{label}: trashed"
                );
                assert_eq!(kept, archived, "{label}");
            }
            "forget" => {
                // asks, and on yes closes with nothing done
                window.invoke_forget();
                assert!(window.get_forget_question(), "{label}");
                assert!(
                    widgets::shows(window.window(), q["forget_questions"][0].as_str().unwrap()),
                    "{label}"
                );
                window.invoke_forget_answered(true);
                assert!(o.bound.archive_delete.borrow().is_none(), "{label}: closes");
                for id in &o.files {
                    assert_eq!(state(&o.store, *id), (true, false), "{label}: nothing done");
                }
            }
            _ => {
                // back to filtering: on the file it was on, with the decisions
                // the reference kept (a decision on the last file is dropped)
                window.invoke_resume();
                assert!(window.get_question().is_empty(), "{label}");
                let after = &case["after"];
                let total = o.files.len();
                assert_eq!(
                    window.get_caption(),
                    format!("{}/{total}", after["current"].as_u64().unwrap() + 1),
                    "{label}"
                );
                // asked again, the question is the reference's for the
                // decisions left
                window.invoke_close_requested();
                let again = case["again"].as_array().unwrap();
                if let Some(q) = again.first() {
                    let mut shown: Vec<String> = Vec::new();
                    let header = window.get_finish_header().to_string();
                    if !header.is_empty() {
                        shown.push(header);
                    }
                    shown.extend(window.get_commit_labels().iter().map(|l| l.to_string()));
                    shown.push("-or-".into());
                    assert_eq!(shown, strings(&q["labels"]), "{label}: asked again");
                } else {
                    assert!(
                        o.bound.archive_delete.borrow().is_none(),
                        "{label}: nothing left"
                    );
                }
            }
        }
    }
    assert!(asked > 30, "{asked}");
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
