//! Flow 5 (`oracle/record_flow_duplicates.py`): the duplicate filter
//! launched from a duplicates page on the database the reference's
//! auto-resolution run left, three pairs decided ("this is better, delete
//! the other", "same quality", "alternates"), and the window closed with
//! its decisions committed. After each step what the filter shows, and what
//! the files and their duplicate groups are, must be what the reference's
//! were.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::service::builtin_keys;
use hydrus_store::duplicates::{DuplicateFilterSettings, FileScope, file_relationships};

use super::{Known, Normaliser, Site, State, compare, install};
use crate::auto_resolution_review::{Opened, opened_from};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

const BATCH: u32 = 6;

fn hex(o: &Opened, id: HashId) -> String {
    o.hex[&id].clone()
}

/// What the filter window shows now (`observe` in the recorder).
fn canvas(o: &Opened, window: &hydrus_gui::DuplicateFilterWindow) -> Json {
    let shown = hydrus_gui::shown_pair().map(|(a, b)| (hex(o, a), hex(o, b)));
    json!({
        "index": window.get_index_text().as_str(),
        "open": window.window().is_visible(),
        "shown": shown.as_ref().map(|s| s.0.clone()),
        "other": shown.map(|s| s.1),
    })
}

/// The files' duplicate relationships, as the Client API lists them
/// (`file_relationships_for_api`).
fn relationships(o: &Opened, files: &[String]) -> Json {
    let snapshot = o.store.snapshot();
    let local = snapshot
        .services
        .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)
        .unwrap()
        .id;
    let my_files = snapshot
        .services
        .builtin(builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let scope = FileScope::Domains {
        current: vec![my_files],
        deleted: Vec::new(),
    };
    let set = |v: &[HashId]| -> Vec<String> {
        let mut out: Vec<String> = v.iter().map(|h| hex(o, *h)).collect();
        out.sort();
        out
    };
    let mut out = BTreeMap::new();
    for file in files {
        let id = o.ids[file];
        let r = o
            .store
            .read(|c| file_relationships(c, &scope, local, id))
            .unwrap();
        out.insert(
            file.clone(),
            json!({
                "0": set(&r.potentials),
                "1": set(&r.false_positives),
                "3": set(&r.alternates),
                "8": set(&r.duplicates),
                "is_king": r.king == id,
                "king": hex(o, r.king),
            }),
        );
    }
    json!(out)
}

#[test]
fn pairs_are_decided_in_the_duplicate_filter_and_committed_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_duplicates.json");
    let _windows = hydrus_gui::headless::init();
    let o = opened_from("auto_resolution");
    install(&o.store, &recording);
    let site = Site::serve("flow_duplicates.json", &recording);
    let n = Normaliser::new(&site);
    o.ui.show().unwrap();
    // the options a user would set in file > options > duplicates
    o.store
        .write(|ctx| {
            let mut options: DuplicateFilterSettings = hydrus_store::settings::get(ctx.conn())?;
            options.max_batch_size = BATCH;
            options.auto_commit_batch_size = None;
            hydrus_store::settings::set(ctx.conn(), &options)
        })
        .unwrap();
    // the pairs are prepared: tags of their own on each file, some shared
    let prepared = recording["prepared"].as_object().unwrap().clone();
    let service = o.store.snapshot().services.by_name("my tags").unwrap().id;
    o.store
        .write_content({
            let ids = o.ids.clone();
            move |w| {
                for (hash, tags) in &prepared {
                    for tag in tags.as_array().unwrap() {
                        let id = hydrus_store::master::intern_tag(
                            w.conn(),
                            &hydrus_core::Tag::from_clean(tag.as_str().unwrap()),
                        )?;
                        w.update_mappings(
                            service,
                            &hydrus_store::content::MappingAction::Add,
                            id,
                            &[ids[hash]],
                        )?;
                    }
                }
                Ok(())
            }
        })
        .unwrap();
    let state = State {
        store: &o.store,
        ui: &o.ui,
        site: &site,
        n: &n,
    };
    let files: Vec<String> = recording["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let step = |name: &str, dialog: Json, asked: Json| -> Json {
        json!({
            "step": name,
            "dialog": dialog,
            "asked": asked,
            "files": state.files(files.iter().cloned().collect::<BTreeSet<_>>()),
            "relationships": relationships(&o, &files),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. launch: the duplicates page's "launch the filter"
    o.ui.invoke_launch_filter();
    let window = o
        .bound
        .filter
        .borrow()
        .as_ref()
        .expect("the filter opens")
        .clone_strong();
    steps.push(step(
        "launch",
        json!({
            "canvas": canvas(&o, &window),
            // (the window shows "n/6": the batch's size)
            "pairs in the batch": window
                .get_index_text()
                .split('/')
                .nth(1)
                .and_then(|s| s.split(' ').next())
                .and_then(|s| s.parse::<u64>().ok()),
        }),
        json!([]),
    ));

    // 2. decide
    let mut decisions = Vec::new();
    for (name, command) in [
        ("better_delete_other", "better-delete"),
        ("same", "same"),
        ("alternates", "alternates"),
    ] {
        window.invoke_decide(command.into());
        decisions.push(json!({"do": name, "canvas": canvas(&o, &window)}));
    }
    steps.push(step("decide", json!({"decisions": decisions}), json!([])));

    // 3. close: it asks to commit the three decisions; "commit"
    window.invoke_close_requested();
    let mut asked = Vec::new();
    while !window.get_question().is_empty() && window.window().is_visible() {
        let question = window.get_question().to_string();
        let buttons: Vec<String> = window.get_answers().iter().map(|a| a.to_string()).collect();
        let at = buttons.iter().position(|b| b == "commit").unwrap();
        let mut sorted = buttons.clone();
        sorted.sort();
        asked.push(json!({"question": question, "buttons": sorted, "pressed": "commit"}));
        window.invoke_answer(i32::try_from(at).unwrap());
    }
    let open = window.window().is_visible();
    // (a window that has closed keeps the words of its last question)
    assert!(
        !open || window.get_question().is_empty(),
        "step 3 (close): a question is left unanswered: {}",
        window.get_question()
    );
    // the commit is written on a worker: wait until the first pair's loser
    // (deleted by the first decision) is in the trash
    let loser = recording["steps"][0]["dialog"]["canvas"]["other"]
        .as_str()
        .unwrap()
        .to_owned();
    let in_trash = || {
        state.files(BTreeSet::from([loser.clone()]))[&loser]["in"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == "trash")
    };
    let wait = std::time::Instant::now();
    while !in_trash() && wait.elapsed() < std::time::Duration::from_secs(30) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        in_trash(),
        "step 3 (close): the first pair's deleted file ({loser}) was not in the trash after 30 seconds"
    );
    steps.push(step("close", json!({"ok to close": !open}), json!(asked)));

    compare(&steps, &recording, KNOWN);
}
