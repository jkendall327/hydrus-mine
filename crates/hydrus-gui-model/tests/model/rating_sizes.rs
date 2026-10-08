//! Recorded rating size inputs, transactional persistence and counter consumers.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::settings::{self, RatingContextSizes};
use serde_json::json;

fn values(sizes: &RatingContextSizes) -> serde_json::Value {
    json!([
        sizes.preview_icon_size,
        sizes.preview_incdec_height,
        sizes.dialog_icon_size,
        sizes.dialog_incdec_height
    ])
}

// leaf: audit-options-ratings-dialogs-dialogs-inc-dec-rating-height
// leaf: audit-options-ratings-dialogs-dialogs-like-dislike-and-numerical-rating-icon-size
#[test]
fn four_sizes_replay_qt_ranges_fractional_values_cancel_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("rating_context_sizes.json");
    // Qt keeps the exact binary side of 31.755's half, while exact binary
    // halves (6.125 and 12.125) round away from zero.
    let boundary = fixture["events"].as_array().unwrap().last().unwrap();
    assert_eq!(boundary["saved"], json!([31.75, 6.13, 31.75, 12.13]));
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = super::options_dialog::fixture_store(&recorded);
    let mut before = store.read(Settings::load).unwrap();
    assert_eq!(
        values(&before.rating_context_sizes),
        json!(serde_json::from_value::<Vec<f64>>(fixture["initial"].clone()).unwrap())
    );
    for event in fixture["events"].as_array().unwrap() {
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "ratings")
            .unwrap();
        editor.show_page(page);
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let row = editor.rows().iter().position(|row| matches!(row, Row::Opt {option,..} if option.label == label.as_str().unwrap())).unwrap();
            editor.text(row, &event["input"][index].to_string());
        }
        let (after, baseline, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(values(&after.rating_context_sizes), event["saved"]);
        assert_eq!(store.read(Settings::load).unwrap(), before);
        let baseline = baseline.clone();
        let applied = after.clone();
        store
            .write(move |writer| applied.save(writer.conn(), &baseline))
            .unwrap();
        let reopened = hydrus_store::Store::open(store.dir()).unwrap();
        before = reopened.read(Settings::load).unwrap();
        assert_eq!(values(&before.rating_context_sizes), event["round_trip"]);
        assert_eq!(values(&before.rating_context_sizes), event["reopened"]);
        for context in event["consumers"].as_array().unwrap() {
            let counter = context
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["kind"] == 22)
                .unwrap();
            assert_eq!(
                hydrus_gui_model::rating_sizes::counter_width(
                    counter["icon"][1].as_f64().unwrap(),
                    12345
                )
                .to_bits(),
                counter["icon"][0].as_f64().unwrap().to_bits()
            );
        }
    }
    assert_eq!(fixture["cancel_before"], fixture["cancel_after"]);
    let partial: RatingContextSizes =
        serde_json::from_value(json!({"dialog_icon_size": 28.5})).unwrap();
    assert_eq!(json!(partial.dialog_icon_size), json!(28.5));
    assert_eq!(json!(partial.preview_incdec_height), json!(12.0));
    let fresh = tempfile::tempdir().unwrap();
    let absent = hydrus_store::Store::open(fresh.path()).unwrap();
    assert_eq!(
        absent.read(settings::get::<RatingContextSizes>).unwrap(),
        RatingContextSizes::default()
    );
}
