//! What a duplicate decision would change, said as the reference's
//! `GetMergeSummaryOnPair` says it: for A, then B, each service's changes
//! ("my tags: add tag mappings: blue, red | delete tag mappings: old"), or
//! "no changes".

use std::collections::HashMap;

use rusqlite::Connection;

use hydrus_core::{HashId, ServiceId, TagId, TimestampMs};
use hydrus_store::Snapshot;
use hydrus_store::content::{DomainRoles, FileTime, MappingAction};
use hydrus_store::duplicates::merge::Change;
use hydrus_store::services::ServiceKind;

/// A number as Python's `str` writes it (`1.0`, `0.6`).
fn python_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// A time with its milliseconds (`TimestampMSToPrettyTime`).
fn pretty_time_ms(ms: i64) -> String {
    format!(
        "{}.{:03}",
        hydrus_import::status::pretty_time(TimestampMs(ms)),
        ms.rem_euclid(1000)
    )
}

/// The service a change is made on, what it is ("add tag mappings"), and
/// its value ("blue"; empty for some).
fn describe(
    change: &Change,
    roles: &DomainRoles,
    notes_service: Option<ServiceId>,
    tags: &HashMap<TagId, String>,
) -> (Option<ServiceId>, String, String) {
    let storage = Some(roles.local_file_storage);
    match change {
        Change::Mapping {
            service,
            action,
            tag,
            ..
        } => {
            let verb = match action {
                MappingAction::Add => "add",
                MappingAction::Delete => "delete",
                MappingAction::Pend => "pending",
                MappingAction::RescindPend => "rescind pending",
                MappingAction::Petition { .. } => "petition",
                MappingAction::RescindPetition => "rescind petition",
            };
            (
                Some(*service),
                format!("{verb} tag mappings"),
                tags.get(tag).cloned().unwrap_or_default(),
            )
        }
        Change::Rating { service, value, .. } => (
            Some(*service),
            "add ratings".into(),
            value.map_or_else(|| "None".into(), python_float),
        ),
        Change::IncDec { service, value, .. } => {
            (Some(*service), "add ratings".into(), value.to_string())
        }
        Change::SetNote { name, .. } => (notes_service, "set notes".into(), name.clone()),
        Change::DeleteNote { name, .. } => (notes_service, "delete notes".into(), name.clone()),
        Change::Archive(_) => (storage, "archive files".into(), String::new()),
        Change::Inbox(_) => (storage, "inbox files".into(), String::new()),
        Change::Delete(_) => (
            Some(roles.combined_local_media),
            "delete files".into(),
            String::new(),
        ),
        Change::AddUrls { urls, .. } => (storage, "add urls".into(), urls.join(", ")),
        Change::FileTime { time, ms, .. } => {
            let kind = match time {
                FileTime::FileModified => "file modified time".to_owned(),
                FileTime::DomainModified(domain) => format!("\"{domain}\" domain modified time"),
                _ => "unknown timestamp type".to_owned(),
            };
            (
                storage,
                "set timestamp".into(),
                format!("{kind}: {}", pretty_time_ms(*ms)),
            )
        }
    }
}

/// What is done on a service ("add tag mappings") and to what, in order.
type Lines = Vec<(String, Vec<String>)>;

/// `changes` to `a` and `b`, summarised as the reference does.
pub fn summary(
    conn: &Connection,
    snapshot: &Snapshot,
    changes: &[Change],
    a: HashId,
    b: HashId,
) -> hydrus_store::Result<String> {
    let services = &snapshot.services;
    let roles = DomainRoles::new(services)?;
    let notes_service = services
        .all()
        .find(|s| matches!(s.kind, ServiceKind::LocalNotes))
        .map(|s| s.id);
    let tag_ids: Vec<TagId> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Mapping { tag, .. } => Some(*tag),
            _ => None,
        })
        .collect();
    let tags: HashMap<TagId, String> = hydrus_store::master::tags(conn, &tag_ids)?
        .into_iter()
        .map(|(id, tag)| (id, tag.to_string()))
        .collect();
    let mut parts = Vec::new();
    for (letter, file) in [("A", a), ("B", b)] {
        // by service, then by what is done, each in the order first done
        let mut work: Vec<(Option<ServiceId>, Lines)> = Vec::new();
        for change in changes.iter().filter(|c| c.file() == file) {
            let (service, what, value) = describe(change, &roles, notes_service, &tags);
            let index = work
                .iter()
                .position(|(s, _)| *s == service)
                .unwrap_or_else(|| {
                    work.push((service, Vec::new()));
                    work.len() - 1
                });
            let lines = &mut work[index].1;
            match lines.iter_mut().find(|(w, _)| *w == what) {
                Some((_, values)) => values.push(value),
                None => lines.push((what, vec![value])),
            }
        }
        let service_lines: Vec<String> = work
            .into_iter()
            .map(|(service, lines)| {
                let name = service
                    .and_then(|s| services.get(s).ok())
                    .map_or_else(|| "unknown service".to_owned(), |s| s.name.clone());
                let lines: Vec<String> = lines
                    .into_iter()
                    .map(|(what, mut values)| {
                        values.sort();
                        let values = values.join(", ");
                        if values.is_empty() {
                            what
                        } else {
                            format!("{what}: {values}")
                        }
                    })
                    .collect();
                format!("{name}: {}", lines.join(" | "))
            })
            .collect();
        let body = if service_lines.is_empty() {
            "no changes".to_owned()
        } else {
            service_lines.join("\n    ")
        };
        parts.push(format!("{letter}:\n    {body}"));
    }
    Ok(parts.join("\n"))
}
