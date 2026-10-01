//! A page's status bar says what the reference's does
//! (`oracle/record_status_bar.py`, at a fixed "now"): for pages of the
//! `basic` fixture's files, of several types and of one, and empty, with
//! nothing selected, several selections and each file alone; and how a
//! search page's says why it is empty.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::InfoLineSettings;
use hydrus_gui::SearchPage;
use hydrus_gui::info_lines::status_line;
use hydrus_gui::status::{Facts, Items, facts, status};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;

fn store() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    (native, store)
}

#[test]
fn the_status_bar_is_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("status_bar.json");
    let (_dir, store) = store();
    let snapshot = store.snapshot();
    let now_ms = fixture["now"].as_i64().unwrap() * 1000;
    let ids = |hashes: &Value| -> Vec<HashId> {
        hashes
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                store
                    .read(|c| {
                        hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap())
                    })
                    .unwrap()
                    .unwrap()
            })
            .collect()
    };
    let facts_of = |files: &[HashId]| -> Vec<Facts> {
        let known = facts(&store, files);
        files
            .iter()
            .map(|f| known.iter().find(|(id, _)| id == f).unwrap().1)
            .collect()
    };
    let mut checked = 0;
    for page in fixture["pages"].as_array().unwrap() {
        let name = page["name"].as_str().unwrap();
        let files = facts_of(&ids(&page["files"]));
        let empty = page["override"].as_str();
        let single_file_info = page["single_file_info"].as_bool().unwrap_or(true);
        for selection in page["selections"].as_array().unwrap() {
            let selected = ids(&selection["selected"]);
            let single_line = match selected[..] {
                [file] if single_file_info => {
                    let media = store
                        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
                        .unwrap()
                        .results
                        .remove(0);
                    Some(status_line(
                        &media,
                        &snapshot.services,
                        &InfoLineSettings::default(),
                        now_ms,
                    ))
                }
                _ => None,
            };
            assert_eq!(
                status(
                    (&files, Items::files(files.len())),
                    (&facts_of(&selected), Items::files(selected.len())),
                    empty,
                    single_line.as_deref()
                ),
                selection["status"].as_str().unwrap(),
                "{name}: {} selected",
                selected.len()
            );
            checked += 1;
        }
    }
    assert!(checked > 60, "{checked}");
}

#[test]
fn a_search_page_says_why_it_is_empty() {
    let (_dir, store) = store();
    let mut page = SearchPage::new(store);
    assert_eq!(page.status(), "no search done yet");
    page.add_predicate("system:inbox");
    // (the inbox file in the trash isn't in "my files")
    assert_eq!(page.status(), "15 files - totalling 169 KB");
    page.select(0);
    assert!(
        page.status().contains(" selected, ") && page.status().contains("imported: "),
        "{}",
        page.status()
    );
    // (once it has shown files, it says so even when they all leave, as
    // the reference does)
    let files = page.results().to_vec();
    page.remove_files(&files);
    assert_eq!(page.status(), "0 file");
    page.remove_predicate(0);
    assert_eq!(page.status(), "no search");
    page.add_predicate("nothing has this tag");
    assert_eq!(page.status(), "no files found for this search");
}
