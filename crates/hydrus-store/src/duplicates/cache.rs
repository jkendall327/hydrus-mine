//! Every potential pair with what reading one needs about its kings, kept
//! between reads until a write changes them.
//!
//! Each duplicate filter request looks at every potential pair (to count,
//! order or pick them), and reading them joins four tables per pair. Every
//! write that changes pairs, groups or a grouped file's properties
//! increments `duplicates_generation` in the same transaction, so a read
//! learns from its own snapshot whether the cached pairs are its pairs (as
//! with the file domains in `domains.rs`).

use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use hydrus_core::HashId;

use crate::error::Result;

/// A potential pair, by its groups and their kings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PairRow {
    pub groups: (i64, i64),
    pub smaller_king: HashId,
    pub larger_king: HashId,
    pub distance: u32,
    /// Whether the kings are pixel duplicates (same pixel hash and width).
    pub pixel_duplicate: bool,
    /// The kings' sizes, for ordering pairs (0 for a king with no file
    /// record).
    pub smaller_size: u64,
    pub larger_size: u64,
}

/// Record, in a write's transaction, that it changed potential pairs,
/// duplicate groups or the properties of a file in a group.
pub(crate) fn changed(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE duplicates_generation SET generation = generation + 1",
        [],
    )?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct PairCache {
    rows: Mutex<Option<(i64, Arc<Vec<PairRow>>)>>,
}

impl PairCache {
    /// Every potential pair, as the read in progress on `conn` sees them.
    pub(crate) fn rows(&self, conn: &Connection) -> Result<Arc<Vec<PairRow>>> {
        let generation: i64 =
            conn.query_row("SELECT generation FROM duplicates_generation", [], |r| {
                r.get(0)
            })?;
        if let Some((cached, rows)) = &*self.rows.lock()
            && *cached == generation
        {
            return Ok(Arc::clone(rows));
        }
        let rows = Arc::new(load(conn)?);
        if crate::domains::may_cache(conn) {
            let mut entry = self.rows.lock();
            if entry
                .as_ref()
                .is_none_or(|(cached, _)| *cached < generation)
            {
                *entry = Some((generation, Arc::clone(&rows)));
            }
        }
        Ok(rows)
    }
}

fn load(conn: &Connection) -> Result<Vec<PairRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT gs.king_hash_id, gl.king_hash_id, p.distance,
                fs.pixel_hash IS NOT NULL AND fs.pixel_hash = fl.pixel_hash AND fs.width = fl.width,
                p.smaller_group_id, p.larger_group_id,
                fs.size, fl.size
         FROM potential_pairs p
         JOIN dup_groups gs ON gs.group_id = p.smaller_group_id
         JOIN dup_groups gl ON gl.group_id = p.larger_group_id
         LEFT JOIN files fs ON fs.hash_id = gs.king_hash_id
         LEFT JOIN files fl ON fl.hash_id = gl.king_hash_id",
    )?;
    let size = |r: &rusqlite::Row<'_>, at: usize| -> rusqlite::Result<u64> {
        Ok(u64::try_from(r.get::<_, Option<i64>>(at)?.unwrap_or(0)).unwrap_or(0))
    };
    let rows = stmt
        .query_map([], |r| {
            Ok(PairRow {
                groups: (r.get(4)?, r.get(5)?),
                smaller_king: r.get(0)?,
                larger_king: r.get(1)?,
                distance: r.get(2)?,
                pixel_duplicate: r.get::<_, Option<bool>>(3)?.unwrap_or(false),
                smaller_size: size(r, 6)?,
                larger_size: size(r, 7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::Db;

    #[test]
    fn cached_pairs_follow_writes_and_only_readers_fill_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hydrus.db"), 2).unwrap();
        let cache = Arc::new(PairCache::default());
        db.write(|ctx| {
            ctx.conn().execute_batch(
                "INSERT INTO dup_groups (group_id, king_hash_id) VALUES (1, 10), (2, 20), (3, 30);
                 INSERT INTO potential_pairs (smaller_group_id, larger_group_id, distance) VALUES (1, 2, 0);",
            )?;
            changed(ctx.conn())
        })
        .unwrap();
        let read = || {
            db.read(|c| {
                Ok(cache
                    .rows(c)?
                    .iter()
                    .map(|r| (r.groups, r.distance))
                    .collect::<Vec<_>>())
            })
            .unwrap()
        };
        // a write's own read is not kept: it could be rolled back
        let in_write = Arc::clone(&cache);
        db.write(move |ctx| in_write.rows(ctx.conn()).map(|_| ()))
            .unwrap();
        assert!(cache.rows.lock().is_none());

        assert_eq!(read(), [((1, 2), 0)]);
        assert!(cache.rows.lock().is_some());
        db.write(|ctx| {
            ctx.conn().execute(
                "INSERT INTO potential_pairs (smaller_group_id, larger_group_id, distance) VALUES (2, 3, 4)",
                [],
            )?;
            changed(ctx.conn())
        })
        .unwrap();
        assert_eq!(read(), [((1, 2), 0), ((2, 3), 4)]);
    }
}
