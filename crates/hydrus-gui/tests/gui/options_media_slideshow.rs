//! The media viewer page's slideshow box against the real viewer: each
//! option is changed in the options window, applied, and the viewer's
//! slideshow menu and timing are what the saved values make them.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::media_viewer::SlideshowSettings;
use hydrus_gui::{MediaViewerWindow, mpv};

use crate::options_gui_support::{box_of, row, show_page};
use crate::options_media_support::Media;

const BOX: &str = "slideshows";

/// The slideshow submenu's rows of a viewer, by label: id and checked.
fn menu_rows(viewer: &MediaViewerWindow) -> Vec<(String, i32, bool)> {
    viewer.invoke_context_menu_requested();
    let menu = viewer.get_context_menu();
    let groups = &menu.slideshow;
    [&groups.g1, &groups.g2, &groups.g3, &groups.g4]
        .into_iter()
        .flat_map(|rows| {
            rows.iter()
                .map(|r| (r.label.to_string(), r.id, r.checkable && r.checked))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn open_viewer(client: &Media, at: usize) -> MediaViewerWindow {
    client.ui.invoke_thumbnail_activated(at as i32);
    client
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens")
}

fn keep(client: &Media, settings: SlideshowSettings) {
    client
        .store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
}

/// Set a number-that-may-be-none row, apply.
fn set_noneable(client: &Media, label: &str, phrase: &str, n: i32) {
    let options = client.open_options();
    show_page(&options, "media viewer");
    let (i, found) = row(&options, label);
    assert_eq!(
        (found.kind, found.none_phrase.as_str()),
        (3, phrase),
        "{label}"
    );
    assert_eq!(box_of(&options, label), BOX);
    options.invoke_number_edited(i, n);
    options.invoke_none_toggled(i, false);
    options.invoke_apply();
    options.hide().unwrap();
}

// leaf: audit-options-media-viewer-slideshows-slideshow-durations
#[test]
fn the_viewer_s_slideshow_menu_lists_the_saved_durations_and_once_through() {
    let client = Media::basic();
    client.search("system:everything");
    let viewer = open_viewer(&client, 0);
    let labels = |viewer: &MediaViewerWindow| -> Vec<String> {
        menu_rows(viewer).into_iter().map(|r| r.0).collect()
    };
    assert_eq!(
        labels(&viewer),
        [
            "1 second",
            "5 seconds",
            "10 seconds",
            "30 seconds",
            "1 minute",
            "very fast",
            "custom interval",
            "shuffle this slideshow",
            "all slideshows shuffle",
            "this slideshow plays media once through",
            "always play media once through"
        ]
    );
    assert!(menu_rows(&viewer).iter().all(|r| !r.2), "nothing checked");
    viewer.invoke_close_requested();

    let options = client.open_options();
    show_page(&options, "media viewer");
    let (d, found) = row(&options, "Slideshow durations:");
    assert_eq!(box_of(&options, "Slideshow durations:"), BOX);
    assert_eq!(found.text, "1.0,5.0,10.0,30.0,60.0");
    let (o, found) = row(&options, "Always play media once through before moving on:");
    assert_eq!((found.kind, found.checked), (1, false));
    options.invoke_text_edited(d, "2.5,7".into());
    options.invoke_check_toggled(o, true);
    // (the viewer opened now still has the saved ones)
    assert_eq!(
        client.setting::<SlideshowSettings>().durations,
        [1.0, 5.0, 10.0, 30.0, 60.0]
    );
    options.invoke_apply();
    options.hide().unwrap();

    let viewer = open_viewer(&client, 0);
    let rows = menu_rows(&viewer);
    assert_eq!(
        rows.iter()
            .map(|r| r.0.as_str())
            .take(4)
            .collect::<Vec<_>>(),
        ["2.5 seconds", "7 seconds", "very fast", "custom interval"]
    );
    assert!(
        rows.iter()
            .any(|r| r.0 == "always play media once through" && r.2),
        "checked: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|r| r.0 == "this slideshow plays media once through" && r.2),
        "a new viewer starts with it"
    );
    // choosing one of them starts a slideshow at it
    let id = rows.iter().find(|r| r.0 == "7 seconds").unwrap().1;
    viewer.invoke_menu_chosen(id);
    let rows = menu_rows(&viewer);
    assert_eq!(rows[0].0, "stop (7 seconds)");
}

/// When the real viewer's slideshow moved on from a real animated file, in
/// seconds from starting the slideshow and from the file being shown.
struct Moved {
    since_start: f64,
    since_shown: f64,
}

/// The real viewer's slideshow, timed on a real animated file: the seconds
/// from starting a slideshow of `period` seconds on it (`settle_ms` after the
/// file was shown) until the viewer moves to another file, with the options as
/// they are saved. A file mpv plays comes round to its end late, so a run
/// where that matters is given `patience`.
fn viewer_moves_on(
    client: &Media,
    gif_index: usize,
    period: f64,
    settle_ms: u64,
    patience: std::time::Duration,
) -> Moved {
    use std::time::{Duration, Instant};
    let viewer = open_viewer(client, gif_index);
    let shown = Instant::now();
    // (let the animation load and be shown before a slideshow starts)
    while shown.elapsed() < Duration::from_millis(settle_ms) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    let before = viewer.get_caption().to_string();
    let rows = menu_rows(&viewer);
    let id = rows.iter().find(|r| r.0 == "custom interval").unwrap().1;
    viewer.invoke_menu_chosen(id);
    let started = Instant::now();
    viewer.invoke_period_answered(true, period.to_string().into());
    let mut moved = None;
    while started.elapsed() < patience {
        slint::platform::update_timers_and_animations();
        if viewer.get_caption() != before {
            moved = Some(Moved {
                since_start: started.elapsed().as_secs_f64(),
                since_shown: shown.elapsed().as_secs_f64(),
            });
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    viewer.invoke_close_requested();
    moved.expect("the slideshow moves on")
}

/// The idle report mode and the debug sink are process-wide, and a viewer
/// running for seconds has its window's idle checks report to whatever sink
/// is set: the timing tests take turns with the debug-menu tests.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    crate::debug_menu_actions::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The patience of a run that waits on mpv's loop (as the playback tests').
const LOOP_PATIENCE: std::time::Duration = std::time::Duration::from_secs(90);
const QUICK: std::time::Duration = std::time::Duration::from_secs(10);

/// A store holding the basic fixture and an animated file (frame delays of
/// 100, 0, 10 and 250 ms, shown with the short ones raised to 100 ms as the
/// reference does: about half a second); its index in the results and its
/// duration in seconds as the viewer knows it.
fn client_with_animation() -> (Media, usize, f64) {
    let client = Media::basic();
    let importer =
        hydrus_import::FileImporter::new(client.store.clone(), hydrus_media::MediaTools::new());
    let imported = importer
        .import_path(
            &hydrus_testkit::fixture_path("media/gif_anim.gif"),
            &hydrus_import::FileImportOptions::default(),
        )
        .unwrap();
    let hash = imported.hash.expect("imported");
    let id = client
        .store
        .read(move |c| hydrus_store::master::hash_ids(c, &[hash]))
        .unwrap()
        .into_values()
        .next()
        .expect("a hash id");
    client.search("system:everything");
    let index = client
        .results()
        .iter()
        .position(|file| *file == id)
        .expect("the animation is in the results");
    let duration_ms = client
        .store
        .read(move |c| hydrus_store::media::load_basic(c, &[id]))
        .unwrap()[0]
        .info
        .as_ref()
        .and_then(|info| info.duration_ms)
        .expect("a duration");
    #[allow(clippy::cast_precision_loss)] // (milliseconds)
    let duration = duration_ms as f64 / 1000.0;
    assert!((0.4..0.7).contains(&duration), "{duration}");
    (client, index, duration)
}

/// The fastest of five tries of a slideshow that should move on early (a
/// loaded machine only ever delays one), in seconds from its start.
fn earliest(client: &Media, index: usize, period: f64) -> f64 {
    (0..5)
        .map(|_| viewer_moves_on(client, index, period, 300, QUICK).since_start)
        .fold(f64::INFINITY, f64::min)
}

/// A slideshow of `period` seconds on the half-second animation, with `preset`
/// saved, holds it until `before` (at least); with `label` set to `n` in the
/// options window it moves on at `after` at least (`None`: as the animation
/// ends) and clearly sooner than before. The late moments are bounded below
/// only (a delay can only make them later); the early one is the best of
/// five tries.
fn timed_threshold(
    preset: SlideshowSettings,
    label: &str,
    n: i32,
    period: f64,
    (before, after): (f64, Option<f64>),
) {
    if mpv::skip_without_libmpv() {
        return;
    }
    let _one = exclusive();
    let (client, index, duration) = client_with_animation();
    keep(&client, preset);
    let held = viewer_moves_on(&client, index, period, 300, LOOP_PATIENCE).since_start;
    assert!(
        held >= before,
        "as saved: {held} (expected {before} or later)"
    );
    set_noneable(&client, label, "do not use", n);
    let moved = earliest(&client, index, period);
    let after = after.unwrap_or(duration);
    assert!(
        moved >= after,
        "with {label} at {n}: {moved} (expected {after} or later)"
    );
    assert!(moved + 0.2 < held, "{moved} against {held}");
}

// leaf: audit-options-media-viewer-slideshows-slideshow-short-media-skip-seconds-threshold
#[test]
fn a_short_file_moves_a_slideshow_on_after_the_skip_seconds() {
    // a 2 second slideshow; with no percentage to shorten it, the animation
    // waits the period; with 1 second it moves on then
    timed_threshold(
        SlideshowSettings {
            short_loop_percentage: None,
            short_cutoff_percentage: None,
            short_loop_seconds: Some(10),
            ..SlideshowSettings::default()
        },
        "Slideshow short-media skip seconds threshold:",
        1,
        2.0,
        (2.0, Some(1.0)),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-short-media-skip-percentage-threshold
#[test]
fn a_short_file_moves_a_slideshow_on_after_the_skip_percentage_of_the_period() {
    // (it does not fit in 5% of 2 seconds; in 50% it plays on to 1 second)
    timed_threshold(
        SlideshowSettings {
            short_loop_percentage: Some(5),
            short_cutoff_percentage: None,
            short_loop_seconds: None,
            ..SlideshowSettings::default()
        },
        "Slideshow short-media skip percentage threshold:",
        50,
        2.0,
        (2.0, Some(1.0)),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-shorter-media-cutoff-percentage-threshold
#[test]
fn a_file_longer_than_the_cutoff_part_of_the_period_moves_on_when_it_has_played_once() {
    // (at 95% of 2 seconds the animation is not longer; at 10%, 0.2 seconds,
    // it is: it moves on as it ends)
    timed_threshold(
        SlideshowSettings {
            short_loop_percentage: None,
            short_cutoff_percentage: Some(95),
            short_loop_seconds: None,
            ..SlideshowSettings::default()
        },
        "Slideshow shorter-media cutoff percentage threshold:",
        10,
        2.0,
        (2.0, None),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-long-media-allowed-delay-percentage-threshold
#[test]
fn a_long_file_holds_a_slideshow_past_its_period_by_the_allowed_delay() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let _one = exclusive();
    // a 0.3 second slideshow moves on at 0.3; with a delay of 200% the
    // animation, which is longer, plays out first
    let (client, index, duration) = client_with_animation();
    keep(
        &client,
        SlideshowSettings {
            long_overspill_percentage: Some(10),
            ..SlideshowSettings::default()
        },
    );
    let early = earliest(&client, index, 0.3);
    assert!(early >= 0.3, "as saved: {early}");
    set_noneable(
        &client,
        "Slideshow long-media allowed delay percentage threshold:",
        "do not use",
        200,
    );
    let held = viewer_moves_on(&client, index, 0.3, 300, LOOP_PATIENCE).since_start;
    assert!(held >= duration, "with the delay at 200%: {held}");
    assert!(early + 0.1 < held, "{early} against {held}");
}

// leaf: audit-options-media-viewer-slideshows-always-play-media-once-through-before-moving-on
#[test]
fn a_slideshow_holds_an_animation_until_it_has_played_once_when_the_option_says() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let _one = exclusive();
    // a 0.1 second slideshow moves off the half-second animation at once; with
    // the option on it waits for the animation to play through (mpv comes
    // round to the end late, so that run is given a long patience)
    let (client, index, duration) = client_with_animation();
    // (started soon after the animation is shown, before it has played through)
    let early = (0..5)
        .map(|_| viewer_moves_on(&client, index, 0.1, 120, QUICK).since_shown)
        .fold(f64::INFINITY, f64::min);
    let options = client.open_options();
    show_page(&options, "media viewer");
    let (o, found) = row(&options, "Always play media once through before moving on:");
    assert_eq!((found.kind, found.checked), (1, false));
    options.invoke_check_toggled(o, true);
    options.invoke_apply();
    options.hide().unwrap();
    let held = viewer_moves_on(&client, index, 0.1, 120, LOOP_PATIENCE);
    assert!(
        held.since_shown >= duration - 0.1,
        "with the option on: {} after the file was shown (the animation lasts {duration}; the \
         moment taken as shown can come a little after mpv starts the clip)",
        held.since_shown
    );
    assert!(
        early + 0.1 < held.since_shown,
        "{early} against {}",
        held.since_shown
    );
}
