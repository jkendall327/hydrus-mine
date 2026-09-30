//! Sidecar routing against the reference: `oracle/fixtures/sidecars.json`
//! holds random routers between `.txt` and `.json` sidecars (as the
//! reference serialises them), the sidecars beside a file, and the folder
//! after the reference's `Router.Work`. Each router is decoded here, run by
//! `hydrus_parse::sidecar::work` on the same files, and the folder compared.

use std::collections::BTreeMap;

use hydrus_legacy::objects::sidecars::router;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::sidecar::{self, Exporter, MediaMetadata, SidecarError, Source};

struct NoMedia;

impl MediaMetadata for NoMedia {
    fn import(&mut self, _: &Source) -> Result<Vec<String>, SidecarError> {
        unreachable!("the cases route between sidecars only")
    }
    fn export(&mut self, _: &Exporter, _: &[String]) -> Result<(), SidecarError> {
        unreachable!("the cases route between sidecars only")
    }
}

fn folder(dir: &std::path::Path, except: &str) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|name| name != except)
        .map(|name| {
            let text = String::from_utf8(std::fs::read(dir.join(&name)).unwrap()).unwrap();
            (name, text)
        })
        .collect()
}

#[test]
fn routers_work_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("sidecars.json");
    let cases = recorded["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["router"].to_string()).unwrap();
        let router = router(&object).unwrap();
        // no dots in the folder: "removing the extension" of a file without
        // one cuts at the last dot in the whole path, as in the reference
        let dir = tempfile::Builder::new()
            .prefix("sidecars")
            .tempdir()
            .unwrap();
        let file_name = case["file"].as_str().unwrap();
        let file_path = dir.path().join(file_name);
        std::fs::write(&file_path, b"file").unwrap();
        for (name, text) in case["before"].as_object().unwrap() {
            std::fs::write(dir.path().join(name), text.as_str().unwrap()).unwrap();
        }
        let file_path = file_path.to_str().unwrap();

        let possible: Vec<String> = router
            .possible_sidecar_paths(file_path)
            .iter()
            .map(|p| p.strip_prefix(dir.path().to_str().unwrap()).unwrap()[1..].to_owned())
            .collect();
        let mut sorted = possible.clone();
        sorted.sort();
        let expected_possible: Vec<String> =
            serde_json::from_value(case["possible"].clone()).unwrap();
        if sorted != expected_possible {
            failures.push(format!(
                "case {i}: possible sidecars {sorted:?}, reference {expected_possible:?}"
            ));
        }

        let result = sidecar::work(&router, file_path, &mut NoMedia);
        match (&result, case.get("worked"), case.get("error")) {
            (Ok(worked), Some(expected), None) if expected.as_bool() == Some(*worked) => {}
            (Err(_), None, Some(_)) => {}
            _ => failures.push(format!(
                "case {i}: gave {result:?}, reference worked {:?} error {:?}\n    router {}",
                case.get("worked"),
                case.get("error"),
                case["router"]
            )),
        }
        let after = folder(dir.path(), file_name);
        let expected: BTreeMap<String, String> =
            serde_json::from_value(case["after"].clone()).unwrap();
        if after != expected {
            failures.push(format!(
                "case {i}: folder {after:?}\n    reference {expected:?}\n    before {}\n    router {}",
                case["before"], case["router"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} problems in {} cases; first:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
