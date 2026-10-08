//! The thumbnails page against the thumbnails it makes: each option is
//! changed in the real options window, applied, and a thumbnail made again
//! from its file (as the reference does for a thumbnail of the wrong size,
//! or in file maintenance) is made as the saved value says.

use slint::ComponentHandle as _;

use hydrus_core::HashId;
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;

use crate::options_gui_support::{box_of, items, row, show_page};
use crate::options_media_support::Media;

/// A file's thumbnail made again under the store's settings now: its
/// pixel size and its bytes.
fn remake(client: &Media, id: HashId) -> ((u32, u32), Vec<u8>) {
    let snapshot = client.store.snapshot();
    let media = client
        .store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
        .unwrap()
        .results
        .remove(0);
    let importer = FileImporter::new(client.store.clone(), MediaTools::new());
    let path = importer
        .regenerate_thumbnail(&media)
        .unwrap()
        .expect("a thumbnail was made");
    let bytes = std::fs::read(path).unwrap();
    let raster = hydrus_media::decode_image(&bytes).unwrap();
    ((raster.width(), raster.height()), bytes)
}

/// The first file `pick` likes, with its size.
fn find(
    client: &Media,
    pick: impl Fn(&hydrus_store::media::FileInfo) -> bool,
) -> (HashId, (u32, u32)) {
    let files = client.results();
    let batch = client
        .store
        .read(|c| hydrus_store::media::load_basic(c, &files))
        .unwrap();
    let (at, info) = batch
        .iter()
        .enumerate()
        .find_map(|(i, m)| m.info.as_ref().filter(|i| pick(i)).map(|info| (i, info)))
        .expect("a file like that");
    (
        files[at],
        (info.width.unwrap_or(0), info.height.unwrap_or(0)),
    )
}

/// Set the number row `label` of the thumbnails page, apply.
fn number(client: &Media, label: &str, limits: (i32, i32), n: i32) {
    let options = client.open_options();
    show_page(&options, "thumbnails");
    let (i, found) = row(&options, label);
    assert_eq!(
        (found.kind, found.minimum, found.maximum),
        (2, limits.0, limits.1),
        "{label:?}"
    );
    assert_eq!(box_of(&options, label), "appearance");
    options.invoke_number_edited(i, n);
    options.invoke_apply();
    options.hide().unwrap();
}

// leaf: audit-options-thumbnails-appearance-thumbnail-scaling
#[test]
fn the_thumbnail_scaling_decides_how_a_small_picture_is_made() {
    const LABEL: &str = "Thumbnail scaling: ";
    let client = Media::basic();
    client.search("system:everything");
    // a picture smaller than the 150 by 125 box
    let (file, (w, h)) = find(&client, |i| {
        i.mime == hydrus_core::Mime::ImageJpeg && i.width.is_some_and(|w| w < 100)
    });
    assert!(w < 100 && h < 100, "{w}x{h}");

    let options = client.open_options();
    show_page(&options, "thumbnails");
    let (_, found) = row(&options, LABEL);
    assert_eq!(
        items(&found),
        ["scale down only", "scale to fit", "scale to fill"]
    );
    assert_eq!((found.kind, found.index), (5, 0));
    assert_eq!(box_of(&options, LABEL), "appearance");
    options.hide().unwrap();

    assert_eq!(
        remake(&client, file).0,
        (w, h),
        "scaled down only: as it is"
    );
    for (choice, scale) in [
        (1, hydrus_core::thumbnail::ThumbnailScale::ToFit),
        (2, hydrus_core::thumbnail::ThumbnailScale::ToFill),
        (0, hydrus_core::thumbnail::ThumbnailScale::DownOnly),
    ] {
        let options = client.open_options();
        show_page(&options, "thumbnails");
        let (i, _) = row(&options, LABEL);
        options.invoke_choice_chosen(i, choice);
        options.invoke_apply();
        options.hide().unwrap();
        let settings = client.store.snapshot().thumbnails;
        assert_eq!(settings.scale, scale);
        let made = remake(&client, file).0;
        assert_eq!(made, settings.resolution(Some(w), Some(h)));
        if choice != 0 {
            assert!(made.0 > w, "{scale:?} makes it bigger: {made:?}");
        }
    }
}

// leaf: audit-options-thumbnails-appearance-thumbnail-ui-scale-supersampling
#[test]
fn the_supersampling_scales_the_thumbnails_made() {
    let client = Media::basic();
    client.search("system:everything");
    let (file, (w, h)) = find(&client, |i| {
        i.mime == hydrus_core::Mime::ImageJpeg && i.width.is_some_and(|w| w > 1000)
    });
    let (before, _) = remake(&client, file);
    assert_eq!(
        before,
        client
            .store
            .snapshot()
            .thumbnails
            .resolution(Some(w), Some(h))
    );
    number(
        &client,
        "Thumbnail UI-scale supersampling %: ",
        (100, 800),
        200,
    );
    let settings = client.store.snapshot().thumbnails;
    assert_eq!(settings.dpr_percent, 200);
    let (after, _) = remake(&client, file);
    assert_eq!(after, settings.resolution(Some(w), Some(h)));
    assert!(
        after.0 > before.0 && after.1 > before.1,
        "{before:?} {after:?}"
    );
}

// leaf: audit-options-thumbnails-appearance-generate-video-thumbnails-this-in
#[test]
fn the_video_percentage_picks_the_frame_the_thumbnail_is_made_from() {
    let client = Media::basic();
    client.search("system:everything");
    let (file, _) = find(&client, |i| {
        i.mime == hydrus_core::Mime::AnimationGif && i.num_frames == Some(5)
    });
    let (_, at_35) = remake(&client, file);
    number(
        &client,
        "Generate video thumbnails this % in: ",
        (0, 100),
        100,
    );
    assert_eq!(client.store.snapshot().thumbnails.video_percentage_in, 100);
    let (_, at_end) = remake(&client, file);
    assert_ne!(at_35, at_end, "another frame");
    number(
        &client,
        "Generate video thumbnails this % in: ",
        (0, 100),
        35,
    );
    assert_eq!(remake(&client, file).1, at_35, "and back");
}

// leaf: audit-options-thumbnails-appearance-fade-thumbnails
#[test]
#[allow(clippy::float_cmp)] // (opacities set, not computed)
fn thumbnails_fade_in_only_if_asked_to() {
    use std::{cell::Cell, rc::Rc, time::Duration};

    use slint::Model as _;

    const LABEL: &str = "Fade thumbnails: ";
    let (dirs, store) = crate::options_gui_support::basic_store();
    // (the old renderer's pages, which the fade test replays)
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::thumbnail_appearance::Preferences {
                    new_renderer: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let client = Media::with(dirs, store);
    client.search("system:everything");
    let native = client.windows.get(0).unwrap();
    hydrus_gui::headless::render(&native, 850, 700);
    let now = Rc::new(Cell::new(Duration::ZERO));
    client.bound.rows.set_paint_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    let fresh_opacity = || {
        client.bound.rows.clear_thumbnail_cache();
        client.bound.rows.paint_tick(true, 0, 10);
        client.bound.rows.row_data(0).unwrap();
        client.bound.rows.wait();
        client
            .bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
            .fade_opacity
    };
    assert_eq!(fresh_opacity(), 0.0, "a new thumbnail starts clear");
    now.set(Duration::from_secs(1));
    client.bound.rows.paint_tick(true, 0, 10);

    let options = client.open_options();
    show_page(&options, "thumbnails");
    let (i, found) = row(&options, LABEL);
    assert_eq!((found.kind, found.checked), (1, true), "on by default");
    assert_eq!(box_of(&options, LABEL), "appearance");
    options.invoke_check_toggled(i, false);
    assert_eq!(fresh_opacity(), 0.0, "not before apply");
    options.invoke_apply();
    options.hide().unwrap();
    assert!(
        !client
            .store
            .read(hydrus_store::settings::get::<hydrus_store::thumbnail_appearance::Preferences>)
            .unwrap()
            .fade
    );
    assert_eq!(fresh_opacity(), 1.0, "shown at once");
}

// The fade of a new thumbnail follows the opacity the reference's new
// thumbnail renderer was recorded drawing at fixed times after the thumbnail
// appeared (`new_fade.samples`, with the animation clock held still), and
// "fade thumbnails" off shows it at full opacity at once.
#[test]
#[allow(clippy::float_cmp)] // (opacities set, not computed)
fn thumbnail_fade_follows_the_recorded_opacity_curve() {
    use std::{cell::Cell, rc::Rc, time::Duration};

    use slint::Model as _;

    let recorded = hydrus_testkit::fixture_json("thumbnail_appearance.json");
    let (dirs, store) = crate::options_gui_support::basic_store();
    let client = Media::with(dirs, store);
    client.search("system:everything");
    let native = client.windows.get(0).unwrap();
    hydrus_gui::headless::render(&native, 850, 700);
    let now = Rc::new(Cell::new(Duration::ZERO));
    client.bound.rows.set_paint_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    let opacity = || {
        client
            .bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
            .fade_opacity
    };
    for sample in recorded["new_fade"]["samples"].as_array().unwrap() {
        now.set(Duration::from_secs(10));
        client.bound.rows.clear_thumbnail_cache();
        client.bound.rows.paint_tick(true, 0, 10);
        client.bound.rows.row_data(0).unwrap();
        client.bound.rows.wait();
        assert_eq!(opacity(), 0.0, "a new thumbnail starts clear");
        now.set(Duration::from_secs(10) + Duration::from_secs_f64(sample["elapsed"].as_f64().unwrap()));
        client.bound.rows.paint_tick(true, 0, 10);
        let expected = sample["opacity"].as_f64().unwrap();
        assert!(
            (f64::from(opacity()) - expected).abs() < 0.01,
            "at {}: {} vs {expected}",
            sample["elapsed"],
            opacity()
        );
    }
}
