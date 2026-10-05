//! Captured metadata targets, cancellable file work and duplicate-neighbour cleanup.
//! Database forcing commits in reference-sized blocks before each block's disk
//! moves. Timestamp content updates are committed by their editor before this
//! disk-only operation starts; cancelling does not roll back earlier work.
use crate::{Store, file_maintenance, popups};
use hydrus_core::{HashId, Mime, Sha256};
use std::{
    cell::Cell,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// One file's identity and MIME values captured when its editor was accepted.
#[derive(Clone, Debug)]
pub struct File {
    pub id: HashId,
    pub hash: Sha256,
    pub mime: Mime,
    pub original_mime: Mime,
}

/// An accepted finite operation, independent of subsequent selections/editors.
#[derive(Clone, Debug)]
pub enum Request {
    Modified {
        files: Vec<File>,
        milliseconds: i64,
        step: i64,
    },
    Force {
        files: Vec<File>,
        mime: Option<Mime>,
    },
}
impl Request {
    /// The exact reference popup title for this work.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Modified { .. } => "setting file modified dates",
            Self::Force { .. } => "forcing filetypes",
        }
    }
}

/// Clock and filesystem effects are explicit, so reference failure/cancel inputs
/// can exercise the real runner without sleeping or depending on permissions.
pub trait Effects {
    /// Current whole seconds, also used for delayed popup/cleanup scheduling.
    fn now(&self) -> i64;
    /// Attempt the normal move; failure falls back to a metadata-preserving copy.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Observe a new reference gauge before its cancellation check.
    fn before_step(&self, _index: usize, _key: &[u8; 32]) {}
    /// Observe publication of the real persisted job.
    fn published(&self, _job: &popups::Job) {}
    /// Yield after completed force blocks, as the reference BigJobPauser does.
    fn block_complete(&self) {}
}

/// Normal local filesystem effects and the current wall clock.
pub struct Local {
    next_pause: Cell<Instant>,
}
impl Default for Local {
    fn default() -> Self {
        Self {
            next_pause: Cell::new(Instant::now() + Duration::from_secs(10)),
        }
    }
}
impl Effects for Local {
    fn now(&self) -> i64 {
        hydrus_core::TimestampMs::now().millis() / 1000
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn block_complete(&self) {
        if Instant::now() > self.next_pause.get() {
            std::thread::sleep(Duration::from_millis(100));
            self.next_pause
                .set(Instant::now() + Duration::from_secs(10));
        }
    }
}

fn attributes(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FILE_WRITE_ATTRIBUTES, without requiring permission to rewrite bytes.
        options.access_mode(0x100);
    }
    options.open(path)
}

fn timestamp(milliseconds: i64) -> Option<SystemTime> {
    let duration = Duration::from_millis(milliseconds.unsigned_abs());
    if milliseconds < 0 {
        UNIX_EPOCH.checked_sub(duration)
    } else {
        UNIX_EPOCH.checked_add(duration)
    }
}

/// Move to the effective extension, preserving copy metadata and reporting whether
/// the original must be cleaned up by the existing maintenance runner.
pub fn move_extension(from: &Path, to: &Path, effects: &dyn Effects) -> io::Result<bool> {
    if from == to {
        return Ok(false);
    }
    let source = fs::metadata(from)?;
    if !source.is_file() {
        return Err(io::Error::other("metadata source is not a file"));
    }
    if let Ok(destination) = fs::metadata(to) {
        if !destination.is_file() {
            return Err(io::Error::other("metadata destination is not a file"));
        }
        if source.len() == destination.len() && source.modified()? == destination.modified()? {
            fs::remove_file(from)?;
            return Ok(false);
        }
    }
    if effects.rename(from, to).is_ok() {
        return Ok(false);
    }
    copy_extension(from, to)?;
    Ok(true)
}

fn copy_extension(from: &Path, to: &Path) -> io::Result<()> {
    let source = fs::metadata(from)?;
    if let Ok(destination) = fs::metadata(to) {
        if source.len() == destination.len() && source.modified()? == destination.modified()? {
            return Ok(());
        }
    }
    fs::copy(from, to)?;
    let times = fs::FileTimes::new()
        .set_modified(source.modified()?)
        .set_accessed(source.accessed()?);
    attributes(to)?.set_times(times)
}

struct Reporting<'a> {
    store: &'a Arc<Store>,
    effects: &'a dyn Effects,
    shutdown: &'a AtomicBool,
    job: popups::Job,
    started: i64,
    published: bool,
    force: bool,
}
impl Reporting<'_> {
    fn publish(&mut self) -> crate::Result<()> {
        let job = self.job.clone();
        let now = self.effects.now();
        self.store
            .write(move |ctx| popups::add(ctx.conn(), &job, now))?;
        self.published = true;
        self.effects.published(&self.job);
        Ok(())
    }
    fn cancelled(&self) -> crate::Result<bool> {
        if self.shutdown.load(Ordering::Acquire) {
            return Ok(true);
        }
        if !self.published {
            return Ok(false);
        }
        let key = self.job.key;
        let now = self.effects.now();
        self.store
            .read(move |conn| Ok(popups::get(conn, &key, now)?.is_none_or(|job| job.cancelled)))
    }
    fn step(&mut self, index: usize, total: usize) -> crate::Result<bool> {
        // Forced types check cancellation before publishing this block's gauge.
        if self.force && self.cancelled()? {
            return Ok(true);
        }
        self.job.status_text_1 = Some(hydrus_core::numbers::value_range(
            index as u64,
            total as u64,
        ));
        self.job.popup_gauge_1 = Some((
            i64::try_from(index).unwrap_or(i64::MAX),
            i64::try_from(total).unwrap_or(i64::MAX),
        ));
        self.effects.before_step(index, &self.job.key);
        let now = self.effects.now();
        if !self.published && !self.force && now > self.started + 3 {
            self.publish()?;
        }
        if self.published {
            let key = self.job.key;
            let text = self.job.status_text_1.clone();
            let gauge = self.job.popup_gauge_1;
            self.store.write(move |ctx| {
                popups::update(ctx.conn(), &key, now, |job| {
                    job.status_text_1 = text;
                    job.popup_gauge_1 = gauge;
                })?;
                Ok(())
            })?;
        }
        // Timestamps check only after setting their gauge and delayed popup.
        Ok(!self.force && self.cancelled()?)
    }
    fn finish(&self, error: Option<&str>) -> crate::Result<()> {
        let now = self.effects.now();
        let key = self.job.key;
        if self.published {
            self.store.write(move |ctx| {
                popups::update(ctx.conn(), &key, now, |job| {
                    job.finish_and_dismiss(None, now)
                })?;
                Ok(())
            })?;
        }
        if let Some(error) = error {
            let mut job = popups::Job::text(error, now as f64);
            job.status_title = self.job.status_title.clone();
            job.had_error = true;
            self.store
                .write(move |ctx| popups::add(ctx.conn(), &job, now))?;
        }
        Ok(())
    }
}

fn path(store: &Store, file: &File, mime: Mime) -> Option<PathBuf> {
    store.snapshot().storage.file_path(&file.hash, mime)
}

/// Execute one captured operation; cancellation occurs at the reference's file
/// or 64-file block boundary, and never reverses an already committed prefix.
pub fn run(
    store: &Arc<Store>,
    request: &Request,
    shutdown: &AtomicBool,
    effects: &dyn Effects,
) -> Result<(), String> {
    let now = effects.now();
    let mut job = popups::Job::new(false, true, now as f64);
    job.status_title = Some(request.title().into());
    let mut reporting = Reporting {
        store,
        effects,
        shutdown,
        job,
        started: now,
        published: false,
        force: matches!(request, Request::Force { .. }),
    };
    if let Request::Force { files, .. } = request {
        if files.len() > 64 {
            reporting.publish().map_err(|e| e.to_string())?;
        }
    }
    // Keep completion/cleanup outside the operation's early error paths.
    let result = (|| -> Result<(), String> {
        match request {
            Request::Modified {
                files,
                milliseconds,
                step,
            } => {
                for (index, file) in files.iter().enumerate() {
                    if reporting
                        .step(index, files.len())
                        .map_err(|e| e.to_string())?
                    {
                        break;
                    }
                    let _claim = store.media_claims().claim(file.hash);
                    let Some(path) = path(store, file, file.mime) else {
                        continue;
                    };
                    if !path.exists() {
                        continue;
                    }
                    let at = i64::try_from(index)
                        .ok()
                        .and_then(|i| i.checked_mul(*step))
                        .and_then(|offset| milliseconds.checked_add(offset));
                    let Some(when) = at.and_then(timestamp) else {
                        return Err("Modified timestamp is outside the filesystem range".into());
                    };
                    if let Err(error) = attributes(&path).and_then(|file| file.set_modified(when)) {
                        if error.kind() == io::ErrorKind::PermissionDenied {
                            eprintln!(
                                "Could not change modified time of {}: {error}",
                                path.display()
                            );
                        } else {
                            return Err(error.to_string());
                        }
                    }
                }
            }
            Request::Force { files, mime } => {
                let ownership: crate::transfer::MediaOwnership = store
                    .read(crate::settings::get)
                    .map_err(|e| e.to_string())?;
                for (block, files_in_block) in files.chunks(64).enumerate() {
                    if reporting
                        .step(block * 64, files.len())
                        .map_err(|e| e.to_string())?
                    {
                        break;
                    }
                    let ids: Vec<_> = files_in_block.iter().map(|file| file.id).collect();
                    let mime = *mime;
                    store
                        .write_content(move |writer| writer.force_filetype(&ids, mime))
                        .map_err(|e| e.to_string())?;
                    let mut copied = Vec::new();
                    for file in files_in_block {
                        let _claim = store.media_claims().claim(file.hash);
                        let destination = mime.unwrap_or(file.original_mime);
                        let (Some(from), Some(to)) =
                            (path(store, file, file.mime), path(store, file, destination))
                        else {
                            continue;
                        };
                        if from == to {
                            continue;
                        }
                        let copied_file = if ownership.shared_with.is_some() {
                            copy_extension(&from, &to).map_err(|e| e.to_string())?;
                            true
                        } else {
                            move_extension(&from, &to, effects).map_err(|e| e.to_string())?
                        };
                        if copied_file {
                            copied.push(file.id);
                        }
                    }
                    if !copied.is_empty() {
                        let when = effects.now() + 3600;
                        store
                            .write(move |ctx| {
                                file_maintenance::add_jobs(
                                    ctx.conn(),
                                    &copied,
                                    file_maintenance::JobType::DeleteNeighbourDupes,
                                    when,
                                )
                            })
                            .map_err(|e| e.to_string())?;
                    }
                    effects.block_complete();
                }
            }
        }
        Ok(())
    })();
    reporting
        .finish(result.as_ref().err().map(String::as_str))
        .map_err(|e| e.to_string())?;
    result
}
