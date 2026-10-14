//! The import options editor's pages for file filtering, prefetch logic,
//! notes and a tag service, driven through the window as a user would and
//! compared with `oracle/fixtures/import_options_fields.json`
//! (`record_import_options_fields.py`): the reference's own panels, driven
//! through the same changes, and what the reference does with the options
//! they give.
use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::Mime;
use hydrus_core::import_options::{
    CallerType, ImportOptionsManager, ImportOptionsSlice, ImportedAs, NoteConflict, PrefetchCheck,
};
use hydrus_import::FileImportOptions;
use serde_json::Value;
use slint::Model as _;

use crate::import_options_fields::{FILE_FILTERING, NOTES, PREFETCH, Setup, TAGS, custom, setup};

fn fixture() -> Value {
    hydrus_testkit::fixture_json("import_options_fields.json")
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn unit_index(units: &Value, unit: &str) -> i32 {
    units
        .as_array()
        .unwrap()
        .iter()
        .position(|u| u == unit)
        .unwrap() as i32
}

fn option_u64(value: &Value) -> Option<u64> {
    value.as_u64()
}

fn option_pair(value: &Value) -> Option<(u32, u32)> {
    value
        .as_array()
        .map(|p| (p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u32))
}

/// What the importer reads from the slice saved on the page: the client's
/// defaults under the importer's own options.
fn full(slice: &ImportOptionsSlice) -> hydrus_core::import_options::FullImportOptions {
    ImportOptionsManager::default().full(CallerType::PostUrls, Some(slice), &[])
}

// leaf: audit-network-options-deleted-bombs
// leaf: audit-network-options-sizes
#[test]
fn file_filtering_page_gives_the_references_options_and_files_are_judged_as_it_judges_them() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let filtering = &fixture["file_filtering"];
    let units = &filtering["units"];

    for scenario in filtering["scenarios"].as_array().unwrap() {
        let name = text(&scenario["name"]);
        let s = setup();
        let editor = s.editor();
        custom(&editor, FILE_FILTERING);
        for op in scenario["ops"].as_array().unwrap() {
            match text(&op[0]) {
                "exclude_deleted" => editor.set_exclude_deleted(op[1].as_bool().unwrap()),
                "bombs" => editor.set_bombs(op[1].as_bool().unwrap()),
                "size" => {
                    let row = ["min", "max", "gif"]
                        .iter()
                        .position(|r| *r == text(&op[1]))
                        .unwrap();
                    editor.invoke_changed();
                    editor.invoke_size_edited(
                        row as i32,
                        !op[2].as_bool().unwrap(),
                        op[3].as_i64().unwrap() as i32,
                        unit_index(units, text(&op[4])),
                    );
                }
                "resolution" => {
                    let row = ["min", "max"]
                        .iter()
                        .position(|r| *r == text(&op[1]))
                        .unwrap();
                    editor.invoke_changed();
                    editor.invoke_resolution_edited(
                        row as i32,
                        !op[2].as_bool().unwrap(),
                        op[3].as_i64().unwrap() as i32,
                        op[4].as_i64().unwrap() as i32,
                    );
                }
                other => panic!("unknown op {other}"),
            }
        }
        editor.invoke_changed();
        editor.invoke_apply();
        let saved = s.saved();
        let got = saved.file_filtering.clone().expect("custom file filtering");
        let want = &scenario["value"];
        assert_eq!(
            got.exclude_deleted,
            want["exclude_deleted"].as_bool().unwrap(),
            "{name}: exclude previously deleted"
        );
        assert_eq!(
            got.allow_decompression_bombs,
            want["bombs"].as_bool().unwrap(),
            "{name}: decompression bombs"
        );
        assert_eq!(
            got.min_size,
            option_u64(&want["min_size"]),
            "{name}: min size"
        );
        assert_eq!(
            got.max_size,
            option_u64(&want["max_size"]),
            "{name}: max size"
        );
        assert_eq!(
            got.max_gif_size,
            option_u64(&want["max_gif_size"]),
            "{name}: max gif size"
        );
        assert_eq!(
            got.min_resolution,
            option_pair(&want["min_resolution"]),
            "{name}: min resolution"
        );
        assert_eq!(
            got.max_resolution,
            option_pair(&want["max_resolution"]),
            "{name}: max resolution"
        );

        // and the importer judges files by them as the reference does
        let services = s.store.snapshot().services.clone();
        let options = FileImportOptions::from_full(&full(&saved), &services);
        for probe in scenario["probes"].as_array().unwrap() {
            let mime = Mime::from_code(probe["mime"].as_u64().unwrap() as u8).unwrap();
            let dimension = |key: &str| probe[key].as_u64().map(|d| d as u32);
            let verdict = options
                .check_values(
                    probe["size"].as_u64().unwrap(),
                    mime,
                    dimension("width"),
                    dimension("height"),
                )
                .err();
            assert_eq!(
                verdict.as_deref(),
                probe["error"].as_str(),
                "{name}: {} bytes {mime:?} {:?}x{:?}",
                probe["size"],
                probe["width"],
                probe["height"]
            );
        }
    }
}

// leaf: audit-network-options-sizes
#[test]
fn file_filtering_page_shows_saved_limits_as_the_references_controls_do() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let filtering = &fixture["file_filtering"];
    let units = &filtering["units"];

    for display in filtering["displays"].as_array().unwrap() {
        let saved = &display["saved"];
        let s = setup();
        let options = hydrus_core::import_options::FileFilteringOptions {
            min_size: option_u64(&saved[0]),
            max_size: option_u64(&saved[1]),
            max_gif_size: option_u64(&saved[2]),
            min_resolution: option_pair(&saved[3]),
            max_resolution: option_pair(&saved[4]),
            ..Default::default()
        };
        let queue = s.queue;
        s.store
            .write(move |ctx| {
                hydrus_store::queues::set_queue_options(
                    ctx.conn(),
                    queue,
                    &ImportOptionsSlice {
                        file_filtering: Some(options),
                        ..ImportOptionsSlice::default()
                    },
                )
            })
            .unwrap();
        // (the page reads its importer's options when it is shown)
        (s.bound.sync)();
        let editor = s.editor();
        editor.invoke_kind_clicked(FILE_FILTERING);
        let sizes = editor.get_sizes();
        for (row, key) in ["min_size", "max_size", "max_gif_size"].iter().enumerate() {
            let want = &display[key];
            let got = sizes.row_data(row).unwrap();
            assert_eq!(
                (got.on, i64::from(got.value), got.unit),
                (
                    !want["none"].as_bool().unwrap(),
                    want["value"].as_i64().unwrap(),
                    unit_index(
                        units,
                        match want["unit"].as_u64().unwrap() {
                            1 => "B",
                            1024 => "KB",
                            0x10_0000 => "MB",
                            0x4000_0000 => "GB",
                            _ => "TB",
                        }
                    )
                ),
                "saved {saved}: {key}"
            );
        }
        let resolutions = editor.get_resolutions();
        for (row, key) in ["min_resolution", "max_resolution"].iter().enumerate() {
            let want = &display[key];
            let got = resolutions.row_data(row).unwrap();
            assert_eq!(
                (got.on, i64::from(got.width), i64::from(got.height)),
                (
                    !want["none"].as_bool().unwrap(),
                    want["width"].as_i64().unwrap(),
                    want["height"].as_i64().unwrap()
                ),
                "saved {saved}: {key}"
            );
        }
    }
}

// leaf: audit-network-options-prefetch
#[test]
fn prefetch_page_follows_the_references_panel_step_by_step() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let prefetch = &fixture["prefetch"];

    let state = |editor: &hydrus_gui::ImportOptionsWindow| {
        (
            editor.get_hash_index(),
            editor.get_url_index(),
            editor.get_fetch_hash(),
            editor.get_fetch_url(),
            editor.get_neighbour_spam(),
        )
    };
    let want = |value: &Value| {
        (
            value["hash"].as_i64().unwrap() as i32,
            value["url"].as_i64().unwrap() as i32,
            value["fetch_hash"].as_bool().unwrap(),
            value["fetch_url"].as_bool().unwrap(),
            value["neighbour"].as_bool().unwrap(),
        )
    };
    for scenario in prefetch["scenarios"].as_array().unwrap() {
        let name = text(&scenario["name"]);
        let s = setup();
        let editor = s.editor();
        custom(&editor, PREFETCH);
        let states = scenario["states"].as_array().unwrap();
        assert_eq!(state(&editor), want(&states[0]), "{name}: the new panel");
        for (n, op) in scenario["ops"].as_array().unwrap().iter().enumerate() {
            match text(&op[0]) {
                "hash" => editor.set_hash_index(op[1].as_i64().unwrap() as i32),
                "url" => editor.set_url_index(op[1].as_i64().unwrap() as i32),
                "fetch_hash" => editor.set_fetch_hash(op[1].as_bool().unwrap()),
                "fetch_url" => editor.set_fetch_url(op[1].as_bool().unwrap()),
                "neighbour" => editor.set_neighbour_spam(op[1].as_bool().unwrap()),
                other => panic!("unknown op {other}"),
            }
            editor.invoke_changed();
            assert_eq!(state(&editor), want(&states[n + 1]), "{name}: after {op}");
        }
        editor.invoke_apply();
        let got = s.saved().prefetch.expect("custom prefetch");
        let last = states.last().unwrap();
        let check = |code: &Value| match code.as_i64().unwrap() {
            0 => PrefetchCheck::DoNotCheck,
            1 => PrefetchCheck::Check,
            _ => PrefetchCheck::CheckAndMatchesAreDispositive,
        };
        assert_eq!(got.hash_check, check(&last["hash_check_type"]), "{name}");
        assert_eq!(got.url_check, check(&last["url_check_type"]), "{name}");
        assert_eq!(
            got.fetch_metadata_even_if_hash_recognised_and_file_already_in_db,
            last["fetch_hash"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            got.fetch_metadata_even_if_url_recognised_and_file_already_in_db,
            last["fetch_url"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            got.url_check_looks_for_neighbour_spam,
            last["neighbour"].as_bool().unwrap(),
            "{name}"
        );
    }
}

// leaf: audit-network-options-note-conflict
#[test]
fn notes_page_gives_the_references_options_and_notes_merge_as_it_merges_them() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let notes = &fixture["notes"];

    for scenario in notes["scenarios"].as_array().unwrap() {
        let name = text(&scenario["name"]);
        let s = setup();
        let editor = s.editor();
        custom(&editor, NOTES);
        for op in scenario["ops"].as_array().unwrap() {
            match text(&op[0]) {
                "get_notes" => editor.set_get_notes(op[1].as_bool().unwrap()),
                "extend" => editor.set_extend_notes(op[1].as_bool().unwrap()),
                "conflict" => editor.set_conflict_index(op[1].as_i64().unwrap() as i32),
                "whitelist" => editor.set_note_whitelist(
                    op[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(text)
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into(),
                ),
                "renames" => editor.set_note_renames(
                    op[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| format!("{} -> {}", text(&p[0]), text(&p[1])))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into(),
                ),
                "all_override" => match op[1].as_str() {
                    Some(to) => {
                        editor.set_rename_all_on(true);
                        editor.set_rename_all(to.into());
                    }
                    None => editor.set_rename_all_on(false),
                },
                other => panic!("unknown op {other}"),
            }
            editor.invoke_changed();
        }
        editor.invoke_apply();
        let saved = s.saved();
        let got = saved.notes.clone().expect("custom notes");
        let value = &scenario["value"];
        assert_eq!(
            got.get_notes,
            value["get_notes"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            got.extend_existing_note_if_possible,
            value["extend"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            got.conflict,
            NoteConflict::from_code(value["conflict"].as_i64().unwrap()).unwrap(),
            "{name}: conflict"
        );
        let mut whitelist = got.name_whitelist.clone();
        whitelist.sort();
        assert_eq!(
            whitelist,
            value["whitelist"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| text(n).to_owned())
                .collect::<Vec<_>>(),
            "{name}: whitelist"
        );
        let mut renames = got.name_overrides.clone();
        renames.sort();
        assert_eq!(
            renames,
            value["renames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (text(&p[0]).to_owned(), text(&p[1]).to_owned()))
                .collect::<Vec<_>>(),
            "{name}: renames"
        );
        assert_eq!(
            got.all_name_override.as_deref(),
            value["all_override"].as_str(),
            "{name}: rename the spare notes"
        );

        // and incoming notes are merged by it as the reference merges them
        let notes_options = full(&saved).notes;
        for probe in scenario["probes"].as_array().unwrap() {
            let existing: BTreeMap<String, String> = probe["existing"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), text(v).to_owned()))
                .collect();
            let incoming: Vec<(String, String)> = probe["incoming"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (text(&p[0]).to_owned(), text(&p[1]).to_owned()))
                .collect();
            let want: BTreeMap<String, String> = probe["updates"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), text(v).to_owned()))
                .collect();
            assert_eq!(
                notes_options.updates(&existing, &incoming),
                want,
                "{name}: {existing:?} + {incoming:?}"
            );
        }
    }
}

// leaf: audit-network-options-tag-fetch
#[test]
fn a_tag_services_rules_follow_the_references_panel() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let tags = &fixture["tags"];
    let key = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);

    let toggles = [
        ("_to_new_files", "to-new"),
        ("_to_already_in_inbox", "to-inbox"),
        ("_to_already_in_archive", "to-archive"),
        ("_get_tags_overwrite_deleted", "get-overwrite"),
        ("_additional_tags_overwrite_deleted", "additional-overwrite"),
        ("_only_add_existing_tags", "only-existing"),
    ];
    for scenario in tags["scenarios"].as_array().unwrap() {
        let name = text(&scenario["name"]);
        let s: Setup = setup();
        let editor = s.editor();
        custom(&editor, TAGS);
        let rows = editor.get_tag_services();
        let mine = rows
            .iter()
            .position(|service| service.name == "my tags")
            .unwrap() as i32;
        // a service no rules were set for starts as the reference's
        let fresh = &tags["fresh"];
        let row = rows.row_data(mine as usize).unwrap();
        assert_eq!(
            (row.get_tags, row.to_new, row.to_inbox, row.to_archive),
            (
                fresh["get_tags"].as_bool().unwrap(),
                fresh["to_new_files"].as_bool().unwrap(),
                fresh["to_already_in_inbox"].as_bool().unwrap(),
                fresh["to_already_in_archive"].as_bool().unwrap()
            ),
            "{name}: a new service"
        );
        for op in scenario["ops"].as_array().unwrap() {
            match text(&op[0]) {
                "get_tags" => {
                    editor.invoke_tag_service_toggled(
                        mine,
                        "get-tags".into(),
                        op[1].as_bool().unwrap(),
                    );
                }
                "cog" => {
                    let which = toggles.iter().find(|(a, _)| *a == text(&op[1])).unwrap().1;
                    editor.invoke_tag_service_toggled(mine, which.into(), op[2].as_bool().unwrap());
                }
                other => panic!("unknown op {other}"),
            }
        }
        editor.invoke_apply();
        let saved = s.saved();
        let options = saved.tags.clone().expect("custom tags");
        // (rules never set are the defaults)
        let got = options.service(&key).cloned().unwrap_or_default();
        let value = &scenario["value"];
        let flag = |k: &str| value[k].as_bool().unwrap();
        assert_eq!(got.get_tags, flag("get_tags"), "{name}: get tags");
        assert_eq!(got.to_new_files, flag("to_new_files"), "{name}: new files");
        assert_eq!(
            got.to_already_in_inbox,
            flag("to_already_in_inbox"),
            "{name}: inbox"
        );
        assert_eq!(
            got.to_already_in_archive,
            flag("to_already_in_archive"),
            "{name}: archive"
        );
        assert_eq!(
            got.get_tags_overwrite_deleted,
            flag("get_tags_overwrite_deleted"),
            "{name}: parsed overwrite"
        );
        assert_eq!(
            got.additional_tags_overwrite_deleted,
            flag("additional_tags_overwrite_deleted"),
            "{name}: additional overwrite"
        );
        assert_eq!(
            got.only_add_existing_tags,
            flag("only_add_existing_tags"),
            "{name}"
        );
        assert_eq!(
            options
                .services
                .iter()
                .filter(|(k, _)| *k == key)
                .any(|(_, o)| o.get_tags),
            flag("worth_fetching_tags"),
            "{name}: worth fetching tags"
        );

        // which kinds of file the rules give tags to (the importer asks
        // `applies_to`): the reference gives the additional tag to exactly
        // those
        let gets: BTreeSet<&str> = [
            ("new", ImportedAs::New),
            ("inbox", ImportedAs::AlreadyInInbox),
            ("archive", ImportedAs::AlreadyInArchive),
        ]
        .into_iter()
        .filter(|(_, imported)| got.applies_to(*imported))
        .map(|(label, _)| label)
        .collect();
        let want: BTreeSet<&str> = ["new", "inbox", "archive"]
            .into_iter()
            .filter(|label| !scenario["outcomes"][label].as_array().unwrap().is_empty())
            .collect();
        assert_eq!(gets, want, "{name}: the files the rules apply to");
    }
}
