//! Hydrus Tag Archive and Tag Pair Archive SQLite interchange.
//!
//! Sources are read-only snapshots. Destination batches use the reference schema
//! and preserve existing content and metadata; malformed archives are rejected.
use super::Content;
use crate::{Result, StoreError};
use hydrus_core::hash::HashKind;
use rusqlite::{Connection, OpenFlags, OptionalExtension as _, params};
use std::path::Path;

/// Metadata shown by the migration path inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metadata {
    Mappings(HashKind),
    Pairs(Content),
}
/// The right side of one archive entry.
#[derive(Debug, Clone)]
pub(super) enum Value {
    Hash(Vec<u8>),
    Tag(String),
}
/// A bounded source entry, retaining raw archive tag strings for filtering.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub left: String,
    pub right: Value,
}
pub(super) struct Archive {
    conn: Connection,
    pub metadata: Metadata,
}
fn hash_code(kind: HashKind) -> i64 {
    match kind {
        HashKind::Md5 => 0,
        HashKind::Sha1 => 1,
        HashKind::Sha256 => 2,
        HashKind::Sha512 => 3,
    }
}
fn hash_kind(code: i64) -> Result<HashKind> {
    match code {
        0 => Ok(HashKind::Md5),
        1 => Ok(HashKind::Sha1),
        2 => Ok(HashKind::Sha256),
        3 => Ok(HashKind::Sha512),
        _ => Err(StoreError::Invalid(format!(
            "unsupported archive hash type: {code}"
        ))),
    }
}
fn pair_code(content: Content) -> Result<i64> {
    match content {
        Content::Siblings => Ok(0),
        Content::Parents => Ok(1),
        Content::Mappings => Err(StoreError::Invalid("expected a tag pair archive".into())),
    }
}
fn metadata(
    conn: &Connection,
    content: Content,
    allow_empty: Option<HashKind>,
) -> Result<Metadata> {
    if content == Content::Mappings {
        // Preparing the joins checks all required tables/columns before use.
        conn.prepare("SELECT hash,tag,namespace FROM mappings JOIN hashes USING(hash_id) JOIN tags USING(tag_id),namespaces LIMIT 0")?;
        let code: Option<i64> = conn
            .query_row("SELECT hash_type FROM hash_type", [], |r| r.get(0))
            .optional()?;
        let kind = if let Some(code) = code {
            hash_kind(code)?
        } else {
            let length: Option<i64> = conn
                .query_row("SELECT length(hash) FROM hashes LIMIT 1", [], |r| r.get(0))
                .optional()?;
            match length {
                Some(16) => HashKind::Md5,
                Some(20) => HashKind::Sha1,
                Some(32) => HashKind::Sha256,
                Some(64) => HashKind::Sha512,
                Some(_) => return Err(StoreError::Invalid("cannot infer archive hash type from its hash length".into())),
                None => allow_empty.ok_or_else(|| StoreError::Invalid("This archive has no hash type set, and as it has no files, no hash type guess can be made.".into()))?,
            }
        };
        Ok(Metadata::Mappings(kind))
    } else {
        conn.prepare("SELECT a.tag,b.tag FROM pairs JOIN tags a ON tag_id_1=a.tag_id JOIN tags b ON tag_id_2=b.tag_id LIMIT 0")?;
        let code: Option<i64> = conn
            .query_row("SELECT pair_type FROM pair_type", [], |r| r.get(0))
            .optional()?;
        if code != Some(pair_code(content)?) {
            let message = match code {
                None => "This Hydrus Tag Pair Archive does not have a pair type set!",
                _ if content == Content::Parents => {
                    "This Hydrus Tag Pair Archive is not a tag parents archive!"
                }
                _ => "This Hydrus Tag Pair Archive is not a tag siblings archive!",
            };
            return Err(StoreError::Invalid(message.into()));
        }
        Ok(Metadata::Pairs(content))
    }
}
/// Inspect an existing archive without creating it or modifying its metadata.
pub fn inspect(path: &Path, content: Content) -> Result<Metadata> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    metadata(&conn, content, None)
}
/// Inspect a destination without creating it; an empty new archive uses the
/// selected hash kind. Existing archive kinds take precedence over the draft.
pub fn inspect_destination(path: &Path, content: Content, desired: HashKind) -> Result<Metadata> {
    if !path.exists() {
        return Ok(if content == Content::Mappings {
            Metadata::Mappings(desired)
        } else {
            Metadata::Pairs(content)
        });
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    metadata(&conn, content, Some(desired))
}
impl Archive {
    pub fn source(path: &Path, content: Content) -> Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.execute_batch("BEGIN DEFERRED")?;
        let metadata = metadata(&conn, content, None)?;
        Ok(Self { conn, metadata })
    }
    pub fn destination(path: &Path, content: Content, desired: HashKind) -> Result<Self> {
        let exists = path.exists();
        let mut conn = Connection::open(path)?;
        let transaction = conn.transaction()?;
        if !exists {
            transaction.execute_batch("CREATE TABLE tags(tag_id INTEGER PRIMARY KEY,tag TEXT); CREATE UNIQUE INDEX tags_tag_index ON tags(tag)")?;
            if content == Content::Mappings {
                transaction.execute_batch("CREATE TABLE hash_type(hash_type INTEGER); CREATE TABLE hashes(hash_id INTEGER PRIMARY KEY,hash BLOB_BYTES); CREATE UNIQUE INDEX hashes_hash_index ON hashes(hash); CREATE TABLE mappings(hash_id INTEGER,tag_id INTEGER,PRIMARY KEY(hash_id,tag_id)); CREATE INDEX mappings_hash_id_index ON mappings(hash_id); CREATE TABLE namespaces(namespace TEXT)")?;
                transaction.execute("INSERT INTO hash_type VALUES(?)", [hash_code(desired)])?;
            } else {
                transaction.execute_batch("CREATE TABLE pair_type(pair_type INTEGER); CREATE TABLE pairs(tag_id_1 INTEGER,tag_id_2 INTEGER,PRIMARY KEY(tag_id_1,tag_id_2))")?;
                transaction.execute("INSERT INTO pair_type VALUES(?)", [pair_code(content)?])?;
            }
        }
        let metadata = metadata(&transaction, content, Some(desired))?;
        if let Metadata::Mappings(kind) = metadata {
            transaction.execute("DELETE FROM hash_type", [])?;
            transaction.execute("INSERT INTO hash_type VALUES(?)", [hash_code(kind)])?;
        }
        transaction.commit()?;
        Ok(Self { conn, metadata })
    }
    pub fn read(&self, cursor: &mut (i64, i64), size: usize) -> Result<Vec<Entry>> {
        let mapping = matches!(self.metadata, Metadata::Mappings(_));
        let sql = if mapping {
            "SELECT m.tag_id,m.hash_id,t.tag,h.hash FROM mappings m JOIN tags t USING(tag_id) JOIN hashes h USING(hash_id) WHERE (m.tag_id,m.hash_id)>(?1,?2) ORDER BY m.tag_id,m.hash_id LIMIT ?3"
        } else {
            "SELECT p.tag_id_1,p.tag_id_2,a.tag,b.tag FROM pairs p JOIN tags a ON p.tag_id_1=a.tag_id JOIN tags b ON p.tag_id_2=b.tag_id WHERE (p.tag_id_1,p.tag_id_2)>(?1,?2) ORDER BY p.tag_id_1,p.tag_id_2 LIMIT ?3"
        };
        let mut statement = self.conn.prepare(sql)?;
        let mut rows = statement.query(params![
            cursor.0,
            cursor.1,
            i64::try_from(size).unwrap_or(1024)
        ])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            *cursor = (row.get(0)?, row.get(1)?);
            result.push(Entry {
                left: row.get(2)?,
                right: if mapping {
                    Value::Hash(row.get(3)?)
                } else {
                    Value::Tag(row.get(3)?)
                },
            });
        }
        Ok(result)
    }
    pub fn write(&mut self, entries: &[Entry]) -> Result<()> {
        let transaction = self.conn.transaction()?;
        for entry in entries {
            let left = intern_tag(&transaction, &entry.left)?;
            match &entry.right {
                Value::Hash(hash) => {
                    let Metadata::Mappings(kind) = self.metadata else {
                        return Err(StoreError::Invalid("mapping in pair archive".into()));
                    };
                    if hash.len() != kind.byte_len() {
                        return Err(StoreError::Invalid(
                            "archive hash has incorrect length".into(),
                        ));
                    }
                    transaction.execute("INSERT OR IGNORE INTO hashes(hash) VALUES(?)", [hash])?;
                    let hash_id: i64 = transaction.query_row(
                        "SELECT hash_id FROM hashes WHERE hash=?",
                        [hash],
                        |r| r.get(0),
                    )?;
                    transaction.execute(
                        "INSERT OR IGNORE INTO mappings VALUES(?1,?2)",
                        params![hash_id, left],
                    )?;
                    if let Some((namespace, _)) = entry.left.split_once(':')
                        && !namespace.is_empty()
                    {
                        transaction.execute("INSERT INTO namespaces(namespace) SELECT ?1 WHERE NOT EXISTS(SELECT 1 FROM namespaces WHERE namespace=?1)",[namespace])?;
                    }
                }
                Value::Tag(right) => {
                    let right = intern_tag(&transaction, right)?;
                    transaction.execute(
                        "INSERT OR IGNORE INTO pairs VALUES(?1,?2)",
                        params![left, right],
                    )?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }
}
fn intern_tag(conn: &Connection, tag: &str) -> Result<i64> {
    conn.execute("INSERT OR IGNORE INTO tags(tag) VALUES(?)", [tag])?;
    Ok(conn.query_row("SELECT tag_id FROM tags WHERE tag=?", [tag], |r| r.get(0))?)
}
