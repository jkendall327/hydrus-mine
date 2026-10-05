//! Shortcuts that apply tags and ratings: their words and what they do
//! (`ApplyContentApplicationCommandToMedia`), on the basic client.
use hydrus_core::ServiceKey;
use hydrus_core::service::builtin_keys;
use hydrus_core::shortcuts::ContentCommand;
use hydrus_gui_model::shortcut_content::{apply, text};
use hydrus_store::Store;
use hydrus_store::services::ServiceKind;

fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn a_tag_is_added_then_flipped_off() {
    let (_dir, store) = store();
    let files = hydrus_gui_model::file_maintenance_new::find(
        &store,
        hydrus_gui_model::file_maintenance_new::Pick::AllMedia,
        &[],
    )
    .unwrap();
    let files = &files[..2];
    let my_tags = ServiceKey::new(builtin_keys::MY_TAGS.to_vec());
    let flip = ContentCommand::Tag {
        service: my_tags.clone(),
        tag: "shortcut test".into(),
        flip: true,
    };
    assert_eq!(
        text(&store.snapshot().services, &flip),
        "flip on/off tag mappings \"shortcut test\" for my tags"
    );
    assert!(apply(&store, files, &flip).unwrap());
    // both have it now, so flipping takes it away; setting does nothing
    let set = ContentCommand::Tag {
        service: my_tags,
        tag: "shortcut test".into(),
        flip: false,
    };
    assert!(!apply(&store, files, &set).unwrap());
    assert!(apply(&store, files, &flip).unwrap());
    assert!(apply(&store, files, &set).unwrap());
}

#[test]
fn ratings_set_flip_and_step() {
    let (_dir, store) = store();
    let files = hydrus_gui_model::file_maintenance_new::find(
        &store,
        hydrus_gui_model::file_maintenance_new::Pick::AllMedia,
        &[],
    )
    .unwrap();
    let file = &files[..1];
    let snapshot = store.snapshot();
    let numerical = snapshot
        .services
        .all()
        .find(|s| matches!(s.kind, ServiceKind::RatingNumerical(_)));
    let Some(numerical) = numerical else {
        // (the basic client may have no numerical rating service)
        return;
    };
    let key = numerical.key.clone();
    let set = ContentCommand::Rating {
        service: key.clone(),
        stars: Some(2),
        flip: true,
    };
    assert!(text(&store.snapshot().services, &set).starts_with("flip on/off ratings 2/"));
    assert!(apply(&store, file, &set).unwrap());
    assert!(apply(&store, file, &set).unwrap()); // cleared
    let up = ContentCommand::Step {
        service: key,
        up: true,
    };
    assert!(text(&store.snapshot().services, &up).starts_with("increment ratings for "));
    assert!(apply(&store, file, &up).unwrap()); // none: to the lowest
}

#[test]
fn the_editor_builds_and_reads_back_commands() {
    use hydrus_gui_model::shortcut_content::{
        ContentService, ValueKind, actions, choices, command,
    };
    let tags = ContentService {
        key: ServiceKey::new(builtin_keys::MY_TAGS.to_vec()),
        name: "my tags".into(),
        value: ValueKind::Tag,
    };
    let stars = ContentService {
        key: ServiceKey::new(b"stars".to_vec()),
        name: "stars".into(),
        value: ValueKind::Stars { min: 0, max: 5 },
    };
    assert_eq!(actions(&tags.value), ["flip on/off", "set"]);
    let made = command(&tags, "flip on/off", "Character:Samus").unwrap();
    assert_eq!(
        made,
        ContentCommand::Tag {
            service: tags.key.clone(),
            tag: "character:samus".into(),
            flip: true,
        }
    );
    let services = [tags.clone(), stars.clone()];
    assert_eq!(
        choices(&services, &made),
        Some((0, "flip on/off", "character:samus".into()))
    );
    assert!(command(&stars, "set", "6").is_err());
    let three = command(&stars, "set", "3").unwrap();
    assert_eq!(choices(&services, &three), Some((1, "set", "3".into())));
    assert_eq!(
        command(&stars, "increment", "").unwrap(),
        ContentCommand::Step {
            service: stars.key,
            up: true
        }
    );
}
