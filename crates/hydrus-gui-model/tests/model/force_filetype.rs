//! The "force filetypes" dialog against the reference's
//! (`oracle/fixtures/force_filetype.json`, from
//! `oracle/record_force_filetype.py`): its text and its choices, in order,
//! for files of one filetype and several, some or all forced.

use std::collections::BTreeMap;

use hydrus_core::Mime;
use hydrus_gui_model::force_filetype::ForceFiletype;
use serde_json::{Value, json};

fn counts(value: &Value) -> BTreeMap<Mime, usize> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, count)| {
            let mime = Mime::ALL
                .iter()
                .copied()
                .find(|m| m.human_name() == name)
                .unwrap_or_else(|| panic!("{name}"));
            (mime, usize::try_from(count.as_u64().unwrap()).unwrap())
        })
        .collect()
}

#[test]
fn the_force_filetypes_dialog_is_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("force_filetype.json");
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dialog = ForceFiletype::new(&counts(&case["original"]), &counts(&case["forced"]));
        assert_eq!(json!([dialog.text]), case["text"], "{name}");
        let choices: Vec<Value> = dialog
            .choices
            .iter()
            .map(|(label, mime)| json!([label, mime.map(Mime::human_name)]))
            .collect();
        assert_eq!(json!(choices), case["choices"], "{name}");
        // (the first chosen)
        assert_eq!(
            json!(dialog.choices[0].1.map(Mime::human_name)),
            case["value"],
            "{name}"
        );
    }
}
