//! Centered preview geometry after a raster has been accepted by its live canvas.
use hydrus_core::{
    Mime,
    media_viewer::{MediaViewerSettings, ZoomType, media_size, preview_canvas_zooms},
};

/// Logical-pixel image bounds; overflowing fill/100% modes are clipped by the pane.
pub fn rect(
    settings: &MediaViewerSettings,
    default_zoom: ZoomType,
    mime: Mime,
    resolution: Option<(u32, u32)>,
    canvas: (u32, u32),
    ratio: f64,
) -> (i32, i32, i32, i32) {
    let zooms = preview_canvas_zooms(settings, mime, resolution, canvas, ratio);
    let (width, height) = media_size(mime, resolution, zooms[&default_zoom]);
    let width = (f64::from(width) / ratio) as i32;
    let height = (f64::from(height) / ratio) as i32;
    (
        (canvas.0 as i32 - width).div_euclid(2),
        (canvas.1 as i32 - height).div_euclid(2),
        width,
        height,
    )
}
