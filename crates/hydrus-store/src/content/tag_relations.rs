//! Sibling and parent content updates, including repository proposals.
//! Relations are committed alone: display counts and the published graph
//! change together, before subsequent mapping writes can use the graph.

use hydrus_core::{ContentStatus, ServiceId, ServiceType, Tag};
use rusqlite::params;

use crate::Store;
use crate::display::{DisplayGraphs, RelationKind};
use crate::error::{Result, StoreError};
use crate::services::ServiceRegistry;

/// A relation content update, with a reason for repository proposals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationAction {
    Add,
    Delete,
    Pend(String),
    RescindPend,
    Petition(String),
    RescindPetition,
}

/// A cleaned pair to change on a tag service.
#[derive(Debug, Clone)]
pub struct RelationUpdate {
    pub service: ServiceId,
    pub left: Tag,
    pub right: Tag,
    pub action: RelationAction,
}

/// Commit all updates atomically, rebuilding affected display counts and
/// publishing the matching graph only after the transaction commits.
pub fn apply(store: &Store, kind: RelationKind, updates: Vec<RelationUpdate>) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }
    store.write_and_refresh(move |ctx| apply_on(ctx.conn(), kind, updates))
}

/// Apply primary relations and derived counts inside an existing transaction.
pub(crate) fn apply_on(
    conn: &rusqlite::Connection,
    kind: RelationKind,
    updates: Vec<RelationUpdate>,
) -> Result<()> {
    let registry = ServiceRegistry::load(conn)?;
    let (table, left, right) = columns(kind);
    let mut sources = std::collections::HashSet::new();
    for update in updates {
        let service_type = registry.get(update.service)?.service_type();
        if !matches!(
            service_type,
            ServiceType::LocalTag | ServiceType::TagRepository
        ) {
            return Err(StoreError::Invalid(
                "service does not hold tag relationships".into(),
            ));
        }
        let a = crate::master::intern_tag(conn, &update.left)?;
        let b = crate::master::intern_tag(conn, &update.right)?;
        let remove = |status: ContentStatus| -> Result<()> {
            conn.execute(&format!("DELETE FROM {table} WHERE service_id=?1 AND status=?2 AND {left}=?3 AND {right}=?4"), params![update.service, status.code(), a, b])?;
            Ok(())
        };
        let insert = |status: ContentStatus, reason: Option<&str>| -> Result<()> {
            let reason = reason
                .map(|r| crate::master::intern_text(conn, r))
                .transpose()?;
            let conflict = if reason.is_some() {
                "REPLACE"
            } else {
                "IGNORE"
            };
            conn.execute(&format!("INSERT OR {conflict} INTO {table} (service_id,status,{left},{right},reason_id) VALUES (?1,?2,?3,?4,?5)"), params![update.service, status.code(), a, b, reason])?;
            Ok(())
        };
        match &update.action {
            RelationAction::Add => {
                remove(ContentStatus::Deleted)?;
                remove(ContentStatus::Pending)?;
                insert(ContentStatus::Current, None)?;
            }
            RelationAction::Delete => {
                remove(ContentStatus::Current)?;
                remove(ContentStatus::Petitioned)?;
                insert(ContentStatus::Deleted, None)?;
            }
            RelationAction::Pend(reason) => insert(ContentStatus::Pending, Some(reason))?,
            RelationAction::Petition(reason) => insert(ContentStatus::Petitioned, Some(reason))?,
            RelationAction::RescindPend => remove(ContentStatus::Pending)?,
            RelationAction::RescindPetition => remove(ContentStatus::Petitioned)?,
        }
        sources.insert(update.service);
    }
    let application = crate::display::load_application(conn, &registry)?;
    let graphs = DisplayGraphs::load(conn, &registry)?;
    for service in registry.tag_services() {
        if application
            .sources(kind, service.id)
            .iter()
            .any(|s| sources.contains(s))
        {
            crate::counts::rebuild_display_service(conn, &registry, service.id, &graphs)?;
        }
    }
    Ok(())
}

/// Primary relation table and tag columns for a relationship kind.
pub fn columns(kind: RelationKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        RelationKind::Siblings => ("tag_siblings", "bad_tag_id", "good_tag_id"),
        RelationKind::Parents => ("tag_parents", "child_tag_id", "parent_tag_id"),
    }
}
