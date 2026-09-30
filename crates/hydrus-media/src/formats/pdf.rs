//! PDF documents: the reference reads them through QtPdf (pdfium).
//!
//! - Resolution: pdfium's page size for the first page (MediaBox/CropBox/
//!   Rotate, inherited through the page tree, `float` arithmetic) at the
//!   reference's assumed 300 dpi. Exact.
//! - Human-readable metadata: a non-empty Title, Author, Subject or Keywords
//!   in the document information dictionary. Exact.
//! - Word count: the reference counts words in pdfium's extracted page text.
//!   We rebuild that text from the glyphs hayro interprets, with pdfium's
//!   spacing and line-break heuristics approximated (pdfium's text layout
//!   analysis depends on font metrics and object boundaries we do not have),
//!   so counts can differ on unusual layouts.
//! - Thumbnails: the first page rendered with hayro onto a transparent
//!   canvas of the target size, as QtPdf does with pdfium. Pixels differ.
//!
//! Password-protected documents (a non-empty user password) cannot be opened,
//! as with pdfium; documents with only an owner password open normally.

use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::hayro_syntax::Pdf;
use hayro::hayro_interpret::hayro_syntax::object::{Dict, Object};
use hayro::hayro_interpret::hayro_syntax::page::Page;
use hayro::hayro_interpret::hayro_syntax::xref::XRef;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache,
    InterpreterSettings, Paint, PathDrawMode, SoftMask, interpret_page,
};
use hayro::{RenderCache, RenderSettings};
use kurbo::{Affine, BezPath, Point, Rect, Shape};

use crate::formats::guarded;
use crate::imaging::Raster;

/// `PDF_ASSUMED_DPI` in the reference.
const ASSUMED_DPI: f64 = 300.0;

/// An opened PDF.
pub(crate) struct Document {
    pdf: Pdf,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("pages", &self.pdf.pages().len())
            .finish()
    }
}

impl Document {
    /// `QtLoadPDF`: `None` where the reference raises (unparseable, needs a
    /// password, no pages).
    pub(crate) fn open(data: Vec<u8>) -> Option<Self> {
        guarded(|| {
            let pdf = Pdf::new(data).ok()?;
            (!pdf.pages().is_empty()).then_some(Self { pdf })
        })
    }

    /// `GetPDFResolutionFromDocument`.
    pub(crate) fn resolution(&self) -> Option<(u32, u32)> {
        let page = self.pdf.pages().first()?;
        let (w, h) = page_size(&PageTree::new(page.raw(), self.pdf.xref()));
        let scale = ASSUMED_DPI / 72.0;
        let px = |v: f32| (f64::from(v) * scale).round_ties_even().max(0.0) as u32;
        Some((px(w), px(h)))
    }

    /// `GetHumanReadableEmbeddedMetadata(...) is not None`.
    pub(crate) fn has_human_readable_metadata(&self) -> bool {
        let meta = self.pdf.metadata();
        [&meta.author, &meta.title, &meta.subject, &meta.keywords]
            .into_iter()
            .flatten()
            .any(|bytes| !decode_text_string(bytes).is_empty())
    }

    /// The `num_words` of `GetPDFInfo`.
    pub(crate) fn word_count(&self) -> u64 {
        guarded(|| {
            let cache = InterpreterCache::new();
            let mut total = 0;
            for page in self.pdf.pages().iter() {
                let text = page_text(page, &cache, &self.pdf);
                total += count_words(&text);
            }
            Some(total)
        })
        .unwrap_or(0)
    }

    /// `GenerateThumbnailNumPyFromPDFPath`: the first page, stretched to
    /// exactly `target`, on a transparent background.
    pub(crate) fn render_first_page(&self, target: (u32, u32)) -> Option<Raster> {
        let (tw, th) = (u16::try_from(target.0).ok()?, u16::try_from(target.1).ok()?);
        guarded(|| {
            let page = self.pdf.pages().first()?;
            let (w, h) = page.render_dimensions();
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let settings = RenderSettings {
                x_scale: f32::from(tw) / w,
                y_scale: f32::from(th) / h,
                width: Some(tw),
                height: Some(th),
                bg_color: hayro::vello_cpu::color::palette::css::TRANSPARENT,
            };
            let pixmap = hayro::render(
                page,
                &RenderCache::new(),
                &InterpreterSettings::default(),
                &settings,
            );
            let data: Vec<u8> = pixmap
                .take_unpremultiplied()
                .into_iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect();
            Raster::new(target.0, target.1, 4, data).ok()
        })
    }
}

// ------------------------------------------------------------------ geometry

/// pdfium's `CFX_FloatRect`, normalised.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FloatRect {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

impl FloatRect {
    const ZERO: Self = Self {
        left: 0.0,
        bottom: 0.0,
        right: 0.0,
        top: 0.0,
    };

    fn is_empty(&self) -> bool {
        self.left >= self.right || self.bottom >= self.top
    }

    fn intersect(&self, other: &Self) -> Self {
        let r = Self {
            left: self.left.max(other.left),
            bottom: self.bottom.max(other.bottom),
            right: self.right.min(other.right),
            top: self.top.min(other.top),
        };
        if r.left > r.right || r.bottom > r.top {
            Self::ZERO
        } else {
            r
        }
    }
}

/// A page's dictionary and the chain of its ancestors, each read afresh
/// from the xref (a page dict as the page list hands it out will not
/// resolve its `/Parent`).
struct PageTree<'a> {
    dicts: Vec<Dict<'a>>,
}

impl<'a> PageTree<'a> {
    fn new(page: &Dict<'a>, xref: &'a XRef) -> Self {
        let first = page
            .obj_id()
            .and_then(|id| xref.get::<Dict<'a>>(id))
            .unwrap_or_else(|| page.clone());
        let mut dicts = vec![first];
        let mut seen = std::collections::HashSet::new();
        while dicts.len() < 1024 {
            let Some(parent) = dicts.last().and_then(|d| d.get_ref(b"Parent")) else {
                break;
            };
            if !seen.insert((parent.obj_number, parent.gen_number)) {
                break;
            }
            let Some(dict) = xref.get::<Dict<'a>>(parent.into()) else {
                break;
            };
            dicts.push(dict);
        }
        Self { dicts }
    }

    /// `CPDF_Page::GetPageAttr`: the value from the nearest dictionary that has `key`.
    fn attr(&self, key: &[u8]) -> Option<Object<'a>> {
        let dict = self.dicts.iter().find(|d| d.contains_key(key))?;
        dict.get::<Object<'_>>(key)
    }
}

fn number(obj: Object<'_>) -> Option<f32> {
    obj.into_number().map(|n| n.as_f32())
}

/// `CPDF_Page::GetBox`.
fn page_box(page: &PageTree<'_>, key: &[u8]) -> FloatRect {
    let Some(array) = page.attr(key).and_then(Object::into_array) else {
        return FloatRect::ZERO;
    };
    let values: Vec<f32> = array
        .iter::<Object<'_>>()
        .map(|o| number(o).unwrap_or(0.0))
        .collect();
    let [a, b, c, d] = values[..] else {
        return FloatRect::ZERO;
    };
    FloatRect {
        left: a.min(c),
        bottom: b.min(d),
        right: a.max(c),
        top: b.max(d),
    }
}

/// `CPDF_Page::GetPageRotation`: quarter turns, 0..4.
fn page_rotation(page: &PageTree<'_>) -> i64 {
    let value = page
        .attr(b"Rotate")
        .and_then(Object::into_number)
        .map_or(0, |n| n.as_f64().trunc() as i64);
    let r = (value / 90) % 4;
    if r < 0 { r + 4 } else { r }
}

/// `CPDF_Page::UpdateDimensions`: the page size in points.
fn page_size(page: &PageTree<'_>) -> (f32, f32) {
    let mut media = page_box(page, b"MediaBox");
    if media.is_empty() {
        media = FloatRect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
    }
    let crop = page_box(page, b"CropBox");
    let bbox = if crop.is_empty() {
        media
    } else {
        crop.intersect(&media)
    };
    let (w, h) = (bbox.right - bbox.left, bbox.top - bbox.bottom);
    if page_rotation(page) % 2 == 1 {
        (h, w)
    } else {
        (w, h)
    }
}

// ---------------------------------------------------------------- metadata

/// pdfium's `PDF_DecodeText`: UTF-16 with a BOM, UTF-8 with a BOM, else PDFDocEncoding.
fn decode_text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|&b| pdf_doc_char(b)).collect()
}

/// PDFDocEncoding (ISO 8859-1 with typographic characters in 0x18..0x1F, 0x80..0xA0).
fn pdf_doc_char(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x2022, 0x2020, 0x2021, 0x2026, 0x2014, 0x2013, 0x0192, 0x2044, 0x2039, 0x203A, 0x2212,
        0x2030, 0x201E, 0x201C, 0x201D, 0x2018, 0x2019, 0x201A, 0x2122, 0xFB01, 0xFB02, 0x0141,
        0x0152, 0x0160, 0x0178, 0x017D, 0x0131, 0x0142, 0x0153, 0x0161, 0x017E, 0xFFFD,
    ];
    const LOW: [u16; 8] = [
        0x02D8, 0x02C7, 0x02C6, 0x02D9, 0x02DD, 0x02DB, 0x02DA, 0x02DC,
    ];
    let code = match b {
        0x18..=0x1F => LOW[usize::from(b - 0x18)],
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        0xA0 => 0x20AC,
        0xAD => 0xFFFD,
        _ => u16::from(b),
    };
    char::from_u32(u32::from(code)).unwrap_or('\u{FFFD}')
}

// -------------------------------------------------------------------- text

/// One drawn glyph, in page space.
#[derive(Debug, Clone)]
struct PlacedGlyph {
    text: String,
    origin: Point,
    /// Where the pen ends up after this glyph.
    end: Point,
    /// The em size, in display units.
    size: f64,
    /// Advance width in 1/1000 em (pdfium's char width).
    width: f64,
    /// Vertical extent of the glyph box.
    bottom: f64,
    top: f64,
}

#[derive(Default)]
struct TextCollector {
    glyphs: Vec<PlacedGlyph>,
}

impl<'a> Device<'a> for TextCollector {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'a>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        _: &Paint<'a>,
        mode: &GlyphDrawMode,
    ) {
        // fill-and-stroke text is drawn twice; pdfium sees one character
        if matches!(mode, GlyphDrawMode::Stroke(_))
            && self
                .glyphs
                .last()
                .is_some_and(|g| g.origin == transform * glyph_transform * Point::ZERO)
        {
            return;
        }
        let m = transform * glyph_transform;
        let text = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => "\u{FFFD}".to_owned(),
        };
        let (width, bbox) = match glyph {
            Glyph::Outline(g) => (
                f64::from(g.advance_width().unwrap_or(0.0)),
                g.outline().bounding_box(),
            ),
            Glyph::Type3(_) => (500.0, Rect::new(0.0, 0.0, 500.0, 700.0)),
        };
        let origin = m * Point::ZERO;
        let end = m * Point::new(width, 0.0);
        let size = (m * Point::new(0.0, 1000.0) - origin).hypot();
        // vertical extent in display space; empty outlines (spaces) get the baseline
        let (y0, y1) = if bbox.area() > 0.0 {
            let r = m.transform_rect_bbox(bbox);
            (r.y0, r.y1)
        } else {
            (origin.y, origin.y)
        };
        self.glyphs.push(PlacedGlyph {
            text,
            origin,
            end,
            size,
            width,
            bottom: y0,
            top: y1,
        });
    }
}

fn page_text<'a>(page: &Page<'a>, cache: &InterpreterCache<'a>, pdf: &'a Pdf) -> String {
    // pdfium measures text in page space, before the page's /Rotate
    let mut context = Context::new(
        Affine::IDENTITY,
        Rect::new(0.0, 0.0, 1.0, 1.0),
        cache,
        pdf.xref(),
        InterpreterSettings::default(),
    );
    let mut collector = TextCollector::default();
    interpret_page(page, &mut context, &mut collector);
    layout_text(&collector.glyphs)
}

/// pdfium's `NormalizeThreshold`.
fn normalize_threshold(threshold: f64, t1: f64, t2: f64, t3: f64) -> f64 {
    if threshold < t1 {
        threshold / 2.0
    } else if threshold < t2 {
        threshold / 4.0
    } else if threshold < t3 {
        threshold / 5.0
    } else {
        threshold / 6.0
    }
}

/// A run of glyphs drawn one after another with no repositioning, which is
/// what pdfium sees as one text object in the common cases.
struct Run<'g> {
    glyphs: Vec<&'g PlacedGlyph>,
    bottom: f64,
    top: f64,
}

fn runs(glyphs: &[PlacedGlyph]) -> Vec<Run<'_>> {
    let mut out: Vec<Run<'_>> = Vec::new();
    for g in glyphs {
        let continues = out.last().and_then(|r| r.glyphs.last()).is_some_and(|p| {
            let tolerance = 0.01 * p.size.max(g.size);
            (g.origin - p.end).hypot() <= tolerance
        });
        if continues && let Some(run) = out.last_mut() {
            run.glyphs.push(g);
            run.bottom = run.bottom.min(g.bottom);
            run.top = run.top.max(g.top);
        } else {
            out.push(Run {
                glyphs: vec![g],
                bottom: g.bottom,
                top: g.top,
            });
        }
    }
    out
}

enum Break {
    None,
    Space,
    Line,
}

/// `ProcessInsertObject`, approximately: how the text of `this` joins `prev`.
fn join(prev: &Run<'_>, this: &Run<'_>) -> Break {
    let (Some(last), Some(first)) = (prev.glyphs.last(), this.glyphs.first()) else {
        return Break::None;
    };
    // EndHorizontalLine: tall enough boxes that do not overlap vertically
    let tall = |r: &Run<'_>| r.top - r.bottom > 4.5;
    if tall(prev) && tall(this) && this.top.min(prev.top) <= this.bottom.max(prev.bottom) {
        return Break::Line;
    }
    let last_width = last.width * last.size / 1000.0;
    let this_width = first.width * first.size / 1000.0;
    let threshold = last_width.max(this_width) / 4.0;
    let dy = first.origin.y - last.origin.y;
    let overlaps = |r: &Run<'_>, y: f64| r.bottom <= y && y <= r.top;
    if (dy > threshold * 2.0 || dy < threshold * -3.0)
        && dy.abs() >= 1.0
        && !overlaps(prev, first.origin.y)
        && !overlaps(this, last.origin.y)
    {
        return Break::Line;
    }
    if first.text.starts_with(' ') || last.text.ends_with(' ') {
        return Break::None;
    }
    let (last_units, this_units) = (last.width, first.width);
    let mut threshold2 = normalize_threshold(last_units.max(this_units), 400.0, 700.0, 800.0);
    threshold2 *= if last_units >= this_units {
        last.size
    } else {
        first.size
    };
    threshold2 /= 1000.0;
    if (threshold2 < 1.4881 && threshold2 > 1.4879)
        || (threshold2 < 1.39001 && threshold2 > 1.38999)
    {
        threshold2 *= 1.5;
    }
    // GenerateSpace, in display x
    let (last_pos, pos) = (last.origin.x, first.origin.x);
    if (last_pos + last_width - pos).abs() <= threshold2 {
        return Break::None;
    }
    let threshold_pos = threshold2 + last_width;
    let diff = pos - last_pos;
    if diff.abs() > threshold_pos || diff > this_width + last_width {
        Break::Space
    } else {
        Break::None
    }
}

/// The page text as pdfium's `CPDF_TextPage` would give it, approximately.
fn layout_text(glyphs: &[PlacedGlyph]) -> String {
    // pdfium drops a character repeated at (almost) the same spot among the
    // last few: fake bold and duplicated text layers
    let mut kept: Vec<PlacedGlyph> = Vec::with_capacity(glyphs.len());
    for g in glyphs {
        let tolerance = 0.07 * g.size;
        let duplicate = kept.iter().rev().take(7).any(|k| {
            k.text == g.text
                && (k.origin.x - g.origin.x).abs() < tolerance
                && (k.origin.y - g.origin.y).abs() < tolerance
        });
        if !duplicate {
            kept.push(g.clone());
        }
    }
    let runs = runs(&kept);
    let mut text = String::new();
    let mut line = String::new();
    let flush = |text: &mut String, line: &mut String| {
        if line.is_empty() {
            return;
        }
        if !text.is_empty() {
            text.push_str("\r\n");
        }
        // CloseTempLine collapses runs of spaces
        let mut prev_space = false;
        for c in line.chars() {
            if c == ' ' && prev_space {
                continue;
            }
            prev_space = c == ' ';
            text.push(c);
        }
        line.clear();
    };
    for (i, run) in runs.iter().enumerate() {
        if i > 0 {
            match join(&runs[i - 1], run) {
                Break::None => {}
                Break::Space => line.push(' '),
                Break::Line => flush(&mut text, &mut line),
            }
        }
        for g in &run.glyphs {
            line.push_str(&g.text);
        }
    }
    flush(&mut text, &mut line);
    text
}

/// Python's `str.isspace`.
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1C}'..='\u{1F}').contains(&c)
}

/// Python's `\w` for `str` patterns.
fn is_py_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The per-page counting in `GetPDFInfo`: punctuation to spaces, runs of two
/// or more whitespace characters to one space, then spaces + 1.
fn count_words(text: &str) -> u64 {
    let depunctuated: Vec<char> = text
        .chars()
        .map(|c| {
            if is_py_word(c) || is_py_space(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut despaced = String::new();
    let mut i = 0;
    while i < depunctuated.len() {
        let c = depunctuated[i];
        if is_py_space(c) {
            let mut j = i;
            while j < depunctuated.len() && is_py_space(depunctuated[j]) {
                j += 1;
            }
            if j - i >= 2 {
                despaced.push(' ');
            } else {
                despaced.push(c);
            }
            i = j;
        } else {
            despaced.push(c);
            i += 1;
        }
    }
    if despaced.is_empty() || despaced == " " {
        return 0;
    }
    despaced.matches(' ').count() as u64 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_counted_like_the_reference() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("!"), 0);
        assert_eq!(
            count_words("Hello there, this is a small test document!"),
            9
        );
        // a lone newline is not a space; \r\n is collapsed to one
        assert_eq!(count_words("one\ntwo"), 1);
        assert_eq!(count_words("one\r\ntwo"), 2);
        assert_eq!(count_words("Second page!\r\n spaced out "), 5);
    }

    #[test]
    fn text_strings_decode() {
        assert_eq!(decode_text_string(b"\xfe\xff\x00A\x00b"), "Ab");
        assert_eq!(decode_text_string(b"\xfe\xff"), "");
        assert_eq!(
            decode_text_string(b"caf\xe9 \x8dq\x8e \x93"),
            "café \u{201C}q\u{201D} \u{FB01}"
        );
    }
}
