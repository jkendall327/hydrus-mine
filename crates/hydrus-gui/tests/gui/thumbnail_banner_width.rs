//! A thumbnail whose tag banners are far wider than the thumbnail: the grid
//! draws them, at several scale factors, showing what the reference shows.

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy};
use slint::platform::WindowAdapter as _;
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

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

/// Gives the first file `tags` in "my tags".
fn tag_first_file(store: &Arc<Store>, tags: Vec<String>) {
    let file = {
        let mut page = SearchPage::new(store.clone());
        page.add_predicate("system:everything");
        page.results()[0]
    };
    let own = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |w| {
            for tag in &tags {
                let tag = hydrus_store::master::intern_tag(w.conn(), &Tag::new(tag).unwrap())?;
                w.update_mappings(
                    own,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &[file],
                )?;
            }
            Ok(())
        })
        .unwrap();
}

/// A thumbnail as drawn: its width and its RGBA pixels, in device pixels.
struct Drawing {
    width: usize,
    pixels: Vec<u8>,
}

fn row(d: &Drawing, y: usize) -> &[u8] {
    &d.pixels[y * d.width * 4..(y + 1) * d.width * 4]
}

/// How much two drawings of a thumbnail differ over `rows`: the mean
/// difference of each channel, `b` moved `shift` pixels left.
fn difference(a: &Drawing, b: &Drawing, shift: usize, rows: &[usize]) -> f64 {
    let (mut total, mut count) = (0u64, 0u64);
    for &y in rows {
        for (p, q) in row(a, y).iter().zip(&row(b, y)[shift * 4..]) {
            total += u64::from(p.abs_diff(*q));
            count += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // (a mean of a few thousand bytes)
    let mean = total as f64 / count.max(1) as f64;
    mean
}

/// Opens a page of every file, waits until the first thumbnail's banners
/// are painted, and draws it at each scale factor; returns the first
/// thumbnail's pixels in each drawing.
fn draw_first_thumbnail(
    windows: &headless::Windows,
    store: &Arc<Store>,
    scales: &[f32],
    want_bottom: &str,
) -> Vec<Drawing> {
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let native = windows.get(windows.count() - 1).unwrap();
    headless::render(&native, 1100, 700);
    let first = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    assert!(
        first().bottom.ends_with(want_bottom),
        "{:?}",
        &first().bottom[first().bottom.len().saturating_sub(80)..]
    );
    for _ in 0..50 {
        if first().paint.bottom == first().bottom {
            break;
        }
        bound.rows.paint_tick(true, 0, 10);
        bound.rows.wait();
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(first().paint.bottom, first().bottom);
    scales
        .iter()
        .map(|&scale_factor| {
            native
                .window()
                .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
            let pixels = headless::render(&native, 1920, 1080);
            assert!(pixels.iter().any(|&p| p != 0), "{scale_factor}");
            // (the first thumbnail, in device pixels)
            let at = |v: f32| (v * scale_factor).round() as usize;
            let left = at(ui.get_grid_origin_x() + ui.get_thumbnail_margin());
            let top = at(ui.get_grid_origin_y() + ui.get_thumbnail_margin());
            let (width, height) = (at(ui.get_thumbnail_width()), at(ui.get_thumbnail_height()));
            let pixels = (top..top + height)
                .flat_map(|y| {
                    let row = (y * 1920 + left) * 4;
                    pixels[row..row + width * 4].to_vec()
                })
                .collect();
            Drawing { width, pixels }
        })
        .collect()
}

fn pages(range: std::ops::RangeInclusive<usize>) -> Vec<String> {
    range.map(|i| format!("page:zoosyn_{i}")).collect()
}

/// A user's inbox crashed the release build (2026-10-15) in the software
/// renderer, drawing the bottom-right banner of a file whose page tags
/// joined to "...-zoosyn_119-zoosyn_120". Its box was as wide as its text and
/// ran off the thumbnail's left edge; the renderer casts every rectangle to
/// 16-bit device pixels, and panics on one that starts left of -32768.
#[test]
fn a_very_long_bottom_banner_is_drawn() {
    let (_dirs, store) = store();
    tag_first_file(&store, pages(1..=120));
    let windows = headless::init();
    draw_first_thumbnail(&windows, &store, &[1.0, 1.25, 1.5, 2.0, 3.0], "zoosyn_120");
}

/// Past the 16-bit range at every scale factor: ~4000 page tags make a
/// banner of ~45000 characters, hundreds of thousands of pixels wide. Only
/// its end shows, the same as a banner of a few hundred tags.
#[test]
fn a_banner_wider_than_the_renderer_can_address_shows_its_end() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let scales = [1.0, 2.0];
    tag_first_file(&store, pages(3_700..=4_000));
    let short = draw_first_thumbnail(&windows, &store, &scales, "zoosyn_4000");
    tag_first_file(&store, pages(1..=3_699));
    let long = draw_first_thumbnail(&windows, &store, &scales, "zoosyn_4000");
    for (scale, (short, long)) in scales.iter().zip(short.iter().zip(&long)) {
        // the end of the banner, as the reference shows it: the same, up to
        // how glyphs half a million pixels from the origin round (much
        // closer than the same banner a pixel to one side)
        let rows: Vec<usize> = (0..short.pixels.len() / (short.width * 4))
            .filter(|&y| row(short, y) != row(long, y))
            .collect();
        let changed = difference(short, long, 0, &rows);
        let moved = difference(short, short, 1, &rows);
        assert!(
            rows.is_empty() || changed * 2.0 < moved,
            "the drawing changed at {scale}: by {changed} in {rows:?}, against {moved}"
        );
    }
}
