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
