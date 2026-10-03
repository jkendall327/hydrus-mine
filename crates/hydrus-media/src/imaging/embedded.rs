//! What the reference's "show detailed embedded file metadata" window shows
//! of a file (`ShowFileEmbeddedMetadata`, `ReviewFileEmbeddedMetadata`):
//! its EXIF rows, its XMP and IPTC rendered as text, its human-readable
//! text, and extra rows (software, subsampling, colour, DPI, JFIF,
//! compression), each as Pillow reads the file. Checked against
//! `oracle/dump_embedded_metadata.py`. PDFs reuse the document decoder for
//! their Author, Title, Subject and Keywords fields.

use hydrus_core::Mime;

use super::decode::{self, InfoValue};
use super::exif::{self, PyValue};
use super::exif_tags::{GPS_TAGS, TAGS};
use super::metadata;
use super::pil::Format;

/// An EXIF entry, separating the displayed value from the raw clipboard value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifRow {
    /// Numeric tag id, shown as decimal in the list.
    pub id: u16,
    /// Pillow's tag name, or "Unknown".
    pub label: String,
    /// Value shown, with byte lengths and visible NUL markers.
    pub value: String,
    /// Raw value copied: plain hex for bytes, unchanged text otherwise.
    pub copy: String,
}

/// The window's boxes: each `None` when the reference leaves it out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbeddedMetadata {
    /// Each EXIF row (id, label, value), sorted by id as the list sorts.
    pub exif: Option<Vec<ExifRow>>,
    pub xmp: Option<String>,
    pub iptc: Option<String>,
    /// The human-readable text box's.
    pub text: Option<String>,
    /// The "extra info" rows: each label and value.
    pub extra: Vec<(String, String)>,
}

const EXIF_MIMES: &[Mime] = &[
    Mime::ImageJpeg,
    Mime::ImageJxl,
    Mime::ImageTiff,
    Mime::ImagePng,
    Mime::ImageWebp,
    Mime::AnimationJxl,
    Mime::AnimationApng,
    Mime::AnimationWebp,
    Mime::ImageAvif,
    Mime::ImageAvifSequence,
    Mime::ImageHeic,
    Mime::ImageHeif,
    Mime::ImageHeicSequence,
    Mime::ImageHeifSequence,
];

const XMP_MIMES: &[Mime] = &[
    Mime::ImageJpeg,
    Mime::ImageTiff,
    Mime::ImagePng,
    Mime::ImageWebp,
    Mime::AnimationWebp,
    Mime::AnimationApng,
    Mime::ImageAvif,
    Mime::ImageAvifSequence,
    Mime::ImageHeic,
    Mime::ImageHeif,
    Mime::ImageHeicSequence,
    Mime::ImageHeifSequence,
];

const IPTC_MIMES: &[Mime] = &[Mime::ImageJpeg, Mime::ImageTiff];

/// (and software/source, which the reference looks for in the same files)
const TEXT_MIMES: &[Mime] = &[
    Mime::ImageJpeg,
    Mime::ImageJxl,
    Mime::ImagePng,
    Mime::ImageBmp,
    Mime::ImageWebp,
    Mime::ImageTiff,
    Mime::ImageIcon,
    Mime::ImageGif,
    Mime::ImageAvif,
    Mime::AnimationJxl,
    Mime::AnimationGif,
    Mime::AnimationApng,
    Mime::AnimationWebp,
];

/// Whether the reference reads a file of this type for the window at all.
pub fn looks_at(mime: Mime) -> bool {
    mime == Mime::ApplicationPdf
        || [EXIF_MIMES, XMP_MIMES, IPTC_MIMES, TEXT_MIMES]
            .iter()
            .any(|set| set.contains(&mime))
}

/// `render_dict` over (key, value) rows: sorted by key, each "key:" and
/// its value's lines indented under it.
fn render(rows: &[(String, Rendered)], depth: usize) -> Option<String> {
    let mut rows: Vec<&(String, Rendered)> = rows.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let indent = "    ";
    let texts: Vec<String> = rows
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Rendered::Dict(inner) => render(inner, depth + 1)
                    .unwrap_or_else(|| format!("{}empty/unknown", indent.repeat(depth + 1))),
                Rendered::Text(text) => crate::text::python_splitlines(text)
                    .into_iter()
                    .map(|line| format!("{}{line}", indent.repeat(depth + 1)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            format!("{}{key}:\n{value}", indent.repeat(depth))
        })
        .collect();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

/// A value `render_dict` shows: text, or a nested dict.
#[derive(Debug, Clone)]
enum Rendered {
    Text(String),
    Dict(Vec<(String, Rendered)>),
}

/// `GetXMPDict` (BeautifulSoup's XML tree of the packet: each element's
/// attributes, namespace declarations among them, and its child elements
/// or its text), rendered; `None` if the packet isn't UTF-8.
fn xmp_text(xmp: &[u8]) -> Option<String> {
    fn value(node: roxmltree::Node<'_, '_>) -> Vec<(String, Rendered)> {
        let mut out: Vec<(String, Rendered)> = Vec::new();
        let mut set = |key: String, value: Rendered| match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v = value,
            None => out.push((key, value)),
        };
        if node.is_element() {
            let inherited: Vec<(Option<&str>, &str)> = node
                .parent()
                .map(|p| p.namespaces().map(|n| (n.name(), n.uri())).collect())
                .unwrap_or_default();
            for ns in node.namespaces() {
                if inherited.contains(&(ns.name(), ns.uri())) {
                    continue;
                }
                let key = ns
                    .name()
                    .map_or_else(|| "xmlns".to_owned(), |p| format!("xmlns:{p}"));
                let v = ns.uri().trim();
                if !v.is_empty() {
                    set(key, Rendered::Text(v.to_owned()));
                }
            }
            for attribute in node.attributes() {
                let key = match attribute
                    .namespace()
                    .and_then(|uri| node.lookup_prefix(uri))
                {
                    Some(prefix) => format!("{prefix}:{}", attribute.name()),
                    None => attribute.name().to_owned(),
                };
                let v = attribute.value().trim();
                if !v.is_empty() {
                    set(key, Rendered::Text(v.to_owned()));
                }
            }
        }
        let children: Vec<roxmltree::Node<'_, '_>> = node
            .children()
            .filter(roxmltree::Node::is_element)
            .collect();
        if children.is_empty() {
            let text: String = node
                .descendants()
                .filter(roxmltree::Node::is_text)
                .filter_map(|n| n.text())
                .collect();
            if !text.is_empty() {
                set("text".to_owned(), Rendered::Text(text));
            }
        } else {
            for child in children {
                let child_value = value(child);
                if child_value.is_empty() {
                    continue;
                }
                set(
                    child.tag_name().name().to_owned(),
                    Rendered::Dict(child_value),
                );
            }
        }
        out
    }
    let end = xmp
        .iter()
        .rposition(|&b| b != 0 && b != b' ')
        .map_or(0, |i| i + 1);
    let text = std::str::from_utf8(&xmp[..end]).ok()?;
    let document = roxmltree::Document::parse(text).ok();
    let root = match &document {
        Some(document) => value(document.root()),
        None => Vec::new(),
    };
    render(&[("[document]".to_owned(), Rendered::Dict(root))], 0)
        .or_else(|| Some("XMP data appears to be empty!".to_owned()))
}

/// `WashPilImageInfoDictForHumanReadableMetadata`'s rows: Pillow's text
/// `info` less what isn't human-readable (the last value of each key).
fn human_readable(text_info: &[(String, InfoValue)]) -> Vec<(String, Rendered)> {
    let mut out: Vec<(String, Rendered)> = Vec::new();
    for (key, value) in text_info {
        let InfoValue::Text(text) = value else {
            // (bytes aren't rendered)
            out.retain(|(k, _)| k != key);
            continue;
        };
        if crate::tools::NOT_HUMAN_READABLE.contains(&key.as_str()) || key.starts_with("Thumb::") {
            continue;
        }
        if (key == "comment" || key == "Comment") && metadata::software_from_comment(text).is_some()
        {
            out.retain(|(k, _)| k != key);
            continue;
        }
        match out.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = Rendered::Text(text.clone()),
            None => out.push((key.clone(), Rendered::Text(text.clone()))),
        }
    }
    out
}

/// `GetSoftwareSourceFromPilInfo`: the Software, comment's "Created with
/// ...", Creator and Source entries, deduplicated, joined by " / ".
fn software_source(text_info: &[(String, InfoValue)]) -> Option<String> {
    let get = |key: &str| {
        text_info
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                InfoValue::Text(t) => Some(t.clone()),
                InfoValue::Bytes => None,
            })
    };
    let mut components: Vec<String> = Vec::new();
    components.extend(["Software", "software"].iter().filter_map(|k| get(k)));
    if let Some(comment) = get("Comment").or_else(|| get("comment")) {
        components.extend(metadata::software_from_comment(&comment).map(str::to_owned));
    }
    components.extend(
        [
            "Creator", "creator", "Source", "source", "Creator", "Source",
        ]
        .iter()
        .filter_map(|k| get(k)),
    );
    let mut seen: Vec<String> = Vec::new();
    for c in components {
        let c = metadata::py_strip(&c).to_owned();
        if !c.is_empty() && !seen.contains(&c) {
            seen.push(c);
        }
    }
    (!seen.is_empty()).then(|| seen.join(" / "))
}

/// A number as Python shows it in a DPI: an int, or a float's repr.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Num {
    Int(i64),
    Float(f64),
    /// An `IFDRational`.
    Rational(i64, i64),
}

impl Num {
    fn value(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
            Num::Rational(n, d) => {
                if d == 0 {
                    f64::NAN
                } else {
                    n as f64 / d as f64
                }
            }
        }
    }

    fn py(self) -> String {
        match self {
            Num::Int(i) => i.to_string(),
            Num::Float(f) => PyValue::Float(f).py_str(),
            Num::Rational(n, d) => PyValue::Rational(n, d).py_str(),
        }
    }
}

/// `render_dpi_tuple`: one number when both agree, else the pair.
#[allow(clippy::float_cmp)] // (compared as Python compares them)
fn render_dpi((x, y): (Num, Num)) -> String {
    if x.value() == y.value() {
        x.py()
    } else {
        format!("({}, {})", x.py(), y.py())
    }
}

/// What the window's extra rows read from Pillow's `info`.
#[derive(Debug, Default)]
struct Info {
    jfif: Option<u16>,
    jfif_unit: Option<u8>,
    jfif_density: Option<(u16, u16)>,
    dpi: Option<(Num, Num)>,
    resolution: Option<(Num, Num)>,
    compression: Option<String>,
    progressive: bool,
    exif: Option<Vec<u8>>,
}

/// A JPEG's APP0 (JFIF), APP1 (EXIF) and frame markers, as
/// `JpegImagePlugin` reads them, and its DPI.
fn jpeg_info(data: &[u8]) -> Info {
    let mut info = Info::default();
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xFF || marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            i += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        let body = data
            .get(i + 4..(i + 2 + len).min(data.len()))
            .unwrap_or_default();
        match marker {
            0xE0 if body.starts_with(b"JFIF") => {
                if let Some(v) = body.get(5..7) {
                    info.jfif = Some(u16::from_be_bytes([v[0], v[1]]));
                }
                if let (Some(&unit), Some(d)) = (body.get(7), body.get(8..12)) {
                    let density = (
                        u16::from_be_bytes([d[0], d[1]]),
                        u16::from_be_bytes([d[2], d[3]]),
                    );
                    match unit {
                        1 => {
                            info.dpi =
                                Some((Num::Int(density.0.into()), Num::Int(density.1.into())));
                        }
                        2 => {
                            info.dpi = Some((
                                Num::Float(f64::from(density.0) * 2.54),
                                Num::Float(f64::from(density.1) * 2.54),
                            ));
                        }
                        _ => {}
                    }
                    info.jfif_unit = Some(unit);
                    info.jfif_density = Some(density);
                }
            }
            0xE1 if body.starts_with(b"Exif\x00\x00") => match &mut info.exif {
                Some(exif) => exif.extend_from_slice(&body[6..]),
                None => info.exif = Some(body.to_vec()),
            },
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                if matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE) {
                    info.progressive = true;
                }
            }
            _ => {}
        }
        i += 2 + len;
    }
    // (no DPI in the header: the EXIF's, else 72)
    if info.dpi.is_none()
        && let Some(blob) = &info.exif
    {
        let fields = exif::merged_fields(strip_exif_prefix(blob), None, None);
        let field = |tag: u16| {
            fields
                .iter()
                .find(|(t, _)| *t == tag)
                .map(|(_, v)| v.clone())
        };
        let dpi = match (field(0x0128), field(0x011A)) {
            (Some(PyValue::Int(unit)), Some(x)) => {
                let x = match x {
                    PyValue::Rational(n, d) if d != 0 => Some(Num::Rational(n, d)),
                    PyValue::Int(i) => Some(Num::Int(i)),
                    PyValue::Float(f) if !f.is_nan() => Some(Num::Float(f)),
                    _ => None,
                };
                x.map(|x| {
                    if unit == 3 {
                        Num::Float(x.value() * 2.54)
                    } else {
                        x
                    }
                })
            }
            _ => None,
        };
        info.dpi = Some(match dpi {
            Some(d) => (d, d),
            None => (Num::Int(72), Num::Int(72)),
        });
    }
    info
}

fn strip_exif_prefix(mut data: &[u8]) -> &[u8] {
    while let Some(rest) = data.strip_prefix(b"Exif\x00\x00") {
        data = rest;
    }
    data
}

/// A PNG's pHYs (DPI) and EXIF (an eXIf chunk, or a "Raw profile type
/// exif" text's hex).
fn png_info(data: &[u8], text_info: &[(String, InfoValue)]) -> Info {
    let mut info = Info::default();
    let mut i = 8;
    while i + 8 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let kind = &data[i + 4..i + 8];
        let body = data
            .get(i + 8..(i + 8 + len).min(data.len()))
            .unwrap_or_default();
        match kind {
            b"pHYs" if body.len() >= 9 && body[8] == 1 => {
                let px = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                let py = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
                info.dpi = Some((
                    Num::Float(f64::from(px) * 0.0254),
                    Num::Float(f64::from(py) * 0.0254),
                ));
            }
            b"eXIf" => {
                let mut blob = b"Exif\x00\x00".to_vec();
                blob.extend_from_slice(body);
                info.exif = Some(blob);
            }
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    if info.exif.is_none()
        && let Some((_, InfoValue::Text(raw))) =
            text_info.iter().find(|(k, _)| k == "Raw profile type exif")
    {
        let hex: String = raw.split('\n').skip(3).collect();
        info.exif = hex::decode(hex.trim()).ok();
    }
    info
}

/// A WebP's EXIF chunk.
fn webp_info(data: &[u8]) -> Info {
    let mut info = Info::default();
    let mut i = 12;
    while i + 8 <= data.len() {
        let len = u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        if &data[i..i + 4] == b"EXIF" {
            info.exif = data
                .get(i + 8..(i + 8 + len).min(data.len()))
                .map(<[u8]>::to_vec);
        }
        i += 8 + len + (len & 1);
    }
    info
}

/// A BMP's compression and DPI (pixels per metre).
fn bmp_info(data: &[u8]) -> Info {
    let mut info = Info::default();
    let u32_at = |at: usize| {
        data.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let header = u32_at(14).unwrap_or(0);
    if header == 12 {
        info.compression = Some("0".into());
    } else if matches!(header, 40 | 52 | 56 | 64 | 108 | 124) {
        info.compression = u32_at(30).map(|c| c.to_string());
        if let (Some(x), Some(y)) = (u32_at(38), u32_at(42)) {
            info.dpi = Some((
                Num::Float(f64::from(x) / 39.3701),
                Num::Float(f64::from(y) / 39.3701),
            ));
        }
    }
    info
}

/// A TIFF's compression and resolution, from its first IFD.
fn tiff_info(data: &[u8]) -> Info {
    let mut info = Info {
        exif: Some(data.to_vec()),
        ..Info::default()
    };
    let fields = exif::merged_fields(data, None, None);
    let field = |tag: u16| {
        fields
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, v)| v.clone())
    };
    let compression = match field(259) {
        Some(PyValue::Int(c)) => c,
        _ => 1,
    };
    info.compression = Some(
        match compression {
            2 => "tiff_ccitt",
            3 => "group3",
            4 => "group4",
            5 => "tiff_lzw",
            6 => "tiff_jpeg",
            7 => "jpeg",
            8 => "tiff_adobe_deflate",
            32771 => "tiff_raw_16",
            32773 => "packbits",
            32809 => "tiff_thunderscan",
            32946 => "tiff_deflate",
            34676 => "tiff_sgilog",
            34677 => "tiff_sgilog24",
            34925 => "lzma",
            50000 => "zstd",
            50001 => "webp",
            _ => "raw",
        }
        .to_owned(),
    );
    let res = |tag: u16| match field(tag) {
        Some(PyValue::Rational(n, d)) => Num::Rational(n, d),
        Some(PyValue::Int(i)) => Num::Int(i),
        Some(PyValue::Float(f)) => Num::Float(f),
        _ => Num::Int(1),
    };
    let (x, y) = (res(282), res(283));
    if x.value() != 0.0 && y.value() != 0.0 {
        match field(296) {
            Some(PyValue::Int(2)) => info.dpi = Some((x, y)),
            Some(PyValue::Int(3)) => {
                info.dpi = Some((Num::Float(x.value() * 2.54), Num::Float(y.value() * 2.54)));
            }
            None => {
                info.dpi = Some((x, y));
                info.resolution = Some((x, y));
            }
            _ => info.resolution = Some((x, y)),
        }
    }
    info
}

/// The window's contents for a file of this type (`has_icc_profile` as
/// the file's record has it): nothing for a type the reference doesn't
/// read, or a file it can't open.
pub fn embedded_metadata(data: &[u8], mime: Mime, has_icc_profile: bool) -> EmbeddedMetadata {
    let mut out = EmbeddedMetadata::default();
    if mime == Mime::ApplicationPdf {
        out.text = crate::formats::pdf::Document::open(data.to_vec())
            .and_then(|doc| doc.human_readable_metadata());
        return out;
    }
    if !looks_at(mime) {
        return out;
    }
    let Ok(opened) = decode::open(data) else {
        return out;
    };
    let format = opened.image.format;
    let info = match format {
        Some(Format::Jpeg) => jpeg_info(data),
        Some(Format::Png) => png_info(data, &opened.text_info),
        Some(Format::Webp) => webp_info(data),
        Some(Format::Bmp) => bmp_info(data),
        Some(Format::Tiff) => tiff_info(data),
        _ => Info::default(),
    };
    // EXIF (`GetEXIFDict`, for the formats Pillow reads it from)
    if EXIF_MIMES.contains(&mime)
        && matches!(
            format,
            Some(Format::Jpeg | Format::Tiff | Format::Png | Format::Webp)
        )
    {
        let fields = match &info.exif {
            Some(blob) => exif::merged_fields(strip_exif_prefix(blob), None, opened.xmp.as_deref()),
            None => exif::merged_fields(b"", None, opened.xmp.as_deref()),
        };
        let mut fields = fields;
        // (a TIFF is turned to its orientation as it loads, and the tag
        // dropped: `exif_transpose` in its `load_end`)
        if format == Some(Format::Tiff) {
            fields.retain(|(tag, value)| !(*tag == 274 && matches!(value, PyValue::Int(2..=8))));
        }
        let mut rows: Vec<(u16, PyValue)> = Vec::new();
        for (tag, value) in fields {
            match value {
                PyValue::Dict(items) => rows.extend(items),
                other => rows.push((tag, other)),
            }
        }
        if !rows.is_empty() {
            let mut shown: Vec<(ExifRow, (u16, String, String))> = rows
                .into_iter()
                .map(|(tag, value)| {
                    let label = TAGS
                        .iter()
                        .chain(GPS_TAGS)
                        .find(|(t, _)| *t == tag)
                        .map(|(_, l)| *l);
                    let (shown, sort) = match &value {
                        PyValue::Bytes(b) => (
                            format!(
                                "{}: {}",
                                hydrus_core::numbers::human_bytes(b.len() as u64),
                                hex::encode(b)
                            ),
                            hex::encode(b),
                        ),
                        other => {
                            let text = other.py_str().replace('\0', "[null]");
                            (text.clone(), text)
                        }
                    };
                    (
                        ExifRow {
                            id: tag,
                            label: label.unwrap_or("Unknown").to_owned(),
                            value: shown,
                            copy: match &value {
                                PyValue::Bytes(bytes) => hex::encode(bytes),
                                other => other.py_str(),
                            },
                        },
                        (
                            tag,
                            label.unwrap_or("zzz").to_lowercase(),
                            sort.to_lowercase(),
                        ),
                    )
                })
                .collect();
            shown.sort_by(|a, b| a.1.cmp(&b.1));
            out.exif = Some(shown.into_iter().map(|(row, _)| row).collect());
        }
    }
    if XMP_MIMES.contains(&mime) {
        out.xmp = opened.xmp.as_deref().and_then(xmp_text);
    }
    if IPTC_MIMES.contains(&mime) {
        out.iptc = opened
            .iptc
            .as_deref()
            .and_then(metadata::iptc_rows)
            .and_then(|rows| {
                let rows: Vec<(String, Rendered)> = rows
                    .into_iter()
                    .map(|(k, v)| (k, Rendered::Text(v)))
                    .collect();
                render(&rows, 0).or_else(|| Some("IPTC data appears to be empty!".to_owned()))
            });
    }
    if TEXT_MIMES.contains(&mime) {
        out.text = render(&human_readable(&opened.text_info), 0);
        if let Some(software) = software_source(&opened.text_info) {
            out.extra.push(("software/source".into(), software));
        }
    }
    // extra rows
    if format == Some(Format::Jpeg) {
        let quality = crate::jpeg::jpeg_quality(data);
        out.extra
            .push(("subsampling".into(), quality.subsampling.name().into()));
        out.extra.push((
            "progressive".into(),
            if info.progressive { "yes" } else { "no" }.into(),
        ));
    }
    let png = &opened.image.png;
    let mut talked_about_icc = false;
    if png.srgb {
        out.extra.push(("colour".into(), "sRGB".into()));
    } else if png.gamma.is_some() {
        let colour = if png.chromaticity.is_some() {
            "gamma + chromaticity"
        } else {
            "gamma"
        };
        out.extra.push(("colour".into(), colour.into()));
    } else if has_icc_profile {
        talked_about_icc = true;
        out.extra.push(("colour".into(), "icc profile".into()));
    }
    if !talked_about_icc && has_icc_profile {
        out.extra.push(("icc profile".into(), "yes".into()));
    }
    let mut stated_a_dpi = false;
    if let Some(dpi) = info.dpi {
        let text = render_dpi(dpi);
        if text != "1" {
            out.extra.push(("dpi".into(), text));
            stated_a_dpi = true;
        }
    }
    if info.jfif.is_some() || info.jfif_unit.is_some() || info.jfif_density.is_some() {
        let mut components: Vec<String> = Vec::new();
        if let Some(version) = info.jfif {
            components.push(
                match version {
                    257 => "v1.01",
                    258 => "v1.02",
                    _ => "unknown version",
                }
                .into(),
            );
        }
        let undefined = info.jfif_density == Some((1, 1)) && info.jfif_unit == Some(0);
        if !undefined {
            if let Some(density) = info.jfif_density
                && info.dpi.is_none()
            {
                let pair = (Num::Int(density.0.into()), Num::Int(density.1.into()));
                if render_dpi(pair) != "1" {
                    components.push(format!("({}, {}) DPI", density.0, density.1));
                    stated_a_dpi = true;
                }
            }
            if stated_a_dpi && let Some(unit) = info.jfif_unit {
                components.push(
                    match unit {
                        0 => "unitless DPI",
                        1 => "dots per inch",
                        2 => "dots per centimetre",
                        _ => "unknown DPI unit",
                    }
                    .into(),
                );
            }
        }
        out.extra.push(("jfif".into(), components.join(", ")));
    }
    if let Some(compression) = info.compression {
        out.extra.push(("compression".into(), compression));
    }
    if mime == Mime::ImageTiff
        && info.dpi.is_none()
        && let Some((x, y)) = info.resolution
    {
        let text = render_dpi((x, y));
        if text != "1" {
            out.extra
                .push(("TIFF DPI".into(), format!("({}, {})", x.py(), y.py())));
        }
    }
    out
}
