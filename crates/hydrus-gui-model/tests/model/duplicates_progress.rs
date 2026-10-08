//! Exact Qt checkbox staging and existing preparation publisher behavior.
use hydrus_gui_model::{
    duplicates_page::preparation,
    options::{Editor, Row, Settings},
};
use hydrus_store::{Store, duplicates_progress, settings};
const LABEL: &str = "Hide the \"x% done\" notification on preparation tab when >99% searched:";
fn editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|v| *v == "duplicates")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==LABEL))
        .unwrap();
    (editor, row)
}
#[test]
fn qt_staging_persistence_and_scoped_save_drive_strict_caught_up_preparation_labels() {
    let fixture = hydrus_testkit::fixture_json("duplicates_progress_option.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up,
        fixture["factory"]
    );
    let (mut cancelled, row) = editor(&store);
    cancelled.check(row, false);
    drop(cancelled);
    assert_eq!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up,
        fixture["cancel_reopened"]
    );
    for case in fixture["cases"].as_array().unwrap() {
        let (mut edit, row) = editor(&store);
        let before = store.read(duplicates_progress::load).unwrap();
        edit.check(row, case["hide"].as_bool().unwrap());
        assert_eq!(store.read(duplicates_progress::load).unwrap(), before);
        store
            .write(|c| {
                let mut filter: hydrus_store::duplicates::DuplicateFilterSettings =
                    settings::get(c.conn())?;
                filter.max_batch_size = 77;
                settings::set(c.conn(), &filter)
            })
            .unwrap();
        let (after, before, problems) = edit.applied();
        assert!(problems.is_empty());
        let before = before.clone();
        store.write(move |c| after.save(c.conn(), &before)).unwrap();
        assert_eq!(
            store
                .read::<hydrus_store::duplicates::DuplicateFilterSettings>(settings::get)
                .unwrap()
                .max_batch_size,
            77
        );
        let saved = Store::open(store.dir())
            .unwrap()
            .read(duplicates_progress::load)
            .unwrap();
        assert_eq!(saved.hide_caught_up, case["reopened"]);
        for row in case["publisher"].as_array().unwrap() {
            let searched = row["counts"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(distance, count)| {
                    let distance: i64 = distance.parse().unwrap();
                    (
                        (distance >= 0).then_some(distance as u32),
                        count.as_u64().unwrap() as usize,
                    )
                })
                .collect();
            let result = preparation(
                &searched,
                row["distance"].as_u64().unwrap() as u32,
                saved.hide_caught_up,
            );
            assert_eq!(result.page_name, row["page_name"]);
            assert_eq!(result.eligible, row["eligible"]);
            assert_eq!(result.searched, row["searched"]);
            assert_eq!(result.can_start, row["can_start"]);
            assert_eq!(
                [result.gauge.0, result.gauge.1],
                serde_json::from_value::<[u64; 2]>(row["gauge"].clone()).unwrap()
            );
        }
    }
}
