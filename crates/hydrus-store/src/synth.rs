//! Synthetic libraries, for benchmarks and load tests.
//!
//! Everything goes through the normal write path ([`crate::content`]), so
//! generating a library also exercises (and measures) it. Deterministic for a
//! given [`SynthSpec`].

use hydrus_core::tag::Tag;
use hydrus_core::time::TimestampMs;
use hydrus_core::{HashId, Mime, ServiceId, Sha256, TagId};

use crate::content::MappingAction;
use crate::error::Result;
use crate::master;
use crate::media::{FileFlags, FileInfo};
use crate::store::Store;

/// The shape of a synthetic library.
#[derive(Debug, Clone, Copy)]
pub struct SynthSpec {
    pub files: u32,
    /// Distinct tags. Popularity is skewed: a few tags are on many files.
    pub tags: u32,
    pub tags_per_file: u32,
    /// Sibling pairs and parent pairs in "my tags".
    pub siblings: u32,
    pub parents: u32,
    /// Files per write transaction.
    pub batch: u32,
    pub seed: u64,
}

impl Default for SynthSpec {
    fn default() -> Self {
        Self {
            files: 10_000,
            tags: 5_000,
            tags_per_file: 20,
            siblings: 200,
            parents: 200,
            batch: 1_000,
            seed: 688,
        }
    }
}

/// What was generated.
#[derive(Debug, Clone)]
pub struct SynthLibrary {
    pub hashes: Vec<(HashId, Sha256)>,
    pub tags: Vec<TagId>,
    pub tag_service: ServiceId,
    pub file_domain: ServiceId,
}

/// A small, fast, deterministic PRNG (xorshift64*).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n.max(1))) as u32
    }

    /// Skewed towards 0: roughly Zipf-like popularity.
    pub fn skewed(&mut self, n: u32) -> u32 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        ((u * u * u) * f64::from(n)) as u32
    }
}

fn tag_name(i: u32) -> Tag {
    let namespaces = ["", "character", "series", "creator", "meta"];
    let ns = namespaces[(i % 5) as usize];
    Tag::from_parts(ns, &format!("tag number {i}"))
}

/// Fill `store` (a fresh one, with the default services) with a library.
pub fn populate(store: &Store, spec: SynthSpec) -> Result<SynthLibrary> {
    let snap = store.snapshot();
    let tag_service = snap
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)?
        .id;
    let file_domain = snap
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)?
        .id;

    // tags and relations first, so every count below goes through the display graph
    let mut rng = Rng::new(spec.seed);
    let mut pairs = |n: u32| -> Vec<(u32, u32)> {
        (0..n)
            .map(|_| (rng.below(spec.tags), rng.below(spec.tags)))
            .collect()
    };
    let siblings = pairs(spec.siblings);
    let parents = pairs(spec.parents);
    let tags = store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        let tags: Vec<TagId> = (0..spec.tags)
            .map(|i| master::intern_tag(conn, &tag_name(i)))
            .collect::<Result<_>>()?;
        for (table, a, b, pairs) in [
            ("tag_siblings", "bad_tag_id", "good_tag_id", &siblings),
            ("tag_parents", "child_tag_id", "parent_tag_id", &parents),
        ] {
            let mut stmt = conn.prepare(&format!(
                "INSERT OR IGNORE INTO {table} (service_id, status, {a}, {b}) VALUES (?1, 0, ?2, ?3)"
            ))?;
            for &(x, y) in pairs {
                if x != y {
                    stmt.execute(rusqlite::params![tag_service, tags[x as usize], tags[y as usize]])?;
                }
            }
        }
        Ok(tags)
    })?;

    let mut hashes = Vec::with_capacity(spec.files as usize);
    let mut next = 0u32;
    while next < spec.files {
        let end = (next + spec.batch).min(spec.files);
        let mut batch_rng =
            Rng::new(spec.seed ^ u64::from(next).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let tags = tags.clone();
        let spec_copy = spec;
        let created = store.write_content(move |w| {
            let mut created = Vec::new();
            // tag -> files, so each tag is one mapping write
            let mut by_tag: std::collections::HashMap<TagId, Vec<HashId>> =
                std::collections::HashMap::new();
            let mut rows = Vec::new();
            for i in next..end {
                let mut bytes = [0u8; 32];
                bytes[..4].copy_from_slice(&i.to_be_bytes());
                bytes[4..12].copy_from_slice(&spec_copy.seed.to_be_bytes());
                let hash = Sha256(bytes);
                let id = master::intern_hash(w.conn(), &hash)?;
                let info = FileInfo {
                    size: 50_000 + u64::from(batch_rng.below(5_000_000)),
                    mime: if i % 10 == 0 {
                        Mime::ImagePng
                    } else {
                        Mime::ImageJpeg
                    },
                    original_mime: None,
                    width: Some(200 + batch_rng.below(3000)),
                    height: Some(200 + batch_rng.below(3000)),
                    duration_ms: None,
                    num_frames: None,
                    has_audio: false,
                    num_words: None,
                    file_modified: Some(TimestampMs::from_millis(
                        1_500_000_000_000 + i64::from(i) * 1000,
                    )),
                    pixel_hash: None,
                    blurhash: None,
                    flags: FileFlags(0),
                };
                w.add_file_info(id, &info, false)?;
                rows.push((id, Some(w.now_ms() - i64::from(spec_copy.files - i) * 1000)));
                for _ in 0..spec_copy.tags_per_file {
                    by_tag
                        .entry(tags[batch_rng.skewed(spec_copy.tags) as usize])
                        .or_default()
                        .push(id);
                }
                created.push((id, hash));
            }
            w.add_files(file_domain, &rows)?;
            for (tag, files) in by_tag {
                w.update_mappings(tag_service, &MappingAction::Add, tag, &files)?;
            }
            Ok(created)
        })?;
        hashes.extend(created);
        next = end;
    }
    Ok(SynthLibrary {
        hashes,
        tags,
        tag_service,
        file_domain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_library_has_consistent_counts() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let spec = SynthSpec {
            files: 300,
            tags: 100,
            tags_per_file: 8,
            siblings: 20,
            parents: 20,
            batch: 64,
            ..SynthSpec::default()
        };
        let lib = populate(&store, spec).unwrap();
        assert_eq!(lib.hashes.len(), 300);
        let dump = |c: &rusqlite::Connection| -> Result<Vec<(i64, i64, i64, i64)>> {
            let t = crate::schema::MappingTables::new(lib.tag_service);
            let mut out = Vec::new();
            for table in [&t.counts, &t.display_counts] {
                let mut stmt = c.prepare(&format!(
                    "SELECT domain_id, tag_id, current, pending FROM {table} ORDER BY 1, 2"
                ))?;
                let rows =
                    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
                out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            }
            Ok(out)
        };
        let incremental = store.read(dump).unwrap();
        store
            .write(|ctx| crate::counts::rebuild_all(ctx.conn()))
            .unwrap();
        assert_eq!(store.read(dump).unwrap(), incremental);
    }
}
