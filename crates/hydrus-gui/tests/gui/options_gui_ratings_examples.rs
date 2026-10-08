//! The ratings page's service style choice and size examples against the
//! reference's `RatingsPanel`: the examples are drawn in the style of the
//! chosen service at the sizes typed beside them, and can be clicked to test.
//! Read from the reference's source (there is no Qt in the sandbox to record
//! with); the page's labels, choices and order are checked against the
//! recording in the model tests.

use slint::Model as _;

use hydrus_gui_model::options::rating_style_choices;
use hydrus_store::reference_options::ReferenceOptions;
use hydrus_store::services::{ServiceKind, StarAppearance};

use crate::options_gui_support::{Client, box_of, items, row, show_page};

const STYLE: &str = "Select rating service for styling numerical stars:";
const MEDIA_EXAMPLES: &str = "Media viewer size examples (click to test):";
const PREVIEW_EXAMPLES: &str = "Preview window size examples (click to test):";
const MEDIA_SIZE: &str = "Media viewer like/dislike and numerical rating icon size:";
const MEDIA_HEIGHT: &str = "Media viewer inc/dec rating icon height:";

/// What a service's style looks like drawn as the example stars.
fn expected(client: &Client, name: &str) -> (String, usize) {
    let snapshot = client.store.snapshot();
    let service = snapshot.services.by_name(name).unwrap();
    match &service.kind {
        ServiceKind::RatingNumerical(c) => (
            hydrus_gui_model::ratings::shape_path(&c.appearance).to_owned(),
            c.num_stars as usize,
        ),
        ServiceKind::RatingLike(c) => (
            hydrus_gui_model::ratings::shape_path(&c.appearance).to_owned(),
            5,
        ),
        other => panic!("{name}: {other:?}"),
    }
}

// leaf: audit-options-ratings-choose-rating-service-style-to-display-for-examples-select-rating-service-for-styling-numerical-stars
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_chosen_service_styles_the_example_stars_and_the_typed_sizes_size_them() {
    let _ = StarAppearance::default();
    let client = Client::basic();
    let options = client.open_options();
    show_page(&options, "ratings");

    // the first box: like/dislike and numerical services to style them by
    assert_eq!(
        box_of(&options, STYLE),
        "choose rating service style to display for examples"
    );
    let (style_row, style) = row(&options, STYLE);
    assert_eq!(style.kind, 15);
    assert_eq!(items(&style), ["favourites", "stars"]);
    assert_eq!(
        style.index, 0,
        "a template that isn't any of them shows the first"
    );
    for (label, boxed) in [
        (MEDIA_EXAMPLES, "media viewer"),
        (PREVIEW_EXAMPLES, "preview window"),
    ] {
        assert_eq!(row(&options, label).1.kind, 39);
        assert_eq!(box_of(&options, label), boxed);
    }

    let star = |options: &hydrus_gui::OptionsWindow| row(options, MEDIA_EXAMPLES).1.star;
    let favourites = expected(&client, "favourites");
    let stars = expected(&client, "stars");
    assert_ne!(favourites, stars, "two styles to tell apart");
    let first = star(&options);
    assert_eq!(
        (
            first.graphic.shape.to_string(),
            first.graphic.shapes.row_count()
        ),
        favourites
    );
    // choose the numerical service
    options.invoke_choice_chosen(style_row, 1);
    let second = star(&options);
    assert_eq!(
        (
            second.graphic.shape.to_string(),
            second.graphic.shapes.row_count()
        ),
        stars
    );

    // sizes: the typed whole pixels, in their own box only
    let (size_row, _) = row(&options, MEDIA_SIZE);
    let (height_row, _) = row(&options, MEDIA_HEIGHT);
    assert_eq!(second.icon_size, 12.0);
    options.invoke_text_edited(size_row, "30.9".into());
    options.invoke_text_edited(height_row, "17.5".into());
    let media = row(&options, MEDIA_EXAMPLES).1;
    assert_eq!(
        (media.star.icon_size, media.incdec.incdec_height),
        (30.0, 17.0)
    );
    let preview = row(&options, PREVIEW_EXAMPLES).1;
    assert_eq!(
        (preview.star.icon_size, preview.incdec.incdec_height),
        (12.0, 12.0)
    );

    // clicking the stars sets a sample (the fraction beside them shows it)
    let before = star(&options).fraction.to_string();
    assert!(before.trim_start().starts_with('-'), "{before}");
    let width = f32::from(u16::try_from(second.graphic.shapes.row_count()).unwrap()) * 30.0;
    options.invoke_rating_example_pointer(
        row(&options, MEDIA_EXAMPLES).0,
        0,
        false,
        width / 2.0,
        width,
        30.0,
        false,
    );
    let after = star(&options).fraction.to_string();
    assert_ne!(before, after, "the click rated the example");
    // a right click clears it; the other boxes' examples are their own
    options.invoke_rating_example_pointer(
        row(&options, MEDIA_EXAMPLES).0,
        0,
        true,
        0.0,
        width,
        30.0,
        false,
    );
    assert_eq!(star(&options).fraction.to_string(), before);
    // the inc/dec box counts up and down
    let incdec_at = row(&options, MEDIA_EXAMPLES).0;
    options.invoke_rating_example_pointer(incdec_at, 1, false, 5.0, 40.0, 30.0, false);
    options.invoke_rating_example_pointer(incdec_at, 1, false, 5.0, 40.0, 30.0, false);
    assert_eq!(row(&options, MEDIA_EXAMPLES).1.incdec.graphic.text, "2");
    options.invoke_rating_example_pointer(incdec_at, 1, true, 5.0, 40.0, 30.0, false);
    assert_eq!(row(&options, MEDIA_EXAMPLES).1.incdec.graphic.text, "1");
    assert_eq!(row(&options, PREVIEW_EXAMPLES).1.incdec.graphic.text, "0");

    // nothing of the style is kept until OK
    options.invoke_cancel();
    let kept = |client: &Client| {
        client
            .setting::<ReferenceOptions>()
            .string("options_ratings_panel_template_service_key")
    };
    let default = kept(&client);
    let (stars_key, _) = rating_style_choices(&client.store)
        .into_iter()
        .find(|(_, name)| name == "stars")
        .unwrap();
    assert_ne!(default.as_deref(), Some(stars_key.to_hex().as_str()));

    // on OK it is, and the page opens on it again; the samples are not
    let options = client.open_options();
    show_page(&options, "ratings");
    options.invoke_choice_chosen(row(&options, STYLE).0, 1);
    options.invoke_apply();
    assert_eq!(kept(&client).as_deref(), Some(stars_key.to_hex().as_str()));
    let options = client.open_options();
    show_page(&options, "ratings");
    assert_eq!(row(&options, STYLE).1.index, 1);
    let reopened = star(&options);
    assert_eq!(
        (
            reopened.graphic.shape.to_string(),
            reopened.graphic.shapes.row_count()
        ),
        stars
    );
    assert_eq!(row(&options, MEDIA_EXAMPLES).1.incdec.graphic.text, "0");

    // (a picture of the page, for review)
    let window = client.windows().get(client.windows().count() - 1).unwrap();
    let pixels = hydrus_gui::headless::render(&window, 900, 640);
    hydrus_gui::headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("options-ratings.png"),
        &pixels,
        900,
        640,
    )
    .unwrap();
}
