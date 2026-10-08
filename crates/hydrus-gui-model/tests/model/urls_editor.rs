//! The "manage urls" dialog replayed against the reference's
//! (`oracle/fixtures/manage_urls.json`, from `oracle/record_manage_urls.py`):
//! its rows (with counts for several files), the warning, the box, what is
//! asked, what copy copies, and the updates, step by step. URLs are
//! normalised as a store with no URL classes does.

use hydrus_core::url::{UrlClassSettings, UrlClasses};
use hydrus_gui_model::urls_editor::{UrlUpdate, UrlsEditor};
use serde_json::{Value, json};

fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}

// leaf: audit-media-urls-edit, audit-media-urls-clipboard
#[test]
fn the_manage_urls_dialog_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("manage_urls.json");
    let classes = UrlClasses::new(UrlClassSettings::default());
    let normalise = |url: &str| -> Option<String> {
        hydrus_core::url::functions::check_full_url(url).ok()?;
        classes.normalise(url, true).ok()
    };
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let files: Vec<(String, Vec<String>)> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f[0].as_str().unwrap().to_owned(), strings(&f[1])))
            .collect();
        let hashes: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
        let mut editor = UrlsEditor::new(files.into_iter().map(|f| f.1).collect());
        let check = |editor: &UrlsEditor, state: &Value, context: &str| {
            let labels: Vec<String> = editor.rows().into_iter().map(|(_, l)| l).collect();
            assert_eq!(labels, strings(&state["rows"]), "{context}");
            assert_eq!(editor.warning(), state["warning"], "{context}");
            assert_eq!(editor.input, state["input"], "{context}");
            let updates: Vec<Value> = editor
                .updates()
                .iter()
                .map(|UrlUpdate { add, url, files }| {
                    let mut ids: Vec<&str> = files.iter().map(|&i| hashes[i].as_str()).collect();
                    ids.sort_unstable();
                    json!([if *add { "add" } else { "delete" }, url, ids])
                })
                .collect();
            assert_eq!(json!(updates), state["updates"], "{context}");
        };
        let steps = case["steps"].as_array().unwrap();
        check(&editor, &steps[0]["state"], &format!("case {c} start"));
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("case {c} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let mut asked = Vec::new();
            let answer = action.get(2).and_then(Value::as_bool).unwrap_or(true);
            let mut force = |q: &str| {
                asked.push(json!({ "asked": q }));
                answer
            };
            let mut copied = Vec::new();
            match action[0].as_str().unwrap() {
                "add" => {
                    editor.input = action[1].as_str().unwrap().to_owned();
                    assert!(!editor.enter_input(&normalise, &mut force));
                }
                "paste" => {
                    editor.paste(action[1].as_str().unwrap(), &normalise, &mut force);
                }
                "select" => editor.select(&strings(&action[1])),
                "delete" => editor.delete_selected(),
                "double" => editor.double_click(action[1].as_str().unwrap()),
                "copy" => copied.push(json!(editor.copy_text())),
                "ok" => {
                    editor.input = action[1].as_str().unwrap().to_owned();
                    let ok = match editor.ok_question() {
                        Some(q) => force(q),
                        None => true,
                    };
                    asked.push(json!({ "ok": ok }));
                }
                other => panic!("{other}"),
            }
            assert_eq!(json!(asked), step["said"], "{context}");
            assert_eq!(json!(copied), step["copied"], "{context}");
            check(&editor, &step["state"], &context);
        }
    }
}
