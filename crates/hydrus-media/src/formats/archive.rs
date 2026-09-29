//! Zip-based formats: telling cbz, ugoira, krita, openraster, epub,
//! procreate and Office Open XML apart from plain zips, and reading their
//! metadata (`HydrusArchiveHandling`, `HydrusUgoiraHandling`,
//! `HydrusKritaHandling`, `HydrusORAHandling`, `HydrusProcreateHandling`,
//! `HydrusOfficeOpenXMLHandling`).
//!
//! Entry order, exact-name lookups and the heuristics' thresholds follow
//! Python's `zipfile` and the reference code.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use hydrus_core::Mime;

use crate::mimes::filename_has_image_ext;
use crate::text::human_sort;

/// One entry of the central directory, in `infolist()` order.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub encrypted: bool,
}

/// An open zip file.
pub(crate) struct Zip {
    archive: zip::ZipArchive<BufReader<File>>,
    entries: Vec<Entry>,
}

impl std::fmt::Debug for Zip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Zip({} entries)", self.entries.len())
    }
}

impl Zip {
    /// `zipfile.ZipFile(path)`; `None` if it is not a readable zip.
    pub(crate) fn open(path: &Path) -> Option<Self> {
        let file = File::open(path).ok()?;
        let mut archive = zip::ZipArchive::new(BufReader::new(file)).ok()?;
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let f = archive.by_index_raw(i).ok()?;
            entries.push(Entry {
                name: f.name().to_owned(),
                is_dir: f.is_dir(),
                encrypted: f.encrypted(),
            });
        }
        Some(Self { archive, entries })
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Non-directory file names, in order.
    pub(crate) fn file_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.name.clone())
            .collect()
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name == name)
    }

    /// Read a member by exact name (checking its CRC, as Python does).
    pub(crate) fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut f = self.archive.by_name(name).ok()?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).ok()?;
        Some(out)
    }

    /// `IsEncryptedZip`.
    pub(crate) fn is_encrypted(&self) -> bool {
        self.entries.iter().any(|e| e.encrypted)
    }
}

/// Parse XML the way ElementTree would accept it (DTDs allowed, BOMs and
/// UTF-16 handled) and run `f` on the document.
pub(crate) fn with_xml<T>(
    bytes: &[u8],
    f: impl FnOnce(&roxmltree::Document<'_>) -> Option<T>,
) -> Option<T> {
    let text: String = if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        String::from_utf8(rest.to_vec()).ok()?
    } else if bytes.starts_with(b"\xff\xfe") || bytes.starts_with(b"\xfe\xff") {
        let le = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&units).ok()?
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
        }
    };
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let doc = roxmltree::Document::parse_with_options(&text, opts).ok()?;
    f(&doc)
}

fn attr<'a>(node: &roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

// ------------------------------------------------------------ mime sniffing

/// `MimeFromOpenDocument`: a `mimetype` member naming krita/epub/openraster.
pub(crate) fn open_document_mime(zip: &mut Zip) -> Option<Mime> {
    let data = zip.read("mimetype")?;
    match std::str::from_utf8(&data).ok()? {
        "application/x-krita" => Some(Mime::ApplicationKrita),
        "application/epub+zip" => Some(Mime::ApplicationEpub),
        "image/openraster" => Some(Mime::ImageOpenraster),
        _ => None,
    }
}

/// `MimeFromMicrosoftOpenXMLDocument`: docx/xlsx/pptx from `[Content_Types].xml`.
pub(crate) fn office_mime(zip: &mut Zip) -> Option<Mime> {
    const KINDS: [(Mime, &str, &str); 3] = [
        (
            Mime::ApplicationDocx,
            "/word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        (
            Mime::ApplicationXlsx,
            "/xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        ),
        (
            Mime::ApplicationPptx,
            "/ppt/presentation.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        ),
    ];
    let data = zip.read("[Content_Types].xml")?;
    with_xml(&data, |doc| {
        let root = doc.root_element();
        let has = |tag: &str, key: &str, key_value: &str, ct: &str| {
            root.descendants().skip(1).any(|n| {
                n.is_element()
                    && n.tag_name().name() == tag
                    && attr(&n, key) == Some(key_value)
                    && attr(&n, "ContentType") == Some(ct)
            })
        };
        for (mime, part, ct) in KINDS {
            if has("Override", "PartName", part, ct) {
                return Some(mime);
            }
        }
        for (mime, _, ct) in KINDS {
            if has("Default", "Extension", "xml", ct) {
                return Some(mime);
            }
        }
        None
    })
}

// ---------------------------------------------------------------- procreate

fn procreate_objects(zip: &mut Zip) -> Option<Vec<plist::Value>> {
    let data = zip.read("Document.archive")?;
    let doc = plist::Value::from_reader(std::io::Cursor::new(data)).ok()?;
    doc.as_dictionary()?.get("$objects")?.as_array().cloned()
}

fn uid_index(v: &plist::Value) -> Option<usize> {
    usize::try_from(v.as_uid()?.get()).ok()
}

/// `ZipLooksLikeProcreate`: an NSKeyedArchiver plist whose root is a `SilicaDocument`.
pub(crate) fn looks_like_procreate(zip: &mut Zip) -> bool {
    let Some(objects) = procreate_objects(zip) else {
        return false;
    };
    let class_name = objects
        .get(1)
        .and_then(|o| o.as_dictionary()?.get("$class"))
        .and_then(uid_index)
        .and_then(|i| {
            objects
                .get(i)?
                .as_dictionary()?
                .get("$classname")?
                .as_string()
        });
    class_name == Some("SilicaDocument")
}

/// `GetProcreateResolution`.
pub(crate) fn procreate_resolution(zip: &mut Zip) -> Option<(u32, u32)> {
    let objects = procreate_objects(zip)?;
    let doc = objects.get(1)?.as_dictionary()?;
    let size = objects.get(uid_index(doc.get("size")?)?)?.as_string()?;
    let parts: Vec<&str> = size
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split(", ")
        .collect();
    let orientation = doc.get("orientation")?;
    let rotated = orientation
        .as_signed_integer()
        .is_some_and(|o| o == 3 || o == 4);
    let (w, h) = if rotated {
        (parts.get(1)?, parts.first()?)
    } else {
        (parts.first()?, parts.get(1)?)
    };
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

// ------------------------------------------------------------------- ugoira

/// `GetUgoiraFrameDataJSON`: the frame list from `animation.json`, raw.
fn ugoira_json_frames(zip: &mut Zip) -> Option<Vec<serde_json::Value>> {
    let data = zip.read("animation.json")?;
    let json: serde_json::Value = serde_json::from_slice(&data).ok()?;
    match json {
        serde_json::Value::Array(frames) => Some(frames),
        serde_json::Value::Object(mut o) => match o.remove("frames")? {
            serde_json::Value::Array(frames) => Some(frames),
            _ => None,
        },
        _ => None,
    }
}

/// Python's `'key' in frame` for the JSON values a frame can be.
fn json_contains(v: &serde_json::Value, key: &str) -> Option<bool> {
    match v {
        serde_json::Value::Object(o) => Some(o.contains_key(key)),
        serde_json::Value::String(s) => Some(s.contains(key)),
        serde_json::Value::Array(a) => Some(a.iter().any(|x| x.as_str() == Some(key))),
        _ => None,
    }
}

fn ugoira_json_is_valid(frames: &[serde_json::Value]) -> Option<bool> {
    if frames.is_empty() {
        return Some(false);
    }
    for f in frames {
        if !(json_contains(f, "delay")? && json_contains(f, "file")?) {
            return Some(false);
        }
    }
    Some(true)
}

/// `ZipLooksLikeUgoira`.
pub(crate) fn looks_like_ugoira(zip: &mut Zip) -> bool {
    if let Some(frames) = ugoira_json_frames(zip)
        && ugoira_json_is_valid(&frames) == Some(true)
    {
        return true;
    }
    if zip.entries().iter().any(|e| e.is_dir) {
        return false;
    }
    let mut ext_seen: Option<String> = None;
    let mut numbers = Vec::new();
    for e in zip.entries() {
        let Some((number, ext)) = e.name.rsplit_once('.') else {
            return false;
        };
        let ext = format!(".{ext}");
        if ext == ".js" || ext == ".json" {
            continue;
        }
        if !crate::mimes::is_image_ext(&ext) {
            return false;
        }
        match &ext_seen {
            None => ext_seen = Some(ext),
            Some(seen) if *seen != ext => return false,
            Some(_) => {}
        }
        numbers.push(number.to_owned());
    }
    if numbers.len() <= 1 {
        return false;
    }
    if numbers
        .iter()
        .enumerate()
        .any(|(i, n)| *n != format!("{i:06}"))
    {
        return false;
    }
    cover_page_readable(zip)
}

/// `GetFramePathsUgoira`: from the JSON if it has one, otherwise the sorted image names.
pub(crate) fn ugoira_frame_paths(zip: &mut Zip) -> Option<Vec<String>> {
    if let Some(frames) = ugoira_json_frames(zip) {
        let files: Option<Vec<String>> = frames
            .iter()
            .map(|f| f.get("file")?.as_str().map(str::to_owned))
            .collect();
        if let Some(files) = files {
            return Some(files);
        }
    }
    ugoira_zip_frame_paths(zip)
}

/// `GetFramePathsFromUgoiraZip`: image files, sorted by code point.
pub(crate) fn ugoira_zip_frame_paths(zip: &Zip) -> Option<Vec<String>> {
    let mut paths: Vec<String> = zip
        .file_names()
        .into_iter()
        .filter(|n| filename_has_image_ext(n))
        .collect();
    if paths.is_empty() {
        return None;
    }
    paths.sort();
    Some(paths)
}

/// Frame delays from `animation.json` (summed by the caller), if usable.
pub(crate) fn ugoira_json_delays(zip: &mut Zip) -> Option<Vec<serde_json::Value>> {
    let frames = ugoira_json_frames(zip)?;
    frames.iter().map(|f| f.get("delay").cloned()).collect()
}

// ---------------------------------------------------------------------- cbz

/// `GetCoverPagePath`: the first image by human sort, ignoring macOS junk.
pub(crate) fn cover_page_path(zip: &Zip) -> Option<String> {
    let mut names = zip.file_names();
    human_sort(&mut names);
    names
        .into_iter()
        .filter(|n| !n.starts_with("__MACOSX/"))
        .find(|n| filename_has_image_ext(n))
}

fn cover_page_readable(zip: &mut Zip) -> bool {
    match cover_page_path(zip) {
        Some(p) => zip.read(&p).is_some(),
        None => false,
    }
}

/// `ZipLooksLikeCBZ`: mostly numbered images, consistently named, few extras.
pub(crate) fn looks_like_cbz(zip: &mut Zip) -> bool {
    const OK_NAMES: [&str; 4] = ["md5sum", "comicbook.xml", "metadata.txt", "info.txt"];
    const OK_EXTS: [&str; 5] = [".sfv", ".nfo", ".txt", ".xml", ".json"];
    let mut dirs_to_images: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dirs_with_stuff: BTreeSet<String> = BTreeSet::new();
    let (mut weird, mut images, mut numbered) = (0usize, 0usize, 0usize);
    for e in zip.entries() {
        if e.is_dir || e.name.starts_with("__MACOSX/") {
            continue;
        }
        let (dir, file) = match e.name.rsplit_once('/') {
            Some((d, f)) => (d.to_owned(), f),
            None => (String::new(), e.name.as_str()),
        };
        dirs_with_stuff.insert(dir.clone());
        let file = file.to_lowercase();
        if OK_NAMES.contains(&file.as_str()) {
            continue;
        }
        if filename_has_image_ext(&file) {
            images += 1;
            if file.chars().any(|c| c.is_ascii_digit()) {
                numbered += 1;
            }
            dirs_to_images.entry(dir).or_default().insert(file);
        } else {
            match file.rsplit_once('.') {
                Some((_, ext)) if OK_EXTS.contains(&format!(".{ext}").as_str()) => weird += 1,
                _ => return false,
            }
        }
    }
    if numbered <= 1 {
        return false;
    }
    if !cover_page_readable(zip) {
        return false;
    }
    if !dirs_to_images.is_empty() {
        let scores: Vec<f64> = dirs_to_images
            .values()
            .map(|names| {
                let templates: BTreeSet<String> = names
                    .iter()
                    .map(|n| n.chars().filter(|c| !c.is_ascii_digit()).collect())
                    .collect();
                (templates.len() as f64 - 1.0) / names.len() as f64
            })
            .collect();
        let average = scores.iter().sum::<f64>() / scores.len() as f64;
        if average > 0.2 {
            return false;
        }
    }
    let dirs = dirs_with_stuff.len();
    if weird * dirs > 5 {
        return false;
    }
    images * dirs >= 1
}

// --------------------------------------------------------------------- epub

/// `os.path.dirname` (POSIX).
fn posix_dirname(p: &str) -> &str {
    let i = p.rfind('/').map_or(0, |i| i + 1);
    let head = &p[..i];
    if !head.is_empty() && head.chars().any(|c| c != '/') {
        head.trim_end_matches('/')
    } else {
        head
    }
}

/// `GetCoverPagePathFromEpub`: follow container.xml to the OPF's cover item.
pub(crate) fn epub_cover_path(zip: &mut Zip) -> Option<String> {
    const CONTAINER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
    const OPF_NS: &str = "http://www.idpf.org/2007/opf";
    let container = zip.read("META-INF/container.xml")?;
    let opf_path = with_xml(&container, |doc| {
        let rootfile = doc.root_element().descendants().skip(1).find(|n| {
            n.is_element()
                && n.tag_name().name() == "rootfile"
                && n.tag_name().namespace() == Some(CONTAINER_NS)
        })?;
        attr(&rootfile, "full-path").map(str::to_owned)
    })?;
    if !zip.contains(&opf_path) {
        return None;
    }
    let opf = zip.read(&opf_path)?;
    let href = with_xml(&opf, |doc| {
        let find = |tag: &str, key: &str, value: &str| {
            doc.root_element().descendants().skip(1).find(|n| {
                n.is_element()
                    && n.tag_name().name() == tag
                    && n.tag_name().namespace() == Some(OPF_NS)
                    && attr(n, key) == Some(value)
            })
        };
        let mut item = find("item", "properties", "cover-image");
        if item.is_none() {
            if let Some(meta) = find("meta", "name", "cover") {
                let id = attr(&meta, "content")?;
                item = find("item", "id", id);
            }
            if item.is_none() {
                item = find("item", "id", "cover");
            }
        }
        if item.is_none() {
            item = find("reference", "type", "cover");
        }
        attr(&item?, "href").map(str::to_owned)
    })?;
    let dir = posix_dirname(&opf_path);
    let path = if dir.is_empty() {
        href
    } else {
        format!("{dir}/{href}")
    };
    zip.contains(&path).then_some(path)
}

// ------------------------------------------------------- krita / openraster

/// `GetKraProperties`: the IMAGE element of maindoc.xml.
pub(crate) fn krita_resolution(zip: &mut Zip) -> Option<(u32, u32)> {
    let data = zip.read("maindoc.xml")?;
    with_xml(&data, |doc| {
        let image = doc.root_element().children().find(|n| {
            n.is_element()
                && n.tag_name().name() == "IMAGE"
                && n.tag_name().namespace() == Some("http://www.calligra.org/DTD/krita")
        })?;
        Some((
            attr(&image, "width")?.trim().parse().ok()?,
            attr(&image, "height")?.trim().parse().ok()?,
        ))
    })
}

/// `GetOraProperties`: the root `w`/`h` of stack.xml.
pub(crate) fn ora_resolution(zip: &mut Zip) -> Option<(u32, u32)> {
    let data = zip.read("stack.xml")?;
    with_xml(&data, |doc| {
        let root = doc.root_element();
        Some((
            attr(&root, "w")?.trim().parse().ok()?,
            attr(&root, "h")?.trim().parse().ok()?,
        ))
    })
}

// ------------------------------------------------------------ office xml

/// `OfficeDocumentWordCount`: `<Words>` in docProps/app.xml.
pub(crate) fn office_word_count(zip: &mut Zip) -> Option<u64> {
    let data = zip.read("docProps/app.xml")?;
    with_xml(&data, |doc| {
        let words = doc.root_element().children().find(|n| {
            n.is_element()
                && n.tag_name().name() == "Words"
                && n.tag_name().namespace()
                    == Some(
                        "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
                    )
        })?;
        words.text()?.trim().parse().ok()
    })
}

/// `PowerPointResolution`: slide size in EMUs at an assumed 300 DPI.
pub(crate) fn pptx_resolution(zip: &mut Zip) -> Option<(u32, u32)> {
    let data = zip.read("ppt/presentation.xml")?;
    with_xml(&data, |doc| {
        let sz = doc.root_element().children().find(|n| {
            n.is_element()
                && n.tag_name().name() == "sldSz"
                && n.tag_name().namespace()
                    == Some("http://schemas.openxmlformats.org/presentationml/2006/main")
        })?;
        let px = |v: &str| -> Option<u32> {
            let emu: i64 = v.trim().parse().ok()?;
            u32::try_from((emu as f64 * (300.0 / 914_400.0)).round_ties_even() as i64).ok()
        };
        Some((px(attr(&sz, "cx")?)?, px(attr(&sz, "cy")?)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirname_matches_posixpath() {
        assert_eq!(posix_dirname("OEBPS/content.opf"), "OEBPS");
        assert_eq!(posix_dirname("content.opf"), "");
        assert_eq!(posix_dirname("a//b"), "a");
        assert_eq!(posix_dirname("/x"), "/");
    }

    #[test]
    fn ugoira_json_validation() {
        let v: Vec<serde_json::Value> =
            serde_json::from_str(r#"[{"file": "a", "delay": 1}]"#).unwrap();
        assert_eq!(ugoira_json_is_valid(&v), Some(true));
        let v: Vec<serde_json::Value> = serde_json::from_str(r#"[{"file": "a"}]"#).unwrap();
        assert_eq!(ugoira_json_is_valid(&v), Some(false));
        let v: Vec<serde_json::Value> = serde_json::from_str("[1]").unwrap();
        assert_eq!(ugoira_json_is_valid(&v), None);
    }
}
