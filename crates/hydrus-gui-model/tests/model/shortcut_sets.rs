//! Options > shortcuts' set lists against `oracle/dump_shortcut_sets.py`:
//! the built-in sets' names, order and descriptions, the default sets a new
//! client has, command and shortcut texts, and naming custom sets.
use std::collections::BTreeSet;

use hydrus_core::shortcuts::{Gesture, Settings, default_sets};
use hydrus_gui_model::shortcut_sets::{
    RESERVED, Restore, SORTED, command_text, custom_rows, delete_custom, description,
    non_dupe_name, pretty_name, reserved_rows, restore, restore_question, save_custom,
};
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn names_order_and_descriptions_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("shortcut_sets.json");
    assert_eq!(strings(&fixture["reserved"]), RESERVED);
    assert_eq!(strings(&fixture["sorted"]), SORTED);
    for name in RESERVED {
        assert_eq!(
            pretty_name(name),
            fixture["pretty_names"][name].as_str().unwrap()
        );
        assert_eq!(description(name), fixture["descriptions"][name].as_str());
    }
    assert_eq!(pretty_name("rating keys"), "rating keys");
    for case in fixture["non_dupe"].as_array().unwrap() {
        let existing: BTreeSet<String> = strings(&case[1]).into_iter().collect();
        assert_eq!(
            non_dupe_name(case[0].as_str().unwrap(), &existing),
            case[2].as_str().unwrap()
        );
    }
}

#[test]
fn a_new_client_has_the_reference_s_default_sets() {
    let fixture = hydrus_testkit::fixture_json("shortcut_sets.json");
    let defaults = default_sets();
    let recorded = fixture["defaults"].as_array().unwrap();
    assert_eq!(defaults.len(), recorded.len());
    for set in recorded {
        let name = set["name"].as_str().unwrap();
        let bindings = &defaults[name];
        let expected = set["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), expected.len(), "{name}");
        for (binding, b) in bindings.iter().zip(expected) {
            let modifiers: Vec<u8> = b["modifiers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| u8::try_from(m.as_u64().unwrap()).unwrap())
                .collect();
            let gesture = Gesture {
                kind: u8::try_from(b["kind"].as_u64().unwrap()).unwrap(),
                key: u32::try_from(b["key"].as_u64().unwrap()).unwrap(),
                press: u8::try_from(b["press"].as_u64().unwrap()).unwrap(),
                modifiers,
            };
            assert_eq!(binding.gesture, gesture, "{name}");
            let recorded_text = b["shortcut_text"].as_str().unwrap();
            // The fixture records Linux display names. Python renders the same
            // modifier identities as command/option on macOS.
            let expected_text = if cfg!(target_os = "macos") {
                recorded_text
                    .split('+')
                    .map(|part| match part {
                        "ctrl" => "command",
                        "alt" => "option",
                        other => other,
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            } else {
                recorded_text.to_owned()
            };
            assert_eq!(binding.gesture.text(false), expected_text, "{name}");
            assert_eq!(command_text(binding), b["text"].as_str().unwrap(), "{name}");
        }
    }
    let settings = Settings::default();
    let rows = reserved_rows(&settings);
    assert_eq!(rows.len(), 11);
    assert_eq!(rows[0].cells(), ["global".to_owned(), "1".to_owned()]);
    assert!(custom_rows(&settings).is_empty());
}

#[test]
fn custom_sets_are_named_apart_deleted_and_defaults_restored() {
    let mut settings = Settings::default();
    assert_eq!(
        save_custom(&mut settings, None, "new shortcuts", Vec::new()),
        "new shortcuts"
    );
    assert_eq!(
        save_custom(&mut settings, None, "new shortcuts", Vec::new()),
        "new shortcuts (1)"
    );
    // renaming keeps its own name free
    assert_eq!(
        save_custom(
            &mut settings,
            Some("new shortcuts (1)"),
            "new shortcuts (1)",
            Vec::new()
        ),
        "new shortcuts (1)"
    );
    // a custom set can't take a built-in name
    assert_eq!(
        save_custom(&mut settings, None, "main_gui", Vec::new()),
        "main_gui (1)"
    );
    let names: Vec<String> = custom_rows(&settings).into_iter().map(|r| r.name).collect();
    assert_eq!(
        names,
        ["main_gui (1)", "new shortcuts", "new shortcuts (1)"]
    );
    delete_custom(&mut settings, &["new shortcuts".into(), "main_gui".into()]);
    assert!(settings.sets.contains_key("main_gui"));
    assert_eq!(custom_rows(&settings).len(), 2);

    settings.sets.get_mut("thumbnails").unwrap().clear();
    assert!(
        matches!(restore_question(&settings, "thumbnails"), Restore::Replace(q) if q == "Are you certain you want to restore the defaults for \"thumbnails\"? Any custom shortcuts you have set will be wiped.")
    );
    restore(&mut settings, "thumbnails");
    assert_eq!(settings.sets["thumbnails"].len(), 25);
    settings.sets.remove("global");
    assert!(
        matches!(restore_question(&settings, "global"), Restore::Missing(m) if m == "It looks like your client was missing the \"global\" shortcut set! It will now be restored.")
    );
    restore(&mut settings, "global");
    assert_eq!(settings.sets["global"].len(), 1);
}
