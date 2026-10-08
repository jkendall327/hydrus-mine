//! The media viewer page's slideshow box against the real viewer: each
//! option is changed in the options window, applied, and the viewer's
//! slideshow menu and timing are what the saved values make them.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::media_viewer::SlideshowSettings;
use hydrus_gui::MediaViewerWindow;
use hydrus_gui::slideshow::{Shown, Slideshow};

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
// leaf: audit-options-media-viewer-slideshows-always-play-media-once-through-before-moving-on
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

/// When (in tenths of a second, from its start) a slideshow of `period`
/// seconds moves on from a half-second animation, with the options as the
/// store holds them: the viewer's own `Slideshow`, started with the saved
/// options (the viewer reads them as it starts one and as it moves on).
fn moves_on_at(client: &Media, period: f64) -> u32 {
    let settings = hydrus_gui::slideshow::settings(&client.store);
    let mut slideshow = Slideshow::new(&settings);
    slideshow.start(period, 0.0, Shown::Playing(Some(0.5)), &settings);
    (0..1000)
        .find(|tenths_of_hundredths| {
            slideshow.due(
                f64::from(*tenths_of_hundredths) / 100.0,
                Shown::Playing(Some(0.5)),
                true,
            )
        })
        .expect("it moves on")
}

/// The slideshow of `period` seconds: `preset` saved, it moves on at
/// `before` hundredths of a second; `label` set to `n` in the options
/// window, at `after`.
fn threshold(
    preset: SlideshowSettings,
    label: &str,
    n: i32,
    period: f64,
    (before, after): (u32, u32),
) {
    let client = Media::basic();
    keep(&client, preset);
    // (the period is exclusive: due when past it, at the next hundredth)
    assert_eq!(moves_on_at(&client, period), before + 1, "as saved");
    set_noneable(&client, label, "do not use", n);
    assert_eq!(
        moves_on_at(&client, period),
        after + 1,
        "with {label} at {n}"
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-short-media-skip-seconds-threshold
#[test]
fn a_short_file_moves_a_slideshow_on_after_the_skip_seconds() {
    // a 4 second slideshow; with no percentage to shorten it, the
    // half-second animation waits the period; with 2 seconds it moves on then
    threshold(
        SlideshowSettings {
            short_loop_percentage: None,
            short_cutoff_percentage: None,
            short_loop_seconds: Some(10),
            ..SlideshowSettings::default()
        },
        "Slideshow short-media skip seconds threshold:",
        2,
        4.0,
        (400, 200),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-short-media-skip-percentage-threshold
#[test]
fn a_short_file_moves_a_slideshow_on_after_the_skip_percentage_of_the_period() {
    // (it fits in 20% of 4 seconds; in 50% it plays on to 2 seconds)
    threshold(
        SlideshowSettings {
            short_loop_percentage: Some(20),
            short_cutoff_percentage: None,
            short_loop_seconds: None,
            ..SlideshowSettings::default()
        },
        "Slideshow short-media skip percentage threshold:",
        50,
        4.0,
        (80, 200),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-shorter-media-cutoff-percentage-threshold
#[test]
fn a_file_longer_than_the_cutoff_part_of_the_period_moves_on_when_it_has_played_once() {
    // (at 10% of 4 seconds, 0.4, the half-second file is longer: it moves
    // on as it ends)
    threshold(
        SlideshowSettings {
            short_loop_percentage: None,
            short_cutoff_percentage: Some(75),
            short_loop_seconds: None,
            ..SlideshowSettings::default()
        },
        "Slideshow shorter-media cutoff percentage threshold:",
        10,
        4.0,
        (400, 50),
    );
}

// leaf: audit-options-media-viewer-slideshows-slideshow-long-media-allowed-delay-percentage-threshold
#[test]
fn a_long_file_holds_a_slideshow_past_its_period_by_the_allowed_delay() {
    // a 0.3 second slideshow moves on at 0.3; with a delay of 100% the
    // half-second animation plays out first
    threshold(
        SlideshowSettings {
            long_overspill_percentage: Some(10),
            ..SlideshowSettings::default()
        },
        "Slideshow long-media allowed delay percentage threshold:",
        100,
        0.3,
        (30, 50),
    );
}
