use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui_model::frame_locations::{Action, Table, normalised};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type RecordedFrame = (
    String,
    bool,
    bool,
    Option<(i32, i32)>,
    Option<(i32, i32)>,
    (i32, i32),
    String,
    bool,
    bool,
);

fn frames(value: &Value) -> BTreeMap<String, FrameLocation> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let (
                name,
                remember_size,
                remember_position,
                last_size,
                last_position,
                default_gravity,
                default_position,
                maximised,
                fullscreen,
            ): RecordedFrame = serde_json::from_value(row.clone()).unwrap();
            (
                name,
                FrameLocation {
                    remember_size,
                    remember_position,
                    last_size,
                    last_position,
                    default_gravity,
                    default_position,
                    maximised,
                    fullscreen,
                },
            )
        })
        .collect()
}

#[test]
fn frame_table_defaults_cells_and_selected_actions_match_actual_qt() {
    let reference = hydrus_testkit::fixture_json("frame_locations.json");
    let initial = frames(&reference["events"][0]["state"]["rows"]);
    assert_eq!(WindowSettings::default().frames(), initial);
    let mut table = Table::new(initial);
    assert_eq!(
        json!(
            table
                .rows
                .iter()
                .map(hydrus_gui_model::frame_locations::Row::cells)
                .collect::<Vec<_>>()
        ),
        reference["events"][0]["state"]["cells"]
    );
    for name in ["main_gui", "media_viewer"] {
        let at = table.rows.iter().position(|r| r.name == name).unwrap();
        table.click(at, name == "media_viewer", false);
    }
    for (index, action) in [
        Action::FlipSize,
        Action::FlipPosition,
        Action::ResetSize,
        Action::ResetPosition,
    ]
    .into_iter()
    .enumerate()
    {
        table.action(action);
        assert_eq!(
            table.values(),
            frames(&reference["events"][index + 1]["state"]["rows"])
        );
        assert_eq!(
            json!(
                table
                    .rows
                    .iter()
                    .map(hydrus_gui_model::frame_locations::Row::cells)
                    .collect::<Vec<_>>()
            ),
            reference["events"][index + 1]["state"]["cells"]
        );
    }
    let selected = table
        .selection
        .in_order(&table.rows.iter().map(|r| r.id).collect::<Vec<_>>());
    table.sort(1, false);
    assert_eq!(table.selection.in_order(&selected), selected);
    assert!(table.selected().is_none());
    let at = table
        .rows
        .iter()
        .position(|r| r.name == "main_gui")
        .unwrap();
    table.click(at, false, false);
    let id = table.selected().unwrap().id;
    let edited = frames(&Value::Array(vec![reference["edits"][1]["edited"].clone()]))
        .remove("main_gui")
        .unwrap();
    table.replace(id, edited.clone());
    assert_eq!(table.selected().unwrap().name, "main_gui");
    assert_eq!(table.values()["main_gui"], edited);
    let out_of_bounds = FrameLocation {
        last_size: Some((1, 2_000_000)),
        last_position: Some((-2_000_000, 2_000_000)),
        ..edited
    };
    let bounded = normalised(out_of_bounds);
    let expected = frames(&Value::Array(vec![reference["bounds"][1].clone()]))
        .remove("main_gui")
        .unwrap();
    assert_eq!(bounded, expected);
}

#[test]
fn settings_keep_unknown_rows_and_merge_unedited_runtime_geometry() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(native.path()).unwrap();
    let before = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let mut editor = hydrus_gui_model::options::Editor::new(before.clone());
    let mut table = Table::new(editor.edited_frame_locations());
    assert_eq!(table.rows.len(), 22);
    let at = table
        .rows
        .iter()
        .position(|r| r.name == "main_gui")
        .unwrap();
    table.click(at, false, false);
    table.action(Action::ResetSize);
    editor.set_frame_locations(table.values());
    let concurrent = FrameLocation {
        last_size: Some((777, 555)),
        maximised: false,
        fullscreen: false,
        ..FrameLocation::media_viewer()
    };
    let expected = concurrent.clone();
    store
        .write(move |ctx| {
            let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
            settings.media_viewer = concurrent;
            settings
                .other_frames
                .insert("authored_unknown_frame".into(), FrameLocation::main_gui());
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    let (after, _, problems) = editor.applied();
    assert!(problems.is_empty());
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved: WindowSettings = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(saved.main_gui.last_size, None);
    assert_eq!(saved.media_viewer, expected);
    assert!(saved.other_frames.contains_key("authored_unknown_frame"));
    let reopened = hydrus_gui_model::options::Editor::new(
        store
            .read(hydrus_gui_model::options::Settings::load)
            .unwrap(),
    );
    assert_eq!(reopened.edited_frame_locations(), saved.frames());
}
