//! Mixed archive/service migration jobs and optional real mapping-count gates.
use super::{
    Action, Content, Progress, Request, Scope, Status,
    archive::{Archive, Entry, Metadata, Value},
};
use crate::{Result, Store, StoreError, services::ServiceRegistry};
use hydrus_core::{HashId, ServiceId, ServiceKey, ServiceType, Sha256, Tag, TagId, hash::HashKind};
use rusqlite::{Connection, params};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

/// Optional pair filtering against real current and pending storage mappings.
#[derive(Debug, Clone)]
pub struct PairCounts {
    pub service: ServiceKey,
    pub left: bool,
    pub right: bool,
    pub either: bool,
}
/// Archive endpoints replace the corresponding service key in `Request`.
#[derive(Debug, Clone)]
pub struct Options {
    pub source: Option<PathBuf>,
    pub destination: Option<PathBuf>,
    pub hash_kind: HashKind,
    pub counts: Option<PairCounts>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            source: None,
            destination: None,
            hash_kind: HashKind::Sha256,
            counts: None,
        }
    }
}
fn endpoints(
    conn: &Connection,
    request: &Request,
    options: &Options,
) -> Result<(Option<ServiceId>, Option<ServiceId>)> {
    if options.source.is_none() && options.destination.is_none() {
        let (a, b) = super::validate(conn, request)?;
        return Ok((Some(a), Some(b)));
    }
    let registry = ServiceRegistry::load(conn)?;
    let resolve = |key: &ServiceKey| -> Result<_> {
        let service = registry.by_key(key)?;
        if !matches!(
            service.service_type(),
            ServiceType::LocalTag | ServiceType::TagRepository
        ) {
            return Err(StoreError::Invalid(
                "migration requires real tag services".into(),
            ));
        }
        Ok(service)
    };
    let source = if options.source.is_none() {
        let service = resolve(&request.source)?;
        if service.service_type() == ServiceType::LocalTag
            && matches!(request.status, Status::Pending | Status::CurrentAndPending)
        {
            return Err(StoreError::Invalid(
                "local sources have no pending status".into(),
            ));
        }
        Some(service.id)
    } else {
        if request.status != Status::Current {
            return Err(StoreError::Invalid(
                "archives have only current content".into(),
            ));
        }
        None
    };
    let destination = if options.destination.is_none() {
        let service = resolve(&request.destination)?;
        if !super::actions(
            service.service_type() == ServiceType::LocalTag,
            false,
            request.content,
            request.status,
        )
        .contains(&request.action)
        {
            return Err(StoreError::Invalid(
                "migration action is unavailable for this destination".into(),
            ));
        }
        if request.action == Action::Petition && request.reason.trim().is_empty() {
            return Err(StoreError::Invalid("a petition reason is required".into()));
        }
        Some(service.id)
    } else {
        if request.action != Action::Add {
            return Err(StoreError::Invalid(
                "archive destinations support only add".into(),
            ));
        }
        None
    };
    Ok((source, destination))
}
fn service_batch(
    conn: &Connection,
    request: &Request,
    sql: &str,
    cursor: &mut (i64, i64),
    size: usize,
) -> Result<Vec<Entry>> {
    let files = match &request.scope {
        Scope::Files(files) => files.as_slice(),
        Scope::Location(_) => &[],
    };
    let mut statement = conn.prepare(sql)?;
    let limit = i64::try_from(size).unwrap_or(1024);
    let mut rows = if sql.contains("?4") {
        statement.query(params![
            cursor.0,
            cursor.1,
            limit,
            crate::master::id_array(files)
        ])?
    } else {
        statement.query(params![cursor.0, cursor.1, limit])?
    };
    let mut entries = Vec::new();
    while let Some(row) = rows.next()? {
        let a: TagId = row.get(0)?;
        let b: u32 = row.get(1)?;
        *cursor = (i64::from(a.get()), i64::from(b));
        let left = crate::master::tag(conn, a)?
            .ok_or_else(|| StoreError::Corrupt("missing migration tag".into()))?
            .as_str()
            .to_owned();
        let right = if request.content == Content::Mappings {
            Value::Hash(
                crate::master::hash(conn, HashId(b))?
                    .ok_or_else(|| StoreError::Corrupt("missing migration hash".into()))?
                    .as_bytes()
                    .to_vec(),
            )
        } else {
            Value::Tag(
                crate::master::tag(conn, TagId(b))?
                    .ok_or_else(|| StoreError::Corrupt("missing migration tag".into()))?
                    .as_str()
                    .to_owned(),
            )
        };
        entries.push(Entry { left, right });
    }
    Ok(entries)
}
fn convert(
    conn: &Connection,
    hash: &[u8],
    from: HashKind,
    to: HashKind,
) -> Result<Option<Vec<u8>>> {
    if hash.len() != from.byte_len() {
        return Err(StoreError::Invalid(
            "archive hash has incorrect length".into(),
        ));
    }
    if from == to {
        return Ok(Some(hash.to_vec()));
    }
    Ok(
        crate::master::convert_hashes(conn, from, to, &[hash.to_vec()])?
            .into_iter()
            .next()
            .map(|(_, h)| h),
    )
}
fn in_scope(conn: &Connection, request: &Request, hash: &[u8]) -> Result<bool> {
    let sha256 = Sha256::from_slice(hash).map_err(|e| StoreError::Invalid(e.to_string()))?;
    let id = crate::master::hash_id(conn, &sha256)?;
    match &request.scope {
        Scope::Files(files) => Ok(id.is_some_and(|id| files.contains(&id))),
        Scope::Location(location) if location.is_all_known_files() => Ok(true),
        Scope::Location(location) => {
            let Some(id) = id else { return Ok(false) };
            let registry = ServiceRegistry::load(conn)?;
            for (keys, table) in [
                (location.current(), "file_domain_current"),
                (location.deleted(), "file_domain_deleted"),
            ] {
                for key in keys {
                    let service = registry.by_key(key)?;
                    if !service.service_type().is_file_service() {
                        return Err(StoreError::Invalid("invalid file domain".into()));
                    }
                    if conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE service_id=?1 AND hash_id=?2)"),params![service.id,id],|r|r.get::<_,bool>(0))? {return Ok(true)}
                }
            }
            Ok(false)
        }
    }
}
struct Gate {
    counts: PairCounts,
    tables: crate::schema::MappingTables,
    graph: Option<std::sync::Arc<crate::display::DisplayGraph>>,
}
fn gate(conn: &Connection, content: Content, counts: &PairCounts) -> Result<Gate> {
    let registry = ServiceRegistry::load(conn)?;
    let service = registry.by_key(&counts.service)?;
    if !matches!(
        service.service_type(),
        ServiceType::LocalTag | ServiceType::TagRepository
    ) {
        return Err(StoreError::Invalid(
            "pair counts require a real tag service".into(),
        ));
    }
    let tables = crate::schema::MappingTables::new(service.id);
    let graph = if content == Content::Siblings {
        Some(crate::display::DisplayGraphs::load(conn, &registry)?.get(service.id))
    } else {
        None
    };
    Ok(Gate {
        counts: counts.clone(),
        tables,
        graph,
    })
}
fn pair_ok(conn: &Connection, gate: &Gate, left: &str, right: &str) -> Result<bool> {
    let counts = &gate.counts;
    let tables = &gate.tables;
    let graph = &gate.graph;
    let has_count = |text: &str, ideal: bool| -> Result<bool> {
        let tag = Tag::new(text).ok_or_else(|| StoreError::Invalid("empty archive tag".into()))?;
        let Some(mut id) = crate::master::tag_id(conn, &tag)? else {
            return Ok(false);
        };
        if ideal && let Some(graph) = graph {
            id = graph.ideal(id)
        }
        Ok(conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {} WHERE tag_id=?1 UNION ALL SELECT 1 FROM {} WHERE tag_id=?1)",tables.current,tables.pending),[id],|r|r.get(0))?)
    };
    if counts.either {
        return Ok(has_count(left, false)? || has_count(right, true)?);
    }
    Ok((!counts.left || has_count(left, false)?) && (!counts.right || has_count(right, true)?))
}
fn filter(
    conn: &Connection,
    request: &Request,
    options: &Options,
    entries: Vec<Entry>,
    from: HashKind,
    to: HashKind,
    gate: Option<&Gate>,
) -> Result<Vec<Entry>> {
    let scope_needed = !matches!(&request.scope,Scope::Location(l) if l.is_all_known_files());
    let mut batch = Vec::new();
    for mut entry in entries {
        if !request.left_filter.tag_ok(&entry.left, false) {
            continue;
        }
        match &entry.right {
            Value::Tag(right) => {
                if !request.right_filter.tag_ok(right, false) {
                    continue;
                }
                if let Some(gate) = gate
                    && !pair_ok(conn, gate, &entry.left, right)?
                {
                    continue;
                }
            }
            Value::Hash(hash) => {
                let fixed = if options.source.is_some() && scope_needed {
                    let Some(sha) = convert(conn, hash, from, HashKind::Sha256)? else {
                        continue;
                    };
                    if !in_scope(conn, request, &sha)? {
                        continue;
                    }
                    convert(conn, &sha, HashKind::Sha256, to)?
                } else {
                    convert(conn, hash, from, to)?
                };
                let Some(hash) = fixed else { continue };
                entry.right = Value::Hash(hash);
            }
        }
        batch.push(entry);
    }
    Ok(batch)
}
fn native_batch(conn: &Connection, entries: &[Entry]) -> Result<Vec<(TagId, u32)>> {
    entries
        .iter()
        .map(|entry| {
            let tag = Tag::new(&entry.left)
                .ok_or_else(|| StoreError::Invalid("empty archive tag".into()))?;
            let a = crate::master::intern_tag(conn, &tag)?;
            let b = match &entry.right {
                Value::Hash(hash) => crate::master::intern_hash(
                    conn,
                    &Sha256::from_slice(hash).map_err(|e| StoreError::Invalid(e.to_string()))?,
                )?
                .get(),
                Value::Tag(tag) => crate::master::intern_tag(
                    conn,
                    &Tag::new(tag)
                        .ok_or_else(|| StoreError::Invalid("empty archive tag".into()))?,
                )?
                .get(),
            };
            Ok((a, b))
        })
        .collect()
}
/// Run mixed archive/service endpoints with bounded atomic destination batches.
/// Unknown alternate hashes are skipped; unsupported metadata is rejected before
/// importing anything. Archive metadata is rechecked independently of UI inspection.
pub fn run(
    store: &Store,
    request: &Request,
    options: &Options,
    cancel: &AtomicBool,
    paused: &AtomicBool,
    size: usize,
    mut progress: impl FnMut(Progress),
) -> Result<Progress> {
    if options.source.is_none() && options.destination.is_none() && options.counts.is_none() {
        return super::run_pausable(store, request, cancel, paused, size, progress);
    }
    if !(1..=1024).contains(&size) {
        return Err(StoreError::Invalid(
            "migration batch size must be 1..=1024".into(),
        ));
    }
    let _guard = store.claim_tag_migration()?;
    store.read(|conn| endpoints(conn, request, options))?;
    if let (Some(source), Some(destination)) = (&options.source, &options.destination)
        && destination.exists()
        && std::fs::canonicalize(source)? == std::fs::canonicalize(destination)?
    {
        return Err(StoreError::Invalid(
            "source and destination archive paths must differ".into(),
        ));
    }
    let source = options
        .source
        .as_ref()
        .map(|p| Archive::source(p, request.content))
        .transpose()?;
    let mut destination = options
        .destination
        .as_ref()
        .map(|p| Archive::destination(p, request.content, options.hash_kind))
        .transpose()?;
    let from = source
        .as_ref()
        .and_then(|a| {
            if let Metadata::Mappings(k) = a.metadata {
                Some(k)
            } else {
                None
            }
        })
        .unwrap_or(HashKind::Sha256);
    let to = destination
        .as_ref()
        .and_then(|a| {
            if let Metadata::Mappings(k) = a.metadata {
                Some(k)
            } else {
                None
            }
        })
        .unwrap_or(HashKind::Sha256);
    let request = std::sync::Arc::new(request.clone());
    let options = std::sync::Arc::new(options.clone());
    store.read(|conn| {
        let (service, _) = endpoints(conn, &request, &options)?;
        let gate = options
            .counts
            .as_ref()
            .map(|counts| gate(conn, request.content, counts))
            .transpose()?;
        let sql = service
            .map(|s| super::source_sql(conn, &request, s))
            .transpose()?
            .map(|s| format!("SELECT a,b FROM ({s}) WHERE (a,b)>(?1,?2) ORDER BY a,b LIMIT ?3"));
        let mut cursor = if source.is_some() {
            (i64::MIN, i64::MIN)
        } else {
            (0_i64, 0_i64)
        };
        let mut done = Progress::default();
        loop {
            if cancel.load(Ordering::Acquire) {
                done.cancelled = true;
                break;
            }
            let entries = if let Some(archive) = &source {
                archive.read(&mut cursor, size)?
            } else {
                service_batch(
                    conn,
                    &request,
                    sql.as_deref()
                        .ok_or_else(|| StoreError::Corrupt("missing migration source".into()))?,
                    &mut cursor,
                    size,
                )?
            };
            let scanned = entries.len();
            if scanned == 0 {
                break;
            }
            let batch = filter(conn, &request, &options, entries, from, to, gate.as_ref())?;
            if cancel.load(Ordering::Acquire) {
                done.cancelled = true;
                break;
            }
            let accepted = batch.len();
            if let Some(archive) = &mut destination {
                archive.write(&batch)?
            } else {
                let settings = request.clone();
                let options = options.clone();
                if request.content == Content::Mappings {
                    store.write_content(move |writer| {
                        let (_, destination) = endpoints(writer.conn(), &settings, &options)?;
                        let batch = native_batch(writer.conn(), &batch)?;
                        super::mappings(
                            writer,
                            destination
                                .ok_or_else(|| StoreError::Corrupt("missing destination".into()))?,
                            &settings,
                            &batch,
                        )
                    })?;
                } else {
                    store.write_and_refresh(move |ctx| {
                        let (_, destination) = endpoints(ctx.conn(), &settings, &options)?;
                        let batch = native_batch(ctx.conn(), &batch)?;
                        super::pairs(
                            ctx.conn(),
                            destination
                                .ok_or_else(|| StoreError::Corrupt("missing destination".into()))?,
                            &settings,
                            &batch,
                        )
                    })?;
                }
            }
            done.scanned += scanned;
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
