//! The ratings over thumbnails against the reference's
//! (`oracle/record_thumbnail_ratings.py`): on the `basic` fixture, rated as
//! the recording rates it, for each recorded case (which services show in
//! thumbnails, even unrated or not; the drawing options; a numerical
//! service's "stars/of" beside its stars), each thumbnail's ratings drawn
//! where the reference draws them, its boxes behind them, its texts, and
//! its icons under them.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value as Json, json};

use hydrus_core::thumbnail::ThumbnailRatingSettings;
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::ratings::{self, Kind};
use hydrus_gui_model::thumbnail_icons;
use hydrus_gui_model::thumbnail_ratings::{self, Drawn, Look, counter_width, fraction_text};
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

fn file(store: &Store, hex: &str) -> HashId {
    let hash: Sha256 = hex.parse().unwrap();
    store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap_or_else(|| panic!("no file {hex}"))
}

/// Rate the files as the recording did.
fn rate(store: &Store, recorded: &Json) {
    let registry = store.snapshot().services.clone();
    for rated in recorded["rated"].as_array().unwrap() {
        let service = registry
            .by_name(rated["service"].as_str().unwrap())
            .unwrap()
            .clone();
        let hash_id = file(store, rated["file"].as_str().unwrap());
        let rating = rated["rating"].clone();
        store
            .write_content(move |w| match service.kind {
                ServiceKind::RatingIncDec(_) => {
                    w.set_incdec(service.id, &[hash_id], rating.as_i64().unwrap_or(0))
                }
                _ => w.set_rating(service.id, &[hash_id], rating.as_f64()),
            })
            .unwrap();
    }
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

fn settings(options: &Json) -> ThumbnailRatingSettings {
    ThumbnailRatingSettings {
        icon_size: options["icon_size"].as_f64().unwrap(),
        incdec_height: options["incdec_height"].as_f64().unwrap(),
        background: options["background"].as_bool().unwrap(),
        numerical_collapsed: options["collapsed"].as_bool().unwrap(),
    }
}

/// A rating as the recording has it: how it was drawn, its service, its
/// state (`ClientRatings`' LIKE 0, DISLIKE 1, NULL 2, SET 3), its rating
/// and where.
fn rating_json(drawn: &Drawn) -> Json {
    let (draw, state, rating) = match &drawn.control.kind {
        Kind::Like { state, .. } => (
            "DrawLike",
            match state {
                Some(true) => 0,
                Some(false) => 1,
                None => 2,
            },
            Json::Null,
        ),
        Kind::Numerical { stars, config, .. } => (
            "DrawNumerical",
            if stars.is_some() { 3 } else { 2 },
            stars.map_or(Json::Null, |s| json!(config.rating(s))),
        ),
        Kind::IncDec { value } => ("DrawIncDec", 3, json!(value)),
    };
    json!({
        "draw": draw,
        "service": drawn.control.name,
        "state": state,
        "rating": rating,
        "x": drawn.x,
        "y": drawn.y,
    })
}

/// The texts drawn, as the recording has them, with their fonts' pixel
/// sizes (a counter's with its box's height; a numerical rating's at its
/// font's, which isn't compared).
fn texts_json(layout: &thumbnail_ratings::Layout, recorded: &[Json]) -> Vec<Json> {
    let mut out = Vec::new();
    for drawn in &layout.drawn {
        match &drawn.look {
            Look::Shapes {
                text: Some(text), ..
            } => {
                let height = recorded
                    .iter()
                    .find(|r| r["text"] == text.text.as_str())
                    .map_or(Json::Null, |r| r["height"].clone());
                out.push(json!({ "text": text.text, "x": text.x, "y": text.y, "width": text.width, "height": height, "pixel_size": text.pixel_size }));
            }
            Look::Counter { height, text, .. } => {
                out.push(json!({ "text": text.text, "x": text.x, "y": text.y, "width": text.width, "height": height - 1, "pixel_size": text.pixel_size }));
            }
            Look::Shapes { text: None, .. } => {}
        }
    }
    out
}

#[test]
fn the_ratings_over_thumbnails_are_the_references() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("thumbnail_ratings.json");
    rate(&store, &recorded);
    let border = i32::try_from(recorded["thumbnail_border"].as_i64().unwrap()).unwrap();
    // a new client's options are ours
    assert_eq!(
        settings(&recorded["new_client_options"]),
        ThumbnailRatingSettings::default()
    );
    // the reference's texts' widths, by text and pixel size
    let mut widths: HashMap<String, i32> = HashMap::new();
    let summaries = hydrus_core::tag_summary::TagSummaries::default();
    let mut seen = std::collections::BTreeSet::new();
    for case in recorded["cases"].as_array().unwrap() {
        for thumbnail in case["thumbnails"].as_array().unwrap() {
            for text in thumbnail["texts"].as_array().unwrap() {
                let width = i32::try_from(text["width"].as_i64().unwrap()).unwrap();
                widths.insert(text["text"].as_str().unwrap().to_owned(), width);
            }
        }
    }
    let text_width = |text: &str, pixel_size: i32| -> i32 {
        assert_eq!(pixel_size, 11, "{text}");
        widths[text]
    };
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let beside = u8::try_from(case["fraction_beside"].as_u64().unwrap()).unwrap();
        show(&store, &case["flags"], beside);
        let settings = settings(&case["options"]);
        let services = store.snapshot().services.clone();
        for thumbnail in case["thumbnails"].as_array().unwrap() {
            let hex = thumbnail["file"].as_str().unwrap();
            let hash_id = file(&store, hex);
            let width = i32::try_from(thumbnail["size"][0].as_i64().unwrap()).unwrap();
            let height = i32::try_from(thumbnail["size"][1].as_i64().unwrap()).unwrap();
            let controls = ratings::controls(&store, hash_id);
            let layout = thumbnail_ratings::layout(
                &services,
                &controls,
                &settings,
                width,
                border,
                &text_width,
            );
            let at = format!("{name}: {hex}");
            let ours: Vec<Json> = layout.drawn.iter().map(rating_json).collect();
            assert_eq!(Json::from(ours), thumbnail["ratings"], "{at}");
            // the boxes behind them come after the thumbnail's own and its
            // tag banners'
            let (top, bottom) = thumbnail_icons::banners(&store, &[hash_id], &summaries);
            let banners = usize::from(!top.is_empty()) + usize::from(!bottom.is_empty());
            let boxes: Vec<Json> = layout
                .boxes
                .iter()
                .map(|b| json!({ "x": b.x, "y": b.y, "width": b.width, "height": b.height }))
                .collect();
            assert_eq!(
                Json::from(boxes),
                Json::from(thumbnail["boxes"].as_array().unwrap()[1 + banners..].to_vec()),
                "{at}"
            );
            // as do the texts
            let texts = thumbnail["texts"].as_array().unwrap()[banners..].to_vec();
            assert_eq!(
                Json::from(texts_json(&layout, &texts)),
                Json::from(texts),
                "{at}"
            );
            // and the icons go under them
            let facts = thumbnail_icons::facts(&store, &[hash_id]);
            let icons: Vec<Json> = thumbnail_icons::placed(
                &facts[&hash_id],
                false,
                border,
                width,
                height,
                layout.top_right_y,
            )
            .iter()
            .map(|p| json!({ "name": p.icon.name(), "x": p.x, "y": p.y }))
            .collect();
            assert_eq!(Json::from(icons), thumbnail["icons"], "{at}");
            for drawn in &layout.drawn {
                seen.insert(rating_json(drawn)["state"].to_string() + drawn.control.name.as_str());
            }
        }
    }
    // (the recording has likes, dislikes, unrated likes and stars, and
    // counters)
    for state in [
        "0favourites",
        "1favourites",
        "2favourites",
        "2stars",
        "3stars",
        "3counter",
    ] {
        assert!(seen.contains(state), "{state} {seen:?}");
    }
}

/// Where a numerical rating's shapes go from where it is drawn, and in
/// what colours, in each way it can be drawn: after its "stars/of" and a
/// pixel when collapsed (one shape, in the like colours if rated, else the
/// unrated ones) or with its "stars/of" on the left; at once otherwise.
#[test]
fn numerical_ratings_shapes_are_placed_as_the_reference_s_are() {
    let (_dirs, store) = store();
    let always =
        json!({ "favourites": [false, false], "stars": [true, true], "counter": [false, false] });
    let recorded = hydrus_testkit::fixture_json("thumbnail_ratings.json");
    rate(&store, &recorded);
    let manifest_file = |i: usize| {
        let hex = recorded["rated"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["service"] == "stars")
            .nth(i)
            .unwrap()["file"]
            .as_str()
            .unwrap()
            .to_owned();
        file(&store, &hex)
    };
    // (rated 0.6, and unrated)
    let (rated, unrated) = (manifest_file(0), manifest_file(3));
    let text_width = |text: &str, _: i32| i32::try_from(text.len()).unwrap() * 6;
    let look = |file: HashId, beside: u8, collapsed: bool| {
        show(&store, &always, beside);
        let services = store.snapshot().services.clone();
        let settings = ThumbnailRatingSettings {
            numerical_collapsed: collapsed,
            ..ThumbnailRatingSettings::default()
        };
        let controls = ratings::controls(&store, file);
        let layout =
            thumbnail_ratings::layout(&services, &controls, &settings, 152, 1, &text_width);
        let [drawn] = layout.drawn.as_slice() else {
            panic!("{:?}", layout.drawn);
        };
        (drawn.clone(), drawn.control.colours)
    };
    let shapes = |drawn: &Drawn| match &drawn.look {
        Look::Shapes {
            first,
            shapes,
            step,
            text,
            ..
        } => (
            *first,
            shapes.clone(),
            *step,
            text.as_ref().map(|t| (t.text.clone(), t.x - drawn.x)),
        ),
        other @ Look::Counter { .. } => panic!("{other:?}"),
    };
    let (drawn, colours) = look(rated, 0, false);
    let (first, drawn_shapes, step, text) = shapes(&drawn);
    assert_eq!((first, step, text), (0, 16, None));
    assert_eq!(
        drawn_shapes,
        [vec![colours.like; 3], vec![colours.dislike; 2]].concat()
    );
    // collapsed: "3/5" 18 wide, then a pixel, then one shape
    let (first, drawn_shapes, _, text) = shapes(&look(rated, 0, true).0);
    assert_eq!((first, text), (19, Some(("3/5".to_owned(), -2))));
    assert_eq!(drawn_shapes, vec![colours.like]);
    let (first, drawn_shapes, _, text) = shapes(&look(unrated, 0, true).0);
    assert_eq!((first, text), (19, Some(("-/5".to_owned(), -2))));
    assert_eq!(drawn_shapes, vec![colours.null]);
    // on the left, as collapsed but every shape
    let (first, drawn_shapes, _, text) = shapes(&look(rated, 1, false).0);
    assert_eq!((first, text), (19, Some(("3/5".to_owned(), -2))));
    assert_eq!(drawn_shapes.len(), 5);
    // on the right, after the last shape and two pixels
    let (first, drawn_shapes, _, text) = shapes(&look(unrated, 2, false).0);
    assert_eq!((first, text), (0, Some(("-/5".to_owned(), 78))));
    assert_eq!(drawn_shapes, vec![colours.null; 5]);
}

#[test]
fn a_fraction_is_written_as_the_reference_writes_it() {
    assert_eq!(fraction_text(Some(3), 5), "3/5");
    assert_eq!(fraction_text(None, 5), "-/5");
    assert_eq!(fraction_text(Some(3), 10), " 3/10");
    assert_eq!(fraction_text(None, 10), " -/10");
    assert_eq!(fraction_text(Some(10), 10), "10/10");
}

#[test]
fn a_counter_is_as_wide_as_the_reference_s() {
    // twice its height up to three digits, then wider
    assert_eq!(counter_width(12, 0), 24);
    assert_eq!(counter_width(12, 999), 24);
    assert_eq!(counter_width(12, 1000), 31);
    assert_eq!(counter_width(12, 12345), 38);
    assert_eq!(counter_width(12, 123_456), 46);
    assert_eq!(counter_width(12, 1_234_567), 53);
    assert_eq!(counter_width(8, 12345), 25);
    // (a count below 0 is as wide as 0)
    assert_eq!(counter_width(12, -12345), 24);
}

/// Two like services' ratings share a row, a pad apart, and two inc/dec
/// services' theirs, a pixel apart, in the services' order (the fixture
/// has one of each; the reference's widths are `size * n + pad * (n - 1)
/// + 2 * margin` and `2 * margin + margin * (n - 1)` and the counters').
#[test]
fn ratings_of_a_kind_share_a_row() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("thumbnail_ratings.json");
    rate(&store, &recorded);
    // a second like service and a second inc/dec one, as the first,
    // shown even unrated
    store
        .write_and_refresh(|ctx| {
            let registry = ServiceRegistry::load(ctx.conn())?;
            for (of, name, byte) in [("favourites", "hearts", 7), ("counter", "tally", 8)] {
                let mut kind = registry.by_name(of).unwrap().kind.clone();
                match &mut kind {
                    ServiceKind::RatingLike(c) => {
                        c.display.show_in_thumbnail = true;
                        c.display.show_in_thumbnail_even_when_null = true;
                    }
                    ServiceKind::RatingIncDec(d) => {
                        d.show_in_thumbnail = true;
                        d.show_in_thumbnail_even_when_null = true;
                    }
                    other => panic!("{of} is {other:?}"),
                }
                services::insert(
                    ctx.conn(),
                    &hydrus_core::ServiceKey::new(vec![byte; 32]),
                    name,
                    &kind,
                )?;
            }
            Ok(())
        })
        .unwrap();
    show(
        &store,
        &json!({ "favourites": [true, true], "stars": [false, false], "counter": [true, true] }),
        0,
    );
    let services = store.snapshot().services.clone();
    let hex = recorded["rated"][0]["file"].as_str().unwrap();
    let controls = ratings::controls(&store, file(&store, hex));
    let layout = thumbnail_ratings::layout(
        &services,
        &controls,
        &ThumbnailRatingSettings::default(),
        152,
        1,
        &|_, _| 0,
    );
    let at: Vec<(&str, i32, i32)> = layout
        .drawn
        .iter()
        .map(|d| (d.control.name.as_str(), d.x, d.y))
        .collect();
    // (the file's counter is 3, the tally unrated)
    assert_eq!(
        at,
        [
            ("favourites", 123, 3),
            ("hearts", 139, 3),
            ("counter", 101, 20),
            ("tally", 126, 20)
        ]
    );
    let boxes: Vec<_> = layout
        .boxes
        .iter()
        .map(|b| (b.x, b.y, b.width, b.height))
        .collect();
    assert_eq!(boxes, [(121, 1, 30, 18), (100, 19, 51, 14)]);
    assert_eq!(layout.top_right_y, 33);
}
