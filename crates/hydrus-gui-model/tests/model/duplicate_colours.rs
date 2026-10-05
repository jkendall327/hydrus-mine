//! Real Qt colour rounding, independent A/B policy, and staged preferences.
use hydrus_gui_model::{
    duplicate_colours,
    options::{Editor, Row, Settings},
};
use hydrus_store::{
    services::Rgb,
    settings::{self, DuplicateColourSettings},
};
use serde_json::{Value, json};

fn values(settings: &DuplicateColourSettings) -> Value {
    json!([
        settings.intensity_a,
        settings.intensity_b,
        settings.checkerboard
    ])
}

#[test]
fn qcolor_rounding_and_independent_transparency_replay_actual_canvas_outputs() {
    let fixture = hydrus_testkit::fixture_json("duplicate_colours.json");
    for case in fixture["colours"].as_array().unwrap() {
        let base = Rgb(serde_json::from_value(case["base"].clone()).unwrap());
        let intensity = serde_json::from_value(case["intensity"].clone()).unwrap();
        assert_eq!(
            json!(duplicate_colours::adjusted(base, intensity).0),
            case["colour"],
            "{case}"
        );
    }
    for case in fixture["canvas"].as_array().unwrap() {
        let settings = DuplicateColourSettings {
            intensity_a: serde_json::from_value(case["a"].clone()).unwrap(),
            intensity_b: serde_json::from_value(case["b"].clone()).unwrap(),
            checkerboard: case["checker"].as_bool().unwrap(),
            ..DuplicateColourSettings::default()
        };
        assert_eq!(
            json!(
                duplicate_colours::background(&settings, true, case["file_a"].as_bool().unwrap()).0
            ),
            case["colour"]
        );
        assert_eq!(
            duplicate_colours::background(&settings, false, false),
            settings.background
        );
        let mode = duplicate_colours::transparency(
            &settings,
            case["transparent"].as_bool().unwrap(),
            case["green"].as_bool().unwrap(),
        );
        let colour = match mode {
            1 => json!([237, 237, 237]),
            2 => json!([34, 255, 0]),
            _ => case["colour"].clone(),
        };
        assert_eq!(colour, case["pixels"]["0,0"]);
    }
}

#[test]
fn options_keep_saved_zero_on_cancel_then_normalise_on_apply_and_preserve_concurrent_background() {
    let fixture = hydrus_testkit::fixture_json("duplicate_colours.json");
    let reference = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = super::options_dialog::fixture_store(&reference);
    assert_eq!(
        values(
            &store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap()
        ),
        fixture["initial"]
    );
    let unvisited = Editor::new(store.read(Settings::load).unwrap());
    let (after, _, problems) = unvisited.applied();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        values(&after.duplicate_colours),
        fixture["initial_displayed"]
    );
    assert_eq!(
        values(
            &store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap()
        ),
        fixture["initial"],
        "unvisited draft still cannot write before Apply"
    );
    for case in fixture["options"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "duplicates")
            .unwrap();
        editor.show_page(page);
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let row = editor.rows().iter().position(|row| matches!(row, Row::Opt {option,..} if option.label == label.as_str().unwrap())).unwrap();
            if index == 2 {
                editor.check(row, case["input"][index].as_bool().unwrap());
            } else {
                editor.none(row, case["input"][index].is_null());
                if let Some(number) = case["input"][index].as_i64() {
                    editor.number(row, number);
                }
            }
        }
        assert_eq!(
            store.read(Settings::load).unwrap(),
            before,
            "draft has not saved"
        );
        let (after, baseline, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(values(&after.duplicate_colours), case["saved"]);
        let after = after.clone();
        let baseline = baseline.clone();
        store
            .write(move |tx| {
                let mut concurrent: DuplicateColourSettings = settings::get(tx.conn())?;
                concurrent.background = Rgb([52; 3]);
                settings::set(tx.conn(), &concurrent)?;
                after.save(tx.conn(), &baseline)
            })
            .unwrap();
        let reopened = hydrus_store::Store::open(store.dir()).unwrap();
        let saved: DuplicateColourSettings = reopened.read(settings::get).unwrap();
        assert_eq!(values(&saved), case["reopened"]);
        assert_eq!(saved.background, Rgb([52; 3]));
    }
    assert_eq!(fixture["cancel_before"], fixture["cancel_after"]);
}
