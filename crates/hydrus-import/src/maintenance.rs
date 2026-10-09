//! Running file maintenance jobs (the reference's
//! `ClientFilesMaintenanceManager._RunJob`): each job reads the file (or
//! its thumbnail) and its result is recorded by
//! [`hydrus_store::file_maintenance::clear_job`].
//!
//! Every job type runs: checking what a file has (metadata flags) and
//! reading its metadata again (renaming it when its type has changed),
//! regenerating its hashes, thumbnail, blurhash and modified time, fixing
//! its permissions, keeping it in or out of the similar-files search,
//! removing stray copies beside it, and the integrity checks, which deal
//! with a missing or damaged file by logging it, asking for it to be
//! downloaded again ([`MaintenanceReport::redownload`]) or removing its
//! record. Media shared with another install is never moved or deleted:
//! a rename or export copies instead.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use hydrus_core::url::UrlType;
use hydrus_core::{ContentStatus, HashId, Mime, Sha256};
use hydrus_media::{MediaError, mimes};
use hydrus_store::file_maintenance::{self, FileMetadata, JobResult, JobType};
use hydrus_store::media::MediaResult;

use crate::{FileImporter, Result};

/// The page hydrus's integrity checks send a bad file's URLs to.
pub const REDOWNLOAD_PAGE_NAME: &str = "missing files redownloader";

/// Where files found to be missing or damaged are recorded, in the store's
/// directory, as the reference keeps them in its database directory.
pub const ERROR_DIR_NAME: &str = "missing_and_invalid_files";

/// What a pass did: jobs done, by type, and their weight.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub done: BTreeMap<JobType, u64>,
    pub weight: u64,
    /// Files the integrity checks found missing or damaged.
    pub bad_files: u64,
    /// URLs to download a missing or damaged file from again, which the
    /// reference adds to its "missing files redownloader" page
    /// ([`REDOWNLOAD_PAGE_NAME`]).
    pub redownload: Vec<String>,
}

/// Forced-pass hooks: metadata commands between fetched batches, gauge before
/// each physical job, and durable results after the file's transaction commits.
pub struct MaintenanceCallbacks<'a> {
    pub before_batch: &'a mut dyn FnMut() -> Result<()>,
    pub before_job: &'a mut dyn FnMut(u64),
    pub committed: &'a mut dyn FnMut(&MaintenanceReport),
}

impl std::fmt::Debug for MaintenanceCallbacks<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaintenanceCallbacks")
            .finish_non_exhaustive()
    }
}

impl MaintenanceReport {
    pub fn total(&self) -> u64 {
        self.done.values().sum()
    }
}

fn now_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// When this process started, as the missing hashes list is named.
static BOOT: LazyLock<String> =
    LazyLock::new(|| jiff::Zoned::now().strftime("%Y-%m-%d %H-%M-%S").to_string());

/// A pass's state shared by its jobs.
struct Pass {
    /// The media belongs to another install.
    shared: bool,
    bad_files: u64,
    redownload: Vec<String>,
}

impl FileImporter {
    /// Run up to `limit` due jobs, stopping once they weigh `max_weight` (a
    /// big job weighs 100), as the reference's throttle does.
    pub fn run_file_maintenance(&self, limit: u64, max_weight: u64) -> Result<MaintenanceReport> {
        self.run_file_maintenance_of(limit, max_weight, &|_| true)
    }

    /// [`Self::run_file_maintenance`] for the job types `wanted` accepts
    /// (as the reference's forced maintenance takes "mandated" types).
    pub fn run_file_maintenance_of(
        &self,
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
    ) -> Result<MaintenanceReport> {
        // Ordinary daemon/CLI work defers on contention. An uncancellable
        // spawn_blocking waiter must not prevent runtime shutdown.
        self.run_file_maintenance_inner::<false, false>(
            limit,
            max_weight,
            wanted,
            None,
            &|| true,
            MaintenanceCallbacks {
                before_batch: &mut || Ok(()),
                before_job: &mut |_| {},
                committed: &mut |_| {},
            },
        )
    }

    /// One batch of due work (one `GetJobs`), as the background manager
    /// takes it: `continue_work` is asked before each file (the throttle),
    /// and `used` is told each file's jobs' weight once they are done. Defers
    /// if the file lease is busy, as ordinary maintenance does.
    pub fn run_file_maintenance_batch(
        &self,
        continue_work: &dyn Fn() -> bool,
        used: &mut dyn FnMut(u64),
    ) -> Result<MaintenanceReport> {
        let mut last = 0;
        self.run_file_maintenance_inner::<false, true>(
            u64::MAX,
            u64::MAX,
            &|_| true,
            None,
            continue_work,
            MaintenanceCallbacks {
                before_batch: &mut || Ok(()),
                before_job: &mut |_| {},
                committed: &mut |report: &MaintenanceReport| {
                    used(report.weight - last);
                    last = report.weight;
                },
            },
        )
    }

    /// One pass of the background file maintenance loop (see
    /// [`hydrus_store::workers::FileMaintenanceThrottle::step`]): a batch of
    /// due jobs as the throttle allows, handing missing files that could be
    /// downloaded again to `redownload`. Returns the wait before the next pass.
    pub fn file_maintenance_pass(
        &self,
        throttle: &hydrus_store::workers::FileMaintenanceThrottle,
        clock: &dyn hydrus_store::workers::WorkClock,
        redownload: &dyn Fn(&[String]),
    ) -> std::time::Duration {
        throttle.step(&self.store, clock, |able, used| {
            let report = self.run_file_maintenance_batch(able, used)?;
            if !report.redownload.is_empty() {
                redownload(&report.redownload);
            }
            Ok::<_, crate::ImportError>(report.total())
        })
    }

    /// Run only captured files, as the thumbnail menu's `RunJobImmediately`
    /// does. Selection applies at queue admission, before any batch limit.
    /// Like ordinary maintenance, this pass defers if the file lease is busy.
    pub fn run_file_maintenance_for_files(
        &self,
        files: &[HashId],
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
    ) -> Result<MaintenanceReport> {
        self.run_file_maintenance_inner::<false, false>(
            limit,
            max_weight,
            wanted,
            Some(files),
            &|| true,
            MaintenanceCallbacks {
                before_batch: &mut || Ok(()),
                before_job: &mut |_| {},
                committed: &mut |_| {},
            },
        )
    }

    /// A forced GUI pass. Cancellation is checked between files, as `_RunJob`
    /// does; progress is published after each file's results are committed.
    pub fn run_file_maintenance_controlled(
        &self,
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
        continue_work: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(&MaintenanceReport),
    ) -> Result<MaintenanceReport> {
        self.run_file_maintenance_with_callbacks(
            limit,
            max_weight,
            wanted,
            continue_work,
            MaintenanceCallbacks {
                before_batch: &mut || Ok(()),
                before_job: &mut |_| {},
                committed: progress,
            },
        )
    }

    /// Force work with a genuine pre-job gauge and between-batch command hook.
    /// Hooks execute off UI while this pass owns the physical-work lease.
    pub fn run_file_maintenance_with_callbacks(
        &self,
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
        continue_work: &dyn Fn() -> bool,
        callbacks: MaintenanceCallbacks<'_>,
    ) -> Result<MaintenanceReport> {
        self.run_file_maintenance_inner::<true, false>(
            limit,
            max_weight,
            wanted,
            None,
            continue_work,
            callbacks,
        )
    }

    /// A selected forced pass with the same lease wait, cancellation and commit
    /// hooks as the global forced pass. Other files' due work stays queued.
    pub fn run_file_maintenance_for_files_with_callbacks(
        &self,
        files: &[HashId],
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
        continue_work: &dyn Fn() -> bool,
        callbacks: MaintenanceCallbacks<'_>,
    ) -> Result<MaintenanceReport> {
        self.run_file_maintenance_inner::<true, false>(
            limit,
            max_weight,
            wanted,
            Some(files),
            continue_work,
            callbacks,
        )
    }

    fn run_file_maintenance_inner<const WAIT: bool, const ONE_BATCH: bool>(
        &self,
        limit: u64,
        max_weight: u64,
        wanted: &dyn Fn(JobType) -> bool,
        files: Option<&[HashId]>,
        continue_work: &dyn Fn() -> bool,
        callbacks: MaintenanceCallbacks<'_>,
    ) -> Result<MaintenanceReport> {
        // File work happens outside the writer. The crash-safe file lease also
        // excludes the independent daemon's ordinary maintenance pass.
        let MaintenanceCallbacks {
            before_batch,
            before_job,
            committed,
        } = callbacks;
        let _lease = loop {
            // (the background manager asks its throttle before each file, not
            // before it knows there is a job due)
            if !ONE_BATCH && !continue_work() {
                return Ok(MaintenanceReport::default());
            }
            if let Some(lease) = hydrus_store::store::lock_file_maintenance(self.store.dir())? {
                break lease;
            }
            if !WAIT {
                return Ok(MaintenanceReport::default());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let ownership: hydrus_store::transfer::MediaOwnership =
            self.store.read(hydrus_store::settings::get)?;
        let mut pass = Pass {
            shared: ownership.shared_with.is_some(),
            bad_files: 0,
            redownload: Vec::new(),
        };
        let mut report = MaintenanceReport::default();
        let mut attempted = 0;
        while report.total() < limit && report.weight < max_weight {
            (before_batch)()?;
            let due: Vec<(HashId, Vec<JobType>)> = self.store.read(|conn| match files {
                Some(files) => file_maintenance::due_jobs_of_files(conn, now_s(), wanted, files),
                None => file_maintenance::due_jobs_of(conn, now_s(), wanted),
            })?;
            if due.is_empty() {
                break;
            }
            let ids: Vec<HashId> = due.iter().map(|(id, _)| *id).collect();
            let media: BTreeMap<HashId, MediaResult> = self
                .store
                .read(|conn| hydrus_store::media::load_basic(conn, &ids))?
                .into_iter()
                .map(|m| (m.hash_id, m))
                .collect();
            for (hash_id, jobs) in due {
                if !continue_work() {
                    report.bad_files = pass.bad_files;
                    report.redownload = pass.redownload;
                    return Ok(report);
                }
                let mut results = Vec::new();
                let mut weight = report.weight;
                for job in jobs {
                    if report.total() + results.len() as u64 >= limit || weight >= max_weight {
                        break;
                    }
                    weight += job.weight();
                    attempted += 1;
                    (before_job)(attempted);
                    let result = match media.get(&hash_id) {
                        Some(m) => self.run_job(m, job, &mut pass)?,
                        None => JobResult::Nothing,
                    };
                    results.push((job, result));
                    // (the file has just changed: its other jobs wait for
                    // the next pass)
                    if job == JobType::FileMetadata {
                        break;
                    }
                }
                if results.is_empty() {
                    break;
                }
                for (job, _) in &results {
                    *report.done.entry(*job).or_default() += 1;
                }
                report.weight = weight;
                self.store.write(move |ctx| {
                    for (job, result) in &results {
                        file_maintenance::clear_job(ctx.conn(), hash_id, *job, result)?;
                    }
                    Ok(())
                })?;
                report.bad_files = pass.bad_files;
                report.redownload.clone_from(&pass.redownload);
                (committed)(&report);
            }
            if ONE_BATCH {
                break;
            }
        }
        report.bad_files = pass.bad_files;
        report.redownload = pass.redownload;
        Ok(report)
    }

    /// The file, if it is on disk.
    fn stored_file(&self, media: &MediaResult) -> Option<(PathBuf, Mime)> {
        let mime = media.info.as_ref()?.mime;
        let path = self.store.snapshot().storage.file_path(&media.hash, mime)?;
        path.is_file().then_some((path, mime))
    }

    /// One job's work (`_RunJob`'s branches).
    fn run_job(&self, media: &MediaResult, job: JobType, pass: &mut Pass) -> Result<JobResult> {
        let Some(info) = &media.info else {
            return Ok(JobResult::Nothing);
        };
        let mime = info.mime;
        let is_update = matches!(
            mime,
            Mime::ApplicationHydrusUpdateContent | Mime::ApplicationHydrusUpdateDefinitions
        );
        if let Some(flag) = job.flag() {
            // (a type that can't have it hasn't, without reading the file)
            let can = match job {
                JobType::HasTransparency => mimes::can_check_transparency(mime),
                JobType::HasExif => mimes::can_have_exif(mime),
                JobType::HasXmp => mimes::can_have_xmp(mime),
                JobType::HasIptc => mimes::can_have_iptc(mime),
                JobType::HasSoftwareSource => mimes::can_have_software_source(mime),
                JobType::HasIccProfile => mimes::can_have_icc_profile(mime),
                _ => mimes::can_have_human_readable_embedded_metadata(mime),
            };
            if !can {
                return Ok(JobResult::Flag(false));
            }
            let Some((path, _)) = self.stored_file(media) else {
                return Ok(JobResult::Nothing);
            };
            let flags = self.tools().flags(&path, &media_info(info));
            let has = match flag {
                hydrus_store::media::FileFlags::TRANSPARENCY => flags.has_transparency,
                hydrus_store::media::FileFlags::EXIF => flags.has_exif,
                hydrus_store::media::FileFlags::XMP => flags.has_xmp,
                hydrus_store::media::FileFlags::IPTC => flags.has_iptc,
                hydrus_store::media::FileFlags::SOFTWARE_SOURCE => flags.has_software_source,
                hydrus_store::media::FileFlags::ICC_PROFILE => flags.has_icc_profile,
                _ => flags.has_human_readable_embedded_metadata,
            };
            return Ok(JobResult::Flag(has));
        }
        Ok(match job {
            JobType::FileMetadata => self.regen_file_metadata(media, pass)?,
            JobType::CheckSimilarFilesMembership => {
                JobResult::SimilarFilesMembership(mimes::has_perceptual_hash(mime))
            }
            JobType::PerceptualHashes => {
                if !mimes::has_perceptual_hash(mime) {
                    JobResult::PerceptualHashes(Vec::new())
                } else if let Some((path, _)) = self.stored_file(media) {
                    JobResult::PerceptualHashes(
                        self.tools()
                            .perceptual_hashes(&path, mime)
                            .iter()
                            .map(|p| p.0.to_vec())
                            .collect(),
                    )
                } else {
                    JobResult::Nothing
                }
            }
            JobType::PixelHash => match self.stored_file(media) {
                Some((path, _)) => self
                    .tools()
                    .pixel_hash(&path, &media_info(info))
                    .map_or(JobResult::Nothing, JobResult::PixelHash),
                None => JobResult::Nothing,
            },
            JobType::OtherHashes if !is_update => match self
                .stored_file(media)
                .and_then(|(path, _)| hydrus_media::hash_file(&path).ok())
            {
                Some(hashes) => JobResult::OtherHashes {
                    md5: hashes.md5.0.to_vec(),
                    sha1: hashes.sha1.0.to_vec(),
                    sha512: hashes.sha512.0.to_vec(),
                },
                None => JobResult::Nothing,
            },
            JobType::FileModifiedTimestamp if !is_update => {
                match self
                    .stored_file(media)
                    .and_then(|(path, _)| std::fs::metadata(path).and_then(|m| m.modified()).ok())
                {
                    Some(modified) => JobResult::FileModified(millis(modified)),
                    None => JobResult::Nothing,
                }
            }
            JobType::FixPermissions => {
                if let Some((path, _)) = self.stored_file(media) {
                    crate::paths::give_nice_permission_bits(&path);
                }
                JobResult::Nothing
            }
            JobType::Blurhash => self.blurhash(media, mime),
            JobType::ForceThumbnail => match self.regenerate_thumbnail_of(media.hash_id) {
                Ok(true) => JobResult::Thumbnail(true),
                _ => JobResult::Nothing,
            },
            JobType::RefitThumbnail => JobResult::Thumbnail(self.refit_thumbnail(media)),
            JobType::DeleteNeighbourDupes => {
                self.delete_neighbour_dupes(media, pass)?;
                JobResult::Nothing
            }
            JobType::IntegrityPresenceRemoveRecord
            | JobType::IntegrityPresenceDeleteRecord
            | JobType::IntegrityPresenceTryUrl
            | JobType::IntegrityPresenceTryUrlElseRemoveRecord
            | JobType::IntegrityDataRemoveRecord
            | JobType::IntegrityDataTryUrl
            | JobType::IntegrityDataTryUrlElseRemoveRecord
            | JobType::IntegrityDataSilentDelete
            | JobType::IntegrityPresenceLogOnly => {
                self.check_integrity(media, job, pass)?;
                JobResult::Nothing
            }
            _ => JobResult::Nothing,
        })
    }

    /// `_RegenFileMetadata`: read the file's metadata again. A file whose
    /// type has changed (and wasn't forced) is renamed to its new
    /// extension; one that can no longer be read gets the integrity check
    /// that tries its URLs, else removes its record.
    fn regen_file_metadata(&self, media: &MediaResult, pass: &mut Pass) -> Result<JobResult> {
        let Some(info) = &media.info else {
            return Ok(JobResult::Nothing);
        };
        let Some((path, _)) = self.stored_file(media) else {
            return Ok(JobResult::Nothing);
        };
        let read = self
            .tools()
            .detect_mime_allowing_updates(&path)
            .and_then(|mime| self.tools().inspect_as(&path, mime));
        let found = match read {
            Ok(found) => found,
            Err(MediaError::Unsupported { .. } | MediaError::ZeroSize | MediaError::Damaged(_)) => {
                self.check_integrity(media, JobType::IntegrityDataTryUrlElseRemoveRecord, pass)?;
                return Ok(JobResult::Nothing);
            }
            Err(e) => {
                tracing::error!(hash = %media.hash, error = %e, "file maintenance could not read a file's metadata");
                return Ok(JobResult::Nothing);
            }
        };
        if found.mime != info.mime && info.original_mime.is_none() {
            self.change_file_ext(
                media.hash_id,
                &media.hash,
                info.mime,
                found.mime,
                pass.shared,
            )?;
        }
        Ok(JobResult::FileMetadata(FileMetadata {
            size: found.size,
            mime: found.mime,
            width: found.width,
            height: found.height,
            duration_ms: found.duration_ms,
            num_frames: found.num_frames,
            has_audio: found.has_audio,
            num_words: found.num_words,
        }))
    }

    /// `ChangeFileExt`: move the file to its new type's name, or copy it
    /// when it can't be moved (or belongs to another install). A copy left
    /// behind here is cleared up an hour later.
    fn change_file_ext(
        &self,
        hash_id: HashId,
        hash: &Sha256,
        old: Mime,
        new: Mime,
        shared: bool,
    ) -> Result<()> {
        let storage = &self.store.snapshot().storage;
        let (Some(old_path), Some(new_path)) =
            (storage.file_path(hash, old), storage.file_path(hash, new))
        else {
            return Ok(());
        };
        if old_path == new_path {
            return Ok(());
        }
        let _claim = self.store.media_claims().claim(*hash);
        let moved = !shared && std::fs::rename(&old_path, &new_path).is_ok();
        if !moved {
            hydrus_store::paths::copy_file(&old_path, &new_path)?;
            if !shared {
                self.store.write(move |ctx| {
                    file_maintenance::add_jobs(
                        ctx.conn(),
                        &[hash_id],
                        JobType::DeleteNeighbourDupes,
                        now_s() + 3600,
                    )
                })?;
            }
        }
        Ok(())
    }

    /// `DeleteNeighbourDupes`: remove copies of the file under other types'
    /// names, once it is where its type says.
    fn delete_neighbour_dupes(&self, media: &MediaResult, pass: &Pass) -> Result<()> {
        if pass.shared {
            return Ok(());
        }
        let Some((correct, mime)) = self.stored_file(media) else {
            return Ok(());
        };
        let storage = &self.store.snapshot().storage;
        let _claim = self.store.media_claims().claim(media.hash);
        let mut failed = false;
        for &other in Mime::ALL {
            if other == mime || !mimes::is_allowed(other) {
                continue;
            }
            let Some(path) = storage.file_path(&media.hash, other) else {
                continue;
            };
            if path != correct && path.exists() && std::fs::remove_file(&path).is_err() {
                failed = true;
            }
        }
        if failed {
            let hash_id = media.hash_id;
            self.store.write(move |ctx| {
                file_maintenance::add_jobs(
                    ctx.conn(),
                    &[hash_id],
                    JobType::DeleteNeighbourDupes,
                    now_s() + 7 * 86_400,
                )
            })?;
        }
        Ok(())
    }

    /// Regenerate a file's thumbnail; whether it was (the file is here).
    fn regenerate_thumbnail_of(&self, hash_id: HashId) -> Result<bool> {
        // (the thumbnail needs the file's locations, so the whole result)
        let full = self.store.read(|conn| {
            hydrus_store::media::load(conn, &self.store.snapshot().services, None, &[hash_id])
        })?;
        Ok(matches!(
            full.results.first().map(|m| self.regenerate_thumbnail(m)),
            Some(Ok(_))
        ))
    }

    /// `_RegenFileThumbnailRefit`: make the thumbnail again if it isn't the
    /// size the thumbnail settings give (or can't be read); whether it was.
    fn refit_thumbnail(&self, media: &MediaResult) -> bool {
        let Some(info) = &media.info else {
            return false;
        };
        if !info.mime.has_thumbnail() {
            return false;
        }
        let always_good_resolution = mimes::is_image(info.mime)
            || mimes::is_animation(info.mime)
            || mimes::is_video(info.mime);
        if always_good_resolution && (info.width.is_none() || info.height.is_none()) {
            // (waiting for its metadata to be read again)
            return false;
        }
        let snap = self.store.snapshot();
        let right_size = (|| {
            let path = snap.storage.thumbnail_path(&media.hash)?;
            if !path.is_file() {
                return None;
            }
            let thumb_mime = self.tools().detect_mime(&path).ok()?;
            let image = self.tools().load_image(&path, thumb_mime).ok()?;
            let expected = hydrus_media::thumbnail_resolution(
                info.width,
                info.height,
                &crate::thumbnail_spec(&snap.thumbnails),
            )?;
            Some((image.width(), image.height()) == expected)
        })();
        if right_size == Some(true) {
            return false;
        }
        self.regenerate_thumbnail_of(media.hash_id).unwrap_or(false)
    }

    /// `_CheckFileIntegrity`: whether the file is missing, or (for the
    /// data checks) its content no longer has its hash. A bad file is
    /// recorded in the error directory with its tags and URLs, and then,
    /// as `job` says, exported there, downloaded again from its URLs, or
    /// its record removed (sent to the trash instead when the delete lock
    /// holds it). Whether the file was bad.
    fn check_integrity(&self, media: &MediaResult, job: JobType, pass: &mut Pass) -> Result<bool> {
        let Some(info) = &media.info else {
            return Ok(false);
        };
        let snap = self.store.snapshot();
        let path = snap.storage.file_path(&media.hash, info.mime);
        let missing = !path.as_deref().is_some_and(Path::is_file);
        if missing {
            tracing::warn!("Missing file: {}!", media.hash);
        }
        let data_check = matches!(
            job,
            JobType::IntegrityDataRemoveRecord
                | JobType::IntegrityDataTryUrl
                | JobType::IntegrityDataTryUrlElseRemoveRecord
                | JobType::IntegrityDataSilentDelete
        );
        let mut invalid = false;
        if let Some(path) = path.as_deref().filter(|_| !missing && data_check) {
            let actual = hydrus_media::hash_file(path)
                .map_err(|e| std::io::Error::other(e.to_string()))?
                .sha256;
            if actual != media.hash {
                tracing::warn!("Invalid file: {} actually had hash {actual}!", media.hash);
                invalid = true;
            }
        }
        if !missing && !invalid {
            return Ok(false);
        }
        pass.bad_files += 1;
        let error_dir = self.store.dir().join(ERROR_DIR_NAME);
        std::fs::create_dir_all(&error_dir)?;
        let hex = media.hash.to_hex();
        append_lines(
            &error_dir.join(format!("{} missing hashes.txt", *BOOT)),
            std::iter::once(hex.as_str()),
        )?;
        let full = self
            .store
            .read(|conn| hydrus_store::media::load(conn, &snap.services, None, &[media.hash_id]))?;
        let (tags, urls) = match full.results.first() {
            Some(m) => {
                let tags: BTreeSet<String> = m
                    .tags
                    .values()
                    .flat_map(|t| {
                        [ContentStatus::Current, ContentStatus::Pending]
                            .into_iter()
                            .filter_map(|s| t.by_status.get(&s))
                            .flatten()
                    })
                    .filter_map(|id| full.tags.get(id))
                    .map(ToString::to_string)
                    .collect();
                (tags, m.urls.clone())
            }
            None => (BTreeSet::new(), Vec::new()),
        };
        if !tags.is_empty()
            && let Err(e) = write_lines(
                &error_dir.join(format!("{hex}.tags.txt")),
                tags.iter().map(String::as_str),
            )
        {
            tracing::warn!(hash = %media.hash, error = %e, "could not export a missing file's tags");
        }
        if !urls.is_empty() {
            let written = write_lines(
                &error_dir.join(format!("{hex}.urls.txt")),
                urls.iter().map(String::as_str),
            )
            .and_then(|()| {
                append_lines(
                    &error_dir.join("all_urls.txt"),
                    urls.iter().map(String::as_str),
                )
            });
            if let Err(e) = written {
                tracing::warn!(hash = %media.hash, error = %e, "could not export a missing file's URLs");
            }
        }
        let classes = &snap.url_classes;
        let useful: Vec<String> = urls
            .into_iter()
            .filter(|url| match classes.class_for(url).map(|c| c.url_type) {
                None | Some(UrlType::File) => true,
                Some(UrlType::Post) => classes.parse_capability(url).parser.is_ok(),
                Some(_) => false,
            })
            .collect();
        let (try_redownload, delete_record) = match job {
            JobType::IntegrityPresenceLogOnly => (false, false),
            JobType::IntegrityPresenceTryUrlElseRemoveRecord
            | JobType::IntegrityDataTryUrlElseRemoveRecord => {
                (!useful.is_empty(), useful.is_empty())
            }
            _ => (
                matches!(
                    job,
                    JobType::IntegrityPresenceTryUrl | JobType::IntegrityDataTryUrl
                ) && !useful.is_empty(),
                matches!(
                    job,
                    JobType::IntegrityPresenceRemoveRecord
                        | JobType::IntegrityPresenceDeleteRecord
                        | JobType::IntegrityDataRemoveRecord
                ),
            ),
        };
        let export = invalid
            && (matches!(
                job,
                JobType::IntegrityDataRemoveRecord
                    | JobType::IntegrityDataTryUrlElseRemoveRecord
                    | JobType::IntegrityDataSilentDelete
            ) || (job == JobType::IntegrityDataTryUrl && try_redownload));
        if export && let Some(path) = &path {
            let dest = error_dir.join(path.file_name().unwrap_or_default());
            let _claim = self.store.media_claims().claim(media.hash);
            let moved = !pass.shared && std::fs::rename(path, &dest).is_ok();
            if !moved {
                hydrus_store::paths::copy_file(path, &dest).map_err(|e| {
                    std::io::Error::other(format!(
                        "Could not move the damaged file \"{}\" to \"{}\"! ({e})",
                        path.display(),
                        dest.display()
                    ))
                })?;
                if !pass.shared {
                    std::fs::remove_file(path)?;
                }
            }
            tracing::warn!(
                "During file maintenance, a file was found to be invalid. It and any known URLs have been moved to \"{}\".",
                error_dir.display()
            );
        }
        if try_redownload {
            pass.redownload.extend(useful);
        }
        if delete_record {
            let leave_record = job == JobType::IntegrityPresenceDeleteRecord;
            let hash_id = media.hash_id;
            let hash = media.hash;
            self.store.write_content(move |w| {
                let roles = w.roles().clone();
                let locked = !hydrus_store::delete_lock::locked(
                    w.conn(),
                    roles.local_file_storage,
                    &[hash_id],
                )?
                .is_empty();
                let leave = if leave_record { "a" } else { "no" };
                if locked {
                    let reason = format!(
                        "Wanted to delete record during File Integrity check, but file was physical delete locked. Wanted to leave {leave} deletion record."
                    );
                    w.delete_files(roles.combined_local_media, &[hash_id], Some(&reason))?;
                    tracing::warn!(
                        "During file maintenance, failed to physically delete {hash}! Wanted to leave {leave} deletion record. It appears to be archived and the archived file delete lock is on, so it has been sent to the trash instead."
                    );
                } else {
                    w.delete_files(
                        roles.local_file_storage,
                        &[hash_id],
                        Some("Record deleted during File Integrity check."),
                    )?;
                    tracing::warn!("During file maintenance, physically deleted {hash}!");
                    if !leave_record {
                        w.clear_local_delete_records(Some(&[hash_id]))?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(true)
    }

    /// `_RegenBlurhash`: from the file's thumbnail.
    fn blurhash(&self, media: &MediaResult, mime: Mime) -> JobResult {
        if !mimes::has_thumbnail(mime) {
            return JobResult::Nothing;
        }
        let Some(path) = self.store.snapshot().storage.thumbnail_path(&media.hash) else {
            return JobResult::Nothing;
        };
        let Ok(thumb_mime) = self.tools().detect_mime(&path) else {
            return JobResult::Nothing;
        };
        self.tools()
            .load_image(&path, thumb_mime)
            .ok()
            .and_then(|image| hydrus_media::blurhash(&image))
            .map_or(JobResult::Nothing, JobResult::Blurhash)
    }
}

/// Write `lines`, each ended by a newline.
fn write_lines<'a>(path: &Path, lines: impl Iterator<Item = &'a str>) -> std::io::Result<()> {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    std::fs::write(path, out)
}

/// Append `lines`, each ended by a newline.
fn append_lines<'a>(path: &Path, lines: impl Iterator<Item = &'a str>) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for line in lines {
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

fn media_info(info: &hydrus_store::media::FileInfo) -> hydrus_media::FileInfo {
    hydrus_media::FileInfo {
        mime: info.mime,
        size: info.size,
        width: info.width,
        height: info.height,
        duration_ms: info.duration_ms,
        num_frames: info.num_frames,
        has_audio: info.has_audio,
        num_words: info.num_words,
    }
}

/// `HydrusTime.MillisecondiseS(os.path.getmtime(path))`.
fn millis(t: std::time::SystemTime) -> i64 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}
