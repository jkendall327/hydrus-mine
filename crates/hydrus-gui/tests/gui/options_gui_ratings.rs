//! The options window's ratings page against the reference's
//! `RatingsPanel`: the sizes and switches of the media viewer and the
//! thumbnails, and the viewer and grid drawing by them once applied.

use serde_json::{Value as Json, json};
use slint::Model as _;

use hydrus_core::thumbnail::ThumbnailRatingSettings;
use hydrus_store::Store;
use hydrus_store::services::{self, ServiceKind, ServiceRegistry};

use crate::options_gui_support::{Client, box_of, row, show_page};

/// Show each service in thumbnails as `flags` say.
fn show(store: &Store, flags: &Json) {
    let flags = flags.clone();
    store
        .write_and_refresh(move |ctx| {
            let registry = ServiceRegistry::load(ctx.conn())?;
            for (name, flag) in flags.as_object().unwrap() {
                let service = registry.by_name(name).unwrap();
                let mut kind = service.kind.clone();
                let display = match &mut kind {
                    ServiceKind::RatingLike(c) => &mut c.display,
                    ServiceKind::RatingNumerical(c) => &mut c.display,
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

// leaf: audit-options-ratings-media-viewer-media-viewer-like-dislike-and-numerical-rating-icon-size
// leaf: audit-options-ratings-media-viewer-media-viewer-inc-dec-rating-icon-height
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_media_viewer_draws_its_ratings_at_the_sizes_the_options_say() {
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let viewer_sizes = |client: &Client| {
        client.ui.invoke_thumbnail_activated(0);
        let viewer = client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opens");
        let sizes = (viewer.get_rating_size(), viewer.get_incdec_height());
        viewer.invoke_close_requested();
        sizes
    };
    assert_eq!(
        viewer_sizes(&client),
        (12.0, 12.0),
        "the reference's defaults"
    );

    let options = client.open_options();
    show_page(&options, "ratings");
    let size = "Media viewer like/dislike and numerical rating icon size:";
    let height = "Media viewer inc/dec rating icon height:";
    // typed floats, from 1 to 255 and from 2 to 255, in the "media viewer" box
    let (size_row, r) = row(&options, size);
    assert_eq!((r.kind, r.text.as_str()), (4, "12.0"));
    assert_eq!(box_of(&options, size), "media viewer");
    let (height_row, r) = row(&options, height);
    assert_eq!((r.kind, r.text.as_str()), (4, "12.0"));
    assert_eq!(box_of(&options, height), "media viewer");
    options.invoke_text_edited(size_row, "30".into());
    options.invoke_text_edited(height_row, "17.5".into());
    options.invoke_apply();
    let saved = client.setting::<hydrus_core::media_viewer::MediaViewerSettings>();
    assert_eq!(
        (saved.rating_icon_size, saved.rating_incdec_height),
        (30.0, 17.5)
    );
    assert_eq!(viewer_sizes(&client), (30.0, 17.5));

    // the reference's spin boxes: 1 to 255 and 2 to 255; a value outside is
    // refused (the saved ones stay) and one on the edge is taken
    let saved_sizes = |client: &Client| {
        let s = client.setting::<hydrus_core::media_viewer::MediaViewerSettings>();
        (s.rating_icon_size, s.rating_incdec_height)
    };
    let try_value = |label: &str, text: &str| {
        let options = client.open_options();
        show_page(&options, "ratings");
        let (at, _) = row(&options, label);
        options.invoke_text_edited(at, text.into());
        options.invoke_apply();
        if client.bound.options.borrow().is_some() {
            client
                .bound
                .options
                .borrow()
                .as_ref()
                .unwrap()
                .invoke_cancel();
        }
    };
    try_value(size, "0.5");
    try_value(size, "255.5");
    try_value(height, "1.5");
    assert_eq!(saved_sizes(&client), (30.0, 17.5), "refused");
    try_value(size, "255");
    try_value(height, "2");
    assert_eq!(saved_sizes(&client), (255.0, 2.0), "the edges");
    try_value(size, "1");
    assert_eq!(saved_sizes(&client).0, 1.0);
}

// leaf: audit-options-ratings-thumbnails-thumbnail-like-dislike-and-numerical-rating-icon-size
// leaf: audit-options-ratings-thumbnails-thumbnail-inc-dec-rating-height
// leaf: audit-options-ratings-thumbnails-give-thumbnail-ratings-a-flat-background
// leaf: audit-options-ratings-thumbnails-always-draw-thumbnail-numerical-ratings-collapsed
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_grid_draws_ratings_at_the_thumbnail_options_applied() {
    let client = Client::basic();
    show(
        &client.store,
        &json!({ "favourites": [true, true], "stars": [true, true], "counter": [true, true] }),
    );
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let first = || {
        client
            .bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    // (kind 0 the likes and stars, 1 a counter)
    let drawn = |client: &Client| {
        let _ = client;
        let thumbnail = first();
        let ratings: Vec<_> = thumbnail.ratings.iter().collect();
        (ratings, thumbnail.rating_boxes.row_count())
    };
    let (before, boxes) = drawn(&client);
    assert_eq!(before.len(), 3);
    assert!(
        before
            .iter()
            .filter(|r| r.kind == 0)
            .all(|r| r.size == 12.0)
    );
    let counter = before.iter().find(|r| r.kind == 1).unwrap();
    assert_eq!(counter.height, 12.0);
    assert!(boxes > 0, "a flat background behind each, by default");
    // (the like has one shape, the stars one each)
    let stars_before = before
        .iter()
        .find(|r| r.kind == 0 && r.shapes.row_count() > 1)
        .expect("the stars");
    assert!(stars_before.text.is_empty(), "stars, not 3/5");

    let options = client.open_options();
    show_page(&options, "ratings");
    let labels = [
        "Thumbnail like/dislike and numerical rating icon size: ",
        "Thumbnail inc/dec rating height: ",
        "Give thumbnail ratings a flat background: ",
        "Always draw thumbnail numerical ratings collapsed: ",
    ];
    for label in labels {
        assert_eq!(box_of(&options, label), "thumbnails", "{label}");
    }
    let (size_row, r) = row(&options, labels[0]);
    assert_eq!((r.kind, r.text.as_str()), (4, "12.0"));
    let (height_row, r) = row(&options, labels[1]);
    assert_eq!((r.kind, r.text.as_str()), (4, "12.0"));
    let (background_row, r) = row(&options, labels[2]);
    assert_eq!((r.kind, r.checked), (1, true));
    let (collapsed_row, r) = row(&options, labels[3]);
    assert_eq!((r.kind, r.checked), (1, false));
    options.invoke_text_edited(size_row, "20".into());
    options.invoke_text_edited(height_row, "18".into());
    options.invoke_check_toggled(background_row, false);
    options.invoke_check_toggled(collapsed_row, true);
    options.invoke_apply();
    let saved = client.setting::<ThumbnailRatingSettings>();
    assert_eq!(
        saved,
        ThumbnailRatingSettings {
            icon_size: 20.0,
            incdec_height: 18.0,
            background: false,
            numerical_collapsed: true
        }
    );
    let (after, boxes) = drawn(&client);
    assert!(after.iter().filter(|r| r.kind == 0).all(|r| r.size == 20.0));
    assert_eq!(after.iter().find(|r| r.kind == 1).unwrap().height, 18.0);
    assert_eq!(boxes, 0, "no flat background");
    let stars = after
        .iter()
        .find(|r| r.kind == 0 && !r.text.is_empty())
        .expect("the stars, collapsed to text and one shape");
    assert_eq!(stars.shapes.row_count(), 1);
    assert!(
        stars.text.contains('/'),
        "collapsed stars read like 3/5: {:?}",
        stars.text
    );
}
