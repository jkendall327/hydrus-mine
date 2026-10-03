//! Sidecar routers as the reference describes them: each router's
//! `ToString` ("Taking notes from media, applying some sorting, sending to
//! .json sidecar (notes)."), from its importers' and exporter's own, and
//! the sidecars button's label.

use hydrus_core::content::TimestampType;
use hydrus_core::url::strings::StringProcessor;
use hydrus_parse::sidecar::{
    Exporter, Importer, Router, Source, TagDisplay, TimestampLocation, TimestampStub,
};

/// A service's name by its key in hex: "unknown service" for one the
/// client doesn't have (`GetNameSafe`).
pub type Namer<'a> = &'a dyn Fn(&str) -> Option<String>;

fn name(namer: Namer<'_>, key: &str) -> String {
    namer(key).unwrap_or_else(|| "unknown service".to_owned())
}

/// ", applying some sorting", if the processor makes changes.
fn applying(processor: &StringProcessor) -> String {
    if processor.makes_changes() {
        format!(", applying {}", processor.summary())
    } else {
        String::new()
    }
}

fn timestamp_kind(kind: TimestampType) -> &'static str {
    match kind {
        TimestampType::ModifiedDomain => "domain modified time",
        TimestampType::ModifiedFile => "file modified time",
        TimestampType::ModifiedAggregate => "aggregate modified time",
        TimestampType::Imported => "imported time",
        TimestampType::Deleted => "deleted time",
        TimestampType::Archived => "archived time",
        TimestampType::LastViewed => "last viewed time",
        TimestampType::PreviouslyImported => "previous imported time (for undelete)",
    }
}

fn canvas(code: i64) -> &'static str {
    match code {
        0 => "media viewer",
        1 => "preview viewer",
        2 => "duplicates filter",
        3 => "archive/delete filter",
        4 => "client api viewer",
        _ => "dialog",
    }
}

/// A timestamp stub (`TimestampData.ToString`, with no time): "archived
/// time", "\"my files\" imported time", "media viewer last viewed time".
pub fn timestamp_text(stub: &TimestampStub, namer: Namer<'_>) -> String {
    let kind = timestamp_kind(stub.kind);
    match (stub.kind, &stub.location) {
        (
            TimestampType::Archived
            | TimestampType::ModifiedFile
            | TimestampType::ModifiedAggregate,
            _,
        ) => kind.to_owned(),
        (
            TimestampType::Imported | TimestampType::Deleted | TimestampType::PreviouslyImported,
            TimestampLocation::Service(key),
        ) => format!("\"{}\" {kind}", name(namer, key)),
        (TimestampType::LastViewed, TimestampLocation::Canvas(code)) => {
            format!("{} {kind}", canvas(*code))
        }
        (TimestampType::ModifiedDomain, TimestampLocation::Domain(domain)) => {
            format!("\"{domain}\" {kind}")
        }
        _ => "unknown timestamp type".to_owned(),
    }
}

/// An importer (`SingleFileMetadataImporter*.ToString`).
pub fn importer_text(importer: &Importer, namer: Namer<'_>) -> String {
    let munge = applying(&importer.processor);
    match &importer.source {
        Source::MediaNotes => format!("notes from media{munge}"),
        Source::MediaTags {
            service_key,
            display,
        } => {
            let display = match display {
                TagDisplay::Storage => "stored tags",
                TagDisplay::DisplayActual => "display tags",
            };
            format!("\"{}\" {display}{munge}", name(namer, service_key))
        }
        Source::MediaTimestamp(stub) => {
            format!("{} from media{munge}", timestamp_text(stub, namer))
        }
        Source::MediaUrls => format!("urls from media{munge}"),
        Source::Txt { .. } => format!("from .txt sidecar{munge}"),
        Source::Json { .. } => format!("from JSON sidecar{munge}"),
    }
}

/// An exporter (`SingleFileMetadataExporter*.ToString`).
pub fn exporter_text(exporter: &Exporter, namer: Namer<'_>) -> String {
    let suffix = |s: &str| {
        if s.is_empty() {
            String::new()
        } else {
            format!(".{s}")
        }
    };
    match exporter {
        Exporter::MediaNotes { .. } => "notes to media".to_owned(),
        Exporter::MediaTags { service_key } => {
            format!("tags to media, on \"{}\"", name(namer, service_key))
        }
        Exporter::MediaTimestamp(stub) => format!("{} to media", timestamp_text(stub, namer)),
        Exporter::MediaUrls => "urls to media".to_owned(),
        Exporter::Txt { naming, .. } => format!("to {}.txt sidecar", suffix(&naming.suffix)),
        Exporter::Json {
            naming,
            nested_object_names,
        } => {
            let nested = if nested_object_names.is_empty() {
                String::new()
            } else {
                format!(" ({})", nested_object_names.join(">"))
            };
            format!("to {}.json sidecar{nested}", suffix(&naming.suffix))
        }
    }
}

/// A router (`SingleFileMetadataRouter.ToString`); not `pretty`, with the
/// reference's "Single File Metadata Router: " before it.
pub fn router_text(router: &Router, pretty: bool, namer: Namer<'_>) -> String {
    let sources = if router.importers.is_empty() {
        "nothing".to_owned()
    } else {
        router
            .importers
            .iter()
            .map(|i| importer_text(i, namer))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let header = if pretty {
        ""
    } else {
        "Single File Metadata Router: "
    };
    format!(
        "{header}Taking {sources}{}, sending {}.",
        applying(&router.processor),
        exporter_text(&router.exporter, namer)
    )
}

/// The sidecars button's label (`SingleFileMetadataRoutersButton.
/// _RefreshLabel`), elided to 64 characters, and its tooltip, the whole.
pub fn button_label(routers: &[Router], namer: Namer<'_>) -> (String, String) {
    let text = match routers {
        [] => "no sidecars".to_owned(),
        [router] => router_text(router, true, namer),
        _ => format!(
            "{} sidecar actions",
            hydrus_core::numbers::human_int(routers.len() as u64)
        ),
    };
    let label = if text.chars().count() > 64 {
        let mut short: String = text.chars().take(63).collect();
        short.push('\u{2026}');
        short
    } else {
        text.clone()
    };
    (label, text)
}

/// `ElideText`, not centred.
fn elide_end(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let kept: String = text.chars().take(max - 1).collect();
        format!("{kept}\u{2026}")
    } else {
        text.to_owned()
    }
}

/// `GetFirstLineSummary`: the first line, and how many more there are.
fn first_line_summary(text: &str) -> String {
    let lines: Vec<&str> = hydrus_parse::text::splitlines(text);
    if lines.len() > 1 {
        format!(
            "{}\u{2026} (+{} lines)",
            lines[0],
            hydrus_core::numbers::human_int(lines.len() as u64 - 1)
        )
    } else {
        text.to_owned()
    }
}

/// "1 note: ...", or "3 notes: ..., ..., ...".
fn summarised(rows: &[String], one: &str, many: &str) -> String {
    if let [row] = rows {
        format!("1 {one}: {}", elide_end(&first_line_summary(row), 64))
    } else {
        let summaries: Vec<String> = rows
            .iter()
            .map(|r| elide_end(&first_line_summary(r), 32))
            .collect();
        format!(
            "{} {many}: {}",
            hydrus_core::numbers::human_int(rows.len() as u64),
            summaries.join(", ")
        )
    }
}

/// Python's `float()` of a string, near enough: surrounding whitespace
/// allowed, and its error text.
fn python_float(text: &str) -> Result<f64, String> {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let finite_or_special = lower.trim_start_matches(['+', '-']);
    let allowed = !trimmed.is_empty()
        && (matches!(finite_or_special, "inf" | "infinity" | "nan")
            || trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-' | '_')));
    allowed
        .then(|| trimmed.replace('_', "").parse::<f64>().ok())
        .flatten()
        .ok_or_else(|| {
            format!(
                "could not convert string to float: {}",
                hydrus_core::url::string_descriptions::python_repr_str(text)
            )
        })
}

/// What an import's routers would read for the file at `path`, a string
/// for each router whose sidecars gave anything (the "sidecars" tab's
/// "metadata" column; `_MetadataRoutersPanel._GetPrettyStrings`). Times
/// are shown in UTC.
pub fn file_preview(routers: &[Router], path: &str, namer: Namer<'_>) -> Vec<String> {
    let mut strings = Vec::new();
    for router in routers {
        let mut gathered: Vec<String> = Vec::new();
        for importer in &router.importers {
            if let Some(Ok(rows)) = hydrus_parse::sidecar::import_sidecar(importer, path) {
                for row in rows {
                    if !gathered.contains(&row) {
                        gathered.push(row);
                    }
                }
            }
        }
        if gathered.is_empty() {
            continue;
        }
        let mut processed = hydrus_parse::sidecar::process(&router.processor, gathered);
        hydrus_core::sort::human_sort(&mut processed);
        let mut processed = match &router.exporter {
            Exporter::MediaTags { service_key } => {
                let mut tags: Vec<String> = processed
                    .iter()
                    .map(|t| hydrus_core::tag::clean_tag(t))
                    // (`CheckTagNotEmpty`: a namespace alone is empty too)
                    .filter(|t| !hydrus_core::tag::split_tag(t).1.is_empty())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let sort = hydrus_core::tag_sort::TagSort {
                    sort_type: hydrus_core::tag_sort::TagSortType::Tag,
                    ascending: true,
                    group_by: hydrus_core::tag_sort::TagGroupBy::Nothing,
                };
                hydrus_core::tag_sort::sort_tags(&sort, &mut tags, String::as_str, |_| 0, &[]);
                vec![format!(
                    "to \"{}\": {}",
                    name(namer, service_key),
                    tags.join(", ")
                )]
            }
            Exporter::MediaNotes { .. } => vec![summarised(&processed, "note", "notes")],
            Exporter::MediaUrls => vec![summarised(&processed, "URL", "URLs")],
            Exporter::MediaTimestamp(stub) => {
                let stub = timestamp_text(stub, namer);
                if let [row] = processed.as_slice() {
                    let time = match python_float(row) {
                        Ok(seconds) => {
                            #[expect(clippy::cast_possible_truncation, reason = "as the reference")]
                            let ms = (seconds * 1000.0).floor() as i64;
                            hydrus_import::status::pretty_time(hydrus_core::TimestampMs(ms))
                        }
                        Err(e) => format!("Could not parse time! {e}"),
                    };
                    vec![format!("{stub}: {time}")]
                } else {
                    vec![format!(
                        "{stub}: {} times?",
                        hydrus_core::numbers::human_int(processed.len() as u64)
                    )]
                }
            }
            Exporter::Txt { .. } | Exporter::Json { .. } => processed,
        };
        processed.sort();
        strings.extend(processed);
    }
    strings
}
