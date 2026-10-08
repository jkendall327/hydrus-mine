//! The real Qt clipboard watcher replayed through native URL routing.

use hydrus_core::url::{UrlClassSettings, UrlClasses};
use hydrus_gui_model::clipboard_urls::{Destination, Watcher};
use hydrus_legacy::objects::domain;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_store::settings::ClipboardUrls;
use serde_json::{Value, json};

// leaf: audit-network-clipboard-monitor
#[test]
fn changed_text_switches_and_url_policy_match_the_reference() {
    let fixture = hydrus_testkit::fixture_json("clipboard_urls.json");
    let settings = UrlClassSettings {
        url_classes: fixture["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tuple| {
                domain::url_class(&SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap())
                    .unwrap()
            })
            .collect(),
        parser_keys: vec![fixture["parser_key"].as_str().unwrap().into()],
        parser_links: fixture["linked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| {
                (
                    key.as_str().unwrap().into(),
                    Some(fixture["parser_key"].as_str().unwrap().into()),
                )
            })
            .collect(),
        ..Default::default()
    };
    let classes = UrlClasses::new(settings);
    let mut watcher = Watcher::default();
    for step in fixture["steps"].as_array().unwrap() {
        let flags = ClipboardUrls {
            watchers: step["watchers"].as_bool().unwrap(),
            other_recognised: step["other_recognised"].as_bool().unwrap(),
        };
        if step["reset"] == true {
            watcher.reset();
        }
        let reading = watcher.reading(flags);
        let routed = if reading && step["failure"] == true {
            watcher.failed();
            Vec::new()
        } else if reading {
            watcher.changed(step["text"].as_str(), &classes)
        } else {
            Vec::new()
        };
        let actual: Vec<Value> = routed.iter().map(|url| json!({
            "url": url.url,
            "destination": match url.destination { Destination::Urls => "urls", Destination::Watchers => "watchers" },
        })).collect();
        assert_eq!(json!(actual), step["routed"], "{}", step["name"]);
    }
    assert!(!watcher.reading(ClipboardUrls {
        watchers: false,
        other_recognised: true
    }));
    watcher.reset();
    assert!(watcher.reading(ClipboardUrls {
        watchers: false,
        other_recognised: true
    }));
}
