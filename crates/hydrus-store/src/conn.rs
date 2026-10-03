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

/// How often the writer checkpoints the write-ahead log after a commit,
/// and how often it empties it (the reference's
/// `WAL_PASSIVE_CHECKPOINT_PERIOD` and `WAL_TRUNCATE_CHECKPOINT_PERIOD`).
const PASSIVE_CHECKPOINT_PERIOD: Duration = Duration::from_secs(300);
const TRUNCATE_CHECKPOINT_PERIOD: Duration = Duration::from_secs(900);
/// How often the query planner's statistics are brought up to date as the
/// tables grow (for the reference's periodic re-analysis).
const OPTIMIZE_PERIOD: Duration = Duration::from_secs(3600);

/// The writer's database upkeep, done between batches.
struct Upkeep {
    checkpointed: Instant,
    truncated: Instant,
    optimized: Instant,
}

impl Upkeep {
    fn new(now: Instant) -> Self {
        Self {
            checkpointed: now,
            truncated: now,
            optimized: now,
        }
    }

    fn after_commit(&mut self, conn: &Connection, now: Instant) {
        if now.duration_since(self.checkpointed) >= PASSIVE_CHECKPOINT_PERIOD {
            let mode = if now.duration_since(self.truncated) >= TRUNCATE_CHECKPOINT_PERIOD {
                self.truncated = now;
                "TRUNCATE"
            } else {
                "PASSIVE"
            };
            if let Err(e) =
                conn.query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |_| Ok(()))
            {
                tracing::warn!(error = %e, "checkpointing the write-ahead log failed");
            }
            self.checkpointed = now;
        }
        if now.duration_since(self.optimized) >= OPTIMIZE_PERIOD {
            optimize(conn);
            self.optimized = now;
        }
    }
}

/// `PRAGMA optimize`, kept quick: it analyzes the tables whose statistics
/// have gone stale, from a sample.
fn optimize(conn: &Connection) {
    if let Err(e) = conn.execute_batch("PRAGMA analysis_limit = 1000; PRAGMA optimize;") {
        tracing::warn!(error = %e, "optimizing the database failed");
    }
}

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

/// What the writer thread is asked to do.
enum Job {
    Write(WriteJob),
    /// Stop writing, with the write-ahead log checkpointed, until released.
    /// Runs outside any batch.
    Pause(PauseRequest),
}

/// A queued write, type-erased. Running it returns whether it succeeded, the
/// effects it registered, and a reply function to call once the batch
/// outcome is known.
struct WriteJob {
    run: Box<dyn FnOnce(&Connection) -> JobOutcome + Send>,
    /// Commit in a batch of its own. For writes whose post-commit effects
    /// (e.g. publishing a new in-memory snapshot) later writes must see.
    barrier: bool,
}

struct PauseRequest {
    /// Told once the log is checkpointed and the writer has stopped.
    paused: Sender<Result<()>>,
    /// The writer waits on this until it is sent to or dropped.
    release: Receiver<()>,
}

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
    num_readers: usize,
}

/// The database, paused (see [`Db::pause`]); dropping this resumes it.
pub struct Paused {
    readers: Vec<Connection>,
    pool: Sender<Connection>,
    _release: Sender<()>,
}

impl std::fmt::Debug for Paused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paused").finish_non_exhaustive()
    }
}

impl Drop for Paused {
    fn drop(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = self.pool.send(reader);
        }
        // dropping `_release` lets the writer go on
    }
}

impl Db {
    /// Open (creating if needed) and migrate the database at `path`.
    pub fn open(path: &Path, num_readers: usize) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        schema::configure(&conn)?;
        schema::migrate(&mut conn)?;

        let num_readers = num_readers.max(1);
        let (give_back, take) = crossbeam_channel::bounded(num_readers);
        for _ in 0..num_readers {
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
            num_readers,
        })
    }

    /// Stop all database work, so the database file can be copied: wait for
    /// running reads and queued writes to finish, move everything in the
    /// write-ahead log into the database file, and hold every connection
    /// idle until the returned guard is dropped. Reads and writes started
    /// meanwhile wait.
    pub fn pause(&self) -> Result<Paused> {
        let (release, release_rx) = crossbeam_channel::bounded(0);
        let mut paused = Paused {
            readers: Vec::with_capacity(self.num_readers),
            pool: self.readers.give_back.clone(),
            _release: release,
        };
        for _ in 0..self.num_readers {
            let reader = self
                .readers
                .take
                .recv()
                .map_err(|_| StoreError::WriterGone)?;
            paused.readers.push(reader);
        }
        let (paused_tx, paused_rx) = crossbeam_channel::bounded(1);
        self.writer
            .jobs
            .as_ref()
            .ok_or(StoreError::WriterGone)?
            .send(Job::Pause(PauseRequest {
                paused: paused_tx,
                release: release_rx,
            }))
            .map_err(|_| StoreError::WriterGone)?;
        paused_rx.recv().map_err(|_| StoreError::WriterGone)??;
        Ok(paused)
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
        self.submit(f, false)
    }

    /// Like [`Db::write`], but the write commits in a batch of its own, and
    /// its post-commit effects have run before any later write starts.
    pub fn write_alone<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.submit(f, true)
    }

    fn submit<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
        barrier: bool,
    ) -> Result<R> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded::<Result<R>>(1);
        let run = Box::new(move |conn: &Connection| {
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
        let job = Job::Write(WriteJob { run, barrier });
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
    let mut upkeep = Upkeep::new(Instant::now());
    // a barrier job that arrived while a batch was open waits for the next one
    let mut carried: Option<Job> = None;
    while let Some(first) = carried.take().or_else(|| jobs.recv().ok()) {
        let first = match first {
            Job::Write(job) => job,
            Job::Pause(request) => {
                let checkpointed = conn
                    .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .map_err(StoreError::from)
                    .and_then(|busy| match busy {
                        0 => Ok(()),
                        _ => Err(StoreError::Invalid(
                            "the write-ahead log could not be checkpointed".into(),
                        )),
                    });
                let _ = request.paused.send(checkpointed);
                // until released, or the guard is dropped
                let _ = request.release.recv();
                continue;
            }
        };
        let batch_started = Instant::now();
        let mut outcomes = Vec::new();

        if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
            let message: Arc<str> = e.to_string().into();
            let outcome = run_job(conn, first);
            (outcome.reply)(Some(message));
            continue;
        }

        let alone = first.barrier;
        outcomes.push(run_job(conn, first));
        while !alone && outcomes.len() < MAX_BATCH_JOBS && batch_started.elapsed() < MAX_BATCH_TIME
        {
            match jobs.try_recv() {
                Ok(Job::Write(job)) if !job.barrier => outcomes.push(run_job(conn, job)),
                Ok(job) => {
                    carried = Some(job);
                    break;
                }
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
        upkeep.after_commit(conn, Instant::now());
    }
    // closing: as SQLite advises before a connection closes
    optimize(conn);
}

/// Run one job inside its own savepoint.
fn run_job(conn: &Connection, job: WriteJob) -> JobOutcome {
    if let Err(e) = conn.execute_batch("SAVEPOINT job") {
        tracing::error!(error = %e, "could not open savepoint");
    }
    let outcome = (job.run)(conn);
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

    #[test]
    fn the_writer_empties_the_log_every_quarter_hour() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hydrus.db");
        let db = Db::open(&path, 1).unwrap();
        db.write(|ctx| {
            ctx.conn()
                .execute("INSERT INTO texts (text) VALUES ('something')", [])?;
            Ok(())
        })
        .unwrap();
        let wal = dir.path().join("hydrus.db-wal");
        assert!(std::fs::metadata(&wal).unwrap().len() > 0);
        let conn = Connection::open(&path).unwrap();
        schema::configure(&conn).unwrap();
        let limit: i64 = conn
            .query_row("PRAGMA journal_size_limit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(limit, 128 * 1024 * 1024);

        let now = Instant::now();
        let mut upkeep = Upkeep::new(now);
        upkeep.after_commit(&conn, now + Duration::from_secs(10));
        assert!(std::fs::metadata(&wal).unwrap().len() > 0, "too soon");
        upkeep.after_commit(&conn, now + TRUNCATE_CHECKPOINT_PERIOD);
        assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0, "emptied");
        drop(db);
    }

    fn temp_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hydrus.db"), 2).unwrap();
        (dir, db)
    }

    #[test]
    fn a_pause_empties_the_log_and_holds_work_until_dropped() {
        let (dir, db) = temp_db();
        let db = Arc::new(db);
        db.write(|ctx| {
            ctx.conn()
                .execute("INSERT INTO texts (text) VALUES ('before')", [])?;
            Ok(())
        })
        .unwrap();
        let paused = db.pause().unwrap();
        let wal = dir.path().join("hydrus.db-wal");
        assert_eq!(std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0), 0);

        let (done_tx, done_rx) = crossbeam_channel::unbounded();
        let writer = {
            let db = Arc::clone(&db);
            let done = done_tx.clone();
            std::thread::spawn(move || {
                db.write(|ctx| {
                    ctx.conn()
                        .execute("INSERT INTO texts (text) VALUES ('during')", [])?;
                    Ok(())
                })
                .unwrap();
                done.send("write").unwrap();
            })
        };
        let reader = {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                db.read(|c| {
                    Ok(c.query_row("SELECT count(*) FROM texts", [], |r| r.get::<_, i64>(0))?)
                })
                .unwrap();
                done_tx.send("read").unwrap();
            })
        };
        assert!(done_rx.recv_timeout(Duration::from_millis(300)).is_err());
        drop(paused);
        let mut finished = vec![done_rx.recv().unwrap(), done_rx.recv().unwrap()];
        finished.sort_unstable();
        assert_eq!(finished, ["read", "write"]);
        writer.join().unwrap();
        reader.join().unwrap();
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
