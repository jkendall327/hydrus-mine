//! The "manage ratings" dialog replayed against the reference's
//! (`oracle/fixtures/manage_ratings.json`, from
//! `oracle/record_manage_ratings.py`), over the `basic` fixture's rating
//! services: each control's state, what copy copies, what paste says, and
//! what apply writes, step by step.

use std::collections::HashMap;

use hydrus_core::ServiceId;
use hydrus_gui_model::ratings_editor::{Like, Numerical, RatingsEditor, State, Update, title};
use hydrus_store::Store;
use hydrus_store::media::Rating;
use serde_json::{Value, json};

fn state(editor: &RatingsEditor) -> Value {
    let rows: Vec<Value> = editor
        .rows
        .iter()
        .map(|row| {
            let (code, rating) = match row.now {
                State::Like(like) => (
                    match like {
                        Like::Like => 0,
                        Like::Dislike => 1,
                        Like::Null => 2,
                        Like::Mixed => 4,
                    },
                    Value::Null,
                ),
                State::Numerical(n) => match n {
                    Numerical::Set(f) => (3, json!(f)),
                    Numerical::Null => (2, json!(0.0)),
                    Numerical::Mixed => (4, json!(0.0)),
                },
                State::IncDec { value, mixed } => (if mixed { 4 } else { 3 }, json!(value)),
            };
            json!([row.name, code, rating])
        })
        .collect();
    json!(rows)
}

#[test]
fn the_manage_ratings_dialog_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("manage_ratings.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let services = store.snapshot().services.clone();
    let ids: HashMap<String, ServiceId> = ["favourites", "stars", "counter"]
        .iter()
        .map(|name| {
            let service = services.by_name(name).unwrap();
            assert_eq!(service.key.to_hex(), recorded["keys"][name]);
            ((*name).to_owned(), service.id)
        })
        .collect();
    let names: HashMap<ServiceId, String> = ids.iter().map(|(n, i)| (*i, n.clone())).collect();
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let files: Vec<HashMap<ServiceId, Rating>> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                f[1].as_object()
                    .unwrap()
                    .iter()
                    .map(|(name, rating)| {
                        let rating = if name == "counter" {
                            Rating::IncDec(rating.as_i64().unwrap())
                        } else {
                            Rating::Fraction(rating.as_f64().unwrap())
                        };
                        (ids[name], rating)
                    })
                    .collect()
            })
            .collect();
        let mut hashes: Vec<&str> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f[0].as_str().unwrap())
            .collect();
        hashes.sort_unstable();
        let mut editor = RatingsEditor::new(&services, &files);
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(state(&editor), steps[0]["state"], "case {c} start");
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("case {c} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let row = |name: &Value| {
                editor_row(&editor, name.as_str().unwrap()).unwrap_or_else(|| panic!("{context}"))
            };
            let mut said = Vec::new();
            let mut copied = Vec::new();
            let mut written = Vec::new();
            match action[0].as_str().unwrap() {
                "left" => {
                    let i = row(&action[1]);
                    editor.left(i, 0.0);
                }
                "right" => {
                    let i = row(&action[1]);
                    editor.right(i);
                }
                "stars" => {
                    // (a click at the nth of the service's stars)
                    let i = row(&action[1]);
                    let n = action[2].as_f64().unwrap();
                    editor.left(i, n / recorded["stars"]["num_stars"].as_f64().unwrap());
                }
                "copy" => {
                    let (text, notice) = editor.copy();
                    copied.push(json!(text));
                    said.push(json!({ "notice": notice }));
                }
                "paste" | "paste_raw" => {
                    let text = if action[0] == "paste" {
                        let pairs: Vec<String> = action[1]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|p| {
                                let name = p[0].as_str().unwrap();
                                let key = recorded["keys"][name].as_str().unwrap_or(name);
                                format!("[\"{key}\", {}]", python(&p[1]))
                            })
                            .collect();
                        format!("[{}]", pairs.join(", "))
                    } else {
                        action[1].as_str().unwrap().to_owned()
                    };
                    match editor.paste(&text) {
                        Ok(notice) => said.push(json!({ "notice": notice })),
                        Err(message) => {
                            said.push(
                                json!({ "critical": "Clipboard Error!", "message": message }),
                            );
                        }
                    }
                }
                "ok" => {
                    for (service, update) in editor.updates() {
                        let rating = match update {
                            Update::Rating(r) => json!(r),
                            Update::IncDec(v) => json!(v),
                        };
                        written.push(json!([names[&service], rating, hashes]));
                    }
                }
                other => panic!("{other}"),
            }
            // (the reference's JSON errors are Python's; ours say the same
            // up to the error itself, and fromhex's exactly)
            let recorded_said: Vec<Value> = step["said"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&said)
                .map(|(theirs, ours)| {
                    match (theirs["message"].as_str(), ours["message"].as_str()) {
                        (Some(t), Some(o)) if !t.contains("ValueError") => {
                            let cut =
                                |m: &str| m[..m.find("general error was:").unwrap()].to_owned();
                            assert_eq!(cut(t), cut(o), "{context}");
                            ours.clone()
                        }
                        _ => theirs.clone(),
                    }
                })
                .collect();
            assert_eq!(json!(said), json!(recorded_said), "{context}");
            assert_eq!(json!(copied), step["copied"], "{context}");
            assert_eq!(
                numbers(&json!(written)),
                numbers(&step["written"]),
                "{context}"
            );
            assert_eq!(
                numbers(&state(&editor)),
                numbers(&step["state"]),
                "{context}"
            );
        }
    }
    assert_eq!(title(3), "manage ratings for 3 files");
}

fn editor_row(editor: &RatingsEditor, name: &str) -> Option<usize> {
    editor.rows.iter().position(|r| r.name == name)
}

/// A pasted value as Python's `json.dumps` writes it.
fn python(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Number(n) if n.is_f64() => {
            let mut out = String::new();
            hydrus_core::pyjson::write_python_float(n.as_f64().unwrap(), &mut out);
            out
        }
        other => other.to_string(),
    }
}

/// Numbers as floats, so a like's 1 is its 1.0.
fn numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(numbers).collect()),
        other => other.clone(),
    }
}

#[test]
fn a_middle_click_count_sets_an_inc_dec_control_and_nothing_else() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let services = store.snapshot().services.clone();
    let counter = services.by_name("counter").unwrap().id;
    let files = vec![
        HashMap::from([(counter, Rating::IncDec(4))]),
        HashMap::new(),
    ];
    let mut editor = RatingsEditor::new(&services, &files);
    let row = editor_row(&editor, "counter").unwrap();
    let stars = editor_row(&editor, "stars").unwrap();
    assert_eq!(editor.count(stars), None);
    editor.set_count(stars, 9);
    assert_eq!(editor.count(stars), None);
    editor.set_count(row, 123_456);
    assert_eq!(editor.count(row), Some(123_456));
    // (a typed count can't go below nothing)
    editor.set_count(row, -3);
    assert_eq!(editor.count(row), Some(0));
}
