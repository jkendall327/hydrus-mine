//! Real Qt overwrite presets, labels and favourite collision behavior.
use hydrus_core::import_options::{CallerType, ImportOptionsManager};
use hydrus_downloader_exchange::import_options;
use hydrus_gui_model::import_options_overwrite::{Overwrite, Preset, save_favourite};
use serde_json::json;

#[test]
fn shared_overwrite_presets_and_manual_default_match_real_dialog() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    let incoming = import_options::decode_text(&fixture["incoming"].to_string()).unwrap();
    let mut draft = Overwrite::new(CallerType::SpecificImporter, true, existing, incoming);
    for step in fixture["overwrite"].as_array().unwrap() {
        match step["action"].as_str().unwrap() {
            "merge" => draft.preset(Preset::Merge),
            "fill in" => draft.preset(Preset::FillIn),
            "replace" => draft.preset(Preset::Replace),
            "manual clear notes" => {
                for i in 0..draft.kinds.len() {
                    draft.tick(false, i, draft.kinds[i].code() == 0);
                    draft.tick(true, i, draft.kinds[i].code() == 5);
                }
            }
            "initial" => {}
            action => panic!("unknown fixture action {action}"),
        }
        let checked = |pasted: bool| {
            draft
                .kinds
                .iter()
                .enumerate()
                .filter_map(|(i, kind)| {
                    (if pasted {
                        draft.take_pasted[i]
                    } else {
                        draft.keep_current[i]
                    })
                    .then_some(kind.code())
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(json!(checked(false)), step["current"]);
        assert_eq!(json!(checked(true)), step["pasted"]);
        let result = draft.value();
        assert_eq!(import_options::tuple(&result).unwrap(), step["options"]);
        for (field, value, incoming) in [
            ("left_labels", &draft.current, false),
            ("pasted_labels", &draft.pasted, true),
            ("result_labels", &result, false),
        ] {
            assert_eq!(
                json!(
                    draft
                        .kinds
                        .iter()
                        .map(|kind| draft.label(value, *kind, incoming, &str::to_owned))
                        .collect::<Vec<_>>()
                ),
                step[field]
            );
        }
    }
}

#[test]
fn favourite_add_rename_and_delete_preserve_reference_names_and_other_defaults() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let mut manager = ImportOptionsManager::default();
    manager.favourites.clear();
    let defaults = manager.caller_defaults.clone();
    for (index, step) in fixture["favourites"].as_array().unwrap().iter().enumerate() {
        match step["action"].as_str().unwrap() {
            "add" | "edit" => {
                let value = match index {
                    0 => &fixture["existing"],
                    1 => &fixture["incoming"],
                    2 => &fixture["tuples"][1],
                    _ => &fixture["tuples"][0],
                };
                let value = import_options::decode_text(&value.to_string()).unwrap();
                let actual = save_favourite(
                    &mut manager,
                    step["original"].as_str(),
                    step["requested"].as_str().unwrap(),
                    value,
                );
                assert_eq!(actual, step["actual"]);
            }
            "delete" => manager
                .favourites
                .retain(|(name, _)| name != step["name"].as_str().unwrap()),
            action => panic!("unknown fixture action {action}"),
        }
    }
    assert_eq!(manager.caller_defaults, defaults);
    for row in fixture["favourite_rows"].as_array().unwrap() {
        let value = manager
            .favourites
            .iter()
            .find(|(name, _)| name == row["name"].as_str().unwrap())
            .unwrap();
        assert_eq!(import_options::tuple(&value.1).unwrap(), row["options"]);
    }
}
