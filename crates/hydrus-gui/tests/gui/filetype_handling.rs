//! Options > media playback > per-filetype handling: "add", "edit" and
//! "delete" in the real Options window (the table, the filetype chooser and
//! the media view options editor) and the zoom the real media viewer then
//! opens a jpeg at, against `oracle/record_filetype_handling.py` (the
//! reference's real `MediaPlaybackPanel` add/edit/delete, and the zooms its
//! `CalculateCanvasZooms` gives for the options each leaves).

// (zooms and positions are exact, as the reference's are)
#![allow(clippy::float_cmp)]

use serde_json::Value;

use slint::{ComponentHandle as _, Model as _};

use crate::options_gui_support::show_page;
use crate::options_media_support::Media;
use crate::options_media_zoom::{CANVAS, rect, small_jpeg_viewer};

type Rect = (f32, f32, f32, f32);

/// The viewer on the small jpeg, its rectangle, closed again.
fn opened(client: &Media) -> (Rect, (f32, f32)) {
    let (viewer, size) = small_jpeg_viewer(client);
    let shown = rect(&viewer);
    viewer.invoke_close_requested();
    (shown, size)
}

/// The table's rows' first cells.
fn listed(options: &hydrus_gui::OptionsWindow) -> Vec<String> {
    options
        .get_media_view_rows()
        .iter()
        .map(|row| row.cells.row_data(0).unwrap().to_string())
        .collect()
}

fn select_jpeg(options: &hydrus_gui::OptionsWindow) {
    let at = listed(options)
        .iter()
        .position(|name| name == "image: jpeg")
        .expect("a jpeg row");
    options.invoke_media_view_clicked(i32::try_from(at).unwrap(), false, false);
}

/// The canvas fit: the largest zoom that keeps the file inside the canvas.
fn fit(canvas: (f64, f64), file: (f64, f64)) -> f64 {
    (canvas.0 / file.0).min(canvas.1 / file.1)
}

/// What the recording's zoom `recorded` for the small jpeg stands for here:
/// 100% is the file's own size; any other is the canvas fit (the recording's
/// own numbers say so: its zoom is the fit of its canvas and file), which the
/// viewer shows as it did at the start.
fn check(shown: Rect, size: (f32, f32), start: Rect, recording: &Value, recorded: f64, what: &str) {
    if (recorded - 1.0).abs() < 1e-9 {
        assert_eq!((shown.2, shown.3), size, "{what}: at 100%");
        return;
    }
    let pair = |v: &Value| (v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
    let canvas = pair(&recording["canvas"]);
    let file = pair(&recording["files"]["small jpeg"]);
    assert!(
        (recorded - fit(canvas, file)).abs() < 1e-9,
        "{what}: the recorded zoom {recorded} is the canvas fit"
    );
    // (the viewer's canvas here is the window; the fit's size, to a pixel)
    let ours = fit(
        (f64::from(CANVAS.0), f64::from(CANVAS.1)),
        (f64::from(size.0), f64::from(size.1)),
    );
    assert!(
        (f64::from(shown.2) - ours * f64::from(size.0)).abs() <= 1.0,
        "{what}: {shown:?} for a fit of {ours}"
    );
    assert_eq!(shown, start, "{what}: as at the start");
}

fn default_zoom(zooms: &Value, file: &str) -> f64 {
    zooms[file]["0"].as_f64().unwrap()
}

// leaf: audit-options-media-playback-per-filetype-handling-add
// leaf: audit-options-media-playback-per-filetype-handling-edit
// leaf: audit-options-media-playback-per-filetype-handling-delete
#[test]
fn filetype_handling_is_added_edited_and_deleted_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("filetype_handling.json");
    let client = Media::basic();
    let (start, size) = opened(&client);
    let recorded_start = default_zoom(&recording["start"]["zooms"], "small jpeg");
    assert!(
        recorded_start > 1.0,
        "a small jpeg is scaled up at the start"
    );
    assert!(start.2 > size.0);

    // the table at the start: no jpeg row, jpeg addable
    let options = client.open_options();
    show_page(&options, "media playback");
    assert_eq!(
        listed(&options).contains(&"image: jpeg".to_owned()),
        recording["start"]["jpeg_row"].as_bool().unwrap()
    );
    assert!(options.get_media_view_can_add());

    // add: the filetype chosen from those without a row, then its editor
    options.invoke_media_view_action("add".into());
    let (chooser, _) = hydrus_gui::options_media_views::open_children();
    let chooser = chooser.expect("the chooser opens");
    let choices: Vec<String> = chooser
        .get_choices()
        .iter()
        .map(|a| a.to_string())
        .collect();
    let add = &recording["phases"][0];
    let mut recorded: Vec<String> = add["asked"][0]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    recorded.sort();
    let mut ours = choices.clone();
    ours.sort();
    assert_eq!(add["asked"][0]["select"], "select the filetype to add");
    assert_eq!(ours, recorded, "the filetypes that can be added");
    let at = choices.iter().position(|c| c == "image: jpeg").unwrap();
    chooser.invoke_chosen(i32::try_from(at).unwrap());
    let (_, editor) = hydrus_gui::options_media_views::open_children();
    let editor = editor.expect("the editor opens");
    assert_eq!(
        editor.get_window_title(),
        add["asked"][1]["dialog"].as_str().unwrap()
    );
    // the editor opens on the general image options, as the reference's did
    assert_eq!(
        i64::from(editor.get_media_up()),
        add["asked"][1]["media_scale_up_before"].as_i64().unwrap()
    );
    editor.set_media_up(i32::try_from(add["scale_up"].as_i64().unwrap()).unwrap());
    editor.invoke_apply();
    assert!(listed(&options).contains(&"image: jpeg".to_owned()));
    assert!(add["row_listed"].as_bool().unwrap());
    // nothing until Options OK (the recording's zooms_before_ok are the start's)
    assert_eq!(
        default_zoom(&add["zooms_before_ok"], "small jpeg"),
        recorded_start
    );
    assert_eq!(opened(&client).0, start, "staged only");
    options.invoke_apply();
    options.hide().unwrap();
    let (shown, size) = opened(&client);
    check(
        shown,
        size,
        start,
        &recording,
        default_zoom(&add["zooms"], "small jpeg"),
        "added",
    );
    // (a big jpeg is shrunk to fit either way)
    assert_eq!(
        default_zoom(&add["zooms"], "big jpeg"),
        default_zoom(&recording["start"]["zooms"], "big jpeg")
    );

    // edit: the row chosen, its editor opening on what was saved
    let edit = &recording["phases"][1];
    let options = client.open_options();
    show_page(&options, "media playback");
    select_jpeg(&options);
    assert!(options.get_media_view_single());
    options.invoke_media_view_action("edit".into());
    let (_, editor) = hydrus_gui::options_media_views::open_children();
    let editor = editor.expect("the editor opens");
    assert_eq!(
        editor.get_window_title(),
        edit["asked"][0]["dialog"].as_str().unwrap()
    );
    assert_eq!(
        i64::from(editor.get_media_up()),
        edit["asked"][0]["media_scale_up_before"].as_i64().unwrap(),
        "opened on the saved 100%"
    );
    editor.set_media_up(i32::try_from(edit["scale_up"].as_i64().unwrap()).unwrap());
    editor.invoke_apply();
    options.invoke_apply();
    options.hide().unwrap();
    let (shown, size) = opened(&client);
    check(
        shown,
        size,
        start,
        &recording,
        default_zoom(&edit["zooms"], "small jpeg"),
        "edited",
    );

    // delete: the row selected and the question answered yes,
    // the filetype falling back to its class's
    let delete = &recording["phases"][2];
    assert!(delete["can_delete"].as_bool().unwrap());
    // (add the 100% back first so that deleting is seen to change it)
    let options = client.open_options();
    show_page(&options, "media playback");
    select_jpeg(&options);
    options.invoke_media_view_action("edit".into());
    let (_, editor) = hydrus_gui::options_media_views::open_children();
    let editor = editor.unwrap();
    editor.set_media_up(0);
    editor.invoke_apply();
    options.invoke_apply();
    options.hide().unwrap();
    assert_eq!(opened(&client).0.2, size.0, "at 100% again");
    let options = client.open_options();
    show_page(&options, "media playback");
    select_jpeg(&options);
    assert!(options.get_media_view_can_delete());
    options.invoke_media_view_action("delete".into());
    // the reference's list asks first; "no" keeps the row, "yes" removes it
    let (question, _) = hydrus_gui::options_media_views::open_children();
    let question = question.expect("the question");
    assert_eq!(
        question.get_message(),
        delete["asked"][0]["yes_no"].as_str().unwrap()
    );
    question.invoke_cancelled();
    assert!(listed(&options).contains(&"image: jpeg".to_owned()), "no");
    options.invoke_media_view_action("delete".into());
    let (question, _) = hydrus_gui::options_media_views::open_children();
    question.expect("asked again").invoke_chosen(0);
    assert_eq!(
        listed(&options).contains(&"image: jpeg".to_owned()),
        delete["row_listed"].as_bool().unwrap()
    );
    assert_eq!(opened(&client).0.2, size.0, "staged only");
    options.invoke_apply();
    options.hide().unwrap();
    let (shown, size) = opened(&client);
    check(
        shown,
        size,
        start,
        &recording,
        default_zoom(&delete["zooms"], "small jpeg"),
        "deleted",
    );
}
