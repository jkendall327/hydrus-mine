//! The SVG stars a rating service can draw with: the installed ones and the
//! user's own (`static/star_shapes` of the database directory), drawn as the
//! reference draws them (`_draw_icon_coloured_outlined`): the SVG's opaque
//! pixels in the fill colour over copies of them spread by the outline's
//! width in the border colour. Each star is one picture in its colours.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hydrus_store::services::PenBrush;

/// The size an SVG is drawn at before it is scaled to a star.
const DRAWN: usize = 96;
/// How far the border spreads at that size: the reference's outline is a
/// twelfth of the star, drawn about four fifths as wide.
const SPREAD: i64 = (DRAWN as i64 / 12) * 4 / 5;

type Key = (String, [u8; 3], [u8; 3]);

thread_local! {
    static DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// The SVG's opaque-pixel alphas, by name (none for one not there).
    static ALPHAS: RefCell<HashMap<String, Option<Vec<u8>>>> = RefCell::new(HashMap::new());
    static STARS: RefCell<HashMap<Key, slint::Image>> = RefCell::new(HashMap::new());
}

/// The database directory the user's SVGs are looked for in, on this thread
/// (the reference refreshes them when its service editor opens, as this does).
pub(crate) fn set_dir(dir: &Path) {
    DIR.with(|d| *d.borrow_mut() = Some(dir.to_path_buf()));
    ALPHAS.with(|a| a.borrow_mut().clear());
    STARS.with(|s| s.borrow_mut().clear());
}

fn alphas(name: &str) -> Option<Vec<u8>> {
    if let Some(known) = ALPHAS.with(|a| a.borrow().get(name).cloned()) {
        return known;
    }
    let drawn = (|| {
        let dir = DIR.with(|d| d.borrow().clone())?;
        let data = hydrus_store::star_shapes::data(&dir, name)?;
        let size = u32::try_from(DRAWN).ok()?;
        let raster = hydrus_media::render_svg(&data, (size, size))?;
        Some(raster.data().chunks_exact(4).map(|p| p[3]).collect())
    })();
    ALPHAS.with(|a| a.borrow_mut().insert(name.to_owned(), drawn.clone()));
    drawn
}

/// A picture of each star of `colours` drawn from the SVG `name`, or none
/// when there is no such SVG (the default star is drawn then).
pub(crate) fn pictures(name: &str, colours: &[PenBrush]) -> Vec<slint::Image> {
    let Some(base) = alphas(name) else {
        return Vec::new();
    };
    colours
        .iter()
        .map(|c| {
            let key = (name.to_owned(), c.pen.0, c.brush.0);
            if let Some(star) = STARS.with(|s| s.borrow().get(&key).cloned()) {
                return star;
            }
            let star = bake(&base, c.pen.0, c.brush.0);
            STARS.with(|s| s.borrow_mut().insert(key, star.clone()));
            star
        })
        .collect()
}

/// The fill over the border, from the SVG's alphas.
fn bake(base: &[u8], border: [u8; 3], fill: [u8; 3]) -> slint::Image {
    let n = DRAWN as i64;
    let mut offsets = Vec::new();
    for dy in -SPREAD..=SPREAD {
        for dx in -SPREAD..=SPREAD {
            if dx * dx + dy * dy <= SPREAD * SPREAD {
                offsets.push((dx, dy));
            }
        }
    }
    let mut pixels = Vec::with_capacity(DRAWN * DRAWN * 4);
    for y in 0..n {
        for x in 0..n {
            let at = |dx: i64, dy: i64| {
                let (px, py) = (x + dx, y + dy);
                if (0..n).contains(&px) && (0..n).contains(&py) {
                    f32::from(base[usize::try_from(py * n + px).unwrap_or(0)]) / 255.0
                } else {
                    0.0
                }
            };
            let inside = at(0, 0);
            let spread = offsets
                .iter()
                .map(|(dx, dy)| at(*dx, *dy))
                .fold(inside, f32::max);
            // the fill over the border
            let alpha = inside + spread * (1.0 - inside);
            for channel in 0..3 {
                let value = if alpha > 0.0 {
                    (f32::from(fill[channel]) * inside
                        + f32::from(border[channel]) * spread * (1.0 - inside))
                        / alpha
                } else {
                    0.0
                };
                pixels.push(value.round().clamp(0.0, 255.0) as u8);
            }
            pixels.push((alpha * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &pixels,
        u32::try_from(DRAWN).unwrap_or(96),
        u32::try_from(DRAWN).unwrap_or(96),
    ))
}
