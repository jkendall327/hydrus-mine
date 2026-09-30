//! The files in each file domain, and their import order, kept between
//! reads until a write changes them.
//!
//! Most searches are limited to a domain ("all my files") and sorted by
//! import time, and loading either costs a row per file in the domain.
//! Every write that changes domain membership or import times increments
//! `domain_generation` in the same transaction, so a read learns from its
//! own snapshot which generation of cached data it may use and never sees
//! files from a different state of the database.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use roaring::RoaringBitmap;
use rusqlite::Connection;

use hydrus_core::ServiceId;

use crate::error::Result;

/// A domain's current files, or the files deleted from it.
type Key = (ServiceId, bool);

/// Values with the generation they were read at.
type ByGeneration<K, V> = Mutex<HashMap<K, (i64, Arc<V>)>>;

#[derive(Debug, Default)]
pub struct DomainCache {
    entries: ByGeneration<Key, RoaringBitmap>,
    import_orders: ByGeneration<ServiceId, Vec<u32>>,
    /// Each domain's size when last loaded or counted, whatever the
    /// generation: domains change size slowly, so it is a good estimate.
    sizes: Mutex<HashMap<Key, u64>>,
}

fn table(deleted: bool) -> &'static str {
    if deleted {
        "file_domain_deleted"
    } else {
        "file_domain_current"
    }
}

impl DomainCache {
    /// The cache as a read in progress on `conn` may use it.
    pub fn for_read<'c>(&'c self, conn: &Connection) -> Result<Domains<'c>> {
        let generation =
            conn.query_row("SELECT generation FROM domain_generation", [], |r| r.get(0))?;
        Ok(Domains {
            cache: self,
            generation,
        })
    }
}

/// Whether data read on `conn` may be cached for other reads: only a
/// reader's, which sees committed data. A write's own uncommitted changes
/// could be rolled back, and the generation they bumped reused.
pub(crate) fn may_cache(conn: &Connection) -> bool {
    conn.pragma_query_value(None, "query_only", |r| r.get::<_, bool>(0))
        .unwrap_or(false)
}

/// Whether to check `candidates` files against a domain of `size` files one
/// by one rather than load the whole domain. Checking a file costs about
/// eight scanned rows (measured at 400,000 files), but a loaded domain is
/// kept for later reads until a write changes it, so loading is worth it
/// well before the costs are equal.
pub fn probe_is_cheaper(candidates: u64, size: u64) -> bool {
    candidates.saturating_mul(16) < size
}

/// Record, in a write's transaction, that it changed domain membership or
/// import times.
/// [`crate::content::ContentWriter`] does this; a write that changes the domain tables
/// any other way must too.
pub fn changed(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE domain_generation SET generation = generation + 1",
        [],
    )?;
    Ok(())
}

/// The cache as one read may use it.
#[derive(Debug, Clone, Copy)]
pub struct Domains<'a> {
    cache: &'a DomainCache,
    generation: i64,
}

impl Domains<'_> {
    /// The files of a domain if they are already cached for this read.
    pub fn cached(&self, service: ServiceId, deleted: bool) -> Option<Arc<RoaringBitmap>> {
        match self.cache.entries.lock().get(&(service, deleted)) {
            Some((generation, files)) if *generation == self.generation => Some(Arc::clone(files)),
            _ => None,
        }
    }

    /// The files currently in (or, if `deleted`, deleted from) a domain.
    pub fn files(
        &self,
        conn: &Connection,
        service: ServiceId,
        deleted: bool,
    ) -> Result<Arc<RoaringBitmap>> {
        if let Some(files) = self.cached(service, deleted) {
            return Ok(files);
        }
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT hash_id FROM {} WHERE service_id = ?",
            table(deleted)
        ))?;
        let mut files = RoaringBitmap::new();
        let mut rows = stmt.query([service])?;
        while let Some(row) = rows.next()? {
            files.insert(row.get(0)?);
        }
        let files = Arc::new(files);
        let key = (service, deleted);
        self.cache.sizes.lock().insert(key, files.len());
        if !may_cache(conn) {
            return Ok(files);
        }
        let mut entries = self.cache.entries.lock();
        // an older read must not displace a newer one's bitmap
        if entries
            .get(&key)
            .is_none_or(|(generation, _)| *generation < self.generation)
        {
            entries.insert(key, (self.generation, Arc::clone(&files)));
        }
        Ok(files)
    }

    /// Roughly how many files a domain has (or has deleted): exact if its
    /// files are cached for this read, else as last seen, else counted.
    pub fn size_estimate(&self, conn: &Connection, service: ServiceId, deleted: bool) -> Result<u64> {
        if let Some(files) = self.cached(service, deleted) {
            return Ok(files.len());
        }
        let key = (service, deleted);
        if let Some(&size) = self.cache.sizes.lock().get(&key) {
            return Ok(size);
        }
        let size: i64 = conn
            .prepare_cached(&format!(
                "SELECT count(*) FROM {} WHERE service_id = ?",
                table(deleted)
            ))?
            .query_row([service], |r| r.get(0))?;
        let size = u64::try_from(size).unwrap_or(0);
        self.cache.sizes.lock().insert(key, size);
        Ok(size)
    }

    /// [`Self::import_order`], if it is already cached for this read.
    pub fn cached_import_order(&self, service: ServiceId) -> Option<Arc<Vec<u32>>> {
        match self.cache.import_orders.lock().get(&service) {
            Some((generation, order)) if *generation == self.generation => Some(Arc::clone(order)),
            _ => None,
        }
    }

    /// A domain's current files by import time then file id, oldest first;
    /// a file with no import time counts as imported at -1 ms.
    pub fn import_order(&self, conn: &Connection, service: ServiceId) -> Result<Arc<Vec<u32>>> {
        if let Some(order) = self.cached_import_order(service) {
            return Ok(order);
        }
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, added_ms FROM file_domain_current WHERE service_id = ?",
        )?;
        let mut rows: Vec<(i64, u32)> = stmt
            .query_map([service], |r| {
                Ok((r.get::<_, Option<i64>>(1)?.unwrap_or(-1), r.get(0)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.sort_unstable();
        let order = Arc::new(rows.into_iter().map(|(_, id)| id).collect::<Vec<u32>>());
        if !may_cache(conn) {
            return Ok(order);
        }
        let mut orders = self.cache.import_orders.lock();
        if orders
            .get(&service)
            .is_none_or(|(generation, _)| *generation < self.generation)
        {
            orders.insert(service, (self.generation, Arc::clone(&order)));
        }
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::Db;

    #[test]
    fn a_read_sees_the_domain_as_its_snapshot_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hydrus.db"), 2).unwrap();
        let cache = DomainCache::default();
        let service = ServiceId(5);
        let add = |hash: u32| {
            db.write(move |ctx| {
                ctx.conn().execute(
                    "INSERT INTO file_domain_current (service_id, hash_id) VALUES (5, ?)",
                    [hash],
                )?;
                changed(ctx.conn())
            })
            .unwrap();
        };
        let files = || {
            db.read(|c| {
                let domains = cache.for_read(c)?;
                Ok(domains.files(c, service, false)?.len())
            })
            .unwrap()
        };
        add(1);
        assert_eq!(files(), 1);
        assert_eq!(files(), 1, "cached");
        add(2);
        assert_eq!(files(), 2, "a write moves the generation on");
        // a write that forgets to say so is not noticed: the generation is
        // the only signal
        db.write(|ctx| {
            ctx.conn().execute(
                "INSERT INTO file_domain_current (service_id, hash_id) VALUES (5, 3)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(files(), 2);
    }

    #[test]
    fn import_order_is_by_time_then_id_and_follows_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hydrus.db"), 2).unwrap();
        let cache = DomainCache::default();
        let write = |sql: &'static str| {
            db.write(move |ctx| {
                ctx.conn().execute_batch(sql)?;
                changed(ctx.conn())
            })
            .unwrap();
        };
        let order = || {
            db.read(|c| Ok(cache.for_read(c)?.import_order(c, ServiceId(5))?.to_vec()))
                .unwrap()
        };
        write(
            "INSERT INTO file_domain_current (service_id, hash_id, added_ms) VALUES
             (5, 1, 20), (5, 2, 10), (5, 3, 20), (5, 4, NULL), (6, 5, 0)",
        );
        assert_eq!(order(), [4, 2, 1, 3]);
        write("UPDATE file_domain_current SET added_ms = 30 WHERE hash_id = 2");
        assert_eq!(order(), [4, 1, 3, 2]);
    }
}
