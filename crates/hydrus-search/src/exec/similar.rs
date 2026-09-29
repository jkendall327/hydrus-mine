//! Similar-file search: exact pixel matches and perceptual hashes within a
//! Hamming distance.
//!
//! Perceptual hashes are 64 bits, so comparing a query against every stored
//! hash is a tight loop over a few megabytes even for a million files; exact
//! matches use the index.

use std::collections::BTreeSet;

use roaring::RoaringBitmap;
use rusqlite::{Connection, OptionalExtension};

use hydrus_core::{PerceptualHash, Sha256};

use super::Result;
use super::sql::{self, int_array};

/// Files similar to the given files (each file included): same pixels, or
/// a perceptual hash within `max_distance` of one of theirs.
pub(crate) fn similar_to_files(
    conn: &Connection,
    files: &BTreeSet<Sha256>,
    max_distance: u64,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    for hash in files {
        let Some(hash_id) = hydrus_store::master::hash_id(conn, hash)? else {
            // an unknown file has no similar files, and is in no domain
            continue;
        };
        out.insert(hash_id.get());
        let pixel_hash: Option<Vec<u8>> = conn
            .prepare_cached("SELECT pixel_hash FROM files WHERE hash_id = ?")?
            .query_row([hash_id], |r| r.get(0))
            .optional()?
            .flatten();
        if let Some(pixel_hash) = pixel_hash {
            out |= files_with_pixel_hash(conn, &pixel_hash)?;
        }
        let mut stmt = conn.prepare_cached(
            "SELECT p.phash FROM file_perceptual_hashes f CROSS JOIN perceptual_hashes p ON p.phash_id = f.phash_id
             WHERE f.hash_id = ?",
        )?;
        let phashes: Vec<PerceptualHash> = stmt
            .query_map([hash_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        out |= files_near(conn, &phashes, max_distance)?;
    }
    Ok(out)
}

/// Files with one of these pixel hashes, or a perceptual hash within
/// `max_distance` of one of these.
pub(crate) fn similar_to_data(
    conn: &Connection,
    pixel_hashes: &BTreeSet<Sha256>,
    perceptual_hashes: &BTreeSet<PerceptualHash>,
    max_distance: u64,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    for pixel_hash in pixel_hashes {
        out |= files_with_pixel_hash(conn, pixel_hash.as_bytes())?;
    }
    let phashes: Vec<PerceptualHash> = perceptual_hashes.iter().copied().collect();
    out |= files_near(conn, &phashes, max_distance)?;
    Ok(out)
}

fn files_with_pixel_hash(conn: &Connection, pixel_hash: &[u8]) -> Result<RoaringBitmap> {
    let mut stmt = conn.prepare_cached("SELECT hash_id FROM files WHERE pixel_hash = ?")?;
    sql::collect(&mut stmt, [pixel_hash])
}

fn files_near(
    conn: &Connection,
    queries: &[PerceptualHash],
    max_distance: u64,
) -> Result<RoaringBitmap> {
    if queries.is_empty() {
        return Ok(RoaringBitmap::new());
    }
    let mut phash_ids: Vec<u32> = Vec::new();
    if max_distance == 0 {
        let mut stmt =
            conn.prepare_cached("SELECT phash_id FROM perceptual_hashes WHERE phash = ?")?;
        for q in queries {
            if let Some(id) = stmt.query_row([q], |r| r.get(0)).optional()? {
                phash_ids.push(id);
            }
        }
    } else {
        let mut stmt = conn.prepare_cached("SELECT phash_id, phash FROM perceptual_hashes")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let phash: PerceptualHash = row.get(1)?;
            if queries
                .iter()
                .any(|q| u64::from(q.distance(&phash)) <= max_distance)
            {
                phash_ids.push(row.get(0)?);
            }
        }
    }
    let mut stmt = conn
        .prepare_cached("SELECT hash_id FROM file_perceptual_hashes WHERE phash_id IN rarray(?)")?;
    sql::collect(&mut stmt, [int_array(phash_ids)])
}
