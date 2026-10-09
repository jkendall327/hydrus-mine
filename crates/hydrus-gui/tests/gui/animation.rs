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

/// The texts the scanbar shows over time while playing, as they change, each
/// with when it first showed and where it falls in `expected` (the earliest
/// place after the last): until the last of `expected` is seen, or ten
/// seconds. A text that fits nowhere is kept, with no place.
fn texts_while_playing(
    viewer: &MediaViewerWindow,
    expected: &[String],
) -> Vec<(String, Instant, Option<usize>)> {
    let mut seen: Vec<(String, Instant, Option<usize>)> = Vec::new();
    let mut next = 0;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        slint::platform::update_timers_and_animations();
        let text = viewer.get_scanbar_text().to_string();
        if !text.is_empty() && seen.last().is_none_or(|(last, ..)| *last != text) {
            let place = expected[next..]
                .iter()
                .position(|e| *e == text)
                .map(|at| at + next);
            if let Some(place) = place {
                next = place + 1;
            }
            seen.push((text, Instant::now(), place));
            if place == Some(expected.len() - 1) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    seen
}

/// Wait until the scanbar shows `wanted` and keeps it.
fn reaches(viewer: &MediaViewerWindow, wanted: &str, what: &str) {
    let got = scanbar_reaches(viewer, wanted);
    assert_eq!(got, wanted, "{what}");
}

/// Nothing changes on the bar for a moment: the player is paused.
fn stays(viewer: &MediaViewerWindow, text: &str, what: &str) {
    assert_eq!(
        watch(viewer, Duration::from_millis(250)).len(),
        1,
        "{what}: the picture moved"
    );
    assert_eq!(viewer.get_scanbar_text(), text, "{what}");
}

fn as_str_vec(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["text"].as_str().unwrap().to_owned())
        .collect()
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

    let recording = hydrus_testkit::fixture_json("animation_playback.json");
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
        let case = &recording[if name.starts_with("ugoira") {
            "ugoira"
        } else {
            "webp"
        }];
        // from the first frame, round twice: the bar's text frame after
        // frame as the reference's player shows them, each for (at least
        // most of) its duration
        let timeline = case["timeline"].as_array().unwrap();
        let mut expected = vec![case["initial"]["text"].as_str().unwrap().to_owned()];
        expected.extend(as_str_vec(&case["timeline"]));
        let mut indexes = vec![0];
        indexes.extend(
            timeline
                .iter()
                .map(|step| step["index"].as_u64().unwrap() as usize),
        );
        // (in order; a poll the machine was too slow for may miss a text,
        // but none comes out of order or is not the reference's)
        let seen = texts_while_playing(&viewer, &expected);
        let unplaced: Vec<&str> = seen
            .iter()
            .filter(|(_, _, place)| place.is_none())
            .map(|(text, ..)| text.as_str())
            .collect();
        assert!(
            unplaced.is_empty(),
            "{name}: not the reference's, or out of order: {unplaced:?}"
        );
        assert!(
            seen.len() * 4 >= expected.len() * 3
                && seen.last().unwrap().2 == Some(expected.len() - 1),
            "{name}: {} of {} texts seen",
            seen.len(),
            expected.len()
        );
        // (the bar's text lags a frame by a poll, so the time is taken over
        // the whole run, from the second text seen to the last, by the
        // frames the reference shows between them)
        let durations = case["durations_ms"].as_array().unwrap();
        let (from, to) = (seen[1].2.unwrap(), seen.last().unwrap().2.unwrap());
        let due: f64 = indexes[from..to]
            .iter()
            .map(|&index| durations[index].as_f64().unwrap())
            .sum();
        let took = seen
            .last()
            .unwrap()
            .1
            .duration_since(seen[1].1)
            .as_secs_f64()
            * 1000.0;
        assert!(took >= due * 0.85, "{name}: {took}ms of {due}");
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
        // its scanbar, driven as the user does and compared with the
        // reference's own Animation and AnimationBar on a frozen clock
        // (oracle/record_animation_playback.py)
        assert!(viewer.get_scanbar_shown(), "{name}");
        let frames = case["num_frames"].as_u64().unwrap() as usize;
        let bar_width = case["bar_width"].as_f64().unwrap() as f32;
        // (paused from here)
        viewer.invoke_toggle_pause();
        watch(&viewer, Duration::from_millis(100));
        // a press, a drag and a release at each position: the frame under
        // the pointer, and playing again if it was before
        let scans = case["scan"].as_array().unwrap();
        for paused_before in [true, false] {
            if !paused_before {
                viewer.invoke_toggle_pause();
            }
            for scan in scans
                .iter()
                .filter(|scan| scan["start_paused"].as_bool().unwrap() == paused_before)
            {
                let x = scan["x"].as_f64().unwrap() as f32;
                let wanted = scan["pressed"]["text"].as_str().unwrap();
                viewer.invoke_scan_started();
                viewer.invoke_scan(x, bar_width);
                reaches(
                    &viewer,
                    wanted,
                    &format!("{name} scan {x} paused {paused_before}"),
                );
                // (a drag pauses playing while it lasts)
                stays(&viewer, wanted, &format!("{name} drag {x}"));
                viewer.invoke_scan_ended();
                if paused_before {
                    stays(
                        &viewer,
                        wanted,
                        &format!("{name} released {x} still paused"),
                    );
                }
            }
            if !paused_before {
                // (let go while playing: it plays on)
                assert!(
                    watch(&viewer, Duration::from_millis(1000)).len() >= 2,
                    "{name}: playing again after the drag"
                );
                viewer.invoke_toggle_pause();
                watch(&viewer, Duration::from_millis(100));
            }
        }
        // seeking by time (ctrl and the arrows), paused: to the frame
        // showing then, or the next if that is this one; past the end,
        // round to the first
        let seeks = case["seek_delta"].as_array().unwrap();
        let goto = |frame: usize| {
            let x = 5.0 + (frame as f32 / (frames - 1) as f32) * (bar_width - 10.0);
            viewer.invoke_scan_started();
            viewer.invoke_scan(x, bar_width);
            viewer.invoke_scan_ended();
        };
        // going to frames one after another, paused, as the reference's
        // `GotoFrame` does (to the same frame, the ends, round again): each
        // shows its frame and leaves it paused. (The recorded goes made
        // while playing are the bar's drag and the frame step, replayed
        // above and below, and a seek, which leaves it playing.)
        for step in case["goto"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| !step["play_first"].as_bool().unwrap())
        {
            goto(step["index"].as_u64().unwrap() as usize);
            let wanted = step["text"].as_str().unwrap();
            reaches(&viewer, wanted, &format!("{name} go to {}", step["index"]));
            stays(
                &viewer,
                wanted,
                &format!("{name} paused at {}", step["index"]),
            );
        }
        goto(frames - 2);
        let start = seeks[0]["start"]["text"].as_str().unwrap();
        reaches(&viewer, start, &format!("{name} before seeking"));
        for step in seeks[0]["steps"].as_array().unwrap() {
            let (direction, ms) = (
                step["direction"].as_i64().unwrap() as i32,
                step["ms"].as_i64().unwrap() as i32,
            );
            viewer.invoke_seek_delta(direction, ms);
            reaches(
                &viewer,
                step["text"].as_str().unwrap(),
                &format!("{name} seek {direction} {ms}"),
            );
        }
        let last = seeks[0]["steps"].as_array().unwrap().last().unwrap();
        stays(
            &viewer,
            last["text"].as_str().unwrap(),
            &format!("{name} still paused"),
        );
        // ctrl+b and ctrl+n: a frame back or on, round the ends
        goto(0);
        reaches(&viewer, case["initial"]["text"].as_str().unwrap(), name);
        let wanted = as_str_vec(&case["frame_step"]);
        for (step, wanted) in case["frame_step"].as_array().unwrap().iter().zip(wanted) {
            let direction = step["direction"].as_i64().unwrap() as i32;
            viewer.invoke_frame_step(direction);
            reaches(&viewer, &wanted, &format!("{name} frame step {direction}"));
        }
        // a seek while playing leaves it playing; a frame step pauses it
        // (`GotoFrame`'s `pause_afterwards`), as the reference's do
        viewer.invoke_toggle_pause();
        viewer.invoke_seek_delta(1, 1);
        assert!(
            watch(&viewer, Duration::from_millis(1000)).len() >= 2,
            "{name}: seeking leaves it playing"
        );
        viewer.invoke_frame_step(1);
        watch(&viewer, Duration::from_millis(100));
        let shown = viewer.get_scanbar_text().to_string();
        stays(&viewer, &shown, &format!("{name} a frame step pauses"));
        assert!(
            frames > 1
                && case["frame_step"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["paused"].as_bool().unwrap()),
            "{name}: the reference's frame step pauses"
        );
        viewer.invoke_toggle_pause();
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
