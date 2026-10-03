//! What the "filename tagging" dialog's "sidecars" tab shows for each file
//! (`oracle/fixtures/sidecar_previews.json`, from
//! `oracle/dump_sidecar_previews.py`): random .txt and .json sidecars for
//! 60 files, read by random routers to a file's tags, notes, URLs and
//! timestamps, each file's strings as the reference's
//! `_GetPrettyStrings` gives them.

use std::collections::HashMap;

use hydrus_gui_model::sidecars::file_preview;
use hydrus_legacy::objects::sidecars::router;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::sidecar::Router;

fn decode(value: &serde_json::Value) -> Router {
    router(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

#[test]
fn each_files_sidecars_are_shown_as_the_reference_shows_them() {
    let recorded = hydrus_testkit::fixture_json("sidecar_previews.json");
    let names: HashMap<String, String> = recorded["names"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    let namer = |key: &str| names.get(key).cloned();
    let dir = tempfile::tempdir().unwrap();
    for file in recorded["files"].as_array().unwrap() {
        for (name, text) in file["sidecars"].as_object().unwrap() {
            std::fs::write(dir.path().join(name), text.as_str().unwrap()).unwrap();
        }
    }
    let mut checked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let routers: Vec<Router> = case["routers"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect();
        for (name, shown) in case["shown"].as_object().unwrap() {
            let path = dir.path().join(name).to_string_lossy().into_owned();
            assert_eq!(
                serde_json::json!(file_preview(&routers, &path, &namer)),
                *shown,
                "{name} with {routers:?}"
            );
            checked += usize::from(shown != &serde_json::json!([]));
        }
    }
    assert!(checked > 1000, "{checked}");
}
