//! Metadata attached to files: notes, ratings, viewing statistics and
//! editable timestamps.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use hydrus_core::content::CanvasType;
use hydrus_core::{HashId, ServiceId};

use super::ContentWriter;
use crate::error::{Result, StoreError};
use crate::master::{intern_label, intern_note, intern_url_domain, label_id, url_domain_id};
use crate::services::ServiceKind;

/// A timestamp of a file that can be read and edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTime {
    /// When a web domain says the file was modified.
    DomainModified(String),
    /// The file's own modified time.
    FileModified,
    /// When the file was added to a file domain.
    Imported(ServiceId),
    /// When the file was deleted from a file domain.
    Deleted(ServiceId),
    /// When a file deleted from a domain had been added to it.
    PreviouslyImported(ServiceId),
    Archived,
    LastViewed(CanvasType),
}

impl ContentWriter<'_> {
    /// A file's notes, by name.
    pub fn notes(&self, hash: HashId) -> Result<BTreeMap<String, String>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT label, note FROM file_notes JOIN labels USING (label_id) JOIN notes USING (note_id)
             WHERE hash_id = ?1",
        )?;
        let rows = stmt.query_map([hash], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Set a note; an empty note deletes it.
    pub fn set_note(&mut self, hash: HashId, name: &str, note: &str) -> Result<()> {
        let label = intern_label(self.conn, name)?;
        self.conn
            .prepare_cached("DELETE FROM file_notes WHERE hash_id = ?1 AND label_id = ?2")?
            .execute(params![hash, label])?;
        if !note.is_empty() {
            let note_id = intern_note(self.conn, note)?;
            self.conn
                .prepare_cached(
                    "INSERT INTO file_notes (hash_id, label_id, note_id) VALUES (?1, ?2, ?3)",
                )?
                .execute(params![hash, label, note_id])?;
        }
        Ok(())
    }

    pub fn delete_note(&mut self, hash: HashId, name: &str) -> Result<()> {
        if let Some(label) = label_id(self.conn, name)? {
            self.conn
                .prepare_cached("DELETE FROM file_notes WHERE hash_id = ?1 AND label_id = ?2")?
                .execute(params![hash, label])?;
        }
        Ok(())
    }

    /// Set (or with `None` clear) a like/dislike or numerical rating, stored
    /// as a fraction in `0.0..=1.0`.
    pub fn set_rating(
        &mut self,
        service: ServiceId,
        hashes: &[HashId],
        rating: Option<f64>,
    ) -> Result<()> {
        match self.snap.services.get(service)?.kind {
            ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) => {}
            _ => {
                return Err(StoreError::Invalid(format!(
                    "service {service} is not a like or numerical rating service"
                )));
            }
        }
        if let Some(r) = rating
            && !(0.0..=1.0).contains(&r)
        {
            return Err(StoreError::Invalid(format!(
                "rating {r} is not between 0 and 1"
            )));
        }
        let mut delete = self
            .conn
            .prepare_cached("DELETE FROM ratings WHERE service_id = ?1 AND hash_id = ?2")?;
        let mut insert = self.conn.prepare_cached(
            "INSERT INTO ratings (service_id, hash_id, rating) VALUES (?1, ?2, ?3)",
        )?;
        for &hash in hashes {
            delete.execute(params![service, hash])?;
            if let Some(r) = rating {
                insert.execute(params![service, hash, r])?;
            }
        }
        Ok(())
    }

    /// Set an inc/dec rating; zero clears it.
    pub fn set_incdec(&mut self, service: ServiceId, hashes: &[HashId], rating: i64) -> Result<()> {
        if !matches!(
            self.snap.services.get(service)?.kind,
            ServiceKind::RatingIncDec(_)
        ) {
            return Err(StoreError::Invalid(format!(
                "service {service} is not an inc/dec rating service"
            )));
        }
        let mut delete = self
            .conn
            .prepare_cached("DELETE FROM ratings_incdec WHERE service_id = ?1 AND hash_id = ?2")?;
        let mut insert = self.conn.prepare_cached(
            "INSERT INTO ratings_incdec (service_id, hash_id, rating) VALUES (?1, ?2, ?3)",
        )?;
        for &hash in hashes {
            delete.execute(params![service, hash])?;
            if rating != 0 {
                insert.execute(params![service, hash, rating])?;
            }
        }
        Ok(())
    }

    /// Record views: add to the counts and set the last viewed time.
    pub fn add_views(
        &mut self,
        hash: HashId,
        canvas: CanvasType,
        viewed_ms: Option<i64>,
        views: i64,
        viewtime_ms: i64,
    ) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO file_viewing_stats (hash_id, canvas_type, views, viewtime_ms, last_viewed_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (hash_id, canvas_type) DO UPDATE SET views = views + excluded.views,
                     viewtime_ms = viewtime_ms + excluded.viewtime_ms, last_viewed_ms = excluded.last_viewed_ms",
            )?
            .execute(params![hash, canvas.code(), views, viewtime_ms, viewed_ms])?;
        Ok(())
    }

    /// Overwrite view statistics; with no time given, a new record's last
    /// viewed time is now and an existing one's is kept.
    pub fn set_views(
        &mut self,
        hash: HashId,
        canvas: CanvasType,
        viewed_ms: Option<i64>,
        views: i64,
        viewtime_ms: i64,
    ) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO file_viewing_stats (hash_id, canvas_type, views, viewtime_ms, last_viewed_ms)
                 VALUES (?1, ?2, ?3, ?4, coalesce(?5, ?6))
                 ON CONFLICT (hash_id, canvas_type) DO UPDATE SET views = excluded.views,
                     viewtime_ms = excluded.viewtime_ms,
                     last_viewed_ms = coalesce(?5, last_viewed_ms)",
            )?
            .execute(params![hash, canvas.code(), views, viewtime_ms, viewed_ms, self.now_ms])?;
        Ok(())
    }

    /// A file's timestamp, if it has one.
    pub fn file_time(&self, hash: HashId, time: &FileTime) -> Result<Option<i64>> {
        let conn = self.conn;
        let value: Option<Option<i64>> = match time {
            FileTime::DomainModified(domain) => match url_domain_id(conn, domain)? {
                None => None,
                Some(domain) => conn
                    .query_row(
                        "SELECT modified_ms FROM file_domain_modified WHERE hash_id = ?1 AND domain_id = ?2",
                        params![hash, domain],
                        |r| r.get(0),
                    )
                    .optional()?,
            },
            FileTime::FileModified => conn
                .query_row("SELECT file_modified_ms FROM files WHERE hash_id = ?1", [hash], |r| r.get(0))
                .optional()?,
            FileTime::Imported(domain) => conn
                .query_row(
                    "SELECT added_ms FROM file_domain_current WHERE service_id = ?1 AND hash_id = ?2",
                    params![domain, hash],
                    |r| r.get(0),
                )
                .optional()?,
            FileTime::Deleted(domain) => conn
                .query_row(
                    "SELECT deleted_ms FROM file_domain_deleted WHERE service_id = ?1 AND hash_id = ?2",
                    params![domain, hash],
                    |r| r.get(0),
                )
                .optional()?,
            FileTime::PreviouslyImported(domain) => conn
                .query_row(
                    "SELECT original_added_ms FROM file_domain_deleted WHERE service_id = ?1 AND hash_id = ?2",
                    params![domain, hash],
                    |r| r.get(0),
                )
                .optional()?,
            FileTime::Archived => conn
                .query_row("SELECT archived_ms FROM file_archived WHERE hash_id = ?1", [hash], |r| r.get(0))
                .optional()?,
            FileTime::LastViewed(canvas) => conn
                .query_row(
                    "SELECT last_viewed_ms FROM file_viewing_stats WHERE hash_id = ?1 AND canvas_type = ?2",
                    params![hash, canvas.code()],
                    |r| r.get(0),
                )
                .optional()?,
        };
        Ok(value.flatten())
    }

    /// Set a timestamp. Setting the archive time archives inboxed files.
    pub fn set_file_time(&mut self, hashes: &[HashId], time: &FileTime, ms: i64) -> Result<()> {
        let conn = self.conn;
        let each = |sql: &str, extra: Option<i64>| -> Result<()> {
            let mut stmt = conn.prepare_cached(sql)?;
            for &hash in hashes {
                match extra {
                    Some(x) => stmt.execute(params![ms, hash, x])?,
                    None => stmt.execute(params![ms, hash])?,
                };
            }
            Ok(())
        };
        match time {
            FileTime::DomainModified(domain) => {
                let domain = i64::from(intern_url_domain(conn, domain)?.get());
                each(
                    "INSERT OR REPLACE INTO file_domain_modified (modified_ms, hash_id, domain_id) VALUES (?1, ?2, ?3)",
                    Some(domain),
                )
            }
            FileTime::FileModified => each(
                "UPDATE files SET file_modified_ms = ?1 WHERE hash_id = ?2",
                None,
            ),
            FileTime::Imported(domain) => {
                // cached import orders go stale
                crate::domains::changed(conn)?;
                each(
                    "UPDATE file_domain_current SET added_ms = ?1 WHERE hash_id = ?2 AND service_id = ?3",
                    Some(i64::from(domain.get())),
                )
            }
            FileTime::Deleted(domain) => each(
                "UPDATE file_domain_deleted SET deleted_ms = ?1 WHERE hash_id = ?2 AND service_id = ?3",
                Some(i64::from(domain.get())),
            ),
            FileTime::PreviouslyImported(domain) => each(
                "UPDATE file_domain_deleted SET original_added_ms = ?1 WHERE hash_id = ?2 AND service_id = ?3",
                Some(i64::from(domain.get())),
            ),
            FileTime::Archived => {
                self.archive(hashes)?;
                each(
                    "INSERT OR REPLACE INTO file_archived (archived_ms, hash_id) VALUES (?1, ?2)",
                    None,
                )
            }
            FileTime::LastViewed(canvas) => each(
                "UPDATE file_viewing_stats SET last_viewed_ms = ?1 WHERE hash_id = ?2 AND canvas_type = ?3",
                Some(i64::from(canvas.code())),
            ),
        }
    }

    /// Clear a web domain's modified time. Other timestamps can't be cleared.
    pub fn clear_domain_modified_time(&mut self, hashes: &[HashId], domain: &str) -> Result<()> {
        let Some(domain) = url_domain_id(self.conn, domain)? else {
            return Ok(());
        };
        let mut stmt = self.conn.prepare_cached(
            "DELETE FROM file_domain_modified WHERE hash_id = ?1 AND domain_id = ?2",
        )?;
        for &hash in hashes {
            stmt.execute(params![hash, domain])?;
        }
        Ok(())
    }
}

impl ContentWriter<'_> {
    /// Record a file's properties. An existing record is kept unless
    /// `overwrite` (the reference only overwrites when regenerating metadata).
    pub fn add_file_info(
        &mut self,
        hash: HashId,
        info: &crate::media::FileInfo,
        overwrite: bool,
    ) -> Result<()> {
        let (detected, forced) = match info.original_mime {
            Some(original) => (original, Some(info.mime)),
            None => (info.mime, None),
        };
        let verb = if overwrite {
            "INSERT OR REPLACE"
        } else {
            "INSERT OR IGNORE"
        };
        self.conn
            .prepare_cached(&format!(
                "{verb} INTO files (hash_id, size, mime, width, height, duration_ms, num_frames, has_audio,
                     num_words, forced_mime, file_modified_ms, pixel_hash, blurhash, flags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
            ))?
            .execute(params![
                hash,
                i64::try_from(info.size).unwrap_or(i64::MAX),
                detected.code(),
                info.width,
                info.height,
                info.duration_ms.and_then(|d| i64::try_from(d).ok()),
                info.num_frames.and_then(|n| i64::try_from(n).ok()),
                info.has_audio,
                info.num_words.and_then(|n| i64::try_from(n).ok()),
                forced.map(hydrus_core::Mime::code),
                info.file_modified.map(hydrus_core::time::TimestampMs::millis),
                info.pixel_hash.map(|h| h.0.to_vec()),
                info.blurhash,
                info.flags.0,
            ])?;
        // cached potential pairs hold their kings' sizes and pixel hashes
        let grouped: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM dup_group_members WHERE hash_id = ?1)
                 OR EXISTS (SELECT 1 FROM dup_groups WHERE king_hash_id = ?1)",
            [hash],
            |r| r.get(0),
        )?;
        if grouped {
            crate::duplicates::cache::changed(self.conn)?;
        }
        Ok(())
    }
}

impl ContentWriter<'_> {
    /// Record that `hashes` are known by each of `urls`.
    pub fn add_urls(&mut self, hashes: &[HashId], urls: &[String]) -> Result<()> {
        let mut insert = self
            .conn
            .prepare_cached("INSERT OR IGNORE INTO file_urls (hash_id, url_id) VALUES (?1, ?2)")?;
        for url in urls {
            let url_id = crate::master::intern_url(self.conn, url)?;
            for hash in hashes {
                insert.execute(params![hash, url_id])?;
            }
        }
        Ok(())
    }

    /// Forget that `hashes` are known by each of `urls`.
    pub fn delete_urls(&mut self, hashes: &[HashId], urls: &[String]) -> Result<()> {
        let mut delete = self
            .conn
            .prepare_cached("DELETE FROM file_urls WHERE hash_id = ?1 AND url_id = ?2")?;
        for url in urls {
            if let Some(url_id) = crate::master::url_id(self.conn, url)? {
                for hash in hashes {
                    delete.execute(params![hash, url_id])?;
                }
            }
        }
        Ok(())
    }
}
