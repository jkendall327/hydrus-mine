//! Tab names against the reference's (`oracle/record_import_status.py`):
//! its notebook's own `_RefreshPageName` on stand-in pages, importers and
//! notebooks of many names, file counts and import progresses, under each
//! of the options that shape them.

use serde_json::Value as Json;

use hydrus_core::pages::{FileCountDisplay, PageNameSettings, TabKind, tab_name};

fn fixture() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/import_status.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn tabs_are_named_as_the_reference_names_them() {
    let recorded = fixture();
    let cases = recorded["tab_names"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for case in cases {
        let options = &case["options"];
        let settings = PageNameSettings {
            max_chars: usize::try_from(options["max_page_name_chars"].as_u64().unwrap()).unwrap(),
            file_counts: FileCountDisplay::from_code(
                options["page_file_count_display"].as_i64().unwrap(),
            )
            .unwrap(),
            import_progress: options["import_page_progress_display"].as_bool().unwrap(),
            decorate_notebooks: options["decorate_page_of_pages_tab_names"]
                .as_bool()
                .unwrap(),
            notebook_decorator: options["page_of_pages_decorator"].as_str().unwrap().into(),
            ..PageNameSettings::default()
        };
        let kind = match case["kind"].as_str().unwrap() {
            "page" => TabKind::Page,
            "importer" => TabKind::Importer,
            _ => TabKind::Notebook,
        };
        let n = |v: &Json| usize::try_from(v.as_u64().unwrap()).unwrap();
        let tab = tab_name(
            case["name"].as_str().unwrap(),
            kind,
            n(&case["num_files"]),
            (n(&case["value_range"][0]), n(&case["value_range"][1])),
            &settings,
        );
        // (an empty name isn't set: the stand-in's tab held one already)
        assert_eq!(tab, case["tab"].as_str().unwrap_or(""), "{case}");
    }
}

#[test]
fn a_new_clients_settings_are_the_references() {
    let recorded = fixture();
    let first = &recorded["tab_names"][0]["options"];
    let defaults = PageNameSettings::default();
    assert_eq!(first["max_page_name_chars"], defaults.max_chars);
    assert_eq!(
        first["page_of_pages_decorator"],
        defaults.notebook_decorator
    );
    assert_eq!(defaults.file_counts, FileCountDisplay::AllIfAny);
}
