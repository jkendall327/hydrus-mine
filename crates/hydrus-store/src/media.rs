//! Media results: everything known about a batch of files, loaded at once.
//!
//! This is the read model behind file metadata in the API and thumbnails in
//! a GUI. Loading is batched (one query per table for the whole batch) so a
//! page of thousands of files costs a handful of queries, not thousands.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use hydrus_core::{CanvasType, ContentStatus, HashId, Mime, ServiceId, Sha256, TagId, TimestampMs};

use crate::display::DisplayGraphs;
use crate::error::Result;
use crate::master::{self, id_array};
use crate::schema::MappingTables;
use crate::services::ServiceRegistry;

/// Bits of `files.flags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileFlags(pub u32);

impl FileFlags {
    pub const EXIF: u32 = 1;
    pub const ICC_PROFILE: u32 = 2;
    pub const HUMAN_READABLE_METADATA: u32 = 4;
    pub const TRANSPARENCY: u32 = 8;
    pub const XMP: u32 = 16;
    pub const IPTC: u32 = 32;
    pub const SOFTWARE_SOURCE: u32 = 64;

    pub fn has(self, flag: u32) -> bool {
        self.0 & flag != 0
    }
}

/// Intrinsic properties of a file's content.
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub size: u64,
    /// The effective type (the user's forced type if any).
    pub mime: Mime,
    /// The detected type, if the user forced a different one.
    pub original_mime: Option<Mime>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub num_frames: Option<u64>,
    pub has_audio: bool,
    pub num_words: Option<u64>,
    pub file_modified: Option<TimestampMs>,
    pub pixel_hash: Option<Sha256>,
    pub blurhash: Option<String>,
    pub flags: FileFlags,
}

/// A file's membership of one file domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentLocation {
    pub service: ServiceId,
    pub added: Option<TimestampMs>,
}

/// A record of a file leaving a domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedLocation {
    pub service: ServiceId,
    pub deleted: Option<TimestampMs>,
    pub originally_added: Option<TimestampMs>,
}

/// Viewing statistics for one viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewingStats {
    pub canvas: CanvasType,
    pub views: u64,
    pub viewtime_ms: u64,
    pub last_viewed: Option<TimestampMs>,
}

/// Stored tags of a file in one tag service, by status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceTags {
    pub by_status: BTreeMap<ContentStatus, Vec<TagId>>,
}

/// A stored rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rating {
    /// Like/dislike (0.0 or 1.0) or numerical (a fraction of max stars).
    Fraction(f64),
    IncDec(i64),
}

/// Everything known about one file.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaResult {
    pub hash_id: HashId,
    pub hash: Sha256,
    pub info: Option<FileInfo>,
    pub current: Vec<CurrentLocation>,
    pub deleted: Vec<DeletedLocation>,
    pub pending: Vec<ServiceId>,
    pub petitioned: Vec<ServiceId>,
    pub inbox: bool,
    pub archived: Option<TimestampMs>,
    pub deletion_reason: Option<String>,
    pub domain_modified: Vec<(String, TimestampMs)>,
    pub viewing: Vec<ViewingStats>,
    pub urls: Vec<String>,
    /// (name, text), sorted by name
    pub notes: Vec<(String, String)>,
    pub ratings: HashMap<ServiceId, Rating>,
    pub tags: HashMap<ServiceId, ServiceTags>,
}

impl MediaResult {
    fn new(hash_id: HashId, hash: Sha256) -> Self {
        Self {
            hash_id,
            hash,
            info: None,
            current: Vec::new(),
            deleted: Vec::new(),
            pending: Vec::new(),
            petitioned: Vec::new(),
            inbox: false,
            archived: None,
            deletion_reason: None,
            domain_modified: Vec::new(),
            viewing: Vec::new(),
            urls: Vec::new(),
            notes: Vec::new(),
            ratings: HashMap::new(),
            tags: HashMap::new(),
        }
    }

    pub fn is_current_in(&self, service: ServiceId) -> bool {
        self.current.iter().any(|c| c.service == service)
    }

    pub fn is_deleted_from(&self, service: ServiceId) -> bool {
        self.deleted.iter().any(|d| d.service == service)
    }

    pub fn added_to(&self, service: ServiceId) -> Option<TimestampMs> {
        self.current
            .iter()
            .find(|c| c.service == service)
            .and_then(|c| c.added)
    }

    /// The earliest of the file's own modified time and every web domain's.
    pub fn aggregate_modified(&self) -> Option<TimestampMs> {
        self.domain_modified
            .iter()
            .map(|(_, t)| *t)
            .chain(self.info.as_ref().and_then(|i| i.file_modified))
            .min()
    }
}

/// A batch of media results plus the tag strings they reference.
#[derive(Debug, Default)]
pub struct MediaBatch {
    pub results: Vec<MediaResult>,
    pub tags: HashMap<TagId, hydrus_core::Tag>,
}

fn opt_u32(v: Option<i64>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

fn opt_u64(v: Option<i64>) -> Option<u64> {
    v.and_then(|v| u64::try_from(v).ok())
}

/// Load media results for `hash_ids` (in that order; ids without a hash are
/// skipped).
/// Which of `hashes` are currently in file domain `domain`.
pub fn current_in(
    conn: &Connection,
    domain: ServiceId,
    hashes: &[HashId],
) -> Result<std::collections::HashSet<HashId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 AND hash_id IN rarray(?2)",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![domain, crate::master::id_array(hashes)],
        |r| r.get(0),
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Load only what `only_return_basic_information` needs: hashes and file
/// properties. Much cheaper than [`load`] for files with many tags.
pub fn load_basic(conn: &Connection, hash_ids: &[HashId]) -> Result<Vec<MediaResult>> {
    let hashes = master::hashes(conn, hash_ids)?;
    let mut results: Vec<MediaResult> = hash_ids
        .iter()
        .filter_map(|id| hashes.get(id).map(|h| MediaResult::new(*id, *h)))
        .collect();
    load_info(conn, &mut results)?;
    Ok(results)
}

/// Fill in each result's file properties.
fn load_info(conn: &Connection, results: &mut [MediaResult]) -> Result<()> {
    let index: HashMap<HashId, usize> = results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.hash_id, i))
        .collect();
    let ids: Vec<HashId> = results.iter().map(|r| r.hash_id).collect();
    let ids = id_array(&ids);
    let mut with = |id: HashId, f: &mut dyn FnMut(&mut MediaResult)| {
        if let Some(&i) = index.get(&id) {
            f(&mut results[i]);
        }
    };
    {
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, size, mime, width, height, duration_ms, num_frames, has_audio, num_words, forced_mime,
                    file_modified_ms, pixel_hash, blurhash, flags
             FROM files WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let detected = Mime::from_code(r.get::<_, u8>(2)?).unwrap_or(Mime::ApplicationUnknown);
            let forced = r.get::<_, Option<u8>>(9)?.and_then(Mime::from_code);
            let info = FileInfo {
                size: r.get::<_, i64>(1)? as u64,
                mime: forced.unwrap_or(detected),
                original_mime: forced.map(|_| detected),
                width: opt_u32(r.get(3)?),
                height: opt_u32(r.get(4)?),
                duration_ms: opt_u64(r.get(5)?),
                num_frames: opt_u64(r.get(6)?),
                has_audio: r.get(7)?,
                num_words: opt_u64(r.get(8)?),
                file_modified: r.get(10)?,
                pixel_hash: r.get(11)?,
                blurhash: r.get(12)?,
                flags: FileFlags(r.get(13)?),
            };
            with(r.get(0)?, &mut |m| m.info = Some(info.clone()));
        }
    }
    Ok(())
}

pub fn load(
    conn: &Connection,
    registry: &ServiceRegistry,
    display: Option<&DisplayGraphs>,
    hash_ids: &[HashId],
) -> Result<MediaBatch> {
    let hashes = master::hashes(conn, hash_ids)?;
    let mut results: Vec<MediaResult> = hash_ids
        .iter()
        .filter_map(|id| hashes.get(id).map(|h| MediaResult::new(*id, *h)))
        .collect();
    load_info(conn, &mut results)?;
    let index: HashMap<HashId, usize> = results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.hash_id, i))
        .collect();
    let ids = id_array(hash_ids);
    let mut with = |id: HashId, f: &mut dyn FnMut(&mut MediaResult)| {
        if let Some(&i) = index.get(&id) {
            f(&mut results[i]);
        }
    };

    {
        let mut stmt =
            conn.prepare_cached("SELECT hash_id, service_id, added_ms FROM file_domain_current WHERE hash_id IN rarray(?)")?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let loc = CurrentLocation {
                service: r.get(1)?,
                added: r.get(2)?,
            };
            with(r.get(0)?, &mut |m| m.current.push(loc));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, service_id, deleted_ms, original_added_ms FROM file_domain_deleted WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let loc = DeletedLocation {
                service: r.get(1)?,
                deleted: r.get(2)?,
                originally_added: r.get(3)?,
            };
            with(r.get(0)?, &mut |m| m.deleted.push(loc));
        }
    }
    for (table, pending) in [
        ("file_domain_pending", true),
        ("file_domain_petitioned", false),
    ] {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT hash_id, service_id FROM {table} WHERE hash_id IN rarray(?)"
        ))?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let service: ServiceId = r.get(1)?;
            with(r.get(0)?, &mut |m| {
                if pending {
                    m.pending.push(service);
                } else {
                    m.petitioned.push(service);
                }
            });
        }
    }
    {
        let mut stmt =
            conn.prepare_cached("SELECT hash_id FROM file_inbox WHERE hash_id IN rarray(?)")?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            with(r.get(0)?, &mut |m| m.inbox = true);
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, archived_ms FROM file_archived WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let t: Option<TimestampMs> = r.get(1)?;
            with(r.get(0)?, &mut |m| m.archived = t);
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT r.hash_id, t.text FROM file_deletion_reasons r JOIN texts t ON t.text_id = r.reason_id
             WHERE r.hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let reason: String = r.get(1)?;
            with(r.get(0)?, &mut |m| m.deletion_reason = Some(reason.clone()));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT m.hash_id, d.domain, m.modified_ms FROM file_domain_modified m JOIN url_domains d USING (domain_id)
             WHERE m.hash_id IN rarray(?) ORDER BY d.domain",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let entry: (String, TimestampMs) = (r.get(1)?, r.get(2)?);
            with(r.get(0)?, &mut |m| m.domain_modified.push(entry.clone()));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, canvas_type, views, viewtime_ms, last_viewed_ms FROM file_viewing_stats WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let Some(canvas) = CanvasType::from_code(r.get(1)?) else {
                continue;
            };
            let stats = ViewingStats {
                canvas,
                views: r.get::<_, i64>(2)? as u64,
                viewtime_ms: r.get::<_, i64>(3)? as u64,
                last_viewed: r.get(4)?,
            };
            with(r.get(0)?, &mut |m| m.viewing.push(stats));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT f.hash_id, u.url FROM file_urls f JOIN urls u USING (url_id) WHERE f.hash_id IN rarray(?) ORDER BY u.url",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let url: String = r.get(1)?;
            with(r.get(0)?, &mut |m| m.urls.push(url.clone()));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT f.hash_id, l.label, n.note FROM file_notes f JOIN labels l USING (label_id) JOIN notes n USING (note_id)
             WHERE f.hash_id IN rarray(?) ORDER BY l.label",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let note: (String, String) = (r.get(1)?, r.get(2)?);
            with(r.get(0)?, &mut |m| m.notes.push(note.clone()));
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, service_id, rating FROM ratings WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let (service, value): (ServiceId, f64) = (r.get(1)?, r.get(2)?);
            with(r.get(0)?, &mut |m| {
                m.ratings.insert(service, Rating::Fraction(value));
            });
        }
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id, service_id, rating FROM ratings_incdec WHERE hash_id IN rarray(?)",
        )?;
        let mut rows = stmt.query([ids.clone()])?;
        while let Some(r) = rows.next()? {
            let (service, value): (ServiceId, i64) = (r.get(1)?, r.get(2)?);
            with(r.get(0)?, &mut |m| {
                m.ratings.insert(service, Rating::IncDec(value));
            });
        }
    }

    let mut all_tag_ids = Vec::new();
    for service in registry.tag_services() {
        let graph = display.map(|d| d.get(service.id));
        let tables = MappingTables::new(service.id);
        for status in ContentStatus::ALL {
            let table = tables.for_status(*status);
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT hash_id, tag_id FROM {table} WHERE hash_id IN rarray(?)"
            ))?;
            let mut rows = stmt.query([ids.clone()])?;
            while let Some(r) = rows.next()? {
                let tag: TagId = r.get(1)?;
                all_tag_ids.push(tag);
                if let Some(graph) = &graph {
                    all_tag_ids.extend(graph.display_tags(tag));
                }
                with(r.get(0)?, &mut |m| {
                    m.tags
                        .entry(service.id)
                        .or_default()
                        .by_status
                        .entry(*status)
                        .or_default()
                        .push(tag);
                });
            }
        }
    }
    all_tag_ids.sort_unstable();
    all_tag_ids.dedup();
    let tags = master::tags(conn, &all_tag_ids)?;
    Ok(MediaBatch { results, tags })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::tests::import_basic;
    use crate::store::Store;

    #[test]
    fn loads_the_fixture() {
        let (_source, dest_dir, _dest) = import_basic();
        let store = Store::open(dest_dir.path()).unwrap();
        let snapshot = store.snapshot();
        let all: Vec<HashId> = store
            .read(|c| {
                Ok(c.prepare("SELECT hash_id FROM files")?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .unwrap();
        let batch = store
            .read(|c| load(c, &snapshot.services, Some(&snapshot.display), &all))
            .unwrap();
        assert_eq!(batch.results.len(), 36);
        assert!(batch.results.iter().all(|m| m.info.is_some()));
        let with_notes = batch.results.iter().filter(|m| !m.notes.is_empty()).count();
        assert_eq!(with_notes, 2);
        let tagged = batch.results.iter().filter(|m| !m.tags.is_empty()).count();
        assert_eq!(tagged, 36);
        assert!(batch.tags.len() > 40);
    }
}
