//! The external programs import options' entries as the reference serialises,
//! words and fires them (`oracle/fixtures/import_external_programs.json`,
//! `record_import_external_programs.py`).
use hydrus_core::import_options::{
    ExternalProgramEntry, ExternalProgramsOptions, ImportOptionsSlice,
};
use hydrus_downloader_exchange::import_options;

fn fixture() -> serde_json::Value {
    hydrus_testkit::fixture_json("import_external_programs.json")
}

#[test]
fn the_references_serialised_options_decode_summarise_and_encode_unchanged() {
    let fixture = fixture();
    // each step of the recorded list editor's value, as the reference serialised it
    for event in fixture["add_events"].as_array().unwrap() {
        let Some(value) = event.get("value") else {
            continue;
        };
        let decoded = ExternalProgramsOptions::from_serialised(&value[2].to_string()).unwrap();
        assert_eq!(
            decoded.entries.len(),
            event["list"].as_array().unwrap().len(),
            "{}",
            event["label"]
        );
        for (entry, text) in decoded
            .entries
            .iter()
            .zip(event["list"].as_array().unwrap())
        {
            assert_eq!(
                entry.summary(),
                text.as_str().unwrap(),
                "{}",
                event["label"]
            );
        }
        let again: serde_json::Value = serde_json::from_str(&decoded.serialised()).unwrap();
        assert_eq!(again, value[2], "{}", event["label"]);
        // inside the slice the clipboard and PNG formats carry
        let slice = ImportOptionsSlice {
            external_programs: Some(decoded.clone()),
            ..ImportOptionsSlice::default()
        };
        let text = import_options::encode_text(&slice).unwrap();
        assert_eq!(import_options::decode_text(&text).unwrap(), slice);
    }
    // the list's summary: the entries', sorted, joined
    let last = fixture["add_events"].as_array().unwrap().last().unwrap()["value"][2].to_string();
    let options = ExternalProgramsOptions::from_serialised(&last).unwrap();
    assert_eq!(options.summary(), fixture["summary"].as_str().unwrap());
    assert_eq!(ExternalProgramsOptions::default().summary(), "");
    assert_eq!(fixture["empty_summary"], "");
}

#[test]
fn an_entry_fires_on_the_imports_the_reference_says() {
    let fixture = fixture();
    for (flags, statuses) in fixture["should_fire"].as_object().unwrap() {
        let (new, already) = match flags.as_str() {
            "True,True" => (true, true),
            "True,False" => (true, false),
            "False,True" => (false, true),
            _ => (false, false),
        };
        let entry = ExternalProgramEntry {
            on_new: new,
            on_already_in: already,
            ..ExternalProgramEntry::default()
        };
        // (1 is successful and new, 2 successful but already in the client)
        assert_eq!(statuses["1"], entry.should_fire(false), "{flags}");
        assert_eq!(statuses["2"], entry.should_fire(true), "{flags}");
        for other in ["0", "3", "4", "7", "8", "9"] {
            assert_eq!(statuses[other], false, "{flags} on status {other}");
        }
    }
}

#[test]
fn a_blank_entry_and_the_old_opaque_form_are_the_references() {
    let fixture = fixture();
    let blank = &fixture["default_entry"][2];
    let entry = ExternalProgramEntry::default();
    assert_eq!(blank[0][2][0], entry.key);
    assert_eq!(blank[0][2][1], entry.name);
    assert_eq!(blank[1], entry.on_new);
    assert_eq!(blank[2], entry.on_already_in);
    // settings saved while the entries were only carried through
    let old = format!(
        r#"{{"stored":{}}}"#,
        serde_json::to_string(&fixture["add_events"][2]["value"][2].to_string()).unwrap()
    );
    let migrated: ExternalProgramsOptions = serde_json::from_str(&old).unwrap();
    assert_eq!(migrated.entries.len(), 1);
    assert_eq!(migrated.entries[0].name, "Alpha 日本 log");
    let saved = serde_json::to_string(&migrated).unwrap();
    assert!(!saved.contains("stored"), "{saved}");
    assert_eq!(
        serde_json::from_str::<ExternalProgramsOptions>(&saved).unwrap(),
        migrated
    );
}
