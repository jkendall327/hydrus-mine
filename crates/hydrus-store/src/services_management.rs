//! Atomic local service lifecycle changes, preserving unrelated file ownership.
use crate::{
    Store,
    error::{Result, StoreError},
    services::{self, Service, ServiceKind, ServiceRegistry},
};
use hydrus_core::{ServiceId, ServiceKey, ServiceType};
use rusqlite::Connection;
use std::collections::HashSet;

/// Whether the desktop editor can add or remove this local service kind.
pub fn editable(kind: &ServiceKind) -> bool {
    matches!(
        kind,
        ServiceKind::LocalFiles
            | ServiceKind::LocalTags
            | ServiceKind::RatingLike(_)
            | ServiceKind::RatingNumerical(_)
            | ServiceKind::RatingIncDec(_)
    )
}
fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}
/// Validate a local service configuration before accepting it at the writer boundary.
pub fn validate(kind: &ServiceKind) -> Result<()> {
    if let ServiceKind::ClientApi(config) = kind
        && config.port == Some(0)
    {
        return Err(invalid("Client API ports must be between 1 and 65535."));
    }
    if let ServiceKind::RatingNumerical(c) = kind
        && (!(1..=20).contains(&c.num_stars)
            || !(-64..=64).contains(&c.custom_pad)
            || c.show_fraction_beside_stars > 2
            || (c.num_stars == 1 && !c.allow_zero))
    {
        return Err(invalid("Invalid numerical rating settings."));
    }
    let appearance = match kind {
        ServiceKind::RatingLike(c) => Some(&c.appearance),
        ServiceKind::RatingNumerical(c) => Some(&c.appearance),
        _ => None,
    };
    if let Some(services::StarAppearance::Shape(shape)) = appearance
        && shape.name().is_none()
    {
        return Err(invalid("Unknown rating shape."));
    }
    Ok(())
}
// Client API is a protected singleton: its type and key stay, and every
// setting the reference's editor has may change.
fn supported_api_change(original: &ServiceKind, desired: &ServiceKind) -> bool {
    matches!(
        (original, desired),
        (ServiceKind::ClientApi(_), ServiceKind::ClientApi(_))
    )
}
fn validate_list(actual: &[Service], desired: &[Service]) -> Result<HashSet<ServiceKey>> {
    let mut keys = HashSet::new();
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for service in desired {
        if !keys.insert(service.key.clone())
            || !names.insert(hydrus_core::casefold::casefold(&service.name))
            || service.name.is_empty()
        {
            return Err(invalid(
                "Service names and keys must be unique and names cannot be empty.",
            ));
        }
        if service.id != ServiceId(0) && !ids.insert(service.id) {
            return Err(invalid("Duplicate service id."));
        }
        if let Some(original) = actual.iter().find(|s| s.key == service.key) {
            if service.id != original.id
                || service.service_type() != original.service_type()
                || (!editable(&original.kind)
                    && service.kind != original.kind
                    && !supported_api_change(&original.kind, &service.kind))
            {
                return Err(invalid(
                    "A service's key, type, and protected configuration cannot be changed.",
                ));
            }
        } else if service.id != ServiceId(0)
            || !editable(&service.kind)
            || service.key.as_bytes().len() != 32
        {
            return Err(invalid(
                "Only new local file, tag, or rating services can be added.",
            ));
        }
        validate(&service.kind)?;
    }
    for service_type in [ServiceType::LocalFileDomain, ServiceType::LocalTag] {
        if !desired.iter().any(|s| s.service_type() == service_type) {
            let message = format!(
                "Unfortunately, you must have at least one service of the type \"{}\". You cannot delete them all.",
                service_type.name()
            );
            return Err(invalid(message));
        }
    }
    Ok(keys)
}
fn validate_removal(conn: &Connection, service: &Service) -> Result<()> {
    if !editable(&service.kind) {
        return Err(invalid(
            "This built-in or remote service cannot be deleted here.",
        ));
    }
    if matches!(service.kind, ServiceKind::LocalFiles) {
        let files: i64 = conn.query_row(
            "SELECT count(*) FROM file_domain_current WHERE service_id=?",
            [service.id],
            |r| r.get(0),
        )?;
        if files > 0 {
            let files = hydrus_core::numbers::human_int(u64::try_from(files).unwrap_or(0));
            let message = format!(
                "The service {} needs to be empty before it can be deleted, but it seems to have {files} files in it! Please delete or migrate all the files from it and then try again.",
                service.name
            );
            return Err(invalid(message));
        }
    }
    Ok(())
}
fn cleanup(conn: &Connection, id: ServiceId) -> Result<()> {
    for table in [
        "file_domain_current",
        "file_domain_deleted",
        "file_domain_pending",
        "file_domain_petitioned",
        "ratings",
        "ratings_incdec",
        "tag_siblings",
        "tag_parents",
        "recent_tags",
    ] {
        conn.execute(&format!("DELETE FROM {table} WHERE service_id = ?"), [id])?;
    }
    services::delete(conn, id)
}
/// Apply a detached service list atomically. Reject a stale editor, nonempty file
/// domain removal, kind/key changes, or removal of the last local file/tag domain.
/// New services have id zero; existing ids and keys are retained exactly.
pub fn apply(store: &Store, expected: Vec<Service>, desired: Vec<Service>) -> Result<()> {
    store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        let actual = ServiceRegistry::load(conn)?
            .all()
            .map(|s| (**s).clone())
            .collect::<Vec<_>>();
        if actual != expected {
            return Err(invalid(
                "Services changed while this editor was open. Close and reopen it before applying.",
            ));
        }
        let keys = validate_list(&actual, &desired)?;
        let removed = actual
            .iter()
            .filter(|s| !keys.contains(&s.key))
            .collect::<Vec<_>>();
        for service in &removed {
            validate_removal(conn, service)?;
        }
        let resync_deleted = removed
            .iter()
            .any(|s| matches!(s.kind, ServiceKind::LocalFiles));
        let mut rebuild_counts = removed
            .iter()
            .any(|s| matches!(s.kind, ServiceKind::LocalFiles | ServiceKind::LocalTags));
        for service in removed {
            cleanup(conn, service.id)?;
        }
        for service in &desired {
            if service.id == ServiceId(0) {
                services::insert(conn, &service.key, &service.name, &service.kind)?;
                rebuild_counts |= matches!(
                    service.kind,
                    ServiceKind::LocalFiles | ServiceKind::LocalTags
                );
            } else {
                let original = actual
                    .iter()
                    .find(|s| s.id == service.id)
                    .ok_or_else(|| invalid("Unknown existing service id."))?;
                if original.name != service.name {
                    conn.execute(
                        "UPDATE services SET name=? WHERE service_id=?",
                        rusqlite::params![service.name, service.id],
                    )?;
                }
                if original.kind != service.kind {
                    services::update_config(conn, service.id, &service.kind)?;
                }
            }
        }
        if resync_deleted {
            let fresh = crate::store::Snapshot::load(conn)?;
            let mut writer = crate::content::ContentWriter::new(
                conn,
                &fresh,
                hydrus_core::time::TimestampMs::now().millis(),
            )?;
            writer.resync_combined_deleted(None)?;
            writer.finish()?;
        }
        if rebuild_counts {
            crate::counts::rebuild_all(conn)?;
        }
        Ok(())
    })
}
