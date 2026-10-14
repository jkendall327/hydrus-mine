//! Options > media playback > system > "Allow loading of truncated images:"
//! through the real options window, the importer's thumbnail generation and
//! the media viewer, against the reference's recorded decoding of cut-off
//! files (oracle/record_image_decoder_policies.py).
use super::image_colour::{query_select, refresh_select, rgb, settle_viewport, wait};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use slint::{ComponentHandle as _, Model as _};

const LABEL: &str = "Allow loading of truncated images:";
const FILES: [&str; 4] = [
    "png-big-cut.png",
    "jpeg-big-cut.jpg",
    "gif-big-cut.gif",
    "bmp-big-cut.bmp",
];

fn recorded(file: &str, allowed: bool) -> Option<Vec<u8>> {
    let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["truncated"] == allowed)
        .unwrap();
    let row = case["decode"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file"] == file)
        .unwrap();
    row["ok"]
        .as_bool()
        .unwrap()
        .then(|| serde_json::from_value(row["pixels"].clone()).unwrap())
}

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "media playback")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}

fn checked(window: &OptionsWindow, row: i32) -> bool {
    window.get_rows().row_data(row as usize).unwrap().checked
}

fn stored(store: &hydrus_store::Store) -> bool {
    hydrus_store::reference_options::load_truncated_images(store)
}

/// A fresh store, its window and the options window: the box set (or left),
/// each cut-off file imported and opened in the viewer. What the viewer shows.
fn viewer_pixels(allowed: bool) -> Vec<(&'static str, Vec<u8>)> {
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    settle_viewport(&ui, &bound, &windows);

    // the reference's default is on, and the window shows it
    assert!(stored(&store));
    let (window, row) = options(&ui, &bound);
    assert!(checked(&window, row));
    if !allowed {
        // cancelled edits keep it on
        window.invoke_check_toggled(row, false);
        window.invoke_cancel();
        assert!(stored(&store));
        // applied, it is off, and shown so when the window is opened again
        let (window, row) = options(&ui, &bound);
        assert!(checked(&window, row), "shown again as saved");
        window.invoke_check_toggled(row, false);
        window.invoke_apply();
        let (again, row) = options(&ui, &bound);
        assert!(!checked(&again, row), "shown again as saved");
        again.invoke_cancel();
    }
    assert_eq!(stored(&store), allowed);

    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let mut ids = Vec::new();
    for name in FILES {
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("image_decoder_policies/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        // (as in the reference, a picture whose thumbnail cannot be made is
        // still imported, with the default thumbnail)
        assert_eq!(
            result.status,
            hydrus_import::ImportStatus::SuccessfulAndNew,
            "{name}"
        );
        let id = store
            .read(|c| hydrus_store::master::hash_id(c, &result.hash.unwrap()))
            .unwrap()
            .unwrap();
        ids.push((name, id));
        let thumbnail = store
            .snapshot()
            .storage
            .thumbnail_path(&result.hash.unwrap())
            .filter(|p| p.is_file());
        if allowed {
            // the thumbnail is the picture that loaded (JPEG-compressed, so
            // near it, not equal)
            let bytes = std::fs::read(thumbnail.expect("a thumbnail")).unwrap();
            let thumb = hydrus_media::decode_image(&bytes).unwrap();
            let want = recorded(name, true).unwrap();
            assert_eq!((thumb.width(), thumb.height()), (64, 64), "{name}");
            let total: u64 = thumb
                .data()
                .iter()
                .zip(&want)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(
                total / (want.len() as u64) < 40,
                "{name}: {}",
                total / want.len() as u64
            );
        } else {
            // none could be made, so the client draws the default one
            assert!(thumbnail.is_none(), "{name}");
        }
    }
    let mut shown = Vec::new();
    for (n, (name, id)) in ids.iter().copied().enumerate() {
        let index = if n == 0 {
            query_select(&ui, &bound, id)
        } else {
            refresh_select(&ui, &bound, id)
        };
        let want = recorded(name, true).unwrap();
        if !allowed {
            ui.invoke_thumbnail_clicked(index, false, false);
        }
        if allowed {
            ui.invoke_thumbnail_activated(index);
            let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
            wait(|| rgb(&viewer.get_media()) == want);
            shown.push((name, rgb(&viewer.get_media())));
            viewer.invoke_close_requested();
        } else {
            // (the reference refuses the file: no picture of it is loaded;
            // the viewer opens, and for as long as a load takes -- the
            // loads above took a few milliseconds -- has nothing to show)
            assert!(recorded(name, false).is_none(), "{name}");
            ui.invoke_thumbnail_activated(index);
            let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
            let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while std::time::Instant::now() < until {
                slint::platform::update_timers_and_animations();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(rgb(&viewer.get_media()).is_empty(), "{name}");
            viewer.invoke_close_requested();
            shown.push((name, Vec::new()));
        }
    }
    if allowed {
        // saved off, the pictures already viewed are dropped from the image
        // cache (the reference clears its image caches when the option
        // changes), so the viewer no longer shows the cut-off picture: only
        // the thumbnail, which is near it and not it
        let (window, row) = options(&ui, &bound);
        window.invoke_check_toggled(row, false);
        window.invoke_apply();
        assert!(!stored(&store));
        for (name, id) in ids {
            let index = refresh_select(&ui, &bound, id);
            ui.invoke_thumbnail_activated(index);
            let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
            let want = recorded(name, true).unwrap();
            wait(|| !rgb(&viewer.get_media()).is_empty());
            assert_ne!(rgb(&viewer.get_media()), want, "{name}");
            viewer.invoke_close_requested();
        }
    }
    shown
}

// leaf: audit-options-media-playback-system-allow-loading-of-truncated-images
#[test]
fn by_default_cut_off_pictures_are_imported_and_the_viewer_shows_what_loaded() {
    // (each is the picture the reference loads: what was there, the rest as
    // Pillow fills it)
    let shown = viewer_pixels(true);
    assert_eq!(shown.len(), FILES.len());
}

// leaf: audit-options-media-playback-system-allow-loading-of-truncated-images
#[test]
fn with_the_option_saved_off_cut_off_pictures_are_not_loaded() {
    // (the reference refuses them: the viewer has no picture of them to show)
    let shown = viewer_pixels(false);
    assert_eq!(shown.len(), FILES.len());
}
