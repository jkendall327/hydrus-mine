//! "Memory for video buffer" (`video_buffer.json`): the options row and its
//! live estimate, as the reference's speed and memory panel has them; and
//! the viewer's animation player keeping as many frames as it says, so a
//! loop that fits is decoded once and plays round again without decoding,
//! while one that doesn't is decoded again, as the reference's
//! `RasterContainerVideo` does.

use std::time::{Duration, Instant};

use hydrus_gui::{Bound, MainWindow, MediaViewerWindow, OptionsWindow, Pages, SearchPage, bind};
use hydrus_gui::{frames_decoded, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::reference_options::ReferenceOptions;
use hydrus_store::settings;
use slint::{ComponentHandle as _, Model as _};

const LABEL: &str = "Memory for video buffer: ";

fn options(ui: &MainWindow, bound: &Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}

/// The row's amount, unit (as the unit list's index) and estimate.
fn shown(window: &OptionsWindow, row: i32) -> (i32, i32, String) {
    let shown = window.get_rows().row_data(row as usize).unwrap();
    assert_eq!(shown.kind, 33);
    (shown.number, shown.index, shown.text.to_string())
}

/// The reference's unit, as the unit list's index.
fn unit(bytes: u64) -> i32 {
    i32::try_from(bytes.ilog(1024)).unwrap()
}

fn saved(store: &Store) -> i64 {
    store
        .read(settings::get::<ReferenceOptions>)
        .unwrap()
        .integer("video_buffer_size")
}

/// The frame the viewer's scanbar says is shown (from 0).
fn frame(viewer: &MediaViewerWindow) -> Option<usize> {
    let text = viewer.get_scanbar_text();
    let shown: usize = text.split('/').next()?.trim().parse().ok()?;
    shown.checked_sub(1)
}

/// Open the viewer on `file`, let it play round from its last frame to its
/// first `loops` times, and close it; how many frames it decoded.
fn play(ui: &MainWindow, bound: &Bound, file: usize, count: usize, loops: usize) -> u64 {
    let before = frames_decoded();
    ui.invoke_thumbnail_activated(i32::try_from(file).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opened");
    let (mut last, mut round, mut seen) = (None, 0, 0);
    let started = Instant::now();
    while round < loops {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{round} loops, frame {last:?}"
        );
        slint::platform::update_timers_and_animations();
        let now = frame(&viewer);
        if now != last {
            if now == Some(0) && last == Some(count - 1) {
                round += 1;
            }
            seen += 1;
            last = now;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(seen > count * loops, "every frame shown: {seen}");
    // (and its first frame shown again, decoded or not)
    std::thread::sleep(Duration::from_millis(100));
    slint::platform::update_timers_and_animations();
    viewer.invoke_close_requested();
    assert!(bound.viewer.borrow().is_none());
    frames_decoded() - before
}

// leaf: audit-options-speed-and-memory-video-buffer-memory-for-video-buffer
#[test]
fn the_video_buffer_option_sizes_the_animation_buffer_and_a_loop_that_fits_is_decoded_once() {
    let fixture = hydrus_testkit::fixture_json("video_buffer.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // the recorded nine-frame WebP, imported
    let animation = native.path().join("nine-frames.webp");
    std::fs::write(
        &animation,
        hex::decode(fixture["animation"]["bytes"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let hash = FileImporter::new(store.clone(), MediaTools::new())
        .import_path(&animation, &FileImportOptions::default())
        .unwrap()
        .hash
        .unwrap();
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let count = fixture["animation"]["durations"].as_array().unwrap().len();

    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|&r| r == id)
        .unwrap();

    // the row as it opens: the default, 96MB, about 36 frames of 720p
    let panel = &fixture["panel"];
    let (window, row) = options(&ui, &bound);
    let default = &panel["default"];
    let displayed = default["displayed"].as_array().unwrap();
    assert_eq!(
        shown(&window, row),
        (
            i32::try_from(displayed[0].as_i64().unwrap()).unwrap(),
            unit(displayed[1].as_u64().unwrap()),
            default["text"].as_str().unwrap().to_owned()
        )
    );
    assert_eq!(saved(&store), default["value"].as_i64().unwrap());
    // the estimate follows the amount and unit as they are typed
    for edit in panel["edits"].as_array().unwrap() {
        window.invoke_number_edited(
            row,
            i32::try_from(edit["amount"].as_i64().unwrap()).unwrap(),
        );
        window.invoke_choice_chosen(row, unit(edit["unit"].as_u64().unwrap()));
        assert_eq!(
            shown(&window, row).2,
            edit["text"].as_str().unwrap(),
            "{edit}"
        );
    }
    // cancelled, nothing is saved
    window.invoke_cancel();
    assert_eq!(saved(&store), default["value"].as_i64().unwrap());

    // with the default, the whole loop is kept: decoded once, and played
    // round again without decoding
    let loops = fixture["loops"].as_array().unwrap();
    let fits = &loops[0];
    assert_eq!(fits["buffer"], default["value"]);
    assert_eq!(fits["total_decoded"].as_u64().unwrap(), count as u64);
    assert_eq!(play(&ui, &bound, file, count, 2), count as u64);

    // a buffer of six frames (four behind, two ahead) doesn't hold it: the
    // next file opened decodes them again each time round
    let small = loops.iter().find(|l| l["buffer"] == 8640).unwrap();
    assert!(small["total_decoded"].as_u64().unwrap() > count as u64);
    let (window, row) = options(&ui, &bound);
    window.invoke_number_edited(row, 8640);
    window.invoke_choice_chosen(row, 0);
    assert_eq!(
        shown(&window, row),
        (8640, 0, "(about 0 frames of 720p video)".to_owned())
    );
    window.invoke_apply();
    assert_eq!(saved(&store), 8640);
    let decoded = play(&ui, &bound, file, count, 2);
    assert!(decoded > count as u64 * 2, "decoded {decoded}");

    // reopened, the row shows what was saved; set back to one that holds
    // the loop, the next file opened keeps it all again
    let (window, row) = options(&ui, &bound);
    assert_eq!(
        shown(&window, row),
        (8640, 0, "(about 0 frames of 720p video)".to_owned())
    );
    let holds = loops.iter().find(|l| l["buffer"] == 17280).unwrap();
    assert_eq!(holds["total_decoded"].as_u64().unwrap(), count as u64);
    window.invoke_number_edited(row, 17280);
    window.invoke_apply();
    assert_eq!(saved(&store), 17280);
    assert_eq!(play(&ui, &bound, file, count, 2), count as u64);
}
