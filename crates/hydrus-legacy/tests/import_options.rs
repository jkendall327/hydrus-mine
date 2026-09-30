//! Import option defaults and their layering, against
//! `oracle/fixtures/import_options.json` (made by
//! `oracle/dump_import_options.py`).

use hydrus_core::import_options::{
    CallerType, FullImportOptions, ImportOptionsManager, ImportOptionsSlice, UrlClassKind,
};
use hydrus_legacy::objects::import_options::{manager, slice};
use hydrus_legacy::serialisable::SerialisableObject;

fn decode_slice(value: &serde_json::Value) -> ImportOptionsSlice {
    slice(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

fn full(slice: ImportOptionsSlice) -> FullImportOptions {
    FullImportOptions {
        prefetch: slice.prefetch.expect("full"),
        file_filtering: slice.file_filtering.expect("full"),
        tag_filtering: slice.tag_filtering.expect("full"),
        locations: slice.locations.expect("full"),
        tags: slice.tags.expect("full"),
        notes: slice.notes.expect("full"),
        presentation: slice.presentation.expect("full"),
    }
}

#[test]
fn the_default_manager_is_ours() {
    let recorded = hydrus_testkit::fixture_json("import_options.json");
    let object =
        SerialisableObject::from_tuple_str(&recorded["default_manager"].to_string()).unwrap();
    let decoded = manager(&object).unwrap();
    let ours = ImportOptionsManager::default();
    for (caller, slice) in &ours.caller_defaults {
        assert_eq!(decoded.caller_default(*caller), Some(slice), "{caller:?}");
    }
    assert_eq!(decoded.caller_defaults.len(), ours.caller_defaults.len());
    assert_eq!(decoded.favourites, ours.favourites);
    assert!(decoded.url_class_defaults.is_empty());
}

#[test]
fn managers_layer_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("import_options.json");
    let mut failures = Vec::new();
    for (i, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["manager"].to_string()).unwrap();
        let m = manager(&object).unwrap();
        for (j, lookup) in case["lookups"].as_array().unwrap().iter().enumerate() {
            let caller = CallerType::from_code(lookup["caller"].as_i64().unwrap()).unwrap();
            let specific = decode_slice(&lookup["specific"]);
            let classes: Vec<(String, UrlClassKind)> = lookup["url_class_keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| (k.as_str().unwrap().to_owned(), UrlClassKind::Other))
                .collect();
            let expected = full(decode_slice(&lookup["full"]));
            let got = m.full(caller, Some(&specific), &classes);
            if got != expected {
                let mut fields = Vec::new();
                macro_rules! compare {
                    ($($field:ident),*) => {$(
                        if got.$field != expected.$field {
                            fields.push(format!(
                                "{}: expected {:?} got {:?}",
                                stringify!($field),
                                expected.$field,
                                got.$field
                            ));
                        }
                    )*};
                }
                compare!(
                    prefetch,
                    file_filtering,
                    tag_filtering,
                    locations,
                    tags,
                    notes,
                    presentation
                );
                failures.push(format!(
                    "case {i} lookup {j} ({caller:?}): {}",
                    fields.join("; ")
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
