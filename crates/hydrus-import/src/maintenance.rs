//! Running file maintenance jobs (the reference's
//! `ClientFilesMaintenanceManager._RunJob`): each job reads the file (or
//! its thumbnail) and its result is recorded by
//! [`hydrus_store::file_maintenance::clear_job`].
//!
//! The jobs that check what a file has (metadata flags), regenerate its
//! hashes and blurhash, its thumbnail and modified time, fix its
//! permissions, and keep it in or out of the similar-files search run here.
//! Integrity checks, re-downloads, file metadata and thumbnail refits stay
//! queued until they are ported.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hydrus_core::{HashId, Mime};
use hydrus_media::mimes;
use hydrus_store::file_maintenance::{self, JobResult, JobType};
use hydrus_store::media::MediaResult;

use crate::{FileImporter, Result};

/// Whether this build runs `job` (the rest wait in the queue).
pub fn runs(job: JobType) -> bool {
    matches!(
        job,
        JobType::ForceThumbnail
            | JobType::OtherHashes
            | JobType::FixPermissions
            | JobType::CheckSimilarFilesMembership
            | JobType::PerceptualHashes
            | JobType::FileModifiedTimestamp
            | JobType::HasIccProfile
            | JobType::PixelHash
            | JobType::HasHumanReadableEmbeddedMetadata
            | JobType::HasExif
            | JobType::Blurhash
            | JobType::HasTransparency
            | JobType::HasXmp
            | JobType::HasIptc
            | JobType::HasSoftwareSource
    )
}

/// What a pass did: jobs done, by type, and their weight.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub done: BTreeMap<JobType, u64>,
    pub weight: u64,
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

impl FileImporter {
    /// Run up to `limit` due jobs of the kinds this build runs, stopping
    /// once they weigh `max_weight` (a big job weighs 100), as the
    /// reference's throttle does.
    pub fn run_file_maintenance(&self, limit: u64, max_weight: u64) -> Result<MaintenanceReport> {
        let mut report = MaintenanceReport::default();
        while report.total() < limit && report.weight < max_weight {
            let due: Vec<(HashId, Vec<JobType>)> = self
                .store
                .read(|conn| file_maintenance::due_jobs_of(conn, now_s(), &runs))?;
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
                let mut results = Vec::new();
                let mut weight = report.weight;
                for job in jobs {
                    if report.total() + results.len() as u64 >= limit || weight >= max_weight {
                        break;
                    }
                    weight += job.weight();
                    let result = match media.get(&hash_id) {
                        Some(m) => self.run_job(m, job)?,
                        None => JobResult::Nothing,
                    };
                    results.push((job, result));
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
            }
        }
        Ok(report)
    }

    /// The file, if it is on disk.
    fn stored_file(&self, media: &MediaResult) -> Option<(PathBuf, Mime)> {
        let mime = media.info.as_ref()?.mime;
        let path = self.store.snapshot().storage.file_path(&media.hash, mime)?;
        path.is_file().then_some((path, mime))
    }

    /// One job's work (`_RunJob`'s branches).
    fn run_job(&self, media: &MediaResult, job: JobType) -> Result<JobResult> {
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
            JobType::ForceThumbnail => {
                // (the thumbnail needs the file's locations, so the whole result)
                let full = self.store.read(|conn| {
                    hydrus_store::media::load(
                        conn,
                        &self.store.snapshot().services,
                        None,
                        &[media.hash_id],
                    )
                })?;
                match full.results.first().map(|m| self.regenerate_thumbnail(m)) {
                    Some(Ok(Some(_))) => JobResult::Thumbnail(true),
                    _ => JobResult::Nothing,
                }
            }
            _ => JobResult::Nothing,
        })
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
