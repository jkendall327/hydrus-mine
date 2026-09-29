//! Connection management: one writer thread, a pool of readers.
//!
//! Writes are closures sent to the writer thread. The writer opens a batch
//! transaction, runs each queued closure inside its own savepoint (so one
//! failing closure rolls back only itself), commits the batch, and only then
//! reports each closure's result and runs its post-commit effects. Callers
//! therefore never observe a result for a write that was not durably
//! committed, and the fsync cost of a commit is shared by every write that
//! queued up behind the previous one (group commit).
//!
//! Reads borrow a pooled connection and run inside a read transaction, so
//! every read sees one consistent WAL snapshot and never blocks the writer.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use rusqlite::{Connection, OpenFlags};

use crate::error::{Result, StoreError};
use crate::schema;

/// Upper bound on how many queued writes share one commit.
const MAX_BATCH_JOBS: usize = 256;
/// Upper bound on how long a batch keeps absorbing queued writes.
const MAX_BATCH_TIME: Duration = Duration::from_millis(200);

/// Work to run after the batch containing a write has committed.
pub(crate) type Effect = Box<dyn FnOnce() + Send>;

/// Handed to write closures: the connection, plus a place to register effects
/// that must only happen if the write commits (publishing in-memory state,
/// notifying listeners).
pub struct WriteCtx<'c> {
    conn: &'c Connection,
    effects: Vec<Effect>,
}

impl std::fmt::Debug for WriteCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteCtx")
            .field("pending_effects", &self.effects.len())
            .finish_non_exhaustive()
    }
}

impl<'c> WriteCtx<'c> {
    pub fn conn(&self) -> &'c Connection {
        self.conn
    }

    /// Run `effect` after this write's batch commits. Dropped if the write
    /// fails or the commit fails.
    pub fn after_commit(&mut self, effect: impl FnOnce() + Send + 'static) {
        self.effects.push(Box::new(effect));
    }
}

/// A queued write, type-erased. Running it returns whether it succeeded, the
/// effects it registered, and a reply function to call once the batch
/// outcome is known.
type Job = Box<dyn FnOnce(&Connection) -> JobOutcome + Send>;

struct JobOutcome {
    succeeded: bool,
    effects: Vec<Effect>,
    reply: Box<dyn FnOnce(Option<Arc<str>>) + Send>,
}

/// Handle to the writer thread.
#[derive(Debug)]
struct Writer {
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

/// A pool of read-only connections.
#[derive(Debug)]
struct ReaderPool {
    take: Receiver<Connection>,
    give_back: Sender<Connection>,
}

/// Owns the database connections. Cheap to share behind an `Arc`.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
    writer: Writer,
    readers: ReaderPool,
}

impl Db {
    /// Open (creating if needed) and migrate the database at `path`.
    pub fn open(path: &Path, num_readers: usize) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        schema::configure(&conn)?;
        schema::migrate(&mut conn)?;

        let (give_back, take) = crossbeam_channel::bounded(num_readers.max(1));
        for _ in 0..num_readers.max(1) {
            let reader = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            schema::configure(&reader)?;
            reader.pragma_update(None, "query_only", true)?;
            give_back.send(reader).map_err(|_| StoreError::WriterGone)?;
        }

        let (jobs_tx, jobs_rx) = crossbeam_channel::unbounded::<Job>();
        let thread = std::thread::Builder::new()
            .name("hydrus-db-writer".into())
            .spawn(move || writer_loop(&conn, &jobs_rx))?;

        Ok(Self {
            path: path.to_path_buf(),
            writer: Writer {
                jobs: Some(jobs_tx),
                thread: Some(thread),
            },
            readers: ReaderPool { take, give_back },
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `f` on a pooled connection inside a read transaction.
    pub fn read<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        let conn = self
            .readers
            .take
            .recv()
            .map_err(|_| StoreError::WriterGone)?;
        let guard = PooledReader {
            conn: Some(conn),
            pool: &self.readers.give_back,
        };
        let conn = guard.conn.as_ref().expect("present until drop");
        conn.execute_batch("BEGIN")?;
        let result = f(conn);
        // A read transaction has nothing to commit; ending it releases the snapshot.
        conn.execute_batch("COMMIT")?;
        result
    }

    /// Run `f` on the writer thread, inside its own savepoint, and wait until
    /// the batch containing it has committed.
    pub fn write<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded::<Result<R>>(1);
        let job: Job = Box::new(move |conn| {
            let mut ctx = WriteCtx {
                conn,
                effects: Vec::new(),
            };
            let result = f(&mut ctx);
            let succeeded = result.is_ok();
            let effects = if succeeded { ctx.effects } else { Vec::new() };
            JobOutcome {
                succeeded,
                effects,
                reply: Box::new(move |commit_error| {
                    let outcome = match commit_error {
                        Some(e) if succeeded => {
                            Err(StoreError::Invalid(format!("commit failed: {e}")))
                        }
                        _ => result,
                    };
                    // The caller may have given up waiting; that's fine.
                    let _ = reply_tx.send(outcome);
                }),
            }
        });
        self.writer
            .jobs
            .as_ref()
            .ok_or(StoreError::WriterGone)?
            .send(job)
            .map_err(|_| StoreError::WriterGone)?;
        reply_rx.recv().map_err(|_| StoreError::WriterGone)?
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // Closing the channel ends the writer loop once queued jobs are done.
        drop(self.writer.jobs.take());
        if let Some(thread) = self.writer.thread.take() {
            let _ = thread.join();
        }
    }
}

struct PooledReader<'p> {
    conn: Option<Connection>,
    pool: &'p Sender<Connection>,
}

impl Drop for PooledReader<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // If a read panicked mid-transaction, make the connection usable again.
            if !conn.is_autocommit() {
                let _ = conn.execute_batch("ROLLBACK");
            }
            let _ = self.pool.send(conn);
        }
    }
}

fn writer_loop(conn: &Connection, jobs: &Receiver<Job>) {
    while let Ok(first) = jobs.recv() {
        let batch_started = Instant::now();
        let mut outcomes = Vec::new();

        if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
            let message: Arc<str> = e.to_string().into();
            let outcome = run_job(conn, first);
            (outcome.reply)(Some(message));
            continue;
        }

        outcomes.push(run_job(conn, first));
        while outcomes.len() < MAX_BATCH_JOBS && batch_started.elapsed() < MAX_BATCH_TIME {
            match jobs.try_recv() {
                Ok(job) => outcomes.push(run_job(conn, job)),
                Err(_) => break,
            }
        }

        let commit_error: Option<Arc<str>> = match conn.execute_batch("COMMIT") {
            Ok(()) => None,
            Err(e) => {
                tracing::error!(error = %e, "batch commit failed; rolling back");
                let _ = conn.execute_batch("ROLLBACK");
                Some(e.to_string().into())
            }
        };

        for outcome in outcomes {
            if commit_error.is_none() && outcome.succeeded {
                for effect in outcome.effects {
                    effect();
                }
            }
            (outcome.reply)(commit_error.clone());
        }
    }
}

/// Run one job inside its own savepoint.
fn run_job(conn: &Connection, job: Job) -> JobOutcome {
    if let Err(e) = conn.execute_batch("SAVEPOINT job") {
        tracing::error!(error = %e, "could not open savepoint");
    }
    let outcome = job(conn);
    let end = if outcome.succeeded {
        "RELEASE job"
    } else {
        "ROLLBACK TO job; RELEASE job"
    };
    if let Err(e) = conn.execute_batch(end) {
        tracing::error!(error = %e, "could not close savepoint");
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hydrus.db"), 2).unwrap();
        (dir, db)
    }

    #[test]
    fn write_then_read() {
        let (_dir, db) = temp_db();
        let id = db
            .write(|ctx| {
                ctx.conn()
                    .execute("INSERT INTO texts (text) VALUES ('hello')", [])?;
                Ok(ctx.conn().last_insert_rowid())
            })
            .unwrap();
        let text: String = db
            .read(|c| {
                Ok(
                    c.query_row("SELECT text FROM texts WHERE text_id = ?", [id], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn failed_write_rolls_back_only_itself() {
        let (_dir, db) = temp_db();
        let db = Arc::new(db);
        let handles: Vec<_> = (0..20)
            .map(|i| {
                let db = Arc::clone(&db);
                std::thread::spawn(move || {
                    db.write(move |ctx| {
                        ctx.conn()
                            .execute("INSERT INTO texts (text) VALUES (?)", [format!("t{i}")])?;
                        if i % 2 == 0 {
                            return Err(StoreError::Invalid("deliberate".into()));
                        }
                        Ok(())
                    })
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 10);
        let count: i64 = db
            .read(|c| Ok(c.query_row("SELECT count(*) FROM texts", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn effects_run_only_after_successful_commit() {
        let (_dir, db) = temp_db();
        let hits = Arc::new(parking_lot::Mutex::new(Vec::new()));
        for succeed in [true, false] {
            let hits = Arc::clone(&hits);
            let _ = db.write(move |ctx| {
                ctx.after_commit(move || hits.lock().push(succeed));
                if succeed {
                    Ok(())
                } else {
                    Err(StoreError::Invalid("no".into()))
                }
            });
        }
        assert_eq!(*hits.lock(), vec![true]);
    }

    #[test]
    fn readers_see_consistent_snapshots_while_writing() {
        let (_dir, db) = temp_db();
        db.write(|ctx| {
            ctx.conn()
                .execute("INSERT INTO texts (text) VALUES ('a')", [])?;
            Ok(())
        })
        .unwrap();
        db.read(|c| {
            let before: i64 = c.query_row("SELECT count(*) FROM texts", [], |r| r.get(0))?;
            // A write committed during this read is invisible to it.
            let db_path = c.path().unwrap().to_owned();
            let other = Connection::open(db_path)?;
            other.execute("INSERT INTO texts (text) VALUES ('b')", [])?;
            let after: i64 = c.query_row("SELECT count(*) FROM texts", [], |r| r.get(0))?;
            assert_eq!(before, after);
            Ok(())
        })
        .unwrap();
    }
}
