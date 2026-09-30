//! Importing files into a hydrus-rs store.
//!
//! A [`FileImporter`] takes a file on disk through what the reference's
//! `FileImportJob` does: hash it, check what the client already knows about
//! it, analyse it (type, metadata, hashes, thumbnail), apply the import
//! options' rules, copy it into file storage and record it. Every source of
//! files (the Client API, import folders, downloaders) goes through here.

pub mod options;
pub mod status;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use hydrus_core::{HashId, Mime, ServiceId, Sha256, TimestampMs};
use hydrus_media::{Analysis, MediaError, MediaTools, ThumbnailSpec};
use hydrus_store::media::{FileFlags, FileInfo};
use hydrus_store::urls::{self, FileState};
use hydrus_store::{Store, StoreError, master};
use rusqlite::params;

pub use options::FileImportOptions;
pub use status::ImportStatus;

/// Errors that stop an import from being attempted at all. Problems with the
/// file itself are reported as an [`ImportStatus::Error`] result instead.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = ImportError> = std::result::Result<T, E>;

/// The outcome of importing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub status: ImportStatus,
    pub hash: Option<Sha256>,
    pub mime: Option<Mime>,
    /// Human-readable detail, as the reference words it.
    pub note: String,
}

/// Imports files into a store.
#[derive(Debug, Clone)]
pub struct FileImporter {
    store: Arc<Store>,
    tools: MediaTools,
}

impl FileImporter {
    pub fn new(store: Arc<Store>, tools: MediaTools) -> Self {
        Self { store, tools }
    }

    pub fn tools(&self) -> &MediaTools {
        &self.tools
    }

    /// Import the file at `path`. The file is copied first, so the source may
    /// change or disappear once this returns.
    pub fn import_path(&self, path: &Path, options: &FileImportOptions) -> Result<ImportResult> {
        let scratch = self.scratch_dir()?;
        let temp = tempfile::NamedTempFile::new_in(&scratch)?;
        std::fs::copy(path, temp.path())?;
        let modified = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| TimestampMs::from_millis(i64::try_from(d.as_millis()).unwrap_or(i64::MAX)));
        self.import_file(temp.path(), modified, options)
    }

    /// Import file content given in memory (an upload).
    pub fn import_bytes(&self, bytes: &[u8], options: &FileImportOptions) -> Result<ImportResult> {
        let scratch = self.scratch_dir()?;
        let temp = tempfile::NamedTempFile::new_in(&scratch)?;
        std::fs::write(temp.path(), bytes)?;
        self.import_file(temp.path(), None, options)
    }

    /// Where temporary copies go: inside the store, so they are on the same
    /// disk as the database and cleaned up with it.
    fn scratch_dir(&self) -> Result<PathBuf> {
        let dir = self.store.dir().join("tmp");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// What the client knows about `hash`, as import statuses word it
    /// (`prefix` starts the note, e.g. `"url recognised: "`).
    pub fn known_status(
        &self,
        hash: &Sha256,
        prefix: &str,
    ) -> Result<(ImportResult, Option<HashId>)> {
        let snap = self.store.snapshot();
        let found = self.store.read(|conn| {
            let Some(id) = master::hash_id(conn, hash)? else {
                return Ok(None);
            };
            Ok(Some((id, urls::file_state(conn, &snap.services, id)?)))
        })?;
        let Some((id, state)) = found else {
            return Ok((unknown(*hash), None));
        };
        let (status, note) = status::describe(&state, prefix, TimestampMs::now());
        let mime = match state {
            FileState::Imported { mime, .. } => mime,
            _ => None,
        };
        let mut result = ImportResult {
            status,
            hash: Some(*hash),
            mime,
            note,
        };
        // the database says we have it, but the file is gone: import again
        if let (ImportStatus::SuccessfulButRedundant, Some(mime)) = (result.status, mime)
            && !snap
                .storage
                .file_path(hash, mime)
                .is_some_and(|p| p.is_file())
        {
            result.status = ImportStatus::Unknown;
            result.note = "The client believed this file was already in the db, but it was truly missing! Import will go ahead, in an attempt to fix the situation.".into();
        }
        Ok((result, Some(id)))
    }

    fn import_file(
        &self,
        temp: &Path,
        modified: Option<TimestampMs>,
        options: &FileImportOptions,
    ) -> Result<ImportResult> {
        let hash = hydrus_media::hash_file(temp)
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .sha256;
        let (pre, _) = self.known_status(&hash, "file recognised: ")?;
        let should_import = match pre.status {
            ImportStatus::Unknown => true,
            ImportStatus::Deleted => !options.exclude_deleted,
            _ => false,
        };
        let result = if should_import {
            self.import_new(temp, hash, modified, options)?
        } else {
            pre
        };
        if result.status == ImportStatus::SuccessfulButRedundant {
            self.update_already_in_db(&hash, options)?;
        }
        Ok(result)
    }

    fn import_new(
        &self,
        temp: &Path,
        hash: Sha256,
        modified: Option<TimestampMs>,
        options: &FileImportOptions,
    ) -> Result<ImportResult> {
        let snap = self.store.snapshot();
        let spec = thumbnail_spec(&snap.thumbnails);
        let analysis = match self.tools.analyse(temp, &spec) {
            Ok(a) => a,
            Err(e) => return Ok(error_result(hash, &e)),
        };
        let mime = analysis.info.mime;
        if let Err(note) = options.check(&analysis.info) {
            return Ok(ImportResult {
                status: ImportStatus::Vetoed,
                hash: Some(hash),
                mime: Some(mime),
                note,
            });
        }

        // media first, under a claim, so the purge job can't race us
        let _claim = self.store.media_claims().claim(hash);
        let file_path = snap
            .storage
            .file_path(&hash, mime)
            .ok_or_else(|| StoreError::Corrupt(format!("no storage location for {hash}")))?;
        write_into_storage(temp, &file_path)?;
        if let Some(thumbnail) = analysis.thumbnail.as_ref().filter(|t| !t.is_default)
            && let Some(thumb_path) = snap.storage.thumbnail_path(&hash)
        {
            if let Some(parent) = thumb_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&thumb_path, &thumbnail.bytes)?;
        }

        let destinations = options.destinations(&snap.services);
        let archive = options.automatically_archive;
        let record = FileRecord::new(hash, &analysis, modified);
        let status = self.store.write_content(move |w| {
            let id = master::intern_hash(w.conn(), &hash)?;
            // another import of the same file may have won the race
            let already = matches!(
                urls::file_state(w.conn(), &w.snapshot().services, id)?,
                FileState::Imported { .. }
            );
            if already {
                return Ok(ImportStatus::SuccessfulButRedundant);
            }
            record.write(w, id)?;
            let now = w.now_ms();
            for domain in destinations {
                w.add_files(domain, &[(id, Some(now))])?;
            }
            w.conn()
                .prepare_cached("INSERT OR IGNORE INTO file_inbox (hash_id) VALUES (?1)")?
                .execute([id])?;
            if archive {
                w.archive(&[id])?;
            }
            Ok(ImportStatus::SuccessfulAndNew)
        })?;
        Ok(ImportResult {
            status,
            hash: Some(hash),
            mime: Some(mime),
            note: String::new(),
        })
    }

    /// What the import options do to a file the client already had.
    pub fn update_already_in_db(&self, hash: &Sha256, options: &FileImportOptions) -> Result<()> {
        let snap = self.store.snapshot();
        let destinations = if options.destinations_for_already_in_db {
            options.destinations(&snap.services)
        } else {
            Vec::new()
        };
        let archive = options.automatically_archive && options.archive_already_in_db;
        if destinations.is_empty() && !archive {
            return Ok(());
        }
        let hash = *hash;
        self.store.write_content(move |w| {
            let Some(id) = master::hash_id(w.conn(), &hash)? else {
                return Ok(());
            };
            let now = w.now_ms();
            for domain in destinations {
                w.add_files(domain, &[(id, Some(now))])?;
            }
            if archive {
                w.archive(&[id])?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn unknown(hash: Sha256) -> ImportResult {
    ImportResult {
        status: ImportStatus::Unknown,
        hash: Some(hash),
        mime: None,
        note: String::new(),
    }
}

/// A file the media tools couldn't handle, as the reference reports it.
fn error_result(hash: Sha256, error: &MediaError) -> ImportResult {
    ImportResult {
        status: ImportStatus::Error,
        hash: Some(hash),
        mime: None,
        note: error.to_string(),
    }
}

fn thumbnail_spec(settings: &hydrus_core::thumbnail::ThumbnailSettings) -> ThumbnailSpec {
    use hydrus_core::thumbnail::ThumbnailScale as Core;
    use hydrus_media::ThumbnailScale as Media;
    ThumbnailSpec {
        bounding: (settings.bounding_width, settings.bounding_height),
        scale: match settings.scale {
            Core::DownOnly => Media::DownOnly,
            Core::ToFit => Media::ToFit,
            Core::ToFill => Media::ToFill,
        },
        dpr_percent: settings.dpr_percent,
        ..ThumbnailSpec::default()
    }
}

/// Copy a file into storage, via a temporary name in the destination
/// directory so a partial copy never sits at the final path.
fn write_into_storage(source: &Path, destination: &Path) -> Result<()> {
    let dir = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("storage path has no directory"))?;
    std::fs::create_dir_all(dir)?;
    let partial = tempfile::NamedTempFile::new_in(dir)?;
    std::fs::copy(source, partial.path())?;
    partial
        .persist(destination)
        .map_err(|e| ImportError::Io(e.error))?;
    Ok(())
}

/// Everything the database records about a newly imported file.
struct FileRecord {
    info: FileInfo,
    md5: Vec<u8>,
    sha1: Vec<u8>,
    sha512: Vec<u8>,
    perceptual_hashes: Vec<Vec<u8>>,
}

impl FileRecord {
    fn new(hash: Sha256, analysis: &Analysis, modified: Option<TimestampMs>) -> Self {
        let a = analysis;
        let mut flags = 0;
        for (on, flag) in [
            (a.flags.has_transparency, FileFlags::TRANSPARENCY),
            (a.flags.has_exif, FileFlags::EXIF),
            (a.flags.has_icc_profile, FileFlags::ICC_PROFILE),
            (
                a.flags.has_human_readable_embedded_metadata,
                FileFlags::HUMAN_READABLE_METADATA,
            ),
        ] {
            if on {
                flags |= flag;
            }
        }
        debug_assert_eq!(a.hashes.sha256, hash);
        Self {
            info: FileInfo {
                size: a.info.size,
                mime: a.info.mime,
                original_mime: None,
                width: a.info.width,
                height: a.info.height,
                duration_ms: a.info.duration_ms,
                num_frames: a.info.num_frames,
                has_audio: a.info.has_audio,
                num_words: a.info.num_words,
                file_modified: modified,
                pixel_hash: a.pixel_hash,
                blurhash: a.blurhash.clone(),
                flags: FileFlags(flags),
            },
            md5: a.hashes.md5.0.to_vec(),
            sha1: a.hashes.sha1.0.to_vec(),
            sha512: a.hashes.sha512.0.to_vec(),
            perceptual_hashes: a.perceptual_hashes.iter().map(|p| p.0.to_vec()).collect(),
        }
    }

    fn write(
        &self,
        w: &mut hydrus_store::content::ContentWriter<'_>,
        id: HashId,
    ) -> hydrus_store::Result<()> {
        w.add_file_info(id, &self.info, true)?;
        let conn = w.conn();
        conn.prepare_cached(
            "INSERT OR REPLACE INTO hash_digests (hash_id, md5, sha1, sha512) VALUES (?1, ?2, ?3, ?4)",
        )?
        .execute(params![id, self.md5, self.sha1, self.sha512])?;
        conn.prepare_cached("DELETE FROM file_perceptual_hashes WHERE hash_id = ?1")?
            .execute([id])?;
        for phash in &self.perceptual_hashes {
            conn.prepare_cached("INSERT OR IGNORE INTO perceptual_hashes (phash) VALUES (?1)")?
                .execute([phash])?;
            conn.prepare_cached(
                "INSERT OR IGNORE INTO file_perceptual_hashes (hash_id, phash_id)
                 SELECT ?1, phash_id FROM perceptual_hashes WHERE phash = ?2",
            )?
            .execute(params![id, phash])?;
        }
        if !self.perceptual_hashes.is_empty() {
            // queued for the similar-files search
            conn.prepare_cached(
                "INSERT OR REPLACE INTO similar_search_status (hash_id, searched_distance) VALUES (?1, NULL)",
            )?
            .execute([id])?;
        }
        Ok(())
    }
}

/// The local file domains of `services`, for picking import destinations.
pub fn local_domains(services: &hydrus_store::services::ServiceRegistry) -> Vec<ServiceId> {
    services
        .of_type(hydrus_core::ServiceType::LocalFileDomain)
        .map(|s| s.id)
        .collect()
}
