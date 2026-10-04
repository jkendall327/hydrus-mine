//! The import options editor against the reference's, recorded by
//! `oracle/record_import_options_editor.py`: for importers of each kind,
//! in simple mode and not, the kinds listed and their labels (whose
//! default each uses, or a custom one's summary), each page's text and
//! choices; and each kind's summary for many settings.

use serde_json::Value as Json;

use hydrus_core::import_options::{
    CallerType, FileFilteringOptions, ImportOptionsManager, ImportOptionsSlice, LocationOptions,
    NoteConflict, NoteImportOptions, PrefetchCheck, PrefetchOptions, PresentationInbox,
    PresentationOptions, PresentationStatus, ServiceTagImportOptions, TagFilteringOptions,
    TagImportOptions,
};
use hydrus_core::service::builtin_keys;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::import_options_editor::{
    CUSTOM_CHOICE, DESCRIPTION, Kind, listed_kinds, source_label, summary, tab_label,
    use_default_label,
};

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/import_options_editor.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The `basic` fixture's names for the services the cases use.
fn name(key: &str) -> String {
    let key = hex::decode(key).unwrap();
    match key.as_slice() {
        k if k == builtin_keys::MY_FILES => "art".into(),
        k if k == builtin_keys::DOWNLOADER_TAGS => "downloader tags".into(),
        k if k == builtin_keys::MY_TAGS => "my tags".into(),
        _ => "unknown".into(),
    }
}

fn labels(
    manager: &ImportOptionsManager,
    caller: CallerType,
    simple: bool,
    slice: &ImportOptionsSlice,
) -> Vec<String> {
    listed_kinds(caller, simple, slice)
        .into_iter()
        .map(|kind| {
            let custom = kind.is_set(slice).then(|| summary(kind, slice, &name));
            tab_label(
                kind,
                custom.as_deref(),
                &source_label(manager, caller, kind),
            )
        })
        .collect()
}

#[test]
fn the_editors_lists_are_the_references() {
    let manager = ImportOptionsManager::default();
    let custom = ImportOptionsSlice {
        presentation: Some(PresentationOptions {
            status: PresentationStatus::NewOnly,
            ..PresentationOptions::default()
        }),
        file_filtering: Some(FileFilteringOptions::default()),
        ..ImportOptionsSlice::default()
    };
    for editor in recorded()["editors"].as_array().unwrap() {
        let caller = CallerType::from_code(editor["code"].as_i64().unwrap()).unwrap();
        let simple = editor["simple"].as_bool().unwrap();
        let at = format!("{} simple {simple}", editor["caller"]);
        assert_eq!(editor["description"], DESCRIPTION, "{at}");
        let empty = ImportOptionsSlice::default();
        assert_eq!(
            serde_json::json!(labels(&manager, caller, simple, &empty)),
            editor["tabs"],
            "{at}"
        );
        let kinds = listed_kinds(caller, simple, &empty);
        for (kind, page) in kinds.iter().zip(editor["pages"].as_array().unwrap()) {
            assert_eq!(page["description"], kind.description(), "{at}");
            assert_eq!(
                page["choices"],
                serde_json::json!([
                    use_default_label(&source_label(&manager, caller, *kind)),
                    CUSTOM_CHOICE
                ]),
                "{at}"
            );
        }
        assert_eq!(
            serde_json::json!(labels(&manager, caller, simple, &custom)),
            editor["custom_tabs"],
            "{at}"
        );
        let set: Vec<&str> = Kind::ALL
            .into_iter()
            .filter(|k| k.is_set(&custom))
            .map(Kind::name)
            .collect();
        // (the reference's canonical order puts presentation last too)
        assert_eq!(serde_json::json!(set), editor["custom_value"], "{at}");
    }
}

fn tag_filter(rules: &Json) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in rules.as_array().into_iter().flatten() {
        let r = if rule[1].as_i64() == Some(0) {
            FilterRule::Whitelist
        } else {
            FilterRule::Blacklist
        };
        filter.set_rule(rule[0].as_str().unwrap(), r);
    }
    filter
}

fn check(code: &Json) -> PrefetchCheck {
    match code.as_i64().unwrap() {
        0 => PrefetchCheck::DoNotCheck,
        1 => PrefetchCheck::Check,
        _ => PrefetchCheck::CheckAndMatchesAreDispositive,
    }
}

/// A recorded case's options.
fn options(case: &Json) -> (Kind, ImportOptionsSlice) {
    let b = |key: &str, default: bool| case.get(key).and_then(Json::as_bool).unwrap_or(default);
    let mut slice = ImportOptionsSlice::default();
    let kind = match case["kind"].as_str().unwrap() {
        "prefetch" => {
            let mut o = PrefetchOptions::default();
            if let Some(c) = case.get("hash") {
                o.hash_check = check(c);
            }
            if let Some(c) = case.get("url") {
                o.url_check = check(c);
            }
            o.fetch_metadata_even_if_hash_recognised_and_file_already_in_db =
                b("fetch_hash", false);
            o.fetch_metadata_even_if_url_recognised_and_file_already_in_db = b("fetch_url", false);
            slice.prefetch = Some(o);
            Kind::Prefetch
        }
        "file filtering" => {
            let mut o = FileFilteringOptions {
                exclude_deleted: b("exclude_deleted", true),
                allow_decompression_bombs: b("bombs", true),
                ..FileFilteringOptions::default()
            };
            let size = |key: &str| case.get(key).and_then(Json::as_u64);
            let res = |key: &str| {
                case.get(key).map(|r| {
                    (
                        u32::try_from(r[0].as_u64().unwrap()).unwrap(),
                        u32::try_from(r[1].as_u64().unwrap()).unwrap(),
                    )
                })
            };
            o.min_size = size("min_size");
            o.max_size = size("max_size");
            o.max_gif_size = size("max_gif_size");
            o.min_resolution = res("min_resolution");
            o.max_resolution = res("max_resolution");
            if let Some(types) = case.get("filetypes") {
                o.filetypes = types
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| u8::try_from(t.as_u64().unwrap()).unwrap())
                    .collect();
            }
            slice.file_filtering = Some(o);
            Kind::FileFiltering
        }
        "tag filtering" => {
            slice.tag_filtering = Some(TagFilteringOptions {
                blacklist: tag_filter(&case["blacklist"]),
                whitelist: case["whitelist"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| t.as_str().unwrap().to_owned())
                    .collect(),
            });
            Kind::TagFiltering
        }
        "locations" => {
            slice.locations = Some(LocationOptions {
                automatically_archive: b("archive", false),
                destinations_for_already_in_db: b("destinations_for_already_in_db", false),
                ..LocationOptions::default()
            });
            Kind::Locations
        }
        "tags" => {
            let mut services = Vec::new();
            for (service, s) in case["tags"].as_object().into_iter().flatten() {
                let key = if service == "downloader" {
                    builtin_keys::DOWNLOADER_TAGS
                } else {
                    builtin_keys::MY_TAGS
                };
                services.push((
                    hex::encode(key),
                    ServiceTagImportOptions {
                        get_tags: s.get("get_tags").and_then(Json::as_bool).unwrap_or(false),
                        get_tags_filter: tag_filter(&s["filter"]),
                        additional_tags: s["additional"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|t| t.as_str().unwrap().to_owned())
                            .collect(),
                        ..ServiceTagImportOptions::default()
                    },
                ));
            }
            slice.tags = Some(TagImportOptions { services });
            Kind::Tags
        }
        "notes" => {
            let mut o = NoteImportOptions {
                get_notes: b("get_notes", true),
                extend_existing_note_if_possible: b("extend", true),
                ..NoteImportOptions::default()
            };
            if let Some(c) = case.get("conflict") {
                o.conflict = match c.as_i64().unwrap() {
                    0 => NoteConflict::Replace,
                    1 => NoteConflict::Ignore,
                    2 => NoteConflict::Append,
                    _ => NoteConflict::Rename,
                };
            }
            o.all_name_override = case
                .get("all_name_override")
                .map(|n| n.as_str().unwrap().to_owned());
            o.name_overrides = case["overrides"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect();
            slice.notes = Some(o);
            Kind::Notes
        }
        _ => {
            let mut o = PresentationOptions::default();
            if let Some(s) = case.get("status") {
                o.status = match s.as_i64().unwrap() {
                    0 => PresentationStatus::AnyGood,
                    1 => PresentationStatus::NewOnly,
                    _ => PresentationStatus::None,
                };
            }
            if let Some(i) = case.get("inbox") {
                o.inbox = match i.as_i64().unwrap() {
                    0 => PresentationInbox::Agnostic,
                    1 => PresentationInbox::RequireInbox,
                    _ => PresentationInbox::AndIncludeAllInbox,
                };
            }
            slice.presentation = Some(o);
            Kind::Presentation
        }
    };
    (kind, slice)
}

#[test]
fn each_kinds_summary_is_the_references() {
    for case in recorded()["summaries"].as_array().unwrap() {
        let (kind, slice) = options(case);
        assert_eq!(summary(kind, &slice, &name), case["summary"], "{case}");
    }
}

#[test]
fn text_fields_read_and_write_lists_and_renames() {
    use hydrus_gui_model::import_options_editor::{lines, parse_renames, renames_text};
    assert_eq!(lines(" a \n\n b\n"), ["a", "b"]);
    let renames = parse_renames("comment -> artist comment\nbad line\n -> x\n");
    assert_eq!(
        renames,
        [("comment".to_owned(), "artist comment".to_owned())]
    );
    assert_eq!(renames_text(&renames), "comment -> artist comment");
}

#[test]
fn defaults_editor_uses_parent_stack_and_retains_hidden_custom_kinds() {
    use hydrus_core::import_options::UrlClassKind;
    use hydrus_gui_model::import_options_editor::{Editor, default_kinds};
    let mut manager = ImportOptionsManager::default();
    let post = manager
        .caller_default(CallerType::PostUrls)
        .unwrap()
        .clone();
    let empty = ImportOptionsSlice::default();
    let editor = Editor::new_for_defaults(&manager, CallerType::PostUrls, true, &post, &[]);
    assert_eq!(editor.source(Kind::Tags), "global");
    assert_eq!(editor.source(Kind::Notes), "global");
    assert!(!editor.kinds.contains(&Kind::Prefetch));
    let url = Editor::new_for_defaults(
        &manager,
        CallerType::UrlClass,
        true,
        &empty,
        &[("site".into(), UrlClassKind::Other)],
    );
    assert_eq!(url.source(Kind::Tags), "gallery/post urls");
    let watch = Editor::new_for_defaults(
        &manager,
        CallerType::UrlClass,
        true,
        &empty,
        &[("thread".into(), UrlClassKind::Watchable)],
    );
    assert_eq!(watch.source(Kind::Tags), "watchable urls");
    assert_ne!(url.values.tags, watch.values.tags);
    manager.url_class_defaults.push((
        "thread".into(),
        ImportOptionsSlice {
            notes: Some(NoteImportOptions {
                get_notes: false,
                ..NoteImportOptions::default()
            }),
            ..ImportOptionsSlice::default()
        },
    ));
    let watch = Editor::new_for_defaults(
        &manager,
        CallerType::UrlClass,
        true,
        &empty,
        &[("thread".into(), UrlClassKind::Watchable)],
    );
    assert!(watch.values.notes.as_ref().unwrap().get_notes);
    assert_eq!(watch.source(Kind::Notes), "global");
    let custom = ImportOptionsSlice {
        notes: Some(NoteImportOptions::default()),
        ..ImportOptionsSlice::default()
    };
    assert_eq!(
        default_kinds(CallerType::Subscription, true, &custom),
        [Kind::Locations, Kind::Presentation, Kind::Notes]
    );
    let importer = Editor::new(&manager, CallerType::Subscription, true, &empty);
    assert_eq!(importer.kinds.len(), 8);
    let fixture = hydrus_testkit::fixture_json("import_options_panel.json");
    for step in fixture["steps"].as_array().unwrap() {
        let Some(call) = step["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["kinds"].is_array())
        else {
            continue;
        };
        let caller = match step["action"].as_str().unwrap() {
            "_EditDefault" => CallerType::PostUrls,
            "_EditURLClass" => CallerType::UrlClass,
            _ => CallerType::Favourites,
        };
        assert_eq!(
            serde_json::json!(
                default_kinds(caller, true, &empty)
                    .iter()
                    .map(|kind| kind.code())
                    .collect::<Vec<_>>()
            ),
            call["kinds"]
        );
    }
}
