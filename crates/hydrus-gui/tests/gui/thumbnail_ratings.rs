//! The grid draws each thumbnail's ratings (laid out as the reference's
//! are: hydrus-gui-model's tests), as the options say, and shows a
//! rating set in the media viewer at once.

use std::sync::Arc;

use serde_json::{Value as Json, json};

use hydrus_core::thumbnail::ThumbnailRatingSettings;
use hydrus_gui::ratings::{self, Kind};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::services::{self, ServiceKind, ServiceRegistry};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// Show each service in thumbnails as `flags` say, and the numerical one's
/// "stars/of" `beside` its stars.
fn show(store: &Store, flags: &Json, beside: u8) {
    let flags = flags.clone();
    store
        .write_and_refresh(move |ctx| {
            let registry = ServiceRegistry::load(ctx.conn())?;
            for (name, flag) in flags.as_object().unwrap() {
                let service = registry.by_name(name).unwrap();
                let mut kind = service.kind.clone();
                let display = match &mut kind {
                    ServiceKind::RatingLike(c) => &mut c.display,
                    ServiceKind::RatingNumerical(c) => {
                        c.show_fraction_beside_stars = beside;
                        &mut c.display
                    }
                    ServiceKind::RatingIncDec(d) => d,
                    other => panic!("{name} is {other:?}"),
                };
                display.show_in_thumbnail = flag[0].as_bool().unwrap();
                display.show_in_thumbnail_even_when_null = flag[1].as_bool().unwrap();
                services::update_config(ctx.conn(), service.id, &kind)?;
            }
            Ok(())
        })
        .unwrap();
}

/// The grid's thumbnails carry their ratings over their boxes, the icons
/// at the top right under them; a rating set in the media viewer shows on
/// its thumbnail at once.
#[test]
#[allow(clippy::float_cmp)] // (whole pixels)
fn the_grid_draws_each_thumbnail_s_ratings() {
    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use slint::Model as _;
    let (_dirs, store) = store();
    // likes shown when rated (the fixture likes or dislikes each file),
    // counters always
    show(
        &store,
        &json!({ "favourites": [true, false], "stars": [false, false], "counter": [true, true] }),
        0,
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let first = bound.current.borrow().borrow().results()[0];
    let counted = |store: &Store| {
        ratings::controls(store, first)
            .into_iter()
            .find_map(|c| match c.kind {
                Kind::IncDec { value } => Some(value),
                _ => None,
            })
            .unwrap()
    };
    let before = counted(&store);
    let thumbnail = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    let drawn = thumbnail();
    let ratings: Vec<_> = drawn.ratings.iter().collect();
    // the like, then the counter under it, against the right border
    assert_eq!(ratings.len(), 2);
    assert_eq!(
        (ratings[0].kind, ratings[0].x, ratings[0].y),
        (0, 139.0, 3.0)
    );
    assert_eq!(ratings[0].shapes.row_count(), 1);
    assert_eq!(
        (ratings[1].kind, ratings[1].y, ratings[1].height),
        (1, 20.0, 12.0)
    );
    assert_eq!(
        ratings[1].text.as_str(),
        hydrus_core::numbers::human_int(before.unsigned_abs())
    );
    // (its number right aligned in a box a pixel smaller each way)
    assert_eq!(
        (
            ratings[1].text_x,
            ratings[1].text_y,
            ratings[1].text_width,
            ratings[1].text_height
        ),
        (ratings[1].x - 1.0, 20.0, ratings[1].width - 1.0, 11.0)
    );
    assert_eq!(ratings[1].text_size, 11.0);
    let boxes: Vec<(f32, f32, f32, f32)> = drawn
        .rating_boxes
        .iter()
        .map(|b| (b.x, b.y, b.width, b.height))
        .collect();
    assert_eq!(boxes, [(137.0, 1.0, 14.0, 18.0), (125.0, 19.0, 26.0, 14.0)]);
    // the icons at the top right start under the boxes
    for icon in drawn.icons.iter().filter(|i| i.x > 76.0) {
        assert_eq!(icon.y, 33.0);
    }
    // (for a look: the grid with them)
    let shown = windows.get(0).unwrap();
    let pixels = headless::render(&shown, 900, 600);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("thumbnail_ratings.png"), &pixels, 900, 600).unwrap();
    // one more on the counter in the viewer: the thumbnail says so
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    viewer.invoke_rating_clicked(2, true, 0.0);
    assert_eq!(counted(&store), before + 1);
    let drawn = thumbnail();
    let counter = drawn.ratings.iter().find(|r| r.kind == 1).unwrap();
    assert_eq!(
        counter.text.as_str(),
        hydrus_core::numbers::human_int((before + 1).unsigned_abs())
    );
}

/// The grid draws ratings as the options say: bigger, and on no boxes;
/// and collapsed, a numerical rating's "3/5" measured by our guess (six
/// tenths of the font's size a character, rounded up).
#[test]
fn the_grid_draws_ratings_as_the_options_say() {
    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use slint::Model as _;
    let (_dirs, store) = store();
    show(
        &store,
        &json!({ "favourites": [true, true], "stars": [false, false], "counter": [false, false] }),
        0,
    );
    let settings = ThumbnailRatingSettings {
        icon_size: 20.0,
        background: false,
        ..ThumbnailRatingSettings::default()
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let drawn = bound
        .rows
        .row_data(0)
        .unwrap()
        .thumbnails
        .row_data(0)
        .unwrap();
    let like = drawn.ratings.row_data(0).unwrap();
    // 20 pixels and the pad, from the right border
    assert_eq!((like.x, like.y, like.size), (131.0, 3.0, 20.0));
    assert_eq!(drawn.rating_boxes.row_count(), 0);
    // collapsed stars: "n/5" at 11 pixels is 20 wide, then a pixel, then
    // one shape
    drop(drawn);
    drop(bound);
    show(
        &store,
        &json!({ "favourites": [false, false], "stars": [true, true], "counter": [false, false] }),
        0,
    );
    let settings = ThumbnailRatingSettings {
        numerical_collapsed: true,
        ..ThumbnailRatingSettings::default()
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let drawn = bound
        .rows
        .row_data(0)
        .unwrap()
        .thumbnails
        .row_data(0)
        .unwrap();
    let stars = drawn.ratings.row_data(0).unwrap();
    assert_eq!(stars.text.len(), 3, "{}", stars.text);
    assert_eq!(
        (stars.text_width, stars.first, stars.shapes.row_count()),
        (20.0, 21.0, 1)
    );
    // the box: the text, the shape and the pad, and a margin each side
    let backing = drawn.rating_boxes.row_data(0).unwrap();
    assert_eq!((backing.x, backing.width), (151.0 - 38.0, 38.0));
}
