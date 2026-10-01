//! The file maintenance queue (the reference's
//! `ClientDBFilesMaintenanceQueue`, and `ClientDBFilesMaintenance.ClearJobs`
//! for what a finished job changes): work queued per file, such as checking
//! a file's metadata flags or regenerating its hashes, done in the
//! background. The work itself, which reads the files, is in
//! `hydrus-import`'s maintenance runner.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::{HashId, Sha256};

use crate::error::Result;
use crate::media::FileFlags;

/// A kind of job (`ClientFilesMaintenance.REGENERATE_FILE_DATA_JOB_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JobType {
    FileMetadata,
    ForceThumbnail,
    RefitThumbnail,
    OtherHashes,
    DeleteNeighbourDupes,
    IntegrityPresenceRemoveRecord,
    IntegrityDataRemoveRecord,
    FixPermissions,
    CheckSimilarFilesMembership,
    PerceptualHashes,
    FileModifiedTimestamp,
    IntegrityPresenceTryUrl,
    IntegrityDataTryUrl,
    IntegrityDataSilentDelete,
    IntegrityPresenceTryUrlElseRemoveRecord,
    IntegrityDataTryUrlElseRemoveRecord,
    HasIccProfile,
    PixelHash,
    IntegrityPresenceLogOnly,
    HasHumanReadableEmbeddedMetadata,
    HasExif,
    IntegrityPresenceDeleteRecord,
    Blurhash,
    HasTransparency,
    HasXmp,
    HasIptc,
    HasSoftwareSource,
}

impl JobType {
    /// Every job type, by code.
    pub const ALL: [JobType; 27] = [
        Self::FileMetadata,
        Self::ForceThumbnail,
        Self::RefitThumbnail,
        Self::OtherHashes,
        Self::DeleteNeighbourDupes,
        Self::IntegrityPresenceRemoveRecord,
        Self::IntegrityDataRemoveRecord,
        Self::FixPermissions,
        Self::CheckSimilarFilesMembership,
        Self::PerceptualHashes,
        Self::FileModifiedTimestamp,
        Self::IntegrityPresenceTryUrl,
        Self::IntegrityDataTryUrl,
        Self::IntegrityDataSilentDelete,
        Self::IntegrityPresenceTryUrlElseRemoveRecord,
        Self::IntegrityDataTryUrlElseRemoveRecord,
        Self::HasIccProfile,
        Self::PixelHash,
        Self::IntegrityPresenceLogOnly,
        Self::HasHumanReadableEmbeddedMetadata,
        Self::HasExif,
        Self::IntegrityPresenceDeleteRecord,
        Self::Blurhash,
        Self::HasTransparency,
        Self::HasXmp,
        Self::HasIptc,
        Self::HasSoftwareSource,
    ];

    /// The order a file's jobs run in (`ALL_REGEN_JOBS_IN_RUN_ORDER`).
    pub const RUN_ORDER: [JobType; 27] = [
        Self::IntegrityPresenceTryUrlElseRemoveRecord,
        Self::IntegrityPresenceTryUrl,
        Self::IntegrityDataTryUrlElseRemoveRecord,
        Self::IntegrityDataTryUrl,
        Self::IntegrityPresenceRemoveRecord,
        Self::IntegrityPresenceDeleteRecord,
        Self::IntegrityDataRemoveRecord,
        Self::IntegrityDataSilentDelete,
        Self::IntegrityPresenceLogOnly,
        Self::FileMetadata,
        Self::RefitThumbnail,
        Self::ForceThumbnail,
        Self::Blurhash,
        Self::PerceptualHashes,
        Self::CheckSimilarFilesMembership,
        Self::FixPermissions,
        Self::FileModifiedTimestamp,
        Self::OtherHashes,
        Self::HasTransparency,
        Self::HasExif,
        Self::HasXmp,
        Self::HasIptc,
        Self::HasHumanReadableEmbeddedMetadata,
        Self::HasSoftwareSource,
        Self::HasIccProfile,
        Self::PixelHash,
        Self::DeleteNeighbourDupes,
    ];

    /// The reference's code.
    pub fn code(self) -> i64 {
        Self::ALL.iter().position(|&j| j == self).unwrap_or(0) as i64
    }

    pub fn from_code(code: i64) -> Option<Self> {
        usize::try_from(code)
            .ok()
            .and_then(|i| Self::ALL.get(i))
            .copied()
    }

    /// As the reference describes it (`regen_file_enum_to_str_lookup`).
    pub fn description(self) -> &'static str {
        match self {
            Self::FileMetadata => "regenerate file metadata",
            Self::ForceThumbnail => "regenerate thumbnail",
            Self::RefitThumbnail => "regenerate thumbnail if incorrect size",
            Self::OtherHashes => "regenerate non-standard hashes",
            Self::DeleteNeighbourDupes => {
                "delete duplicate neighbours with incorrect file extension"
            }
            Self::IntegrityPresenceRemoveRecord => {
                "if file is missing, remove record (leave no delete record)"
            }
            Self::IntegrityPresenceDeleteRecord => {
                "if file is missing, remove record (leave a delete record)"
            }
            Self::IntegrityPresenceTryUrl => {
                "if file is missing, then if has URL try to redownload"
            }
            Self::IntegrityPresenceTryUrlElseRemoveRecord => {
                "if file is missing, then if has URL try to redownload, else remove record"
            }
            Self::IntegrityPresenceLogOnly => "if file is missing, note it in log",
            Self::IntegrityDataRemoveRecord => {
                "if file is missing/incorrect, move file out and remove record"
            }
            Self::IntegrityDataTryUrl => {
                "if file is missing/incorrect, then move file out, and if has URL try to redownload"
            }
            Self::IntegrityDataTryUrlElseRemoveRecord => {
                "if file is missing/incorrect, then move file out, and if has URL try to redownload, else remove record"
            }
            Self::IntegrityDataSilentDelete => "if file is incorrect, move file out",
            Self::FixPermissions => "fix file read/write permissions",
            Self::CheckSimilarFilesMembership => {
                "check for membership in the potential duplicate pairs search system"
            }
            Self::PerceptualHashes => "regenerate perceptual hashes",
            Self::FileModifiedTimestamp => "regenerate file modified time",
            Self::HasTransparency => "determine if the file has transparency",
            Self::HasExif => "determine if the file has EXIF metadata",
            Self::HasXmp => "determine if the file has XMP metadata",
            Self::HasIptc => "determine if the file has IPTC metadata",
            Self::HasHumanReadableEmbeddedMetadata => {
                "determine if the file has human-readable metadata"
            }
            Self::HasSoftwareSource => "determine if the file has software/source metadata",
            Self::HasIccProfile => "determine if the file has an icc profile",
            Self::PixelHash => "regenerate pixel hashes",
            Self::Blurhash => "regenerate blurhash",
        }
    }

    /// Jobs this one makes unnecessary (`regen_file_enum_to_overruled_jobs`).
    pub fn overruled(self) -> &'static [JobType] {
        match self {
            Self::ForceThumbnail => &[Self::RefitThumbnail],
            Self::IntegrityPresenceDeleteRecord
            | Self::IntegrityPresenceRemoveRecord
            | Self::IntegrityPresenceTryUrl => &[Self::IntegrityPresenceLogOnly],
            Self::IntegrityPresenceTryUrlElseRemoveRecord => &[
                Self::IntegrityPresenceLogOnly,
                Self::IntegrityPresenceTryUrl,
                Self::IntegrityPresenceRemoveRecord,
            ],
            Self::IntegrityDataRemoveRecord => &[
                Self::IntegrityPresenceLogOnly,
                Self::IntegrityPresenceRemoveRecord,
            ],
            Self::IntegrityDataTryUrl => &[
                Self::IntegrityPresenceLogOnly,
                Self::IntegrityPresenceTryUrl,
            ],
            Self::IntegrityDataTryUrlElseRemoveRecord => &[
                Self::IntegrityPresenceLogOnly,
                Self::IntegrityPresenceTryUrl,
                Self::IntegrityPresenceRemoveRecord,
                Self::IntegrityDataTryUrl,
                Self::IntegrityDataRemoveRecord,
            ],
            // (as the reference says, regenerating them does that job)
            Self::PerceptualHashes => &[Self::CheckSimilarFilesMembership],
            _ => &[],
        }
    }

    /// How much work it is, a big job being 100
    /// (`regen_file_enum_to_job_weight_lookup`).
    pub fn weight(self) -> u64 {
        match self {
            Self::FileMetadata
            | Self::OtherHashes
            | Self::IntegrityDataRemoveRecord
            | Self::IntegrityDataTryUrl
            | Self::IntegrityDataTryUrlElseRemoveRecord
            | Self::IntegrityDataSilentDelete
            | Self::PerceptualHashes
            | Self::PixelHash => 100,
            Self::ForceThumbnail => 50,
            Self::IntegrityPresenceTryUrlElseRemoveRecord => 30,
            Self::RefitThumbnail
            | Self::DeleteNeighbourDupes
            | Self::IntegrityPresenceTryUrl
            | Self::FixPermissions
            | Self::HasTransparency
            | Self::HasExif
            | Self::HasXmp
            | Self::HasIptc
            | Self::HasHumanReadableEmbeddedMetadata
            | Self::HasSoftwareSource
            | Self::HasIccProfile => 25,
            Self::Blurhash => 15,
            Self::FileModifiedTimestamp => 10,
            Self::IntegrityPresenceRemoveRecord
            | Self::IntegrityPresenceDeleteRecord
            | Self::IntegrityPresenceLogOnly => 5,
            Self::CheckSimilarFilesMembership => 1,
        }
    }

    /// The metadata flag a "determine if the file has" job checks.
    pub fn flag(self) -> Option<u32> {
        Some(match self {
            Self::HasExif => FileFlags::EXIF,
            Self::HasIccProfile => FileFlags::ICC_PROFILE,
            Self::HasHumanReadableEmbeddedMetadata => FileFlags::HUMAN_READABLE_METADATA,
            Self::HasTransparency => FileFlags::TRANSPARENCY,
            Self::HasXmp => FileFlags::XMP,
            Self::HasIptc => FileFlags::IPTC,
            Self::HasSoftwareSource => FileFlags::SOFTWARE_SOURCE,
            _ => return None,
        })
    }
}

/// When file maintenance runs in the background, and how fast (the
/// client options' `file_maintenance_during_*` and `_throttle_*`): at most
/// `files` big jobs' worth of work every `seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileMaintenanceSettings {
    pub during_idle: bool,
    pub during_active: bool,
    pub idle_files: u64,
    pub idle_seconds: u64,
    pub active_files: u64,
    pub active_seconds: u64,
}

impl Default for FileMaintenanceSettings {
    fn default() -> Self {
        Self {
            during_idle: true,
            during_active: true,
            idle_files: 1,
            idle_seconds: 2,
            active_files: 1,
            active_seconds: 20,
        }
    }
}

impl crate::settings::Setting for FileMaintenanceSettings {
    const KEY: &'static str = "file_maintenance";
}

/// Queue `job` for each of `hash_ids`, from `time_can_start` (seconds; 0
/// for now), replacing any it overrules (`AddJobs`).
pub fn add_jobs(
    conn: &Connection,
    hash_ids: &[HashId],
    job: JobType,
    time_can_start: i64,
) -> Result<()> {
    let mut delete = conn
        .prepare_cached("DELETE FROM file_maintenance_jobs WHERE hash_id = ?1 AND job_type = ?2")?;
    let mut add = conn.prepare_cached(
        "REPLACE INTO file_maintenance_jobs (hash_id, job_type, time_can_start) VALUES (?1, ?2, ?3)",
    )?;
    for &hash_id in hash_ids {
        for overruled in job.overruled() {
            delete.execute(params![hash_id, overruled.code()])?;
        }
        add.execute(params![hash_id, job.code(), time_can_start])?;
    }
    Ok(())
}

/// Some due work of the types `wanted` accepts (`GetJobs`): files with a
/// due job of the first such type in run order that has any, up to 256 of
/// them, each with all its due jobs of those types in run order.
pub fn due_jobs_of(
    conn: &Connection,
    now_s: i64,
    wanted: &dyn Fn(JobType) -> bool,
) -> Result<Vec<(HashId, Vec<JobType>)>> {
    for job in JobType::RUN_ORDER.into_iter().filter(|&j| wanted(j)) {
        let hash_ids: Vec<HashId> = conn
            .prepare_cached(
                "SELECT hash_id FROM file_maintenance_jobs
                 WHERE job_type = ?1 AND time_can_start < ?2 LIMIT 256",
            )?
            .query_map(params![job.code(), now_s], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if hash_ids.is_empty() {
            continue;
        }
        let mut stmt = conn.prepare_cached(
            "SELECT job_type FROM file_maintenance_jobs WHERE hash_id = ?1 AND time_can_start < ?2",
        )?;
        let mut out = Vec::with_capacity(hash_ids.len());
        for hash_id in hash_ids {
            let mut jobs: Vec<JobType> = stmt
                .query_map(params![hash_id, now_s], |r| r.get::<_, i64>(0))?
                .filter_map(|code| code.ok().and_then(JobType::from_code))
                .filter(|&j| wanted(j))
                .collect();
            jobs.sort_by_key(|j| JobType::RUN_ORDER.iter().position(|o| o == j));
            out.push((hash_id, jobs));
        }
        return Ok(out);
    }
    Ok(Vec::new())
}

/// How many jobs of each type are due, and how many are waiting
/// (`GetJobCounts`).
pub fn job_counts(conn: &Connection, now_s: i64) -> Result<BTreeMap<JobType, (u64, u64)>> {
    let mut out: BTreeMap<JobType, (u64, u64)> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT job_type, time_can_start < ?1, count(*) FROM file_maintenance_jobs GROUP BY 1, 2",
    )?;
    let rows = stmt.query_map([now_s], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, bool>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (code, due, n) = row?;
        let n = u64::try_from(n).unwrap_or(0);
        if let Some(job) = JobType::from_code(code) {
            let entry = out.entry(job).or_default();
            if due {
                entry.0 += n;
            } else {
                entry.1 += n;
            }
        }
    }
    Ok(out)
}

/// What a job found. `Nothing` is a job that found nothing to record (the
/// file is missing, say): it is cleared all the same.
#[derive(Debug, Clone, PartialEq)]
pub enum JobResult {
    Nothing,
    /// A "determine if the file has" job's answer.
    Flag(bool),
    /// md5, sha1 and sha512.
    OtherHashes {
        md5: Vec<u8>,
        sha1: Vec<u8>,
        sha512: Vec<u8>,
    },
    PixelHash(Sha256),
    FileModified(i64),
    PerceptualHashes(Vec<Vec<u8>>),
    /// Whether the file's type has perceptual hashes, so belongs in the
    /// similar-files search if they are useful.
    SimilarFilesMembership(bool),
    /// Whether the thumbnail was made again.
    Thumbnail(bool),
    Blurhash(String),
}

/// Record a finished job's result and take it (and the jobs it overrules)
/// off the queue, queueing what follows from it, as `ClearJobs` does.
pub fn clear_job(
    conn: &Connection,
    hash_id: HashId,
    job: JobType,
    result: &JobResult,
) -> Result<()> {
    let mut reset_auto_resolution = false;
    let appearance_changed = |conn: &Connection| -> Result<()> {
        // `_ScheduleJobsForChangedAppearance` (a new thumbnail brings a new
        // blurhash)
        for follow in [
            JobType::ForceThumbnail,
            JobType::PixelHash,
            JobType::PerceptualHashes,
        ] {
            add_jobs(conn, &[hash_id], follow, 0)?;
        }
        Ok(())
    };
    match (job, result) {
        (_, JobResult::Flag(has)) => {
            if let Some(flag) = job.flag() {
                let flags: Option<u32> = conn
                    .query_row(
                        "SELECT flags FROM files WHERE hash_id = ?1",
                        [hash_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(flags) = flags
                    && (flags & flag != 0) != *has
                {
                    let flags = if *has { flags | flag } else { flags & !flag };
                    conn.execute(
                        "UPDATE files SET flags = ?1 WHERE hash_id = ?2",
                        params![flags, hash_id],
                    )?;
                    match job {
                        JobType::HasTransparency | JobType::HasIccProfile => {
                            appearance_changed(conn)?;
                        }
                        // `_ScheduleJobsForPossiblyChangedRotation`
                        JobType::HasExif => add_jobs(conn, &[hash_id], JobType::FileMetadata, 0)?,
                        _ => {}
                    }
                    reset_auto_resolution = true;
                }
            }
        }
        (JobType::OtherHashes, JobResult::OtherHashes { md5, sha1, sha512 }) => {
            conn.execute(
                "INSERT OR REPLACE INTO hash_digests (hash_id, md5, sha1, sha512) VALUES (?1, ?2, ?3, ?4)",
                params![hash_id, md5, sha1, sha512],
            )?;
        }
        (JobType::PixelHash, JobResult::PixelHash(pixel_hash)) => {
            let current: Option<Option<Vec<u8>>> = conn
                .query_row(
                    "SELECT pixel_hash FROM files WHERE hash_id = ?1",
                    [hash_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(current) = current
                && current.as_deref() != Some(&pixel_hash.0[..])
            {
                conn.execute(
                    "UPDATE files SET pixel_hash = ?1 WHERE hash_id = ?2",
                    params![&pixel_hash.0[..], hash_id],
                )?;
                // cached potential pairs hold their kings' pixel hashes
                crate::duplicates::cache::changed(conn)?;
                reset_auto_resolution = true;
            }
        }
        (JobType::FileModifiedTimestamp, JobResult::FileModified(ms)) => {
            conn.execute(
                "UPDATE files SET file_modified_ms = ?1 WHERE hash_id = ?2",
                params![ms, hash_id],
            )?;
            reset_auto_resolution = true;
        }
        (JobType::PerceptualHashes, JobResult::PerceptualHashes(phashes)) => {
            reset_auto_resolution = crate::similar::set_perceptual_hashes(conn, hash_id, phashes)?;
        }
        (JobType::CheckSimilarFilesMembership, JobResult::SimilarFilesMembership(include)) => {
            if *include {
                crate::similar::ensure_in_or_out_of_search(conn, hash_id)?;
            } else {
                crate::similar::stop_searching(conn, hash_id)?;
            }
        }
        (JobType::ForceThumbnail | JobType::RefitThumbnail, JobResult::Thumbnail(made)) => {
            if *made || job == JobType::ForceThumbnail {
                add_jobs(conn, &[hash_id], JobType::Blurhash, 0)?;
            }
        }
        (JobType::Blurhash, JobResult::Blurhash(blurhash)) => {
            conn.execute(
                "UPDATE files SET blurhash = ?1 WHERE hash_id = ?2",
                params![blurhash, hash_id],
            )?;
        }
        _ => {}
    }
    let mut delete = conn
        .prepare_cached("DELETE FROM file_maintenance_jobs WHERE hash_id = ?1 AND job_type = ?2")?;
    for cleared in std::iter::once(&job).chain(job.overruled()) {
        delete.execute(params![hash_id, cleared.code()])?;
    }
    if reset_auto_resolution {
        crate::duplicates::auto::reset_file_search_progress(conn, hash_id)?;
    }
    Ok(())
}

/// The jobs queued for one file, by type.
pub fn jobs_for(conn: &Connection, hash_id: HashId) -> Result<HashSet<JobType>> {
    Ok(conn
        .prepare_cached("SELECT job_type FROM file_maintenance_jobs WHERE hash_id = ?1")?
        .query_map([hash_id], |r| r.get::<_, i64>(0))?
        .filter_map(|code| code.ok().and_then(JobType::from_code))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_types_keep_the_reference_s_codes() {
        assert_eq!(JobType::FileMetadata.code(), 0);
        assert_eq!(JobType::CheckSimilarFilesMembership.code(), 8);
        assert_eq!(JobType::HasIccProfile.code(), 16);
        assert_eq!(JobType::IntegrityPresenceDeleteRecord.code(), 21);
        assert_eq!(JobType::HasSoftwareSource.code(), 26);
        for job in JobType::ALL {
            assert_eq!(JobType::from_code(job.code()), Some(job));
            assert!(JobType::RUN_ORDER.contains(&job), "{job:?}");
        }
        assert_eq!(JobType::from_code(27), None);
    }
}
