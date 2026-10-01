//! Importing files into a hydrus-rs store.
//!
//! A [`FileImporter`] takes a file on disk through what the reference's
//! `FileImportJob` does: hash it, check what the client already knows about
//! it, analyse it (type, metadata, hashes, thumbnail), apply the import
//! options' rules, copy it into file storage and record it. Every source of
//! files (the Client API, import folders, downloaders) goes through here.

pub mod maintenance;
pub mod options;
pub mod paths;
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
    /// Set when the reference's import job would have raised (a file it
    /// can't import): the exception's message, which is what a file seed
    /// notes (and a seed then keeps no hash).
    pub raised: Option<String>,
}

/// Imports files into a store.
#[derive(Debug, Clone)]
pub struct FileImporter {
    store: Arc<Store>,
    tools: MediaTools,
}

impl FileImporter {
    /// An importer for `store`, which also applies the store's file
    /// handling settings to this process (as the reference applies its
    /// options at boot).
    pub fn new(store: Arc<Store>, tools: MediaTools) -> Self {
        apply_file_handling(&store);
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
        hydrus_store::paths::copy_file(path, temp.path())?;
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
            raised: None,
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
        if !options.allow_decompression_bombs
            && let Ok(info) = self.tools.inspect(temp)
            && is_decompression_bomb(&info)
        {
            // (the reference's job raises this, a veto, from GenerateInfo)
            let note = "Image seems to be a Decompression Bomb!".to_owned();
            return Ok(ImportResult {
                status: ImportStatus::Vetoed,
                hash: Some(hash),
                mime: Some(info.mime),
                note: note.clone(),
                raised: Some(note),
            });
        }
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
                raised: None,
            });
        }
        // (the reference's job raises this before the file reaches the
        // database; here it is before the file reaches storage)
        let destinations = match options.destinations(&snap.services) {
            Ok(d) => d,
            Err(note) => {
                return Ok(ImportResult {
                    status: ImportStatus::Vetoed,
                    hash: Some(hash),
                    mime: Some(mime),
                    note: note.to_owned(),
                    raised: Some(note.to_owned()),
                });
            }
        };

        // media first, under a claim, so the purge job can't race us
        let _claim = self.store.media_claims().claim(hash);
        let file_path = snap
            .storage
            .file_path(&hash, mime)
            .ok_or_else(|| StoreError::Corrupt(format!("no storage location for {hash}")))?;
        let size = std::fs::metadata(temp)?.len();
        if free_space(&file_path).is_some_and(|free| free < MIN_FREE_SPACE || free < size) {
            return Err(self.critical_drive_error(format!(
                "The disk for path \"{}\" is almost full and cannot take the file \"{hash}\", which is {}! Shut the client down now and fix this!",
                file_path.display(),
                hydrus_core::numbers::human_bytes(size),
            )));
        }
        if let Err(e) = write_into_storage(temp, &file_path) {
            return Err(self.critical_drive_error(format!(
                "Copying the file from \"{}\" to \"{}\" failed ({e})! Other import queues have been paused. You should shut the client down now and fix this!",
                temp.display(),
                file_path.display(),
            )));
        }
        if let Some(thumbnail) = analysis.thumbnail.as_ref().filter(|t| !t.is_default)
            && let Some(thumb_path) = snap.storage.thumbnail_path(&hash)
        {
            if let Some(parent) = thumb_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&thumb_path, &thumbnail.bytes)?;
        }

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
            raised: None,
        })
    }

    /// What the import options do to a file the client already had.
    pub fn update_already_in_db(&self, hash: &Sha256, options: &FileImportOptions) -> Result<()> {
        let snap = self.store.snapshot();
        let destinations = if options.destinations_for_already_in_db {
            // (with no destination, there is nowhere to add it)
            options.destinations(&snap.services).unwrap_or_default()
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
        raised: None,
    }
}

/// A file the media tools couldn't handle, as the reference reports it.
fn error_result(hash: Sha256, error: &MediaError) -> ImportResult {
    let raised = match error {
        MediaError::Unsupported { reason, .. } => reason.clone(),
        MediaError::ZeroSize => "File is of zero length!".into(),
        MediaError::Damaged(message) => message.clone(),
        other => other.to_string(),
    };
    ImportResult {
        status: ImportStatus::Error,
        hash: Some(hash),
        mime: None,
        note: error.to_string(),
        raised: Some(raised),
    }
}

/// `IsDecompressionBomb`: a JPEG or PNG Pillow refuses to open, having
/// more than twice the pixel limit the reference sets (512 MiB / 3).
fn is_decompression_bomb(info: &hydrus_media::FileInfo) -> bool {
    const MAX_IMAGE_PIXELS: u64 = (512 * 1024 * 1024) / 3;
    matches!(info.mime, Mime::ImageJpeg | Mime::ImagePng)
        && match (info.width, info.height) {
            (Some(w), Some(h)) => u64::from(w) * u64::from(h) > 2 * MAX_IMAGE_PIXELS,
            _ => false,
        }
}

/// The least free space an import leaves on a media disk (the reference's).
const MIN_FREE_SPACE: u64 = 100 * 1_048_576;

/// The free space on the disk `path` is (or will be) on.
fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    fs4::available_space(existing).ok()
}

impl FileImporter {
    /// `_HandleCriticalDriveError`: a media disk is full or failing, so stop
    /// the importers (import folders, subscriptions, file queues) before
    /// they lose more files, and say why.
    fn critical_drive_error(&self, message: String) -> ImportError {
        tracing::error!("{message}");
        let paused = self.store.write(|ctx| {
            let conn = ctx.conn();
            let mut folders: hydrus_store::settings::FolderSettings =
                hydrus_store::settings::get(conn)?;
            folders.pause_import_folders = true;
            hydrus_store::settings::set(conn, &folders)?;
            let mut pauses: hydrus_store::settings::Pauses = hydrus_store::settings::get(conn)?;
            pauses.subscriptions = true;
            pauses.file_queues = true;
            hydrus_store::settings::set(conn, &pauses)
        });
        match paused {
            Ok(()) => tracing::error!(
                "A critical drive error has occurred. All importers--subscriptions, import folders, and file import queues--have been paused. Once the issue is clear, resume them (hydrus resume <store> subscriptions, file-queues and import-folders)."
            ),
            Err(e) => tracing::error!(error = %e, "pausing the importers failed"),
        }
        ImportError::Io(std::io::Error::other(message))
    }

    /// `RegenerateThumbnail`, for a thumbnail that has gone missing: make it
    /// again from the file, under the client's thumbnail settings. Its path,
    /// or `None` when the file has no thumbnail of its own (its type has
    /// none, or it is shown with its type's icon); an error when the file
    /// isn't stored here or can't be read.
    pub fn regenerate_thumbnail(
        &self,
        media: &hydrus_store::media::MediaResult,
    ) -> Result<Option<std::path::PathBuf>> {
        let missing =
            |why: String| ImportError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, why));
        let snap = self.store.snapshot();
        let Some(info) = &media.info else {
            return Err(missing(format!("no metadata for file {}", media.hash)));
        };
        if !info.mime.has_thumbnail() {
            return Ok(None);
        }
        let local_storage =
            hydrus_store::content::DomainRoles::new(&snap.services)?.local_file_storage;
        if !media.current.iter().any(|c| c.service == local_storage) {
            return Err(missing(
                "I was called to regenerate a thumbnail from source, but the source file does not think it is in the local file store!".into(),
            ));
        }
        let file = snap
            .storage
            .file_path(&media.hash, info.mime)
            .filter(|p| p.is_file())
            .ok_or_else(|| {
                missing(format!(
                    "The thumbnail for file {} could not be regenerated from the original file because the original file is missing! This event could indicate hard drive corruption. Please check everything is ok.",
                    media.hash
                ))
            })?;
        let file_info = hydrus_media::FileInfo {
            mime: info.mime,
            size: info.size,
            width: info.width,
            height: info.height,
            duration_ms: info.duration_ms,
            num_frames: info.num_frames,
            has_audio: info.has_audio,
            num_words: info.num_words,
        };
        let thumbnail = self
            .tools
            .thumbnail(&file, &file_info, &thumbnail_spec(&snap.thumbnails))
            .map_err(|e| {
                missing(format!(
                    "The thumbnail for file {} could not be regenerated from the original file ({e}).",
                    media.hash
                ))
            })?;
        if thumbnail.is_default {
            return Ok(None);
        }
        let path = snap.storage.thumbnail_path(&media.hash).ok_or_else(|| {
            StoreError::Corrupt(format!("no storage location for {}", media.hash))
        })?;
        write_bytes_into_storage(&thumbnail.bytes, &path)?;
        Ok(Some(path))
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
        video_percentage_in: settings.video_percentage_in,
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
    hydrus_store::paths::copy_file(source, partial.path())?;
    partial
        .persist(destination)
        .map_err(|e| ImportError::Io(e.error))?;
    paths::give_nice_permission_bits(destination);
    Ok(())
}

/// Write bytes into storage the same way.
fn write_bytes_into_storage(bytes: &[u8], destination: &Path) -> Result<()> {
    let dir = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("storage path has no directory"))?;
    std::fs::create_dir_all(dir)?;
    let mut partial = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut partial, bytes)?;
    partial
        .persist(destination)
        .map_err(|e| ImportError::Io(e.error))?;
    paths::give_nice_permission_bits(destination);
    Ok(())
}

/// Apply the store's file handling settings to this process: comic book
/// detection, what counts as transparency, and whether files' permissions
/// are left alone.
pub fn apply_file_handling(store: &Store) {
    use hydrus_media::TransparencyStrictness as Level;
    let settings: hydrus_store::settings::FileHandlingSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    hydrus_media::set_comic_book_detection(settings.comic_book_detection);
    hydrus_media::set_transparency_strictness(match settings.transparency_strictness {
        0 => Level::ChannelPresence,
        1 => Level::NotBlackOrWhite,
        _ => Level::Human,
    });
    hydrus_store::paths::set_do_not_chmod(settings.do_not_chmod);
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
            (a.flags.has_xmp, FileFlags::XMP),
            (a.flags.has_iptc, FileFlags::IPTC),
            (a.flags.has_software_source, FileFlags::SOFTWARE_SOURCE),
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
        // (queued for the similar-files search if they are useful and new)
        hydrus_store::similar::set_perceptual_hashes(conn, id, &self.perceptual_hashes)?;
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
