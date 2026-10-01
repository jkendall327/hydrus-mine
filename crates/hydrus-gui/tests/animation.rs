//! A ugoira and an animated WebP play in the media viewer with the
//! client's own player, as the reference plays them: frame after frame at
//! their timings, looping, and pausing; and their scanbar shows the frame
//! and goes to the one clicked.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use slint::Model as _;

use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// The viewer's picture: its size and pixels.
fn picture(viewer: &MediaViewerWindow) -> Option<((u32, u32), Vec<u8>)> {
    let pixels = viewer.get_media().to_rgba8()?;
    Some((
        (pixels.width(), pixels.height()),
        pixels.as_bytes().to_vec(),
    ))
}

/// Wait (a while at most) until the scanbar's text starts `wanted` and
/// stays so past its next update (the text from before a seek may read so
/// for a moment), and say what it is.
fn scanbar_reaches(viewer: &MediaViewerWindow, wanted: &str) -> String {
    let started = Instant::now();
    loop {
        watch(viewer, Duration::from_millis(20));
        let timed_out = started.elapsed() > Duration::from_secs(10);
        if viewer.get_scanbar_text().starts_with(wanted) {
            watch(viewer, Duration::from_millis(120));
            if viewer.get_scanbar_text().starts_with(wanted) || timed_out {
                return viewer.get_scanbar_text().to_string();
            }
        } else if timed_out {
            return viewer.get_scanbar_text().to_string();
        }
    }
}

/// The distinct pictures shown over `time`, as the event loop would
/// show them.
fn watch(viewer: &MediaViewerWindow, time: Duration) -> HashSet<((u32, u32), Vec<u8>)> {
    let mut seen = HashSet::new();
    let started = Instant::now();
    while started.elapsed() < time {
        slint::platform::update_timers_and_animations();
        seen.extend(picture(viewer));
        std::thread::sleep(Duration::from_millis(5));
    }
    seen
}

#[test]
fn animations_play_in_the_viewer_with_the_client_s_own_player() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let mut hashes = Vec::new();
    for name in ["ugoira_json.zip", "webp_anim.webp"] {
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("media/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
        hashes.push((name, result.hash.unwrap()));
    }

    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    for (name, hash) in hashes {
        let id = store
            .read(|c| hydrus_store::master::hash_id(c, &hash))
            .unwrap()
            .unwrap();
        let index = results.iter().position(|&r| r == id).unwrap();
        ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
        let viewer = bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opened");
        // (frames of 60 to 100ms, looping)
        let shown = watch(&viewer, Duration::from_millis(1500));
        assert!(shown.len() >= 3, "{name}: {} pictures", shown.len());
        let sizes: HashSet<(u32, u32)> = shown.iter().map(|p| p.0).collect();
        assert_eq!(sizes.len(), 1, "{name}: {sizes:?}");
        // paused, the picture stays; resumed, it moves on
        viewer.invoke_toggle_pause();
        watch(&viewer, Duration::from_millis(50));
        assert_eq!(
            watch(&viewer, Duration::from_millis(400)).len(),
            1,
            "{name}"
        );
        viewer.invoke_toggle_pause();
        assert!(
            watch(&viewer, Duration::from_millis(1000)).len() >= 2,
            "{name}"
        );
        // its scanbar: by frame; a drag pauses, goes to the frame under
        // the pointer, and resumes when let go
        assert!(viewer.get_scanbar_shown(), "{name}");
        watch(&viewer, Duration::from_millis(200));
        let text = viewer.get_scanbar_text().to_string();
        assert!(
            text.contains("/5 - ") || text.contains("/3 - "),
            "{name}: {text}"
        );
        let frames: usize = text.split(['/', ' ']).nth(1).unwrap().parse().unwrap();
        let width = 210.0;
        let x = 5.0 + 0.75 * (width - 10.0);
        let target = (0.75 * (frames - 1) as f64 + 0.5) as usize;
        viewer.invoke_scan_started();
        viewer.invoke_scan(x, width);
        // (once the frame is decoded and shown)
        // (back and forth: each seek lands on its frame, however the
        // decoder's thread runs meanwhile)
        for round in 0..20 {
            let (to, wanted) = if round % 2 == 0 {
                (5.0, 1)
            } else {
                (x, target + 1)
            };
            viewer.invoke_scan(to, width);
            let wanted = format!("{wanted}/{frames} - ");
            let got = scanbar_reaches(&viewer, &wanted);
            assert!(got.starts_with(&wanted), "{name} round {round}: {got}");
        }
        viewer.invoke_scan(x, width);
        let there = scanbar_reaches(&viewer, &format!("{}/{frames} - ", target + 1));
        assert!(
            there.starts_with(&format!("{}/{frames} - ", target + 1)),
            "{name}: {there}"
        );
        if name.starts_with("ugoira") {
            // (60, 70 and 80ms before the fourth frame)
            assert_eq!(there, "4/5 - 0.210/0.400");
        }
        assert_eq!(
            watch(&viewer, Duration::from_millis(300)).len(),
            1,
            "{name}: paused"
        );
        assert_eq!(viewer.get_scanbar_text(), there.as_str());
        // seeking by time (ctrl and the arrows), still paused: to the frame
        // showing then, or the next if that is this one; past the end, the
        // first
        viewer.invoke_seek_delta(1, 1);
        let next = format!("{}/{frames} - ", (target + 1) % frames + 1);
        assert!(scanbar_reaches(&viewer, &next).starts_with(&next), "{name}");
        if name.starts_with("ugoira") {
            // (frames of 60, 70, 80, 90 and 100ms)
            for ((direction, step), wanted) in [
                ((-1, 50), "4/5 - 0.210/0.400"),
                ((-1, 10), "3/5 - 0.130/0.400"),
                ((1, 10), "4/5 - 0.210/0.400"),
                ((1, 100), "5/5 - 0.300/0.400"),
                ((-1, 2500), "1/5 - 0.000/0.400"),
                ((-1, 1), "1/5 - 0.000/0.400"),
                ((1, 140), "3/5 - 0.130/0.400"),
                ((1, 5000), "1/5 - 0.000/0.400"),
            ] {
                viewer.invoke_seek_delta(direction, step);
                assert_eq!(
                    scanbar_reaches(&viewer, wanted),
                    wanted,
                    "{direction} {step}"
                );
            }
        }
        assert_eq!(
            watch(&viewer, Duration::from_millis(200)).len(),
            1,
            "{name}: still paused"
        );
        viewer.invoke_scan_ended();
        assert!(
            watch(&viewer, Duration::from_millis(1000)).len() >= 2,
            "{name}: playing again"
        );
        viewer.invoke_close_requested();
        assert!(bound.viewer.borrow().is_none());
    }
    // (the page's files are all still there)
    assert!(ui.get_thumbnail_rows().row_count() > 0);
}
