//! A ugoira and an animated WebP play in the media viewer with the
//! client's own player, as the reference plays them: frame after frame at
//! their timings, looping, and pausing.

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
        viewer.invoke_close_requested();
        assert!(bound.viewer.borrow().is_none());
    }
    // (the page's files are all still there)
    assert!(ui.get_thumbnail_rows().row_count() > 0);
}
