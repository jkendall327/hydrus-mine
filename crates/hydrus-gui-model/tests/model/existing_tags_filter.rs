//! The captured per-service filter's staged acceptance and saved importer options.
use hydrus_core::import_options::{
    CallerType, ImportOptionsManager, ImportOptionsSlice, ServiceTagImportOptions, TagImportOptions,
};
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::import_options_editor::{Editor, Kind, TagFilterTarget};

#[test]
fn existing_filter_acceptance_enables_only_the_captured_service_and_round_trips() {
    let fixture = hydrus_testkit::fixture_json("existing_tags_filter.json");
    let first = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let second = hex::encode(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS);
    let mut parsed = TagFilter::new();
    parsed.set_rule("creator:blocked", FilterRule::Blacklist);
    let own = ImportOptionsSlice {
        tags: Some(TagImportOptions {
            services: vec![
                (
                    first.clone(),
                    ServiceTagImportOptions {
                        get_tags: true,
                        get_tags_filter: parsed.clone(),
                        additional_tags: vec!["series:added".into()],
                        ..Default::default()
                    },
                ),
                (
                    second.clone(),
                    ServiceTagImportOptions {
                        get_tags: true,
                        ..Default::default()
                    },
                ),
            ],
        }),
        ..Default::default()
    };
    let mut editor = Editor::new(
        &ImportOptionsManager::default(),
        CallerType::PostUrls,
        false,
        &own,
    );
    let target = TagFilterTarget::ExistingTags(first.clone());
    let captured = editor.tag_filter(&target).unwrap();
    assert_eq!(captured, TagFilter::new());
    // A cancelled detached draft does not accept its edited value.
    let mut child = captured;
    for rule in fixture["saved"]["rules"].as_array().unwrap() {
        child.set_rule(
            rule[0].as_str().unwrap(),
            if rule[1] == 0 {
                FilterRule::Whitelist
            } else {
                FilterRule::Blacklist
            },
        );
    }
    assert_eq!(editor.value(), own);
    editor.set_tag_filter(&target, child.clone());
    let accepted = editor.value();
    let service = accepted.tags.as_ref().unwrap().service(&first).unwrap();
    assert!(service.only_add_existing_tags);
    assert_eq!(service.only_add_existing_tags_filter, child);
    assert_eq!(service.get_tags_filter, parsed);
    assert_eq!(service.additional_tags, ["series:added"]);
    assert_eq!(
        accepted.tags.as_ref().unwrap().service(&second),
        own.tags.as_ref().unwrap().service(&second)
    );
    let json = hydrus_downloader_exchange::import_options::encode_text(&accepted).unwrap();
    assert_eq!(
        hydrus_downloader_exchange::import_options::decode_text(&json).unwrap(),
        accepted
    );
    // Switching back to defaults retires the captured field; a late result
    // cannot recreate custom options or change the remembered value.
    editor.set_custom(Kind::Tags, false);
    editor.set_tag_filter(&target, TagFilter::new());
    assert!(editor.value().tags.is_none());
    editor.set_custom(Kind::Tags, true);
    assert_eq!(editor.value(), accepted);
}
