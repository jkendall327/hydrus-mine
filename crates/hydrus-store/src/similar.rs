//! The similar-files search: finding potential duplicate pairs from files'
//! perceptual hashes (`ClientDBSimilarFiles`, and the reference's
//! `_PerceptualHashesSearchForPotentialDuplicates`).
//!
//! Each file with a useful (non-blank) perceptual hash is searched once at
//! the client's search distance: every file sharing its pixel hash, or with
//! a perceptual hash within that Hamming distance of one of its own, becomes
//! a potential duplicate of it, with the smallest distance found. The
//! reference walks a VP-tree kept in the database; the set of hashes within
//! a distance doesn't depend on the index, so we keep an in-memory
//! multi-index instead: for distance `r` the 64 bits are split into `r + 1`
//! chunks, and any hash within `r` must match at least one chunk exactly
//! (the pigeonhole principle), so only a few candidates need checking.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{Connection, params};

use hydrus_core::{HashId, ServiceId};

use crate::duplicates::RelationshipWriter;
use crate::error::Result;

/// `CC.BLANK_PERCEPTUAL_HASH`: the hash of a flat colour.
pub const BLANK_PERCEPTUAL_HASH: u64 = 0x8000_0000_0000_0000;

/// Above this distance, searching the chunks costs more than checking every
/// hash.
const MAX_CHUNKED_DISTANCE: u32 = 7;

/// `DiscardBlankPerceptualHashes`: a hash within 4 of a flat colour's says
/// nothing about the image.
pub fn is_blank(phash: u64) -> bool {
    (phash ^ BLANK_PERCEPTUAL_HASH).count_ones() <= 4
}

/// A stored perceptual hash as a number.
pub fn phash_value(bytes: &[u8]) -> Option<u64> {
    <[u8; 8]>::try_from(bytes).ok().map(u64::from_be_bytes)
}

/// One chunk of the index: `(bit offset, bit width)`, and chunk value to
/// the positions of the hashes with it.
type Chunk = ((u32, u32), HashMap<u64, Vec<u32>>);

/// Every perceptual hash, indexed for finding those near a given one.
#[derive(Debug)]
pub struct PhashIndex {
    radius: u32,
    phashes: Vec<u64>,
    ids: Vec<i64>,
    /// Per chunk: `(bit offset, bit width)`, and chunk value to the positions
    /// of the hashes with it. Empty above [`MAX_CHUNKED_DISTANCE`].
    chunks: Vec<Chunk>,
}

impl PhashIndex {
    /// Index `(phash id, hash)` pairs for searches within `radius`.
    pub fn new(entries: impl IntoIterator<Item = (i64, u64)>, radius: u32) -> Self {
        let (ids, phashes): (Vec<i64>, Vec<u64>) = entries.into_iter().unzip();
        let mut chunks = Vec::new();
        if radius <= MAX_CHUNKED_DISTANCE {
            let count = radius + 1;
            let mut offset = 0;
            for i in 0..count {
                // spread the 64 bits as evenly as possible
                let width = 64 / count + u32::from(i < 64 % count);
                let mut map: HashMap<u64, Vec<u32>> = HashMap::new();
                for (position, &phash) in phashes.iter().enumerate() {
                    map.entry(chunk(phash, offset, width))
                        .or_default()
                        .push(position as u32);
                }
                chunks.push(((offset, width), map));
                offset += width;
            }
        }
        Self {
            radius,
            phashes,
            ids,
            chunks,
        }
    }

    /// Every perceptual hash in the database.
    pub fn load(conn: &Connection, radius: u32) -> Result<Self> {
        let mut stmt = conn.prepare("SELECT phash_id, phash FROM perceptual_hashes")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        let mut entries = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            if let Some(phash) = phash_value(&bytes) {
                entries.push((id, phash));
            }
        }
        Ok(Self::new(entries, radius))
    }

    /// `(phash id, distance)` of every hash within the index's radius of
    /// `query`.
    pub fn within(&self, query: u64) -> Vec<(i64, u32)> {
        let close = |position: usize| {
            let distance = (self.phashes[position] ^ query).count_ones();
            (distance <= self.radius).then(|| (self.ids[position], distance))
        };
        if self.chunks.is_empty() {
            return (0..self.phashes.len()).filter_map(close).collect();
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for ((offset, width), map) in &self.chunks {
            let Some(positions) = map.get(&chunk(query, *offset, *width)) else {
                continue;
            };
            for &position in positions {
                if seen.insert(position)
                    && let Some(found) = close(position as usize)
                {
                    out.push(found);
                }
            }
        }
        out
    }
}

fn chunk(phash: u64, offset: u32, width: u32) -> u64 {
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    (phash >> offset) & mask
}

/// `SearchFile`: files similar to `hash_id` within `distance`, with the
/// distance each was found at (itself first, at 0).
pub fn search_file(
    conn: &Connection,
    index: &PhashIndex,
    hash_id: HashId,
    distance: u32,
) -> Result<Vec<(HashId, u32)>> {
    let mut found: Vec<(HashId, u32)> = vec![(hash_id, 0)];
    // the same pixels
    let mut stmt = conn.prepare_cached(
        "SELECT other.hash_id FROM files AS this JOIN files AS other ON other.pixel_hash = this.pixel_hash
         WHERE this.hash_id = ? AND this.pixel_hash IS NOT NULL ORDER BY other.hash_id",
    )?;
    for id in stmt.query_map([hash_id], |r| r.get::<_, HashId>(0))? {
        found.push((id?, 0));
    }
    if distance == 0 {
        // the same perceptual hash
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT hash_id FROM file_perceptual_hashes
             WHERE phash_id IN (SELECT phash_id FROM file_perceptual_hashes WHERE hash_id = ?)
             ORDER BY hash_id",
        )?;
        for id in stmt.query_map([hash_id], |r| r.get::<_, HashId>(0))? {
            found.push((id?, 0));
        }
    } else {
        let mut phash_distances: BTreeMap<i64, u32> = BTreeMap::new();
        let mut stmt = conn.prepare_cached(
            "SELECT phash FROM perceptual_hashes NATURAL JOIN file_perceptual_hashes WHERE hash_id = ?",
        )?;
        let own: Vec<u64> = stmt
            .query_map([hash_id], |r| r.get::<_, Vec<u8>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .filter_map(|b| phash_value(b))
            .collect();
        for phash in own {
            for (id, d) in index.within(phash) {
                let entry = phash_distances.entry(id).or_insert(d);
                *entry = (*entry).min(d);
            }
        }
        let mut file_distances: BTreeMap<HashId, u32> = BTreeMap::new();
        let mut stmt =
            conn.prepare_cached("SELECT hash_id FROM file_perceptual_hashes WHERE phash_id = ?")?;
        for (phash_id, d) in phash_distances {
            for id in stmt.query_map([phash_id], |r| r.get::<_, HashId>(0))? {
                let entry = file_distances.entry(id?).or_insert(d);
                *entry = (*entry).min(d);
            }
        }
        let mut by_distance: Vec<(HashId, u32)> = file_distances.into_iter().collect();
        by_distance.sort_by_key(|&(id, d)| (d, id));
        found.extend(by_distance);
    }
    let mut seen = HashSet::new();
    found.retain(|pair| seen.insert(*pair));
    Ok(found)
}

/// Files waiting to be searched at `distance` (never searched, or searched
/// at a smaller distance).
pub fn files_to_search(conn: &Connection, distance: u32, limit: usize) -> Result<Vec<HashId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT hash_id FROM similar_search_status
         WHERE searched_distance IS NULL OR searched_distance < ? ORDER BY hash_id LIMIT ?",
    )?;
    let ids = stmt
        .query_map(params![distance, limit as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// How many files have been searched to each distance (`None`: not yet).
pub fn search_status_counts(conn: &Connection) -> Result<BTreeMap<Option<u32>, usize>> {
    let mut stmt = conn.prepare_cached(
        "SELECT searched_distance, COUNT(*) FROM similar_search_status GROUP BY searched_distance",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, Option<u32>>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().map(|(d, n)| (d, n as usize)).collect())
}

/// Search some files at `distance` (`_PerceptualHashesSearchForPotentialDuplicates`),
/// adding what they find as potential duplicates; how many were searched.
pub fn search_some_files(
    conn: &Connection,
    local_storage: ServiceId,
    index: &PhashIndex,
    distance: u32,
    limit: usize,
) -> Result<usize> {
    let files = files_to_search(conn, distance, limit)?;
    let writer = RelationshipWriter::new(conn, local_storage);
    let mut set_status = conn.prepare_cached(
        "UPDATE similar_search_status SET searched_distance = ? WHERE hash_id = ?",
    )?;
    for &hash_id in &files {
        let found: Vec<(HashId, u32)> = search_file(conn, index, hash_id, distance)?
            .into_iter()
            .filter(|&(id, _)| id != hash_id)
            .collect();
        writer.add_similar_files(hash_id, &found)?;
        set_status.execute(params![distance, hash_id])?;
    }
    Ok(files.len())
}

/// `EnsureFileIsCorrectlyInOrOutOfPairDiscoverySearch` after a file's
/// perceptual hashes were set: searched afresh if they changed, left alone
/// if not, and not searched at all if they are all blank.
pub fn set_perceptual_hashes(
    conn: &Connection,
    hash_id: HashId,
    phashes: &[Vec<u8>],
) -> Result<()> {
    let current: HashSet<Vec<u8>> = conn
        .prepare_cached(
            "SELECT phash FROM perceptual_hashes NATURAL JOIN file_perceptual_hashes WHERE hash_id = ?",
        )?
        .query_map([hash_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let wanted: HashSet<Vec<u8>> = phashes.iter().cloned().collect();
    let changed = current != wanted;
    if changed {
        conn.prepare_cached("DELETE FROM file_perceptual_hashes WHERE hash_id = ?")?
            .execute([hash_id])?;
        for phash in &wanted {
            conn.prepare_cached("INSERT OR IGNORE INTO perceptual_hashes (phash) VALUES (?)")?
                .execute([phash])?;
            conn.prepare_cached(
                "INSERT OR IGNORE INTO file_perceptual_hashes (hash_id, phash_id)
                 SELECT ?, phash_id FROM perceptual_hashes WHERE phash = ?",
            )?
            .execute(params![hash_id, phash])?;
        }
    }
    let useful = wanted
        .iter()
        .filter_map(|p| phash_value(p))
        .any(|p| !is_blank(p));
    let in_search = conn
        .prepare_cached("SELECT 1 FROM similar_search_status WHERE hash_id = ?")?
        .exists([hash_id])?;
    if useful && (!in_search || changed) {
        conn.prepare_cached(
            "INSERT OR REPLACE INTO similar_search_status (hash_id, searched_distance) VALUES (?, NULL)",
        )?
        .execute([hash_id])?;
    } else if !useful && in_search {
        conn.prepare_cached("DELETE FROM similar_search_status WHERE hash_id = ?")?
            .execute([hash_id])?;
    }
    Ok(())
}

/// When and how far the similar-files search runs (the client options'
/// `similar_files_duplicate_pairs_search_distance` and
/// `maintain_similar_files_duplicate_pairs_during_active` / `_idle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SimilarFilesSettings {
    pub search_distance: u32,
    pub during_active: bool,
    pub during_idle: bool,
}

impl Default for SimilarFilesSettings {
    fn default() -> Self {
        Self {
            search_distance: 0,
            during_active: true,
            during_idle: true,
        }
    }
}

impl crate::settings::Setting for SimilarFilesSettings {
    const KEY: &'static str = "similar_files";
}

/// Search up to `batch` waiting files at the client's search distance, if
/// the search is on; how many were searched.
pub fn run_search(store: &crate::Store, batch: usize) -> Result<usize> {
    let settings: SimilarFilesSettings = store.read(crate::settings::get)?;
    // (there is no "idle" without a GUI: either switch runs it)
    if !settings.during_active && !settings.during_idle {
        return Ok(0);
    }
    let distance = settings.search_distance;
    if store
        .read(|conn| files_to_search(conn, distance, 1))?
        .is_empty()
    {
        return Ok(0);
    }
    let index = store.read(|conn| PhashIndex::load(conn, distance))?;
    store.write_content(move |w| {
        let local_storage = w.roles().local_file_storage;
        search_some_files(w.conn(), local_storage, &index, distance, batch)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_finds_exactly_what_checking_everything_finds() {
        // (a small deterministic generator, so no dev-dependency is needed)
        let mut state = 0x1234_5678_9abc_def0_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut hashes: Vec<u64> = (0..3000).map(|_| next()).collect();
        // near neighbours of some
        for i in 0..300 {
            let base = hashes[i];
            let flips = (next() % 9) as u32;
            let mut h = base;
            for _ in 0..flips {
                h ^= 1 << (next() % 64);
            }
            hashes.push(h);
        }
        let entries: Vec<(i64, u64)> = hashes
            .iter()
            .enumerate()
            .map(|(i, &h)| (i as i64, h))
            .collect();
        for radius in [0, 1, 2, 4, 7, 8, 12] {
            let index = PhashIndex::new(entries.clone(), radius);
            for &query in hashes.iter().step_by(37) {
                let mut got = index.within(query);
                got.sort_unstable();
                let mut want: Vec<(i64, u32)> = entries
                    .iter()
                    .filter_map(|&(id, h)| {
                        let d = (h ^ query).count_ones();
                        (d <= radius).then_some((id, d))
                    })
                    .collect();
                want.sort_unstable();
                assert_eq!(got, want, "radius {radius}");
            }
        }
    }

    /// The reference's own search, at distance 8 on the fixture's files
    /// (`oracle/make_fixture_db.py`), against ours on the same files.
    #[test]
    fn the_search_finds_the_pairs_the_reference_found() {
        use std::collections::BTreeSet;

        let (source, dest_dir, _) = crate::import::tests::import_basic();
        let reference: BTreeSet<(String, String, u32)> = {
            let conn = Connection::open(source.path().join("client.db")).unwrap();
            conn.execute(
                &format!(
                    "ATTACH '{}' AS m",
                    source.path().join("client.master.db").display()
                ),
                [],
            )
            .unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT hex(a.hash), hex(b.hash), p.distance FROM potential_duplicate_pairs AS p
                     JOIN duplicate_files AS da ON da.media_id = p.smaller_media_id
                     JOIN duplicate_files AS db ON db.media_id = p.larger_media_id
                     JOIN m.hashes AS a ON a.hash_id = da.king_hash_id
                     JOIN m.hashes AS b ON b.hash_id = db.king_hash_id",
                )
                .unwrap();
            stmt.query_map([], |r| {
                let (a, b): (String, String) = (r.get(0)?, r.get(1)?);
                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                Ok((a, b, r.get(2)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
        };
        assert!(!reference.is_empty());

        let store = crate::Store::open(dest_dir.path()).unwrap();
        let ours = |store: &crate::Store| -> BTreeSet<(String, String, u32)> {
            store
                .read(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT hex(a.sha256), hex(b.sha256), p.distance FROM potential_pairs AS p
                         JOIN dup_groups AS ga ON ga.group_id = p.smaller_group_id
                         JOIN dup_groups AS gb ON gb.group_id = p.larger_group_id
                         JOIN hashes AS a ON a.hash_id = ga.king_hash_id
                         JOIN hashes AS b ON b.hash_id = gb.king_hash_id",
                    )?;
                    let rows = stmt
                        .query_map([], |r| {
                            let (a, b): (String, String) = (r.get(0)?, r.get(1)?);
                            let (a, b) = if a <= b { (a, b) } else { (b, a) };
                            Ok((a, b, r.get(2)?))
                        })?
                        .collect::<rusqlite::Result<_>>()?;
                    Ok(rows)
                })
                .unwrap()
        };
        assert_eq!(ours(&store), reference, "as migrated");

        // forget them and search again, as the reference did
        store
            .write(|ctx| {
                ctx.conn().execute_batch(
                    "DELETE FROM potential_pairs; UPDATE similar_search_status SET searched_distance = NULL;",
                )?;
                crate::duplicates::cache::changed(ctx.conn())?;
                crate::settings::set(
                    ctx.conn(),
                    &SimilarFilesSettings {
                        search_distance: 8,
                        ..SimilarFilesSettings::default()
                    },
                )
            })
            .unwrap();
        while run_search(&store, 7).unwrap() > 0 {}
        assert_eq!(ours(&store), reference, "searched again");
        let counts = store.read(search_status_counts).unwrap();
        assert!(counts.keys().all(|d| *d == Some(8)), "{counts:?}");
    }

    /// Timing at the scale of a large install: 414k hashes, 50k searches.
    /// `cargo test --release -p hydrus-store similar::tests::scale -- --ignored --nocapture`
    #[test]
    #[ignore = "a timing, not a check"]
    fn scale() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let hashes: Vec<(i64, u64)> = (0..414_569).map(|i| (i, next())).collect();
        for radius in [2, 4, 8] {
            let started = std::time::Instant::now();
            let index = PhashIndex::new(hashes.clone(), radius);
            let built = started.elapsed();
            let started = std::time::Instant::now();
            let mut found = 0;
            for &(_, h) in hashes.iter().take(50_000) {
                found += index.within(h).len();
            }
            println!(
                "distance {radius}: index built in {built:.2?}, 50,000 searches in {:.2?} ({found} found)",
                started.elapsed()
            );
        }
    }

    #[test]
    fn flat_colours_are_blank() {
        assert!(is_blank(BLANK_PERCEPTUAL_HASH));
        assert!(is_blank(BLANK_PERCEPTUAL_HASH | 0b1111));
        assert!(!is_blank(BLANK_PERCEPTUAL_HASH | 0b11111));
    }
}
