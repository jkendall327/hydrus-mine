//! Options > external programs > external calls, edit and import, replaying
//! the reference's own sequence (`oracle/fixtures/external_calls.json`, from
//! `oracle/record_external_calls.py`): the platform defaults, the synthetic
//! call added twice, the first edited to "edited 日本", Apply, reopen, then
//! the recorded export imported back and a weird call declined and accepted.
use super::external_calls::{call, named, open, question, saved, store};
use hydrus_core::external_calls::Callable;
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn rows(w: &OptionsWindow) -> Vec<Vec<String>> {
    w.get_external_call_rows()
        .iter()
        .map(|row| row.cells.iter().map(|c| c.to_string()).collect())
        .collect()
}

/// The same names, in any order: the recorder added the calls through the
/// panel's `_AddCallableFullyFormed`/`_AddCallableViaImport` themselves, which
/// append, where the list's import button then sorts
/// (`_ImportFromClipboard`), as ours does.
fn same_names(ours: Vec<String>, recorded: &Value) -> bool {
    let mut ours = ours;
    let mut recorded = strings(recorded);
    ours.sort();
    recorded.sort();
    ours == recorded
}

fn names(w: &OptionsWindow) -> Vec<String> {
    rows(w).into_iter().map(|row| row[0].clone()).collect()
}

/// Paste `text` into the list's import window and accept it.
fn import(bound: &Bound, w: &OptionsWindow, text: &str) {
    w.invoke_external_call_action("import".into());
    let exchange = bound
        .options_external_calls
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    exchange.set_text(text.into());
    exchange.invoke_action("review".into());
    assert!(exchange.get_ready(), "{}", exchange.get_error());
    exchange.invoke_action("accept".into());
    bound.options_external_calls.exchange.cancel();
}

/// The recorder's synthetic call (its export's first call, as it was before
/// the edit renamed it), twice in one serialised list.
fn synthetic_pair(reference: &Value) -> String {
    let export: Value = serde_json::from_str(reference["export"].as_str().unwrap()).unwrap();
    let mut call = export[2][0][1].clone();
    call[1] = json!("synthetic call");
    call[3][0] = json!("11".repeat(32));
    json!([26, 3, [[2, call.clone()], [2, call]]]).to_string()
}

/// A staged call's key, read from what the list's export would write.
fn key_of(bound: &Bound, w: &OptionsWindow, name: &str) -> [u8; 32] {
    w.invoke_external_call_clicked(named(w, name), false, false);
    w.invoke_external_call_action("export".into());
    let text = bound
        .options_external_calls
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .get_text();
    bound.options_external_calls.exchange.cancel();
    let calls = hydrus_downloader_exchange::external_calls::decode_text(&text).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, name);
    calls[0].key
}

/// Everything of a call but its key.
fn keyless(calls: &[Callable]) -> Vec<Callable> {
    calls
        .iter()
        .map(|c| Callable {
            key: [0; 32],
            ..c.clone()
        })
        .collect()
}

// leaf: audit-options-external-programs-external-calls-edit
#[test]
fn editing_a_call_replays_the_reference_s_list_staging_save_and_reopen() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    // the reference's list starts with the Linux defaults; add them as a user would
    w.invoke_external_call_action("defaults-all".into());
    question(&bound).invoke_answered(true);
    let original: Vec<Vec<String>> = reference["original_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(strings)
        .collect();
    assert_eq!(rows(&w), original);
    w.invoke_external_call_clicked(-1, false, false);

    import(&bound, &w, &synthetic_pair(&reference));
    let recorded_rows: Vec<Vec<String>> = reference["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(strings)
        .collect();
    let added: Vec<Vec<String>> = rows(&w)
        .into_iter()
        .filter(|row| row[0].starts_with("synthetic"))
        .collect();
    assert_eq!(added, recorded_rows);
    let first = key_of(&bound, &w, "synthetic call");
    let second = key_of(&bound, &w, "synthetic call (1)");
    // fresh keys, unique (keys_fresh, keys_unique)
    assert_eq!(
        vec![first != [0x11; 32], second != [0x11; 32]],
        reference["keys_fresh"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(first != second, reference["keys_unique"].as_bool().unwrap());

    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.set_name("edited 日本".into());
    // the list does not change until the child is applied
    assert!(same_names(
        names(&w),
        &reference["parent_before_child_accept"]
    ));
    child.invoke_apply();
    assert_eq!(
        key_of(&bound, &w, "edited 日本") == first,
        reference["child_retains_key"].as_bool().unwrap()
    );
    assert!(saved(&store).calls.is_empty(), "staged until Options Apply");
    w.invoke_apply();

    let recorded = hydrus_downloader_exchange::external_calls::decode_manager(
        &reference["saved_manager"].to_string(),
    )
    .unwrap();
    let persisted = saved(&store);
    let mut ours = keyless(&persisted.calls);
    let mut theirs = keyless(&recorded.calls);
    ours.sort_by(|a, b| a.name.cmp(&b.name));
    theirs.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(ours, theirs);
    assert!(
        persisted
            .calls
            .iter()
            .any(|c| c.key == first && c.name == "edited 日本")
    );

    let w = open(&ui, &bound);
    assert_eq!(names(&w), strings(&reference["reopened_names"]));
    w.invoke_cancel();
}

// leaf: audit-options-external-programs-external-calls-import
#[test]
fn importing_replays_the_reference_s_names_keys_and_weird_call_question() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    let (_dirs, store) = store();
    // the reference's list when it imported: its saved manager
    let recorded = hydrus_downloader_exchange::external_calls::decode_manager(
        &reference["saved_manager"].to_string(),
    )
    .unwrap();
    let before = recorded.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &before))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    assert_eq!(names(&w), strings(&reference["reopened_names"]));

    import(&bound, &w, reference["export"].as_str().unwrap());
    assert!(
        same_names(names(&w), &reference["after_import"]),
        "{:?}",
        names(&w)
    );
    let exported = hydrus_downloader_exchange::external_calls::decode_text(
        reference["export"].as_str().unwrap(),
    )
    .unwrap();
    for name in ["edited 日本 (1)", "synthetic call (1) (1)"] {
        let key = key_of(&bound, &w, name);
        assert!(
            exported.iter().all(|c| c.key != key) && recorded.calls.iter().all(|c| c.key != key),
            "an imported call has a fresh key"
        );
    }

    // a weird call: the reference asks; no declines with its error
    let weird = json!([
        26,
        3,
        [[
            2,
            [
                156,
                "weird",
                1,
                [
                    "22".repeat(32),
                    1,
                    [
                        157,
                        1,
                        ["x".repeat(257), [], [26, 3, []], 15, false, true, true]
                    ]
                ]
            ]
        ]]
    ])
    .to_string();
    import(&bound, &w, &weird);
    let asked = question(&bound);
    assert_eq!(
        asked.get_message().as_str(),
        reference["questions"][0]["message"]
    );
    asked.invoke_answered(false);
    assert_eq!(w.get_external_call_error().as_str(), reference["declined"]);
    assert!(
        same_names(names(&w), &reference["after_import"]),
        "{:?}",
        names(&w)
    );
    // and yes adds it
    import(&bound, &w, &weird);
    question(&bound).invoke_answered(true);
    assert!(names(&w).contains(&"weird".to_owned()));

    // nothing is saved until Options is applied
    assert_eq!(saved(&store), recorded);
    w.invoke_apply();
    assert_eq!(saved(&store).calls.len(), recorded.calls.len() + 3);
}
