//! Actual simple-panel entry/paste and heterogeneous selected-files semantics.
use hydrus_core::{ServiceKey, search::context::LocationContext, tag::clean_tag_checked};
use hydrus_gui_model::{
    filename_tagging::ServiceTagging,
    write_autocomplete::{TagEntry, WriteAutocomplete},
};
use hydrus_store::Store;
use serde_json::{Value, json};

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn additive_entry_and_selected_union_replay_real_simple_panel_without_spreading_untouched_tags() {
    let fixture = hydrus_testkit::fixture_json("filename_simple.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let service: ServiceKey = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let mut tagging = ServiceTagging::default();
    let mut selected = Vec::new();
    let paths = strings(&fixture["paths"]);
    for step in fixture["steps"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        match action {
            "select" => {
                selected = step["value"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
                    .collect()
            }
            "enter_all"
            | "enter_single"
            | "autocomplete_paste_single"
            | "remove_all"
            | "remove_single" => {
                let all = action.ends_with("all");
                let before: Vec<_> = if all {
                    tagging.options.tags_for_all.iter().cloned().collect()
                } else {
                    tagging.selected_single(&selected)
                };
                let mut editor = TagEntry::new(
                    WriteAutocomplete::new(
                        store.clone(),
                        service.clone(),
                        LocationContext::default(),
                    ),
                    &before,
                )
                .additions_only();
                // An unchanged child Apply preserves each file's original tags.
                let original = tagging.clone();
                tagging.apply_selected(&selected, &before, &before, &[]);
                assert_eq!(tagging, original);
                let input = strings(&step["value"]);
                if action.starts_with("enter") {
                    for tag in &input {
                        editor.input.set_text(tag);
                        editor.enter(None);
                    }
                } else if action.starts_with("autocomplete") {
                    editor.paste(&input);
                } else {
                    for tag in &input {
                        let index = editor.tags().iter().position(|t| t == tag).unwrap();
                        editor.remove(index);
                    }
                }
                if all {
                    tagging.options.tags_for_all = editor.tags().into_iter().collect();
                } else {
                    tagging.apply_selected(&selected, &before, &editor.tags(), &editor.additions());
                }
            }
            "paste_all" | "paste_single" => {
                let tags: Vec<_> = step["value"]
                    .as_str()
                    .unwrap()
                    .lines()
                    .filter_map(clean_tag_checked)
                    .collect();
                if action == "paste_all" {
                    tagging.options.tags_for_all.extend(tags);
                } else {
                    tagging.add_single(&selected, &tags);
                }
            }
            "filename_text" => {
                tagging.options.add_filename = Some(step["value"].as_str().unwrap().into())
            }
            "filename_check" => {
                if !step["value"].as_bool().unwrap() {
                    tagging.options.add_filename = None;
                }
            }
            "directory_text" => {
                let index = step["value"][0].as_i64().unwrap();
                let namespace = step["value"][1].as_str().unwrap().to_owned();
                tagging.options.directories.retain(|(i, _)| *i != index);
                tagging.options.directories.push((index, namespace));
            }
            "directory_check" => {
                if !step["value"][1].as_bool().unwrap() {
                    tagging
                        .options
                        .directories
                        .retain(|(i, _)| *i != step["value"][0].as_i64().unwrap());
                }
            }
            "clipboard_missing" => {
                assert!(step["error"].as_str().unwrap().starts_with("TypeError:"));
            }
            _ => panic!("unknown reference action {action}"),
        }
        assert_eq!(
            json!(tagging.options.tags_for_all),
            step["state"]["all"],
            "{action}"
        );
        assert_eq!(
            json!(tagging.selected_single(&selected)),
            step["state"]["single"],
            "{action}"
        );
        let native_paths: Vec<_> = paths
            .iter()
            .map(|path| {
                if cfg!(windows) {
                    path.replace('/', "\\")
                } else {
                    path.clone()
                }
            })
            .collect();
        let tags: Vec<_> = native_paths
            .iter()
            .enumerate()
            .map(|(i, path)| tagging.tags(i, path))
            .collect();
        assert_eq!(json!(tags), step["state"]["tags"], "{action}");
    }
    assert_eq!(
        json!(tagging.options.tags_for_all),
        fixture["reopened"]["all"]
    );
}
