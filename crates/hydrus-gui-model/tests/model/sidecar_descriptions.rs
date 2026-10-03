//! Sidecar routers described as the reference describes them
//! (`oracle/fixtures/sidecar_descriptions.json`, from
//! `oracle/dump_sidecar_descriptions.py`): random routers from every kind
//! of importer and exporter, and the sidecars button's label for lists of
//! them.

use std::collections::HashMap;

use hydrus_gui_model::sidecars::{button_label, router_text};
use hydrus_legacy::objects::sidecars::router;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::sidecar::Router;

fn decode(value: &serde_json::Value) -> Router {
    router(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

#[test]
fn routers_are_described_as_the_reference_describes_them() {
    let recorded = hydrus_testkit::fixture_json("sidecar_descriptions.json");
    let names: HashMap<String, String> = recorded["names"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    let namer = |key: &str| names.get(key).cloned();
    for case in recorded["cases"].as_array().unwrap() {
        let router = decode(&case["router"]);
        assert_eq!(
            router_text(&router, true, &namer),
            case["pretty"],
            "{router:?}"
        );
        assert_eq!(router_text(&router, false, &namer), case["plain"]);
    }
    for case in recorded["buttons"].as_array().unwrap() {
        let routers: Vec<Router> = case["routers"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect();
        assert_eq!(button_label(&routers, &namer).0, case["label"]);
    }
}
