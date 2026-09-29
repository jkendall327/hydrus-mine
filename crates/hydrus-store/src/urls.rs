//! Which files are known by a URL, and what the client knows about them.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use hydrus_core::{HashId, Mime, ServiceId, Sha256, TimestampMs};

use crate::error::Result;
use crate::master;
use crate::services::{ServiceKind, ServiceRegistry};

/// What the client knows about a file found by URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileState {
    /// Deleted from local storage (time unknown for old deletions).
    Deleted {
        deleted: Option<TimestampMs>,
        reason: String,
    },
    /// In the trash since `trashed`.
    InTrash {
        trashed: TimestampMs,
        reason: String,
    },
    /// In local storage since `imported`.
    Imported {
        imported: TimestampMs,
        mime: Option<Mime>,
    },
    /// Known only by its hash (e.g. a URL was associated with it).
    Unknown,
}

/// A file a URL points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlFile {
    pub hash_id: HashId,
    pub hash: Sha256,
    pub state: FileState,
}

fn service_of(services: &ServiceRegistry, want: fn(&ServiceKind) -> bool) -> Option<ServiceId> {
    services.all().find(|s| want(&s.kind)).map(|s| s.id)
}

/// The files stored under any of `urls` (each URL exactly as stored), by id.
pub fn files_for_urls(
    conn: &Connection,
    services: &ServiceRegistry,
    urls: &[String],
) -> Result<Vec<UrlFile>> {
    let mut hash_ids = BTreeSet::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT f.hash_id FROM urls u JOIN file_urls f ON f.url_id = u.url_id WHERE u.url = ?",
        )?;
        for url in urls {
            for row in stmt.query_map([url], |r| r.get::<_, HashId>(0))? {
                hash_ids.insert(row?);
            }
        }
    }
    let ids: Vec<HashId> = hash_ids.into_iter().collect();
    let hashes = master::hashes(conn, &ids)?;
    let storage = service_of(services, |k| matches!(k, ServiceKind::LocalFileStorage));
    let trash = service_of(services, |k| matches!(k, ServiceKind::Trash));
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(&hash) = hashes.get(&id) else {
            continue;
        };
        out.push(UrlFile {
            hash_id: id,
            hash,
            state: file_state(conn, storage, trash, id)?,
        });
    }
    Ok(out)
}

fn file_state(
    conn: &Connection,
    storage: Option<ServiceId>,
    trash: Option<ServiceId>,
    hash_id: HashId,
) -> Result<FileState> {
    let reason: String = conn
        .prepare_cached(
            "SELECT t.text FROM file_deletion_reasons r JOIN texts t ON t.text_id = r.reason_id WHERE r.hash_id = ?",
        )?
        .query_row([hash_id], |r| r.get(0))
        .optional()?
        .unwrap_or_else(|| "Unknown deletion reason.".to_owned());
    let current_since = |service: Option<ServiceId>| -> Result<Option<Option<TimestampMs>>> {
        let Some(service) = service else {
            return Ok(None);
        };
        Ok(conn
            .prepare_cached(
                "SELECT added_ms FROM file_domain_current WHERE service_id = ? AND hash_id = ?",
            )?
            .query_row(rusqlite::params![service, hash_id], |r| r.get(0))
            .optional()?)
    };
    if let Some(storage) = storage {
        let deleted: Option<Option<TimestampMs>> = conn
            .prepare_cached(
                "SELECT deleted_ms FROM file_domain_deleted WHERE service_id = ? AND hash_id = ?",
            )?
            .query_row(rusqlite::params![storage, hash_id], |r| r.get(0))
            .optional()?;
        if let Some(deleted) = deleted {
            return Ok(FileState::Deleted { deleted, reason });
        }
    }
    if let Some(Some(trashed)) = current_since(trash)? {
        return Ok(FileState::InTrash { trashed, reason });
    }
    if let Some(Some(imported)) = current_since(storage)? {
        let mime: Option<Mime> = conn
            .prepare_cached("SELECT coalesce(forced_mime, mime) FROM files WHERE hash_id = ?")?
            .query_row([hash_id], |r| r.get::<_, u8>(0))
            .optional()?
            .and_then(Mime::from_code);
        return Ok(FileState::Imported { imported, mime });
    }
    Ok(FileState::Unknown)
}
