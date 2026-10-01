//! A file's ratings in the media viewer's top-right hover frame, as the
//! reference has them: like/dislike, numerical and inc/dec services, drawn
//! in their shapes and colours, and set by clicking as its controls are.

// (outlines are the reference's exact arithmetic)
#![allow(clippy::float_cmp)]

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::ratings::{self, Kind, left_click, right_click, stars_at};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::services::{NumericalRatingConfig, RatingDisplay, StarAppearance, StarShape};
use slint::ComponentHandle as _;
use slint::Model as _;
use slint::platform::{PointerEventButton, WindowEvent};

fn numerical(num_stars: u32, allow_zero: bool) -> NumericalRatingConfig {
    NumericalRatingConfig {
        display: RatingDisplay::default(),
        appearance: StarAppearance::default(),
        num_stars,
        allow_zero,
        custom_pad: 4,
        show_fraction_beside_stars: 0,
    }
}

#[test]
fn clicks_along_stars_set_them_as_the_reference_s_do() {
    // allowing zero: rounded, halves to even
    let zero = numerical(5, true);
    assert_eq!(stars_at(&zero, 0.0), 0);
    assert_eq!(stars_at(&zero, 0.5), 2);
    assert_eq!(stars_at(&zero, 0.7), 4);
    assert_eq!(stars_at(&zero, 1.0), 5);
    // not: the star clicked on, from the first
    let one = numerical(5, false);
    assert_eq!(stars_at(&one, 0.0), 1);
    assert_eq!(stars_at(&one, 0.5), 3);
    assert_eq!(stars_at(&one, 0.99), 5);
    assert_eq!(stars_at(&one, 1.0), 5);
    // shapes: each of the reference's, a named SVG as the fat star
    let fat = ratings::shape_path(&StarAppearance::default());
    assert_eq!(ratings::shape_path(&StarAppearance::Svg("x".into())), fat);
    for code in [
        0, 1, 3, 4, 5, 6, 7, 30, 31, 32, 33, 40, 42, 43, 44, 50, 60, 61, 101, 102, 103,
    ] {
        let path = ratings::shape_path(&StarAppearance::Shape(StarShape(code)));
        assert_ne!(path, fat, "{code}");
    }
    assert_eq!(
        ratings::shape_path(&StarAppearance::Shape(StarShape(99))),
        fat
    );
    assert_eq!(ratings::outline_width(12.0), 1.0);
    assert_eq!(ratings::outline_width(30.0), 2.5);
    assert_eq!(ratings::outline_width(100.0), 4.0);
}

fn store() -> (tempfile::TempDir, tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    (legacy, native, store)
}

fn kinds(store: &Store, file: HashId) -> Vec<Kind> {
    ratings::controls(store, file)
        .into_iter()
        .map(|c| c.kind)
        .collect()
}

#[test]
fn a_file_s_ratings_are_set_by_clicks() {
    let (_legacy, _native, store) = store();
    let mut page = SearchPage::new(store.clone());
    page.enter();
    let file = page.results()[0];
    // the fixture's services: a like/dislike, a numerical of five stars
    // allowing zero, and an inc/dec
    let controls = ratings::controls(&store, file);
    let names: Vec<&str> = controls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["favourites", "stars", "counter"]);
    let like = |s: &Store| ratings::controls(s, file).remove(0);
    let stars = |s: &Store| ratings::controls(s, file).remove(1);
    let counter = |s: &Store| ratings::controls(s, file).remove(2);
    // like: left likes, again unrates; right dislikes, again unrates
    right_click(&store, file, &like(&store)).unwrap();
    right_click(&store, file, &like(&store)).unwrap();
    assert!(matches!(like(&store).kind, Kind::Like { state: None, .. }));
    left_click(&store, file, &like(&store), 0.5).unwrap();
    assert!(matches!(
        like(&store).kind,
        Kind::Like {
            state: Some(true),
            ..
        }
    ));
    left_click(&store, file, &like(&store), 0.5).unwrap();
    assert!(matches!(like(&store).kind, Kind::Like { state: None, .. }));
    right_click(&store, file, &like(&store)).unwrap();
    assert!(matches!(
        like(&store).kind,
        Kind::Like {
            state: Some(false),
            ..
        }
    ));
    // numerical: the stars clicked on; right unrates
    left_click(&store, file, &stars(&store), 0.7).unwrap();
    assert!(matches!(
        stars(&store).kind,
        Kind::Numerical { stars: Some(4), .. }
    ));
    let shapes = stars(&store).shapes();
    let colours = stars(&store).colours;
    assert_eq!(
        shapes,
        [
            colours.like,
            colours.like,
            colours.like,
            colours.like,
            colours.dislike
        ]
    );
    right_click(&store, file, &stars(&store)).unwrap();
    assert!(matches!(
        stars(&store).kind,
        Kind::Numerical { stars: None, .. }
    ));
    assert_eq!(stars(&store).shapes(), [colours.null; 5]);
    // inc/dec: left one more, right one less, not below nothing
    let Kind::IncDec { value: start } = counter(&store).kind else {
        unreachable!("an inc/dec")
    };
    left_click(&store, file, &counter(&store), 0.0).unwrap();
    assert_eq!(counter(&store).kind, Kind::IncDec { value: start + 1 });
    for _ in 0..start + 3 {
        right_click(&store, file, &counter(&store)).unwrap();
    }
    assert_eq!(counter(&store).kind, Kind::IncDec { value: 0 });
    assert_eq!(kinds(&store, file).len(), 3);

    // the viewer: near the top right, the frame shows them; a click on the
    // like control likes the file
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|&f| f == file)
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    assert_eq!(viewer.get_ratings().row_count(), 3);
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    let window = viewer.window();
    let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(400.0, 300.0),
    });
    assert!(!viewer.get_ratings_showing());
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(780.0, 30.0),
    });
    assert!(viewer.get_ratings_showing());
    let pixels = headless::render(&drawn, 800, 600);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("ratings.png"), &pixels, 800, 600).unwrap();
    // (the like control: the first row's, right-aligned in the frame's
    // padding)
    let control = at(800.0 - 6.0 - 6.0, 6.0 + 6.0);
    window.dispatch_event(WindowEvent::PointerMoved { position: control });
    assert!(viewer.get_ratings_showing(), "still showing over a control");
    window.dispatch_event(WindowEvent::PointerPressed {
        position: control,
        button: PointerEventButton::Right,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: control,
        button: PointerEventButton::Right,
    });
    assert!(matches!(like(&store).kind, Kind::Like { state: None, .. }));
    window.dispatch_event(WindowEvent::PointerPressed {
        position: control,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: control,
        button: PointerEventButton::Left,
    });
    assert!(matches!(
        like(&store).kind,
        Kind::Like {
            state: Some(true),
            ..
        }
    ));
    let row = viewer.get_ratings().row_data(0).unwrap();
    let liked = like(&store).colours.like.brush;
    assert_eq!(
        row.shapes.row_data(0).unwrap().brush,
        slint::Color::from_rgb_u8(liked.0[0], liked.0[1], liked.0[2]),
        "drawn liked"
    );
}
