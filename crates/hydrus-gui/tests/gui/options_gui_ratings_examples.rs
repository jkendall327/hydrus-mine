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
        // (opened on the reference's default look, circles, until a service is
        // chosen: the dropdown shows the first without applying it)
        (
            hydrus_gui_model::ratings::shape_path(&StarAppearance::Shape(
                hydrus_store::services::StarShape(0)
            ))
            .to_owned(),
            5
        )
    );
    // choose the numerical service: only its shape and colours are taken, the
    // stars' count staying as the opening template made it
    options.invoke_choice_chosen(style_row, 1);
    let second = star(&options);
    assert_eq!(second.graphic.shape.to_string(), stars.0);
    assert_eq!(second.graphic.shapes.row_count(), favourites.1);
    assert_ne!(second.graphic.shape.to_string(), favourites.0);

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

    // the style is kept the moment it is chosen, cancelled dialog or not
    options.invoke_cancel();
    let kept = |client: &Client| {
        client
            .setting::<ReferenceOptions>()
            .string("options_ratings_panel_template_service_key")
    };
    let (stars_key, _) = rating_style_choices(&client.store)
        .into_iter()
        .find(|(_, name)| name == "stars")
        .unwrap();
    assert_eq!(kept(&client).as_deref(), Some(stars_key.to_hex().as_str()));

    // reopened, the page opens on it, and its stars are the service's own
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
    // (and the samples are not kept)
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

/// The example stars' look: their shape, each star's pen and brush, and
/// their pad (from the options window's media viewer example).
type Look = (String, Vec<([u8; 4], [u8; 4])>, f32);

fn look(options: &hydrus_gui::OptionsWindow) -> Look {
    let star = row(options, MEDIA_EXAMPLES).1.star;
    let rgba = |c: slint::Color| [c.red(), c.green(), c.blue(), c.alpha()];
    (
        star.graphic.shape.to_string(),
        star.graphic
            .shapes
            .iter()
            .map(|s| (rgba(s.pen), rgba(s.brush)))
            .collect(),
        star.graphic.pad,
    )
}

/// The stars' look the reference recorded: shape, the stars' colours (a
/// group's count of them, one after another) and pad.
fn theirs(v: &serde_json::Value, state: &serde_json::Value) -> Look {
    use hydrus_store::services::{StarAppearance, StarShape};
    let colour = |c: &serde_json::Value| -> [u8; 4] { serde_json::from_value(c.clone()).unwrap() };
    let shape = StarAppearance::Shape(StarShape(
        u8::try_from(state["shape"].as_u64().unwrap()).unwrap(),
    ));
    let mut stars = Vec::new();
    for group in state["groups"].as_array().unwrap() {
        for _ in 0..group["count"].as_u64().unwrap() {
            stars.push((colour(&group["pen"]), colour(&group["brush"])));
        }
    }
    let _ = v;
    (
        hydrus_gui_model::ratings::shape_path(&shape).to_owned(),
        stars,
        state["pad"].as_f64().unwrap() as f32,
    )
}

/// Choosing a style for the examples against the reference's real ratings
/// page (oracle/record_rating_examples.py): what the stars look like first
/// (before any choice), after each choice, rated, and when the page is opened
/// again with a style saved; the typed sizes of every box, clamped as its spin
/// box does, and the whole pixels the example takes.
// leaf: audit-options-ratings-choose-rating-service-style-to-display-for-examples-select-rating-service-for-styling-numerical-stars
#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp
)] // (whole pixels)
fn the_examples_are_styled_by_the_chosen_service_as_the_reference_does() {
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("rating_examples.json");
    let client = Client::basic();
    let choices: Vec<String> = serde_json::from_value(recorded["choices"].clone()).unwrap();
    let options = client.open_options();
    show_page(&options, "ratings");
    let (style_row, style) = row(&options, STYLE);
    assert_eq!(items(&style), choices);
    assert_eq!(
        items(&style)[usize::try_from(style.index).unwrap()],
        recorded["initial"].as_str().unwrap()
    );

    // the styles: first as the page opens (nothing chosen), then the choices in turn
    let media = row(&options, MEDIA_EXAMPLES).0;
    let sequence = [None, Some(1), Some(1), Some(0)];
    for (n, (choice, style_case)) in sequence
        .iter()
        .zip(recorded["styles"].as_array().unwrap())
        .enumerate()
    {
        if let Some(choice) = choice {
            options.invoke_choice_chosen(style_row, *choice);
        }
        let null = &style_case["null"];
        assert_eq!(
            look(&options),
            theirs(&recorded, null),
            "style {n}: unrated"
        );
        assert_eq!(
            row(&options, MEDIA_EXAMPLES).1.star.fraction.trim(),
            null["fraction"].as_str().unwrap(),
            "style {n}: the fraction"
        );
        // rated: a click leaving k stars on
        for rated in style_case["rated"].as_array().unwrap() {
            let on = rated["groups"][0]["count"].as_u64().unwrap();
            if on == 0 {
                continue;
            }
            let stars = null["num_stars"].as_f64().unwrap() as f32;
            let width = stars * 12.0 + (stars - 1.0) * null["pad"].as_f64().unwrap() as f32;
            // where the click rounds to `on` stars (the active width is the widget's less a pixel each side)
            let x = (1.0 + on as f32 / stars * (width - 2.0))
                .min(width - 2.0)
                .round();
            options.invoke_rating_example_pointer(media, 0, false, x, width, 12.0, false);
            assert_eq!(
                look(&options),
                theirs(&recorded, rated),
                "style {n}: rated {}",
                rated["rating"]
            );
            assert_eq!(
                row(&options, MEDIA_EXAMPLES).1.star.fraction.trim(),
                rated["fraction"].as_str().unwrap(),
                "style {n}: rated {}",
                rated["rating"]
            );
            options.invoke_rating_example_pointer(media, 0, true, 0.0, width, 12.0, false);
        }
    }
    options.invoke_cancel();

    // opened again with each service saved
    for reopened in recorded["reopened"].as_array().unwrap() {
        let saved = reopened["saved"].as_str().unwrap();
        let key = rating_style_choices(&client.store)
            .into_iter()
            .find(|(_, name)| name == saved)
            .map_or_else(|| "00".repeat(32), |(key, _)| key.to_hex());
        client
            .store
            .write(move |ctx| {
                let mut reference: ReferenceOptions = hydrus_store::settings::get(ctx.conn())?;
                reference.set_string("options_ratings_panel_template_service_key", Some(key));
                hydrus_store::settings::set(ctx.conn(), &reference)
            })
            .unwrap();
        let options = client.open_options();
        show_page(&options, "ratings");
        let (_, style) = row(&options, STYLE);
        assert_eq!(
            items(&style)[usize::try_from(style.index).unwrap()],
            reopened["shown"].as_str().unwrap(),
            "{saved}"
        );
        assert_eq!(
            look(&options),
            theirs(&recorded, &reopened["null"]),
            "{saved}: unrated"
        );
        options.invoke_cancel();
    }

    // the typed sizes: each box clamps as its spin box does, the example takes whole pixels
    let options = client.open_options();
    show_page(&options, "ratings");
    let labels = [
        ("media viewer size", MEDIA_SIZE, 1usize),
        ("media viewer height", MEDIA_HEIGHT, 1),
        (
            "preview size",
            "Preview window like/dislike and numerical rating icon size:",
            2,
        ),
        (
            "preview height",
            "Preview window inc/dec rating icon height:",
            2,
        ),
        (
            "thumbnail size",
            "Thumbnail like/dislike and numerical rating icon size: ",
            3,
        ),
        ("thumbnail height", "Thumbnail inc/dec rating height: ", 3),
        (
            "dialog size",
            "Dialogs like/dislike and numerical rating icon size:",
            4,
        ),
        ("dialog height", "Dialogs inc/dec rating height:", 4),
    ];
    let examples = [
        MEDIA_EXAMPLES,
        PREVIEW_EXAMPLES,
        "Thumbnail size examples (click to test):",
        "Dialog size examples (click to test):",
    ];
    for entry in recorded["sizes"].as_array().unwrap() {
        let name = entry["box"].as_str().unwrap();
        let (_, label, context) = labels.iter().find(|(n, _, _)| *n == name).unwrap();
        let (at, found) = row(&options, label);
        assert_eq!(
            found.text,
            format!("{:?}", entry["initial"].as_f64().unwrap()),
            "{name}: opens at"
        );
        let height = name.ends_with("height");
        for value in entry["values"].as_array().unwrap() {
            let typed = value["typed"].as_f64().unwrap();
            options.invoke_text_edited(at, format!("{typed}").into());
            let example = row(&options, examples[context - 1]).1;
            let ours = if height {
                (example.incdec.incdec_height, example.incdec.incdec_height)
            } else {
                (example.star.icon_size, example.star.icon_size)
            };
            let theirs = value["example"].as_array().unwrap();
            let expected = if height {
                theirs[1].as_f64().unwrap() as f32
            } else {
                theirs[0].as_f64().unwrap() as f32
            };
            assert_eq!(ours.0, expected, "{name}: typed {typed}");
        }
    }
    options.invoke_cancel();
}
