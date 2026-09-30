//! Sidecars and metadata routing (`ClientMetadataMigration*`): how import
//! and export folders move a file's tags, URLs, notes and timestamps to and
//! from `.txt` and `.json` files beside it, or between the file's own
//! metadata.
//!
//! A [`Router`] gathers strings from its [`Importer`]s, dedupes them, runs
//! its string processor over them and hands them to its [`Exporter`]. This
//! module holds the model and the parts that need no database: sidecar
//! paths, reading and writing sidecar files, and the string forms of notes.
//! Reading and writing a file's own metadata is the caller's.

use hydrus_core::pyjson::PyJson;
use hydrus_core::url::strings::{StringConverter, StringProcessor};
use serde::{Deserialize, Serialize};

use crate::formula::{Formula, ParsingContext};
use crate::text::splitlines;

/// Between a note's name and its text in a row (`NOTE_CONNECTOR_STRING`).
pub const NOTE_CONNECTOR: &str = ": ";
/// A connector inside a note's name, escaped (`NOTE_NAME_ESCAPE_STRING`).
pub const NOTE_NAME_ESCAPE: &str = ":\\ ";

/// Where a sidecar sits relative to its file (`SidecarNode`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarNaming {
    /// `my_image.txt` rather than `my_image.jpg.txt`.
    pub remove_actual_filename_ext: bool,
    /// `my_image.jpg.tags.txt` for a suffix of `tags`.
    pub suffix: String,
    /// Applied to the sidecar's filename (not its directory), if it has
    /// conversions.
    pub filename_converter: StringConverter,
}

impl SidecarNaming {
    /// `GetSidecarPath`: the sidecar with extension `extension` for the
    /// file at `file_path`. Works on the path as text, as the reference
    /// does (so without an extension on the file, "removing" it cuts at
    /// the last dot anywhere in the path).
    pub fn path(&self, file_path: &str, extension: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        match file_path.rsplit_once('.') {
            Some((without, _)) if self.remove_actual_filename_ext => parts.push(without),
            _ => parts.push(file_path),
        }
        if !self.suffix.is_empty() {
            parts.push(&self.suffix);
        }
        parts.push(extension);
        let path = parts.join(".");
        if !self.filename_converter.makes_changes() {
            return path;
        }
        let (dir, name) = split_path(&path);
        match self.filename_converter.convert(name) {
            Ok(converted) => join_path(dir, &converted),
            // the reference prints a warning and keeps the unconverted path
            Err(_) => path,
        }
    }
}

/// `os.path.split`.
fn split_path(path: &str) -> (&str, &str) {
    let cut = path
        .rfind(std::path::MAIN_SEPARATOR)
        .or_else(|| if cfg!(windows) { path.rfind('/') } else { None });
    match cut {
        None => ("", path),
        Some(i) => {
            let dir = &path[..i];
            let head = if dir.is_empty() { &path[..=i] } else { dir };
            (head, &path[i + 1..])
        }
    }
}

/// `os.path.join` of a directory from [`split_path`] and a name.
fn join_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else if dir.ends_with(std::path::MAIN_SEPARATOR) {
        format!("{dir}{name}")
    } else {
        format!("{dir}{}{name}", std::path::MAIN_SEPARATOR)
    }
}

/// `ClientTags.TAG_DISPLAY_*` as media tag importers use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagDisplay {
    /// The tags as stored.
    Storage,
    /// With siblings and parents applied.
    DisplayActual,
}

/// Which of a file's timestamps (`TimestampData` without a time): its
/// kind (`HC.TIMESTAMP_TYPE_*`) and, for some kinds, where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampStub {
    pub kind: hydrus_core::content::TimestampType,
    pub location: TimestampLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampLocation {
    None,
    /// A file service's key, hex (imported, deleted, previously imported).
    Service(String),
    /// A canvas type (last viewed).
    Canvas(i64),
    /// A domain (modified by a website).
    Domain(String),
}

/// Where an importer's strings come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The file's notes, as `name: text` rows.
    MediaNotes,
    /// The file's current tags on a service (hex key; the combined tag
    /// service's too).
    MediaTags {
        service_key: String,
        display: TagDisplay,
    },
    /// One of the file's timestamps, in seconds.
    MediaTimestamp(TimestampStub),
    /// The file's URLs.
    MediaUrls,
    /// A `.txt` sidecar, split on a separator (lines for `"\n"`).
    Txt {
        naming: SidecarNaming,
        separator: String,
    },
    /// A `.json` sidecar, read with a JSON parsing formula.
    Json {
        naming: SidecarNaming,
        formula: Box<Formula>,
    },
}

/// One source of strings for a [`Router`], with its own processing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Importer {
    pub source: Source,
    pub processor: StringProcessor,
}

/// Where a router's strings go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exporter {
    /// Notes on the file: rows are `name: text`, or all text under a
    /// forced name.
    MediaNotes { forced_name: Option<String> },
    /// Tags on the file, on a service (hex key): added on a local tag
    /// service, pended on a repository.
    MediaTags { service_key: String },
    /// One of the file's timestamps, from the first row (in seconds).
    MediaTimestamp(TimestampStub),
    /// URLs on the file.
    MediaUrls,
    /// A `.txt` sidecar, rows joined with a separator.
    Txt {
        naming: SidecarNaming,
        separator: String,
    },
    /// A `.json` sidecar: a list of the rows, or the rows put under a path
    /// of nested object names in the existing JSON.
    Json {
        naming: SidecarNaming,
        nested_object_names: Vec<String>,
    },
}

/// `SingleFileMetadataRouter`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Router {
    pub importers: Vec<Importer>,
    pub processor: StringProcessor,
    pub exporter: Exporter,
}

impl Source {
    /// The sidecars this source would read for a file, in the order they
    /// are tried (lower-case extension first).
    pub fn sidecar_paths(&self, file_path: &str) -> Vec<String> {
        match self {
            Source::Txt { naming, .. } => {
                vec![naming.path(file_path, "txt"), naming.path(file_path, "TXT")]
            }
            Source::Json { naming, .. } => {
                vec![
                    naming.path(file_path, "json"),
                    naming.path(file_path, "JSON"),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub fn is_sidecar(&self) -> bool {
        matches!(self, Source::Txt { .. } | Source::Json { .. })
    }
}

impl Router {
    /// `GetPossibleImporterSidecarPaths`: every sidecar this router's
    /// importers might read for a file.
    pub fn possible_sidecar_paths(&self, file_path: &str) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        for importer in &self.importers {
            for path in importer.source.sidecar_paths(file_path) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    /// What the router sends on, given what each importer gathered (each
    /// already through its importer's processor): deduped, keeping first
    /// appearances, then through the router's processor.
    pub fn route(&self, gathered: Vec<String>) -> Vec<String> {
        let mut rows: Vec<String> = Vec::with_capacity(gathered.len());
        for row in gathered {
            if !rows.contains(&row) {
                rows.push(row);
            }
        }
        process(&self.processor, rows)
    }
}

/// Why a sidecar couldn't be read or written, or a file's own metadata
/// couldn't be.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("could not read {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("could not write {path}: {reason}")]
    Write { path: String, reason: String },
    #[error("{0}")]
    Media(String),
}

/// A file's own metadata, for the media ends of a router.
pub trait MediaMetadata {
    /// The strings a media source gives, before its importer's processor.
    fn import(&mut self, source: &Source) -> Result<Vec<String>, SidecarError>;
    /// Put rows (never empty) into the file's metadata.
    fn export(&mut self, exporter: &Exporter, rows: &[String]) -> Result<(), SidecarError>;
}

/// `SingleFileMetadataRouter.Work`: gather, route and export for the file
/// at `file_path`. Whether there was anything to export.
pub fn work(
    router: &Router,
    file_path: &str,
    media: &mut dyn MediaMetadata,
) -> Result<bool, SidecarError> {
    let mut gathered = Vec::new();
    for importer in &router.importers {
        let rows = match import_sidecar(importer, file_path) {
            Some(rows) => rows?,
            None => process(&importer.processor, media.import(&importer.source)?),
        };
        gathered.extend(rows);
    }
    let rows = router.route(gathered);
    if rows.is_empty() {
        return Ok(false);
    }
    if !export_sidecar(&router.exporter, file_path, &rows)? {
        media.export(&router.exporter, &rows)?;
    }
    Ok(true)
}

/// The strings a sidecar source gives for a file, through the importer's
/// processor (`SingleFileMetadataImporterSidecar.Import`): nothing if none
/// of its sidecars exist. `None` for sources that aren't sidecars.
pub fn import_sidecar(
    importer: &Importer,
    file_path: &str,
) -> Option<Result<Vec<String>, SidecarError>> {
    if !importer.source.is_sidecar() {
        return None;
    }
    let Some(path) = importer
        .source
        .sidecar_paths(file_path)
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
    else {
        return Some(Ok(Vec::new()));
    };
    let read_error = |reason: String| SidecarError::Read {
        path: path.clone(),
        reason,
    };
    let result = (|| {
        let bytes = std::fs::read(&path).map_err(|e| read_error(e.to_string()))?;
        let text = read_text(&bytes).map_err(|e| read_error(e.to_string()))?;
        let rows = match &importer.source {
            Source::Txt { separator, .. } => read_txt(&text, separator),
            Source::Json { formula, .. } => {
                read_json(&text, formula).map_err(|e| read_error(e.to_string()))?
            }
            _ => unreachable!("checked it is a sidecar"),
        };
        Ok(process(&importer.processor, rows))
    })();
    Some(result)
}

/// Write rows to an exporter's sidecar for a file (nothing for no rows).
/// Returns whether it was a sidecar exporter.
pub fn export_sidecar(
    exporter: &Exporter,
    file_path: &str,
    rows: &[String],
) -> Result<bool, SidecarError> {
    let (path, text) = match exporter {
        Exporter::Txt { naming, separator } => {
            let path = naming.path(file_path, "txt");
            if rows.is_empty() {
                return Ok(true);
            }
            let text = write_txt(rows, separator);
            (path, text)
        }
        Exporter::Json {
            naming,
            nested_object_names,
        } => {
            let path = naming.path(file_path, "json");
            if rows.is_empty() {
                return Ok(true);
            }
            let existing =
                if nested_object_names.is_empty() || !std::path::Path::new(&path).exists() {
                    None
                } else {
                    let bytes = std::fs::read(&path).map_err(|e| SidecarError::Read {
                        path: path.clone(),
                        reason: e.to_string(),
                    })?;
                    Some(read_text(&bytes).map_err(|e| SidecarError::Read {
                        path: path.clone(),
                        reason: e.to_string(),
                    })?)
                };
            let text =
                write_json(existing.as_deref(), nested_object_names, rows).map_err(|reason| {
                    SidecarError::Write {
                        path: path.clone(),
                        reason,
                    }
                })?;
            (path, text)
        }
        _ => return Ok(false),
    };
    std::fs::write(&path, text_to_write(&text)).map_err(|e| SidecarError::Write {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    Ok(true)
}

/// `StringProcessor.ProcessStrings`, where a step the processor can't run
/// leaves nothing.
pub fn process(processor: &StringProcessor, rows: Vec<String>) -> Vec<String> {
    processor.process(rows).unwrap_or_default()
}

/// What Python's text-mode read gives for a file's bytes: UTF-8, with
/// `\r\n` and `\r` read as `\n`.
pub fn read_text(bytes: &[u8]) -> Result<String, std::str::Utf8Error> {
    let text = std::str::from_utf8(bytes)?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// The text Python's text-mode write puts on disk for `text` (`\n` as the
/// platform's line ending).
pub fn text_to_write(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

/// A `.txt` sidecar's rows (`SingleFileMetadataImporterTXT`).
pub fn read_txt(text: &str, separator: &str) -> Vec<String> {
    if separator == "\n" {
        splitlines(text).into_iter().map(str::to_owned).collect()
    } else if separator.is_empty() {
        // Python's str.split('') raises; the reference's import then fails
        Vec::new()
    } else {
        text.split(separator).map(str::to_owned).collect()
    }
}

/// A `.json` sidecar's rows (`SingleFileMetadataImporterJSON`).
pub fn read_json(text: &str, formula: &Formula) -> Result<Vec<String>, crate::ParseError> {
    formula.parse(&ParsingContext::new(), text, false)
}

/// A `.txt` sidecar's text (`SingleFileMetadataExporterTXT`).
pub fn write_txt(rows: &[String], separator: &str) -> String {
    rows.join(separator)
}

/// A `.json` sidecar's text (`SingleFileMetadataExporterJSON`): the rows as
/// a list, or, with nested object names, the existing JSON (if any) with
/// the rows put at that path. Errors if the existing file isn't a JSON
/// object.
pub fn write_json(
    existing: Option<&str>,
    nested_object_names: &[String],
    rows: &[String],
) -> Result<String, String> {
    let list = PyJson::List(rows.iter().map(|r| PyJson::Str(r.clone())).collect());
    let Some((last, path)) = nested_object_names.split_last() else {
        return Ok(list.to_python_string());
    };
    let mut root = match existing {
        None => PyJson::Object(Vec::new()),
        Some(text) => {
            let value = PyJson::parse(text)
                .map_err(|e| format!("Could not read the existing JSON! {e}"))?
                .with_python_dict_semantics();
            if !matches!(value, PyJson::Object(_)) {
                return Err("The existing JSON file was not a JSON Object!".into());
            }
            value
        }
    };
    let mut node = &mut root;
    for name in path {
        let PyJson::Object(entries) = node else {
            // the reference indexes into whatever is there and fails
            return Err(format!(
                "Could not put the rows under {name:?}: not a JSON Object"
            ));
        };
        let i = if let Some(i) = entries.iter().position(|(k, _)| k == name) {
            i
        } else {
            entries.push((name.clone(), PyJson::Object(Vec::new())));
            entries.len() - 1
        };
        node = &mut entries[i].1;
    }
    let PyJson::Object(entries) = node else {
        return Err(format!(
            "Could not put the rows under {last:?}: not a JSON Object"
        ));
    };
    match entries.iter_mut().find(|(k, _)| k == last) {
        Some((_, value)) => *value = list,
        None => entries.push((last.clone(), list)),
    }
    Ok(root.to_python_string())
}

/// A file's notes as rows (`SingleFileMetadataImporterMediaNotes`).
pub fn notes_to_rows<'a>(notes: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<String> {
    notes
        .into_iter()
        .map(|(name, text)| {
            format!(
                "{}{NOTE_CONNECTOR}{text}",
                name.replace(NOTE_CONNECTOR, NOTE_NAME_ESCAPE)
            )
        })
        .collect()
}

/// Rows as notes (`SingleFileMetadataExporterMediaNotes`): `(name, text)`,
/// skipping rows without a name or text.
pub fn rows_to_notes(rows: &[String], forced_name: Option<&str>) -> Vec<(String, String)> {
    let mut notes = Vec::new();
    for row in rows {
        let (name, text) = match forced_name {
            Some(name) => (name.to_owned(), row.clone()),
            None => match row.split_once(NOTE_CONNECTOR) {
                Some((name, text)) => (name.to_owned(), text.to_owned()),
                None => continue,
            },
        };
        if name.is_empty() || text.is_empty() {
            continue;
        }
        notes.push((name.replace(NOTE_NAME_ESCAPE, NOTE_CONNECTOR), text));
    }
    notes
}

/// The reference's `re_leading_double_colon.sub(':', tag)`: `::)` as `:)`.
pub fn undouble_leading_colon(tag: &str) -> String {
    match tag.strip_prefix("::") {
        Some(rest) if !rest.starts_with(':') => format!(":{rest}"),
        _ => tag.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naming(remove: bool, suffix: &str) -> SidecarNaming {
        SidecarNaming {
            remove_actual_filename_ext: remove,
            suffix: suffix.into(),
            filename_converter: StringConverter::default(),
        }
    }

    #[test]
    fn sidecar_paths_are_named_like_the_reference() {
        assert_eq!(naming(false, "").path("/a/b.jpg", "txt"), "/a/b.jpg.txt");
        assert_eq!(naming(true, "").path("/a/b.jpg", "txt"), "/a/b.txt");
        assert_eq!(
            naming(true, "tags").path("/a/b.jpg", "json"),
            "/a/b.tags.json"
        );
        // no extension on the file: the last dot in the path is cut
        assert_eq!(naming(true, "").path("/a.d/b", "txt"), "/a.txt");
    }

    #[test]
    fn json_sidecars_nest_rows_into_existing_objects() {
        let rows = vec!["a".to_owned(), "é".to_owned()];
        assert_eq!(write_json(None, &[], &rows).unwrap(), r#"["a", "\u00e9"]"#);
        let names = vec!["x".to_owned(), "tags".to_owned()];
        assert_eq!(
            write_json(Some(r#"{"z": 1, "x": {"tags": 2, "k": 3}}"#), &names, &rows).unwrap(),
            r#"{"z": 1, "x": {"tags": ["a", "\u00e9"], "k": 3}}"#
        );
        assert!(write_json(Some("[1]"), &names, &rows).is_err());
    }

    #[test]
    fn notes_round_trip_through_rows() {
        let rows = notes_to_rows([("a: b", "text: more")]);
        assert_eq!(rows, vec!["a:\\ b: text: more"]);
        assert_eq!(
            rows_to_notes(&rows, None),
            vec![("a: b".to_owned(), "text: more".to_owned())]
        );
        assert_eq!(
            rows_to_notes(&["x".to_owned(), ": y".to_owned()], None),
            Vec::<(String, String)>::new()
        );
    }
}
