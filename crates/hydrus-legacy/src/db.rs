//! Opening a reference database directory, read-only.
//!
//! A reference database directory holds four SQLite files, which the
//! reference attaches under fixed schema names:
//!
//! | file | schema |
//! |---|---|
//! | `client.db` | `main` |
//! | `client.caches.db` | `external_caches` |
//! | `client.mappings.db` | `external_mappings` |
//! | `client.master.db` | `external_master` |
//!
//! [`LegacyDb`] opens them the same way, but read-only: every file is opened
//! with the SQLite URI parameter `mode=ro` and the connection additionally
//! sets `PRAGMA query_only`, so no statement can modify them.
//!
//! # Running alongside the reference client
//!
//! The reference uses WAL journaling. In WAL mode readers never block the
//! writer and the writer never blocks readers, so a [`LegacyDb`] can read
//! while the Python client is running: each read sees the last committed
//! state (wrap a whole import in [`LegacyDb::snapshot`] to see a single
//! consistent state throughout).
//!
//! SQLite needs the `-wal` and `-shm` files beside each database to read a
//! WAL database. When the client is running they exist; when it is not,
//! SQLite creates them (empty) on first read and leaves them behind. That
//! is the only change a read-only open makes to the directory: the database
//! files themselves are never written. The reference client uses such files
//! normally the next time it starts, provided it runs as the same OS user.
//!
//! If the directory is not writable and the `-wal` files do not exist (a
//! backup on read-only media, say), there can be no writer, and the open
//! falls back to SQLite's `immutable=1` mode, which creates nothing.
//! [`OpenMode::Immutable`] requests that mode up front.
//!
//! If the reference was started with a non-default rollback journal
//! (`--db_journal_mode`), readers and the writer do block each other; close
//! the reference client before importing in that case.

use std::cell::Cell;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hydrus_core::{ServiceId, ServiceKey, ServiceType};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::error::{LegacyError, Result};

/// The only reference database version this crate reads.
pub const SUPPORTED_VERSION: u32 = hydrus_core::REFERENCE_VERSION;

/// The database files and the schema names the reference attaches them as.
pub const DATABASE_FILES: [(&str, &str); 4] = [
    ("main", "client.db"),
    ("external_caches", "client.caches.db"),
    ("external_mappings", "client.mappings.db"),
    ("external_master", "client.master.db"),
];

/// Rows fetched per query when streaming a table.
pub const DEFAULT_BATCH_SIZE: usize = 10_000;

/// How to open the database files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenMode {
    /// Read-only, safe while the reference client is running (see the
    /// module docs). Falls back to [`OpenMode::Immutable`] when SQLite
    /// cannot create the files WAL readers need and no writer can exist.
    #[default]
    Shared,
    /// SQLite's `immutable=1`: nothing is created or locked, but the files
    /// must not change while they are open. For backups and read-only media.
    Immutable,
}

/// Options for [`LegacyDb::open_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenOptions {
    pub mode: OpenMode,
    /// Rows fetched per query by the streaming readers.
    pub batch_size: usize,
    /// How long to wait for a lock held by the reference client (WAL
    /// recovery, checkpoints) before failing.
    pub busy_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            mode: OpenMode::Shared,
            batch_size: DEFAULT_BATCH_SIZE,
            busy_timeout: Duration::from_secs(30),
        }
    }
}

/// The basic identity of a service, cached when the database is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    pub id: ServiceId,
    pub key: ServiceKey,
    pub service_type: ServiceType,
    pub name: String,
}

/// A read-only view of a reference (v688) client database directory.
#[derive(Debug)]
pub struct LegacyDb {
    connection: Connection,
    db_dir: PathBuf,
    mode: OpenMode,
    batch_size: Cell<usize>,
    services: Vec<ServiceInfo>,
}

impl LegacyDb {
    /// Open a database directory with default options.
    pub fn open(db_dir: impl AsRef<Path>) -> Result<LegacyDb> {
        Self::open_with(db_dir, OpenOptions::default())
    }

    /// Open a database directory.
    ///
    /// Fails with [`LegacyError::UnsupportedVersion`] unless the database is
    /// exactly version 688: older databases must first be updated by
    /// running the v688 reference client on them.
    pub fn open_with(db_dir: impl AsRef<Path>, options: OpenOptions) -> Result<LegacyDb> {
        let db_dir = db_dir.as_ref();
        let db_dir = std::fs::canonicalize(db_dir).map_err(|source| LegacyError::Io {
            path: db_dir.to_owned(),
            source,
        })?;
        for (_, file) in DATABASE_FILES {
            if !db_dir.join(file).is_file() {
                return Err(LegacyError::NotADatabase {
                    path: db_dir,
                    reason: format!("{file} is missing"),
                });
            }
        }
        let attempt = Self::open_mode(&db_dir, options, options.mode);
        match attempt {
            Err(e) if options.mode == OpenMode::Shared && cannot_create_wal_files(&e, &db_dir) => {
                Self::open_mode(&db_dir, options, OpenMode::Immutable)
            }
            other => other,
        }
    }

    fn open_mode(db_dir: &Path, options: OpenOptions, mode: OpenMode) -> Result<LegacyDb> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let main = sqlite_uri(&db_dir.join(DATABASE_FILES[0].1), mode);
        let connection = Connection::open_with_flags(&main, flags)?;
        connection.busy_timeout(options.busy_timeout)?;
        connection.pragma_update(None, "query_only", true)?;
        check_version(&connection, db_dir)?;
        for (schema, file) in &DATABASE_FILES[1..] {
            let uri = sqlite_uri(&db_dir.join(file), mode);
            connection.execute(&format!("ATTACH DATABASE ?1 AS {schema}"), [uri])?;
        }
        let mut db = LegacyDb {
            connection,
            db_dir: db_dir.to_owned(),
            mode,
            batch_size: Cell::new(options.batch_size.max(1)),
            services: Vec::new(),
        };
        db.services = db.read_service_infos()?;
        Ok(db)
    }

    /// The (canonicalised) database directory. Relative file storage
    /// locations are relative to it.
    pub fn db_dir(&self) -> &Path {
        &self.db_dir
    }

    /// The mode the files were actually opened in.
    pub fn mode(&self) -> OpenMode {
        self.mode
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn batch_size(&self) -> usize {
        self.batch_size.get()
    }

    /// Change how many rows streaming readers fetch per query.
    pub fn set_batch_size(&self, batch_size: usize) {
        self.batch_size.set(batch_size.max(1));
    }

    /// Hold one read transaction until the guard is dropped, so every read
    /// in between sees the same committed state even if the reference
    /// client is writing. (In WAL mode, commits spanning several of the
    /// four files are atomic per file only, so a snapshot taken while the
    /// reference is mid-commit may see one file's half.)
    pub fn snapshot(&self) -> Result<Snapshot<'_>> {
        let transaction = self.connection.unchecked_transaction()?;
        // a WAL read snapshot starts at the first read of each file
        for (schema, _) in DATABASE_FILES {
            transaction.query_row(
                &format!("SELECT count(*) FROM {schema}.sqlite_master"),
                [],
                |_| Ok(()),
            )?;
        }
        Ok(Snapshot {
            _transaction: transaction,
        })
    }

    /// Every service, in id order, without decoding settings. See
    /// [`LegacyDb::services`] for the full records.
    pub fn service_infos(&self) -> &[ServiceInfo] {
        &self.services
    }

    /// Look up a service by id.
    pub fn service_info(&self, id: ServiceId) -> Result<&ServiceInfo> {
        self.services
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| LegacyError::BadService {
                service_id: id.get(),
                problem: "does not exist".into(),
            })
    }

    /// Look up a service by key.
    pub fn service_by_key(&self, key: &ServiceKey) -> Option<&ServiceInfo> {
        self.services.iter().find(|s| &s.key == key)
    }

    fn read_service_infos(&self) -> Result<Vec<ServiceInfo>> {
        let mut statement = self.connection.prepare(
            "SELECT service_id, service_key, service_type, name FROM main.services ORDER BY service_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, ServiceId>(0)?,
                row.get::<_, ServiceKey>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, key, code, name) = row?;
            let service_type = u8::try_from(code)
                .ok()
                .and_then(ServiceType::from_code)
                .ok_or_else(|| {
                    LegacyError::bad_value(
                        "services",
                        format!("service {id} has unknown type {code}"),
                    )
                })?;
            Ok(ServiceInfo {
                id,
                key,
                service_type,
                name,
            })
        })
        .collect()
    }

    /// Whether `schema.table` exists.
    pub fn table_exists(&self, schema: &str, table: &str) -> Result<bool> {
        let found: Option<i64> = self
            .connection
            .query_row(
                &format!("SELECT 1 FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1"),
                [table],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// The names of every table in a schema, sorted.
    pub fn table_names(&self, schema: &str) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT name FROM {schema}.sqlite_master WHERE type = 'table' ORDER BY name"
        ))?;
        let names = statement.query_map([], |row| row.get(0))?;
        Ok(names.collect::<rusqlite::Result<_>>()?)
    }

    /// Count the rows of `schema.table` (for progress reporting and checks).
    pub fn count_rows(&self, schema: &str, table: &str) -> Result<u64> {
        if !self.table_exists(schema, table)? {
            return Err(LegacyError::MissingTable(format!("{schema}.{table}")));
        }
        let count: i64 = self.connection.query_row(
            &format!("SELECT count(*) FROM {schema}.{table}"),
            [],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// The qualified name of a per-service table, after checking the
    /// service exists, is of a kind that has such tables, and the table exists.
    pub(crate) fn service_table(
        &self,
        service: ServiceId,
        schema: &str,
        prefix: &str,
        kind_ok: fn(ServiceType) -> bool,
        kind_description: &str,
    ) -> Result<String> {
        let info = self.service_info(service)?;
        if !kind_ok(info.service_type) {
            return Err(LegacyError::BadService {
                service_id: service.get(),
                problem: format!("is a {}, not {kind_description}", info.service_type),
            });
        }
        let table = format!("{prefix}_{}", service.get());
        if !self.table_exists(schema, &table)? {
            return Err(LegacyError::MissingTable(format!("{schema}.{table}")));
        }
        Ok(format!("{schema}.{table}"))
    }

    /// Qualify a fixed table name after checking it exists.
    pub(crate) fn table(&self, schema: &str, table: &str) -> Result<String> {
        if self.table_exists(schema, table)? {
            Ok(format!("{schema}.{table}"))
        } else {
            Err(LegacyError::MissingTable(format!("{schema}.{table}")))
        }
    }
}

/// A read transaction; see [`LegacyDb::snapshot`].
pub struct Snapshot<'db> {
    _transaction: rusqlite::Transaction<'db>,
}

impl std::fmt::Debug for Snapshot<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Snapshot")
    }
}

fn check_version(connection: &Connection, db_dir: &Path) -> Result<()> {
    let has_version_table: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM main.sqlite_master WHERE type = 'table' AND name = 'version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if has_version_table.is_none() {
        return Err(LegacyError::NotADatabase {
            path: db_dir.to_owned(),
            reason: "client.db has no version table".into(),
        });
    }
    let version: Option<i64> = connection
        .query_row("SELECT version FROM main.version", [], |row| row.get(0))
        .optional()?;
    let Some(version) = version else {
        return Err(LegacyError::NotADatabase {
            path: db_dir.to_owned(),
            reason: "client.db has an empty version table".into(),
        });
    };
    if version == i64::from(SUPPORTED_VERSION) {
        return Ok(());
    }
    Err(LegacyError::UnsupportedVersion {
        found: version,
        supported: SUPPORTED_VERSION,
        advice: if version < i64::from(SUPPORTED_VERSION) {
            "update it first by running hydrus client v688 on it once, then import again"
        } else {
            "it was written by a newer hydrus client; import from a v688 backup instead"
        },
    })
}

/// Whether an open failed only because SQLite could not create the `-wal`
/// and `-shm` files a WAL reader needs, in a directory where no writer can
/// be active (no `-wal` file exists).
fn cannot_create_wal_files(error: &LegacyError, db_dir: &Path) -> bool {
    let LegacyError::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) = error else {
        return false;
    };
    let code_matches = matches!(
        failure.code,
        rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::ReadOnly
    );
    code_matches
        && DATABASE_FILES
            .iter()
            .all(|(_, file)| !db_dir.join(format!("{file}-wal")).exists())
}

/// A SQLite `file:` URI for a path, percent-encoding anything that is not
/// safe in a URI path.
fn sqlite_uri(path: &Path, mode: OpenMode) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file:");
    // Windows drive paths need a leading slash: file:/C:/...
    if !text.starts_with('/') {
        uri.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            uri.push(byte as char);
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.push_str(match mode {
        OpenMode::Shared => "?mode=ro",
        OpenMode::Immutable => "?mode=ro&immutable=1",
    });
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_escaping() {
        assert_eq!(
            sqlite_uri(Path::new("/a b/c?d#e%f/client.db"), OpenMode::Shared),
            "file:/a%20b/c%3Fd%23e%25f/client.db?mode=ro"
        );
        assert_eq!(
            sqlite_uri(Path::new("/x/é.db"), OpenMode::Immutable),
            "file:/x/%C3%A9.db?mode=ro&immutable=1"
        );
    }
}
