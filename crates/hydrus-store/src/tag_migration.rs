//! Bounded service-to-service migrations over a stable reader snapshot.
//!
//! Each destination batch commits atomically. Cancellation retains exactly the
//! committed prefix; a read transaction prevents same-service edits from moving
//! the source beneath pagination. Service keys are resolved again on the writer.

use crate::content::{ContentWriter, MappingAction};
use crate::display::RelationKind;
use crate::services::ServiceRegistry;
use crate::{Result, Store, StoreError};
use hydrus_core::search::context::LocationContext;
use hydrus_core::{ContentStatus, HashId, ServiceId, ServiceKey, ServiceType, TagFilter, TagId};
use rusqlite::{Connection, params};
use std::sync::atomic::{AtomicBool, Ordering};

/// Reserves one pooled reader while allowing ordinary UI reads to continue.
#[derive(Debug)]
pub(crate) struct Guard(std::sync::Arc<AtomicBool>);
impl Guard {
    pub(crate) fn claim(active: std::sync::Arc<AtomicBool>) -> Result<Self> {
        active.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map_err(|_|StoreError::Invalid("a tag migration is already running; cancel it or wait for completion before starting another".into()))?;
        Ok(Self(active))
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// The primary content being migrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    Mappings,
    Siblings,
    Parents,
}
/// Destination operation; repository proposals are retained locally for upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Add,
    Delete,
    ClearDeletion,
    Pend,
    Petition,
}
/// Source statuses supported by the reference migration window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Current,
    CurrentAndPending,
    Pending,
    Deleted,
}
impl Status {
    /// Source primary-table statuses, with duplicate entries unioned.
    pub fn statuses(self) -> &'static [ContentStatus] {
        match self {
            Self::Current => &[ContentStatus::Current],
            Self::CurrentAndPending => &[ContentStatus::Current, ContentStatus::Pending],
            Self::Pending => &[ContentStatus::Pending],
            Self::Deleted => &[ContentStatus::Deleted],
        }
    }
}
/// Which files qualify for a mappings migration; pairs ignore file scope.
#[derive(Debug, Clone)]
pub enum Scope {
    Files(Vec<HashId>),
    Location(LocationContext),
}
/// Complete immutable job settings.
#[derive(Debug, Clone)]
pub struct Request {
    pub source: ServiceKey,
    pub destination: ServiceKey,
    pub content: Content,
    pub status: Status,
    pub action: Action,
    pub scope: Scope,
    pub left_filter: TagFilter,
    pub right_filter: TagFilter,
    pub reason: String,
}
/// Progress after a durably committed batch, including filtered-out rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub scanned: usize,
    pub accepted: usize,
    pub cancelled: bool,
}
/// Available actions, excluding the reference's redundant same-service actions.
pub fn actions(local: bool, same: bool, content: Content, status: Status) -> Vec<Action> {
    let existing = same && status.statuses().contains(&ContentStatus::Current);
    let deleted = same && status == Status::Deleted;
    let mut out = Vec::new();
    if !existing {
        out.push(if local { Action::Add } else { Action::Pend });
    }
    if !deleted {
        out.push(if local {
            Action::Delete
        } else {
            Action::Petition
        });
    }
    if local && !existing && content == Content::Mappings {
        out.push(Action::ClearDeletion);
    }
    out
}
fn validate(conn: &Connection, request: &Request) -> Result<(ServiceId, ServiceId)> {
    let registry = ServiceRegistry::load(conn)?;
    let source = registry.by_key(&request.source)?;
    let destination = registry.by_key(&request.destination)?;
    let real = |t| matches!(t, ServiceType::LocalTag | ServiceType::TagRepository);
    if !real(source.service_type()) || !real(destination.service_type()) {
        return Err(StoreError::Invalid(
            "migration requires real tag services".into(),
        ));
    }
    if source.service_type() == ServiceType::LocalTag
        && matches!(request.status, Status::Pending | Status::CurrentAndPending)
    {
        return Err(StoreError::Invalid(
            "local sources have no pending status".into(),
        ));
    }
    if !actions(
        destination.service_type() == ServiceType::LocalTag,
        source.key == destination.key,
        request.content,
        request.status,
    )
    .contains(&request.action)
    {
        return Err(StoreError::Invalid(
            "migration action is unavailable for these services and statuses".into(),
        ));
    }
    if request.action == Action::Petition && request.reason.trim().is_empty() {
        return Err(StoreError::Invalid("a petition reason is required".into()));
    }
    Ok((source.id, destination.id))
}
fn source_sql(conn: &Connection, request: &Request, source: ServiceId) -> Result<String> {
    if request.content != Content::Mappings {
        let kind = if request.content == Content::Siblings {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        let (table, left, right) = crate::content::tag_relations::columns(kind);
        let union=request.status.statuses().iter().map(|status|format!(
            "SELECT {left} AS a,{right} AS b FROM {table} WHERE service_id={} AND status={}",source.get(),status.code()
        )).collect::<Vec<_>>().join(" UNION ");
        return Ok(union);
    }
    let tables = crate::schema::MappingTables::new(source);
    let union = request
        .status
        .statuses()
        .iter()
        .map(|s| {
            format!(
                "SELECT tag_id AS a,hash_id AS b FROM {}",
                tables.for_status(*s)
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ");
    let location = match &request.scope {
        Scope::Files(_) => "b IN rarray(?4)".to_owned(),
        Scope::Location(location) if location.is_all_known_files() => "1".into(),
        Scope::Location(location) => {
            let registry = ServiceRegistry::load(conn)?;
            let mut clauses = Vec::new();
            for (keys, table) in [
                (location.current(), "file_domain_current"),
                (location.deleted(), "file_domain_deleted"),
            ] {
                for key in keys {
                    let service = registry.by_key(key)?;
                    if !service.service_type().is_file_service() {
                        return Err(StoreError::Invalid("invalid file domain".into()));
                    }
                    clauses.push(format!(
                        "EXISTS (SELECT 1 FROM {table} WHERE service_id={} AND hash_id=b)",
                        service.id.get()
                    ));
                }
            }
            if clauses.is_empty() {
                "0".into()
            } else {
                clauses.join(" OR ")
            }
        }
    };
    Ok(format!("SELECT a,b FROM ({union}) WHERE ({location})"))
}
/// Run on a worker thread with source batches bounded by `batch_size` (1..=1024).
/// Selected-file IDs and filters are retained once for the duration of the job.
/// The callback observes committed progress and may request cancellation.
pub fn run(
    store: &Store,
    request: &Request,
    cancel: &AtomicBool,
    batch_size: usize,
    progress: impl FnMut(Progress),
) -> Result<Progress> {
    run_pausable(
        store,
        request,
        cancel,
        &AtomicBool::new(false),
        batch_size,
        progress,
    )
}
/// Run with the reference's pause/resume boundary after each committed batch.
/// Cancellation wakes a paused job and retains its committed prefix.
pub fn run_pausable(
    store: &Store,
    request: &Request,
    cancel: &AtomicBool,
    paused: &AtomicBool,
    batch_size: usize,
    mut progress: impl FnMut(Progress),
) -> Result<Progress> {
    if !(1..=1024).contains(&batch_size) {
        return Err(StoreError::Invalid(
            "migration batch size must be 1..=1024".into(),
        ));
    }
    let _guard = store.claim_tag_migration()?;
    let request = std::sync::Arc::new(request.clone());
    store.read(|conn| {
        let transaction = conn;
        let (source, _) = validate(transaction, &request)?;
        let sql = source_sql(transaction, &request, source)?;
        let sql = format!("SELECT a,b FROM ({sql}) WHERE (a,b)>(?1,?2) ORDER BY a,b LIMIT ?3");
        let files = match &request.scope {
            Scope::Files(files) => files.as_slice(),
            Scope::Location(_) => &[],
        };
        let mut cursor = (0_i64, 0_i64);
        let mut done = Progress::default();
        loop {
            if cancel.load(Ordering::Acquire) {
                done.cancelled = true;
                break;
            }
            let mut statement = transaction.prepare(&sql)?;
            let mut rows = if sql.contains("?4") {
                statement.query(params![
                    cursor.0,
                    cursor.1,
                    i64::try_from(batch_size).unwrap_or(1024),
                    crate::master::id_array(files)
                ])?
            } else {
                statement.query(params![
                    cursor.0,
                    cursor.1,
                    i64::try_from(batch_size).unwrap_or(1024)
                ])?
            };
            let mut batch = Vec::new();
            let mut scanned = 0;
            while let Some(row) = rows.next()? {
                let a: TagId = row.get(0)?;
                let b: u32 = row.get(1)?;
                cursor = (i64::from(a.get()), i64::from(b));
                scanned += 1;
                let left = crate::master::tag(transaction, a)?
                    .ok_or_else(|| StoreError::Corrupt("missing migration tag".into()))?;
                if !request.left_filter.tag_ok(left.as_str(), false) {
                    continue;
                }
                if request.content != Content::Mappings {
                    let right = crate::master::tag(transaction, TagId(b))?
                        .ok_or_else(|| StoreError::Corrupt("missing migration tag".into()))?;
                    if !request.right_filter.tag_ok(right.as_str(), false) {
                        continue;
                    }
                }
                batch.push((a, b));
            }
            if scanned == 0 {
                break;
            }
            if cancel.load(Ordering::Acquire) {
                done.cancelled = true;
                break;
            }
            let accepted = batch.len();
            let settings = request.clone();
            if request.content == Content::Mappings {
                store.write_content(move |writer| {
                    let (_, destination) = validate(writer.conn(), &settings)?;
                    mappings(writer, destination, &settings, &batch)
                })?;
            } else {
                store.write_and_refresh(move |ctx| {
                    let (_, destination) = validate(ctx.conn(), &settings)?;
                    pairs(ctx.conn(), destination, &settings, &batch)
                })?;
            }
            done.scanned += scanned;
            // Progress counts accepted source entries; idempotent destination
            // changes may leave an entry's existing state untouched.
            done.accepted += accepted;
            progress(done);
            while paused.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
        progress(done);
        Ok(done)
    })
}
fn mappings(
    writer: &mut ContentWriter<'_>,
    destination: ServiceId,
    request: &Request,
    batch: &[(TagId, u32)],
) -> Result<()> {
    let action = match request.action {
        Action::Add => MappingAction::Add,
        Action::Delete => MappingAction::Delete,
        Action::Pend => MappingAction::Pend,
        Action::Petition => MappingAction::Petition {
            reason: request.reason.clone(),
        },
        Action::ClearDeletion => {
            let table = crate::schema::MappingTables::new(destination).deleted;
            for (tag, hash) in batch {
                writer.conn().execute(
                    &format!("DELETE FROM {table} WHERE tag_id=?1 AND hash_id=?2"),
                    params![tag, hash],
                )?;
            }
            return Ok(());
        }
    };
    for (tag, hash) in batch {
        writer.update_mappings(destination, &action, *tag, &[HashId(*hash)])?;
    }
    Ok(())
}
fn pairs(
    conn: &Connection,
    destination: ServiceId,
    request: &Request,
    batch: &[(TagId, u32)],
) -> Result<()> {
    let kind = if request.content == Content::Siblings {
        RelationKind::Siblings
    } else {
        RelationKind::Parents
    };
    let action = match request.action {
        Action::Add => crate::content::tag_relations::RelationAction::Add,
        Action::Delete => crate::content::tag_relations::RelationAction::Delete,
        Action::Pend => crate::content::tag_relations::RelationAction::Pend(request.reason.clone()),
        Action::Petition => {
            crate::content::tag_relations::RelationAction::Petition(request.reason.clone())
        }
        Action::ClearDeletion => {
            return Err(StoreError::Invalid(
                "pairs have no clear deletion action".into(),
            ));
        }
    };
    let updates = batch
        .iter()
        .map(|(a, b)| {
            Ok(crate::content::tag_relations::RelationUpdate {
                service: destination,
                left: crate::master::tag(conn, *a)?
                    .ok_or_else(|| StoreError::Corrupt("missing tag".into()))?,
                right: crate::master::tag(conn, TagId(*b))?
                    .ok_or_else(|| StoreError::Corrupt("missing tag".into()))?,
                action: action.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    crate::content::tag_relations::apply_on(conn, kind, updates)
}
