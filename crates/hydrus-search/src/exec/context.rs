//! The resolved search environment: the file domain, the tag domain and
//! lazily computed facts shared by every predicate of one search.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;

use roaring::RoaringBitmap;
use rusqlite::Connection;

use hydrus_core::{ContentStatus, NamespaceId, ServiceId, ServiceKey, ServiceType, TagId};
use hydrus_store::Snapshot;
use hydrus_store::display::DisplayGraph;
use hydrus_store::domains::Domains;
use hydrus_store::schema::MappingTables;
use hydrus_store::services::{Service, ServiceKind};
use hydrus_store::settings::{self, FileViewingStatistics};

use super::sql::{self, int_array};
use super::time::Clock;
use super::{Result, SearchError};
use crate::context::{FileSearchContext, LocationContext, TagContext};

/// A file domain table: files currently in, or deleted from, a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DomainTable {
    pub service: ServiceId,
    pub deleted: bool,
}

impl DomainTable {
    pub fn table(self) -> &'static str {
        if self.deleted {
            "file_domain_deleted"
        } else {
            "file_domain_current"
        }
    }
}

/// The files a search covers.
#[derive(Debug, Clone)]
pub(crate) enum Domain {
    /// Files in any of these tables, minus files currently in `except`.
    Tables {
        tables: Vec<DomainTable>,
        except: Option<ServiceId>,
    },
    /// "All known files": every file with a tag in the tag domain.
    AllKnownFiles,
}

/// One real tag service a search reads, with the display rules it applies.
#[derive(Debug, Clone)]
pub(crate) struct TagService {
    pub id: ServiceId,
    pub graph: Arc<DisplayGraph>,
    pub tables: MappingTables,
}

/// The tag domain: which services and which mapping statuses are searched.
#[derive(Debug, Clone)]
pub(crate) struct TagScope {
    pub services: Vec<TagService>,
    pub statuses: Vec<ContentStatus>,
}

impl TagScope {
    /// Every (service, mapping table) pair searched.
    pub fn tables(&self) -> impl Iterator<Item = (&TagService, &str)> {
        self.services.iter().flat_map(move |s| {
            self.statuses
                .iter()
                .map(move |status| (s, s.tables.for_status(*status)))
        })
    }
}

/// Facts about the location that some predicates depend on.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LocationFacts {
    /// Every domain is a local one (so every file is stored locally).
    pub covered_by_local_storage: bool,
    /// The domain is exactly "deleted from hydrus local file storage".
    pub local_storage_deleted_only: bool,
    /// The single current domain, if the location is exactly one.
    pub single_current: Option<ServiceId>,
}

/// How the planner picks between probing candidates and scanning indexes.
/// Tests force each path; searches use [`Strategy::Auto`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Strategy {
    /// Whichever is estimated to read fewer rows.
    #[default]
    Auto,
    #[cfg_attr(not(test), allow(dead_code))]
    AlwaysProbe,
    #[cfg_attr(not(test), allow(dead_code))]
    AlwaysScan,
}

/// Everything one search needs, resolved once.
pub(crate) struct Env<'a> {
    pub conn: &'a Connection,
    pub snapshot: &'a Snapshot,
    pub clock: &'a Clock,
    pub domain: Domain,
    pub location: LocationFacts,
    pub tags: TagScope,
    pub viewing: FileViewingStatistics,
    pub strategy: Strategy,
    cache: Domains<'a>,
    domain_files: OnceCell<Arc<RoaringBitmap>>,
    domain_size: OnceCell<Option<u64>>,
    graph_namespaces: OnceCell<HashMap<TagId, NamespaceId>>,
}

impl std::fmt::Debug for Env<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("domain", &self.domain)
            .field("tags", &self.tags)
            .finish_non_exhaustive()
    }
}

impl<'a> Env<'a> {
    pub fn new(
        conn: &'a Connection,
        snapshot: &'a Snapshot,
        search: &FileSearchContext,
        clock: &'a Clock,
        strategy: Strategy,
    ) -> Result<Self> {
        let (domain, location) = resolve_location(snapshot, &search.location)?;
        let tags = resolve_tags(snapshot, &search.tags)?;
        Ok(Self {
            conn,
            snapshot,
            clock,
            domain,
            location,
            tags,
            viewing: settings::get(conn)?,
            strategy,
            cache: snapshot.domains.for_read(conn)?,
            domain_files: OnceCell::new(),
            domain_size: OnceCell::new(),
            graph_namespaces: OnceCell::new(),
        })
    }

    /// The same environment over a different tag scope (for predicates
    /// that name their own tag service).
    pub fn with_tags(&self, tags: TagScope) -> Env<'a> {
        Env {
            conn: self.conn,
            snapshot: self.snapshot,
            clock: self.clock,
            domain: self.domain.clone(),
            location: self.location,
            tags,
            viewing: self.viewing.clone(),
            strategy: self.strategy,
            cache: self.cache,
            domain_files: OnceCell::new(),
            domain_size: OnceCell::new(),
            graph_namespaces: OnceCell::new(),
        }
    }

    /// Whether to probe `candidates` files rather than scan an index of
    /// `scan_rows` rows (see [`sql::probe_is_cheaper`]).
    pub fn prefer_probe(&self, candidates: u64, scan_rows: Option<u64>) -> bool {
        match self.strategy {
            Strategy::Auto => sql::probe_is_cheaper(candidates, scan_rows),
            Strategy::AlwaysProbe => true,
            Strategy::AlwaysScan => false,
        }
    }

    /// Every file in the domain.
    pub fn domain_files(&self) -> Result<&RoaringBitmap> {
        if let Some(files) = self.domain_files.get() {
            return Ok(files);
        }
        let files = self.load_domain()?;
        Ok(self.domain_files.get_or_init(|| files))
    }

    /// Every file in the domain, if the domain's files are already known
    /// (cached by an earlier search), so using them costs no reads.
    fn known_domain_files(&self) -> Result<Option<&RoaringBitmap>> {
        if let Some(files) = self.domain_files.get() {
            return Ok(Some(files));
        }
        let Domain::Tables { tables, except } = &self.domain else {
            return Ok(None);
        };
        let parts = tables.iter().map(|t| (t.service, t.deleted));
        let all_cached = parts
            .chain(except.map(|e| (e, false)))
            .all(|(service, deleted)| self.cache.cached(service, deleted).is_some());
        if all_cached {
            self.domain_files().map(Some)
        } else {
            Ok(None)
        }
    }

    fn load_domain(&self) -> Result<Arc<RoaringBitmap>> {
        match &self.domain {
            Domain::Tables { tables, except } => {
                if let ([t], None) = (tables.as_slice(), except) {
                    return Ok(self.cache.files(self.conn, t.service, t.deleted)?);
                }
                let mut out = RoaringBitmap::new();
                for t in tables {
                    out |= &*self.cache.files(self.conn, t.service, t.deleted)?;
                }
                if let Some(except) = except {
                    out -= &*self.cache.files(self.conn, *except, false)?;
                }
                Ok(Arc::new(out))
            }
            Domain::AllKnownFiles => {
                let mut out = RoaringBitmap::new();
                for (_, table) in self.tags.tables() {
                    let mut stmt = self
                        .conn
                        .prepare_cached(&format!("SELECT DISTINCT hash_id FROM {table}"))?;
                    out |= sql::collect(&mut stmt, [])?;
                }
                Ok(Arc::new(out))
            }
        }
    }

    /// Roughly how many files the domain has, if that is cheap to know.
    pub fn domain_size(&self) -> Result<Option<u64>> {
        if let Some(files) = self.known_domain_files()? {
            return Ok(Some(files.len()));
        }
        if let Some(size) = self.domain_size.get() {
            return Ok(*size);
        }
        let size = match &self.domain {
            Domain::Tables { tables, .. } => {
                let mut total = 0u64;
                for t in tables {
                    let n: i64 = self
                        .conn
                        .prepare_cached(&format!(
                            "SELECT count(*) FROM {} WHERE service_id = ?",
                            t.table()
                        ))?
                        .query_row([t.service], |r| r.get(0))?;
                    total += n.max(0) as u64;
                }
                Some(total)
            }
            Domain::AllKnownFiles => None,
        };
        Ok(*self.domain_size.get_or_init(|| size))
    }

    /// The files of `set` that are in the domain.
    pub fn restrict_to_domain(&self, set: RoaringBitmap) -> Result<RoaringBitmap> {
        if set.is_empty() {
            return Ok(set);
        }
        if let Some(files) = self.known_domain_files()? {
            return Ok(set & files);
        }
        let probe = match self.strategy {
            Strategy::AlwaysProbe => true,
            Strategy::AlwaysScan => false,
            Strategy::Auto => match self.domain_size()? {
                Some(size) => (set.len() as f64) * sql::PROBE_COST < size as f64,
                None => true,
            },
        };
        if !probe {
            return Ok(set & self.domain_files()?);
        }
        match &self.domain {
            Domain::Tables { tables, except } => {
                let mut out = RoaringBitmap::new();
                for t in tables {
                    out |= sql::probe(
                        self.conn,
                        &format!(
                            "SELECT hash_id FROM {} WHERE hash_id IN rarray(?1) AND service_id = ?2",
                            t.table()
                        ),
                        &set,
                        &[&t.service],
                    )?;
                }
                if let Some(except) = except {
                    let excluded = sql::probe(
                        self.conn,
                        "SELECT hash_id FROM file_domain_current WHERE hash_id IN rarray(?1) AND service_id = ?2",
                        &out,
                        &[except],
                    )?;
                    out -= excluded;
                }
                Ok(out)
            }
            Domain::AllKnownFiles => {
                let mut out = RoaringBitmap::new();
                for (_, table) in self.tags.tables() {
                    let remaining = &set - &out;
                    out |= sql::probe(
                        self.conn,
                        &format!(
                            "SELECT value FROM rarray(?1) WHERE EXISTS (SELECT 1 FROM {table} WHERE hash_id = value)"
                        ),
                        &remaining,
                        &[],
                    )?;
                }
                Ok(out)
            }
        }
    }

    /// The namespace of every tag the display graphs mention, for working
    /// out which namespaces a stored tag displays in.
    pub fn graph_namespaces(&self) -> Result<&HashMap<TagId, NamespaceId>> {
        if let Some(map) = self.graph_namespaces.get() {
            return Ok(map);
        }
        let mut all: Vec<u32> = Vec::new();
        for service in &self.tags.services {
            all.extend(service.graph.all_tags().into_iter().map(TagId::get));
        }
        all.sort_unstable();
        all.dedup();
        let mut map = HashMap::with_capacity(all.len());
        if !all.is_empty() {
            let mut stmt = self.conn.prepare_cached(
                "SELECT tag_id, namespace_id FROM tags WHERE tag_id IN rarray(?)",
            )?;
            let mut rows = stmt.query([int_array(all)])?;
            while let Some(row) = rows.next()? {
                map.insert(row.get(0)?, row.get(1)?);
            }
        }
        Ok(self.graph_namespaces.get_or_init(|| map))
    }

    /// The single service of a type, e.g. hydrus local file storage.
    /// Cached data about file domains, as this search's snapshot has them.
    pub fn domain_cache(&self) -> Domains<'a> {
        self.cache
    }

    pub fn service_of_type(&self, service_type: ServiceType) -> Option<&Arc<Service>> {
        self.snapshot.services.of_type(service_type).next()
    }
}

/// Turn a tag context into the real tag services to read.
pub(crate) fn resolve_tags(snapshot: &Snapshot, context: &TagContext) -> Result<TagScope> {
    let service = snapshot
        .services
        .by_key(&context.service)
        .map_err(|_| SearchError::NoSuchService(context.service.clone()))?;
    let services: Vec<&Arc<Service>> = match service.service_type() {
        ServiceType::CombinedTag => snapshot.services.tag_services().collect(),
        t if t.is_real_tag_service() => vec![service],
        _ => {
            return Err(SearchError::WrongServiceKind {
                key: context.service.clone(),
                expected: "tag service",
            });
        }
    };
    let mut statuses = Vec::new();
    if context.include_current {
        statuses.push(ContentStatus::Current);
    }
    if context.include_pending {
        statuses.push(ContentStatus::Pending);
    }
    Ok(TagScope {
        services: services
            .into_iter()
            .map(|s| tag_service(snapshot, s.id))
            .collect(),
        statuses,
    })
}

pub(crate) fn tag_service(snapshot: &Snapshot, id: ServiceId) -> TagService {
    TagService {
        id,
        graph: snapshot.display.get(id),
        tables: MappingTables::new(id),
    }
}

/// Service types whose files are all stored locally.
fn is_local_storage_type(t: ServiceType) -> bool {
    matches!(
        t,
        ServiceType::CombinedLocalFileDomains
            | ServiceType::LocalFileDomain
            | ServiceType::LocalFileUpdateDomain
            | ServiceType::LocalFileTrashDomain
            | ServiceType::HydrusLocalFileStorage
    )
}

fn resolve_location(
    snapshot: &Snapshot,
    location: &LocationContext,
) -> Result<(Domain, LocationFacts)> {
    let lookup = |key: &ServiceKey| -> Result<&Arc<Service>> {
        let service = snapshot
            .services
            .by_key(key)
            .map_err(|_| SearchError::NoSuchService(key.clone()))?;
        if service.service_type().is_file_service() {
            Ok(service)
        } else {
            Err(SearchError::WrongServiceKind {
                key: key.clone(),
                expected: "file service",
            })
        }
    };
    let current: Vec<&Arc<Service>> = location
        .current()
        .iter()
        .map(lookup)
        .collect::<Result<_>>()?;
    let deleted: Vec<&Arc<Service>> = location
        .deleted()
        .iter()
        .map(lookup)
        .collect::<Result<_>>()?;

    if current
        .iter()
        .any(|s| matches!(s.kind, ServiceKind::AllKnownFiles))
    {
        return Ok((Domain::AllKnownFiles, LocationFacts::default()));
    }

    let facts = LocationFacts {
        covered_by_local_storage: deleted.is_empty()
            && current
                .iter()
                .all(|s| is_local_storage_type(s.service_type())),
        local_storage_deleted_only: current.is_empty()
            && deleted.len() == 1
            && deleted[0].service_type() == ServiceType::HydrusLocalFileStorage,
        single_current: (current.len() == 1 && deleted.is_empty()).then(|| current[0].id),
    };
    // Searching "hydrus local file storage" alone hides repository update
    // files, which are internal.
    let except = if current.len() == 1
        && deleted.is_empty()
        && current[0].service_type() == ServiceType::HydrusLocalFileStorage
    {
        snapshot
            .services
            .of_type(ServiceType::LocalFileUpdateDomain)
            .next()
            .map(|s| s.id)
    } else {
        None
    };
    let tables = current
        .iter()
        .map(|s| DomainTable {
            service: s.id,
            deleted: false,
        })
        .chain(deleted.iter().map(|s| DomainTable {
            service: s.id,
            deleted: true,
        }))
        .collect();
    Ok((Domain::Tables { tables, except }, facts))
}
