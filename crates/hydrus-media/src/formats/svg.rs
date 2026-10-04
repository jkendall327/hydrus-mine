//! SVG files: the reference uses Qt's `QSvgRenderer`.
//!
//! - Resolution: `QSvgRenderer.defaultSize()`, the root `width`/`height` in
//!   pixels (Qt's 90 dpi unit conversions), else the `viewBox` size, rounded
//!   like `QSizeF::toSize()`. Exact for the common cases.
//! - Thumbnails: rendered with resvg into a transparent canvas of the
//!   target size, aspect ratio kept and centred as Qt's `KeepAspectRatio`
//!   does. Pixels differ from Qt's rasteriser.

use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

use crate::formats::archive::with_xml;
use crate::formats::guarded;
use crate::imaging::Raster;

/// `qRound`: round half away from zero.
fn q_round(v: f64) -> i64 {
    if v >= 0.0 {
        (v + 0.5).floor() as i64
    } else {
        (v - 0.5).ceil() as i64
    }
}

enum Length {
    Px(f64),
    Percent(f64),
}

fn parse_length(s: &str) -> Option<Length> {
    let s = s.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(s.len());
    // an 'e' may start the "em"/"ex" unit rather than an exponent
    let split = if s[..split].ends_with(['e', 'E']) && s[split..].starts_with(['m', 'x']) {
        split - 1
    } else {
        split
    };
    let value: f64 = s[..split].parse().ok()?;
    Some(match s[split..].trim() {
        "" | "px" => Length::Px(value),
        "pt" => Length::Px(value * 1.25),
        "pc" => Length::Px(value * 15.0),
        "mm" => Length::Px(value * 3.543_307),
        "cm" => Length::Px(value * 35.433_07),
        "in" => Length::Px(value * 90.0),
        "%" => Length::Percent(value),
        _ => return None,
    })
}

/// The document's default size, or `None` when Qt would report nothing useful.
pub(crate) fn resolution(data: &[u8]) -> Option<(u32, u32)> {
    with_xml(data, |doc| {
        let root = doc.root_element();
        if root.tag_name().name() != "svg" {
            return None;
        }
        let view_box: Option<(f64, f64)> = root.attribute("viewBox").and_then(|v| {
            let nums: Vec<f64> = v
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().ok())
                .collect::<Option<_>>()?;
            (nums.len() == 4).then(|| (nums[2], nums[3]))
        });
        let dim = |attr: &str, vb: Option<f64>| -> Option<f64> {
            match parse_length(root.attribute(attr)?)? {
                Length::Px(v) => Some(v),
                Length::Percent(p) => vb.map(|v| v * p / 100.0),
            }
        };
        let w = dim("width", view_box.map(|v| v.0)).or(view_box.map(|v| v.0))?;
        let h = dim("height", view_box.map(|v| v.1)).or(view_box.map(|v| v.1))?;
        let (w, h) = (q_round(w), q_round(h));
        Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))
    })
}

/// System fonts, loaded once: text in SVGs renders with them, as with Qt.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            // A system database can contain fonts while the default generic
            // families (Arial/Times New Roman/Courier New) are unresolved.
            // Preserve every resolvable desktop choice; fill only missing ones.
            db.load_font_data(include_bytes!("../../assets/OpenSans-Regular.ttf").to_vec());
            for (family, kind) in [
                (usvg::fontdb::Family::SansSerif, 0),
                (usvg::fontdb::Family::Serif, 1),
                (usvg::fontdb::Family::Monospace, 2),
            ] {
                if db
                    .query(&usvg::fontdb::Query {
                        families: &[family],
                        ..usvg::fontdb::Query::default()
                    })
                    .is_none()
                {
                    match kind {
                        0 => db.set_sans_serif_family("Open Sans"),
                        1 => db.set_serif_family("Open Sans"),
                        _ => db.set_monospace_family("Open Sans"),
                    }
                }
            }
            Arc::new(db)
        })
        .clone()
}

/// `GenerateThumbnailNumPyFromSVGPath`: the drawing fitted into `target`,
/// centred, on transparent black.
pub(crate) fn render(data: &[u8], target: (u32, u32)) -> Option<Raster> {
    guarded(|| {
        let options = usvg::Options {
            dpi: 90.0,
            fontdb: fonts(),
            // embedded (data: URL) images only; never read other files
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_string: Box::new(|_, _| None),
                ..usvg::ImageHrefResolver::default()
            },
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_data(data, &options).ok()?;
        let size = tree.size();
        let (tw, th) = (target.0 as f32, target.1 as f32);
        let scale = (tw / size.width()).min(th / size.height());
        let dx = (tw - size.width() * scale) / 2.0;
        let dy = (th - size.height() * scale) / 2.0;
        let mut pixmap = tiny_skia::Pixmap::new(target.0, target.1)?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
            &mut pixmap.as_mut(),
        );
        let pixels = pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        Raster::new(target.0, target.1, 4, pixels).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(
            resolution(br#"<svg width="200" height="120"/>"#),
            Some((200, 120))
        );
        assert_eq!(
            resolution(br#"<svg viewBox="0 0 64.5 32.2"/>"#),
            Some((65, 32))
        );
        assert_eq!(
            resolution(br#"<svg width="1in" height="10pt"/>"#),
            Some((90, 13))
        );
        assert_eq!(resolution(b"<html/>"), None);
    }
}
