//! Stills drawn at their zoom as the reference draws them: the part that
//! shows, cut out and resized with its file type's qualities (area
//! shrinking, Lanczos growing, by default), to exactly the device pixels
//! it covers.

use hydrus_core::Mime;
use hydrus_core::media_viewer::MediaViewerSettings;
use hydrus_gui::still::{Plan, plan, render};
use hydrus_media::Raster;
use hydrus_media::resample::{Interpolation, resize};

fn rules() -> hydrus_core::media_viewer::ZoomRules {
    MediaViewerSettings::default().view(Mime::ImageJpeg).zoom
}

#[test]
fn the_part_showing_is_resized_to_the_pixels_it_covers() {
    // fitted, shrunk: all of it, by area
    assert_eq!(
        plan((0, 0, 1000, 750), (1000, 750), 1.0, (4000, 3000), &rules()),
        Some(Plan {
            clip: (0, 0, 4000, 3000),
            target: (1000, 750),
            interpolation: Interpolation::Area,
            rect: (0.0, 0.0, 1000.0, 750.0),
        })
    );
    // at 100%, nothing to resize
    assert_eq!(
        plan(
            (-333, -250, 4000, 3000),
            (1000, 750),
            1.0,
            (4000, 3000),
            &rules()
        ),
        None
    );
    // grown and panned: the whole pixels showing, by Lanczos
    assert_eq!(
        plan((-100, -50, 600, 400), (400, 300), 1.0, (300, 200), &rules()),
        Some(Plan {
            clip: (50, 25, 200, 150),
            target: (400, 300),
            interpolation: Interpolation::Lanczos4,
            rect: (0.0, 0.0, 400.0, 300.0),
        })
    );
    // a part pixel at the edge is included whole
    let fraction = plan((10, 10, 300, 300), (200, 200), 1.0, (1000, 1000), &rules()).unwrap();
    assert_eq!(fraction.clip, (0, 0, 634, 634));
    assert_eq!(fraction.target, (190, 190));
    assert_eq!(fraction.rect, (10.0, 10.0, 190.0, 190.0));
    // device pixels, at a ratio of 2
    let doubled = plan((0, 0, 500, 375), (500, 375), 2.0, (4000, 3000), &rules()).unwrap();
    assert_eq!(doubled.target, (1000, 750));
    assert_eq!(doubled.rect, (0.0, 0.0, 500.0, 375.0));
    // off the canvas, nothing
    assert_eq!(
        plan(
            (-2000, 0, 1000, 750),
            (1000, 750),
            1.0,
            (4000, 3000),
            &rules()
        ),
        None
    );
    // the file type's qualities: linear growing, nearest (as Slint draws)
    // not at all
    let mut linear = rules();
    linear.scale_up_quality = 1;
    let grown = plan((0, 0, 600, 400), (600, 400), 1.0, (300, 200), &linear).unwrap();
    assert_eq!(grown.interpolation, Interpolation::Linear);
    linear.scale_down_quality = 0;
    assert_eq!(
        plan((0, 0, 1000, 750), (1000, 750), 1.0, (4000, 3000), &linear),
        None
    );
}

#[test]
fn rendering_cuts_out_the_part_and_resizes_it() {
    // a 4x3 RGB still, each pixel distinct
    let data: Vec<u8> = (0..36).collect();
    let still = Raster::new(4, 3, 3, data).unwrap();
    let cut = Plan {
        clip: (1, 1, 2, 2),
        target: (2, 2),
        interpolation: Interpolation::Area,
        rect: (0.0, 0.0, 2.0, 2.0),
    };
    let part = render(&still, &cut);
    assert_eq!(
        part.data(),
        &[15, 16, 17, 18, 19, 20, 27, 28, 29, 30, 31, 32]
    );
    let grown = render(
        &still,
        &Plan {
            target: (5, 5),
            interpolation: Interpolation::Lanczos4,
            ..cut
        },
    );
    assert_eq!(grown, resize(&part, 5, 5, Interpolation::Lanczos4));
}
