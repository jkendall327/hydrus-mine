//! The SVG stars a rating service can draw with: the installed ones and the
//! user's own (`static/star_shapes` of the database directory).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The size an SVG is drawn at before it is scaled to a star.
const DRAWN: u32 = 96;

thread_local! {
    static DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static DRAWN_SVGS: RefCell<HashMap<String, Option<slint::Image>>> = RefCell::new(HashMap::new());
}

/// The database directory the user's SVGs are looked for in, on this thread
/// (the reference refreshes them when its service editor opens, as this does).
pub(crate) fn set_dir(dir: &Path) {
    DIR.with(|d| *d.borrow_mut() = Some(dir.to_path_buf()));
    DRAWN_SVGS.with(|cache| cache.borrow_mut().clear());
}

/// The SVG called `name`, if there is one (a rating drawn with one that is
/// not there is drawn as the default star).
pub(crate) fn image(name: &str) -> Option<slint::Image> {
    if let Some(drawn) = DRAWN_SVGS.with(|cache| cache.borrow().get(name).cloned()) {
        return drawn;
    }
    let drawn = draw(name);
    DRAWN_SVGS.with(|cache| cache.borrow_mut().insert(name.to_owned(), drawn.clone()));
    drawn
}

fn draw(name: &str) -> Option<slint::Image> {
    let dir = DIR.with(|d| d.borrow().clone())?;
    let data = hydrus_store::star_shapes::data(&dir, name)?;
    let raster = hydrus_media::render_svg(&data, (DRAWN, DRAWN))?;
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        raster.data(),
        raster.width(),
        raster.height(),
    );
    Some(slint::Image::from_rgba8(pixels))
}
