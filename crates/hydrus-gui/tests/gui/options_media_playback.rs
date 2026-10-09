//! The media playback page against its consumers: each option is changed in
//! the real options window, applied, and what reads the saved value acts on
//! it.

use slint::ComponentHandle as _;

use hydrus_import::FileImporter;
use hydrus_media::MediaTools;

use crate::options_gui_support::{box_of, items, row, show_page};
use crate::options_media_support::Media;

// leaf: audit-options-media-playback-transparency-consider-a-file-as-having-transparency-when
#[test]
fn the_transparency_strictness_is_what_the_importer_judges_images_by() {
    const LABEL: &str = "Consider a file as \"having transparency\" when:";
    // the reference's import job at each level (record_transparency_strictness.py)
    let recording = hydrus_testkit::fixture_json("transparency_strictness.json");
    // (what counts as transparency is the process's, as in the reference)
    let _global = crate::options_system_consumers::FILE_HANDLING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let client = Media::basic();

    let options = client.open_options();
    show_page(&options, "media playback");
    let (_, found) = row(&options, LABEL);
    assert_eq!(box_of(&options, LABEL), "transparency");
    let choices: Vec<String> = recording["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(items(&found), choices);
    assert_eq!(
        i64::from(found.index),
        recording["default_index"].as_i64().unwrap()
    );
    options.hide().unwrap();

    for case in recording["levels"].as_array().unwrap() {
        // (a client of its own, on a thread of its own, so that each file is
        // new to it)
        std::thread::scope(|scope| {
            scope
                .spawn(|| imports_at_level(case, LABEL))
                .join()
                .unwrap();
        });
    }
}

/// The level the importer reads.
fn level(client: &Media) -> u8 {
    client
        .setting::<hydrus_store::settings::FileHandlingSettings>()
        .transparency_strictness
}

/// One recorded level: chosen in the options and applied, then the recorded
/// files imported by an importer made before the change.
fn imports_at_level(case: &serde_json::Value, label: &str) {
    {
        let client = Media::basic();
        // an importer already running when the option changes, as the daemon's
        let importer = FileImporter::new(client.store.clone(), MediaTools::new());
        let choice = i32::try_from(case["index"].as_i64().unwrap()).unwrap();
        let saved_before = level(&client);
        let options = client.open_options();
        show_page(&options, "media playback");
        let (i, _) = row(&options, label);
        options.invoke_choice_chosen(i, choice);
        // (nothing before apply)
        assert_eq!(level(&client), saved_before);
        options.invoke_apply();
        options.hide().unwrap();
        assert_eq!(i64::from(level(&client)), case["saved"].as_i64().unwrap());
        // each file imported, and its stored flag read back
        for (name, expected) in case["has_transparency"].as_object().unwrap() {
            let path = hydrus_testkit::fixture_path(format!("transparency/{name}"));
            let imported = importer
                .import_path(&path, &hydrus_import::FileImportOptions::default())
                .unwrap();
            assert_eq!(
                imported.status,
                hydrus_import::ImportStatus::SuccessfulAndNew,
                "{name}: {}",
                imported.note
            );
            let hash = imported.hash.unwrap();
            let flags: u32 = client
                .store
                .read(|conn| {
                    Ok(conn.query_row(
                        "SELECT flags FROM files JOIN hashes USING (hash_id) WHERE sha256 = ?1",
                        [hash.as_bytes().to_vec()],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert_eq!(
                hydrus_store::media::FileFlags(flags)
                    .has(hydrus_store::media::FileFlags::TRANSPARENCY),
                expected.as_bool().unwrap(),
                "choice {choice}: {name}"
            );
        }
    }
}
