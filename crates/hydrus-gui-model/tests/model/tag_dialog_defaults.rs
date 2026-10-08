//! Real manage-tags service preferences survive cancellation independently of tag drafts.
use std::sync::Arc;

use hydrus_core::{HashId, ServiceKey};
use hydrus_gui_model::manage_tags::ManageTags;
use hydrus_store::{Store, settings, tag_editing::TagEditingSettings};
use serde_json::{Value, json};

fn key(store: &Store, name: &str) -> ServiceKey {
    store.snapshot().services.by_name(name).unwrap().key.clone()
}
fn preferences(store: &Store) -> Value {
    let p: TagEditingSettings = store.read(settings::get).unwrap();
    let s = store.snapshot();
    let name = s
        .services
        .by_key(&p.default_service)
        .map_or("missing service", |s| s.name.as_str());
    json!({"remember":p.remember_service,"service":name})
}
fn set(store: &Store, remember: bool, name: &str) {
    let default_service = if name == "missing service" {
        ServiceKey::new(b"missing tag service".to_vec())
    } else {
        key(store, name)
    };
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &TagEditingSettings {
                    remember_service: remember,
                    default_service,
                    ..TagEditingSettings::default()
                },
            )
        })
        .unwrap();
}

// leaf: audit-options-tag-editing-tag-dialogs-default-tag-service-in-tag-dialogs
// leaf: audit-options-tag-editing-tag-dialogs-remember-last-used-default-tag-service-in-manage-tag-dialogs
#[test]
fn tag_service_defaults_and_immediate_memory_replay_reference_dialogs() {
    let f = hydrus_testkit::fixture_json("tag_dialog_defaults.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    assert_eq!(preferences(&store), f["initial_preferences"]);
    let files: Vec<HashId> = store
        .read(|c| {
            let mut q = c.prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 1")?;
            Ok(q.query_map([], |r| Ok(HashId(r.get(0)?)))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    set(&store, false, "my tags");
    let mut editor: Option<ManageTags> = None;
    for event in f["events"].as_array().unwrap() {
        let action = event["action"].as_str().unwrap();
        match action {
            "open_fixed" | "reopen_after_fixed_cancel" | "reopen_after_remember_cancel" => {
                editor = Some(ManageTags::new(store.clone(), files.clone()).unwrap());
                assert!(!editor.as_ref().unwrap().has_changes());
                assert!(
                    !editor
                        .as_ref()
                        .unwrap()
                        .rows()
                        .iter()
                        .any(|(t, _)| t == "cancelled preference test")
                );
            }
            "open_remember" => {
                set(&store, true, "my tags");
                editor = Some(ManageTags::new(store.clone(), files.clone()).unwrap());
            }
            "change_fixed" | "change_remember" | "change_after_disable" => {
                if action == "change_after_disable" {
                    // An already-open dialog must honour newly applied options.
                    set(&store, false, "downloader tags");
                }
                let e = editor.as_mut().unwrap();
                let i = e
                    .service_names()
                    .iter()
                    .position(|n| n == event["selected"].as_str().unwrap())
                    .unwrap();
                e.choose_service(i).unwrap();
                e.enter("cancelled preference test").unwrap();
                assert!(e.has_changes());
            }
            "missing_service_fallback" => {
                set(&store, false, "missing service");
                editor = Some(ManageTags::new(store.clone(), files.clone()).unwrap());
            }
            _ => panic!("unknown reference event {action}"),
        }
        let e = editor.as_ref().unwrap();
        assert_eq!(
            *event,
            json!({"action":action,"selected":e.service_names()[e.service()],"preferences":preferences(&store)}),
            "{action}"
        );
    }
    // Invalid callback indexes leave the selected tab and preference intact.
    let e = editor.as_mut().unwrap();
    let selected = e.service();
    let before = preferences(&store);
    e.choose_service(usize::MAX).unwrap();
    assert_eq!(e.service(), selected);
    assert_eq!(preferences(&store), before);
    let old: TagEditingSettings =
        serde_json::from_value(json!({"remember_service":false})).unwrap();
    assert!(!old.remember_service);
    assert_eq!(
        old.default_service,
        TagEditingSettings::default().default_service
    );
}
