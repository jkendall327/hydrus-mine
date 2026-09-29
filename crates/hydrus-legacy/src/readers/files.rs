//! File metadata and file domain membership.

use hydrus_core::{
    CanvasType, HashId, Mime, ServiceId, ServiceType, TextId, TimestampMs, UrlDomainId,
};

use super::{column, mime_column, timestamp_column};
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};
use crate::rows::{Paging, Rows};

/// Basic metadata of a file (`files_info`). Known for every file the client
/// has ever had locally, and for some remote files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub hash_id: HashId,
    pub size: i64,
    pub mime: Mime,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub num_frames: Option<i64>,
    pub has_audio: Option<bool>,
    pub num_words: Option<i64>,
}

/// Per-file boolean properties, each stored as a table of hash ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileProperty {
    IccProfile,
    Exif,
    Xmp,
    Iptc,
    HumanReadableEmbeddedMetadata,
    SoftwareSource,
    Transparency,
}

impl FileProperty {
    pub const ALL: [FileProperty; 7] = [
        FileProperty::IccProfile,
        FileProperty::Exif,
        FileProperty::Xmp,
        FileProperty::Iptc,
        FileProperty::HumanReadableEmbeddedMetadata,
        FileProperty::SoftwareSource,
        FileProperty::Transparency,
    ];

    /// The table listing files with this property.
    pub const fn table(self) -> &'static str {
        match self {
            FileProperty::IccProfile => "has_icc_profile",
            FileProperty::Exif => "has_exif",
            FileProperty::Xmp => "has_xmp",
            FileProperty::Iptc => "has_iptc",
            FileProperty::HumanReadableEmbeddedMetadata => "has_human_readable_embedded_metadata",
            FileProperty::SoftwareSource => "has_software_source",
            FileProperty::Transparency => "has_transparency",
        }
    }
}

/// A file currently in a file domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentFile {
    pub hash_id: HashId,
    /// When it was added to this domain.
    pub added: Option<TimestampMs>,
}

/// A file deleted from a file domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedFile {
    pub hash_id: HashId,
    pub deleted: Option<TimestampMs>,
    /// When it had been added, before it was deleted.
    pub originally_added: Option<TimestampMs>,
}

/// How often and how long a file has been looked at in one kind of viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewingStats {
    pub hash_id: HashId,
    pub canvas: CanvasType,
    pub last_viewed: Option<TimestampMs>,
    pub views: i64,
    pub viewtime_ms: i64,
}

/// Services that have `current_files_N` etc.: every file service except
/// the virtual "all known files".
fn has_file_tables(service_type: ServiceType) -> bool {
    service_type.is_file_service() && service_type != ServiceType::CombinedFile
}

impl LegacyDb {
    fn hash_id_rows(&self, table: &str) -> Rows<'_, HashId> {
        let name = table.to_owned();
        Rows::new(
            self,
            &Paging {
                table,
                keys: &["hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(move |row| column(row, 0, &name)),
        )
    }

    fn hash_and_value_rows<V: rusqlite::types::FromSql + 'static>(
        &self,
        table: &str,
        value: &str,
    ) -> Rows<'_, (HashId, V)> {
        let name = table.to_owned();
        Rows::new(
            self,
            &Paging {
                table,
                keys: &["hash_id"],
                columns: &[value],
                join: "",
            },
            Box::new(move |row| Ok((column(row, 0, &name)?, column(row, 1, &name)?))),
        )
    }

    /// Basic file metadata (`files_info`).
    pub fn files_info(&self) -> Result<Rows<'_, FileInfo>> {
        let table = self.table("main", "files_info")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &[
                    "size",
                    "mime",
                    "width",
                    "height",
                    "duration",
                    "num_frames",
                    "has_audio",
                    "num_words",
                ],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "files_info";
                let has_audio: Option<i64> = column(row, 7, T)?;
                Ok(FileInfo {
                    hash_id: column(row, 0, T)?,
                    size: column(row, 1, T)?,
                    mime: mime_column(row, 2, T)?,
                    width: column(row, 3, T)?,
                    height: column(row, 4, T)?,
                    duration_ms: column(row, 5, T)?,
                    num_frames: column(row, 6, T)?,
                    has_audio: has_audio.map(|a| a != 0),
                    num_words: column(row, 8, T)?,
                })
            }),
        ))
    }

    /// Files whose type the user has overridden (`files_info_forced_filetypes`).
    pub fn forced_filetypes(&self) -> Result<Rows<'_, (HashId, Mime)>> {
        let table = self.table("main", "files_info_forced_filetypes")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &["forced_mime"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "files_info_forced_filetypes";
                Ok((column(row, 0, T)?, mime_column(row, 1, T)?))
            }),
        ))
    }

    /// Files that have a property (`has_exif`, `has_transparency`, ...).
    pub fn files_with_property(&self, property: FileProperty) -> Result<Rows<'_, HashId>> {
        Ok(self.hash_id_rows(&self.table("main", property.table())?))
    }

    /// Files currently in a file domain (`current_files_N`).
    pub fn current_files(&self, service: ServiceId) -> Result<Rows<'_, CurrentFile>> {
        let table = self.service_table(
            service,
            "main",
            "current_files",
            has_file_tables,
            "a file service",
        )?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &["timestamp_ms"],
                join: "",
            },
            Box::new(move |row| {
                Ok(CurrentFile {
                    hash_id: column(row, 0, &name)?,
                    added: timestamp_column(row, 1, &name)?,
                })
            }),
        ))
    }

    /// Files deleted from a file domain (`deleted_files_N`).
    pub fn deleted_files(&self, service: ServiceId) -> Result<Rows<'_, DeletedFile>> {
        let table = self.service_table(
            service,
            "main",
            "deleted_files",
            has_file_tables,
            "a file service",
        )?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &["timestamp_ms", "original_timestamp_ms"],
                join: "",
            },
            Box::new(move |row| {
                Ok(DeletedFile {
                    hash_id: column(row, 0, &name)?,
                    deleted: timestamp_column(row, 1, &name)?,
                    originally_added: timestamp_column(row, 2, &name)?,
                })
            }),
        ))
    }

    /// Files pending upload to a file repository or IPFS (`pending_files_N`).
    pub fn pending_files(&self, service: ServiceId) -> Result<Rows<'_, HashId>> {
        let table = self.service_table(
            service,
            "main",
            "pending_files",
            has_file_tables,
            "a file service",
        )?;
        Ok(self.hash_id_rows(&table))
    }

    /// Files petitioned for deletion from a remote file service, with the
    /// reason (`petitioned_files_N`).
    pub fn petitioned_files(&self, service: ServiceId) -> Result<Rows<'_, (HashId, TextId)>> {
        let table = self.service_table(
            service,
            "main",
            "petitioned_files",
            has_file_tables,
            "a file service",
        )?;
        Ok(self.hash_and_value_rows(&table, "reason_id"))
    }

    /// Why local files were deleted (`local_file_deletion_reasons`).
    pub fn deletion_reasons(&self) -> Result<Rows<'_, (HashId, TextId)>> {
        Ok(self.hash_and_value_rows(
            &self.table("main", "local_file_deletion_reasons")?,
            "reason_id",
        ))
    }

    /// Files in the inbox (`file_inbox`). Local files not listed are archived.
    pub fn inbox(&self) -> Result<Rows<'_, HashId>> {
        Ok(self.hash_id_rows(&self.table("main", "file_inbox")?))
    }

    /// When files were archived (`archive_timestamps`).
    pub fn archive_timestamps(&self) -> Result<Rows<'_, (HashId, TimestampMs)>> {
        Ok(self.hash_and_value_rows(
            &self.table("main", "archive_timestamps")?,
            "archived_timestamp_ms",
        ))
    }

    /// File modified times from the file system (`file_modified_timestamps`).
    pub fn file_modified_timestamps(&self) -> Result<Rows<'_, (HashId, TimestampMs)>> {
        Ok(self.hash_and_value_rows(
            &self.table("main", "file_modified_timestamps")?,
            "file_modified_timestamp_ms",
        ))
    }

    /// File modified times as reported by web domains
    /// (`file_domain_modified_timestamps`).
    pub fn domain_modified_timestamps(
        &self,
    ) -> Result<Rows<'_, (HashId, UrlDomainId, TimestampMs)>> {
        let table = self.table("main", "file_domain_modified_timestamps")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "domain_id"],
                columns: &["file_modified_timestamp_ms"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "file_domain_modified_timestamps";
                Ok((column(row, 0, T)?, column(row, 1, T)?, column(row, 2, T)?))
            }),
        ))
    }

    /// File viewing statistics per viewer (`file_viewing_stats`).
    pub fn viewing_stats(&self) -> Result<Rows<'_, ViewingStats>> {
        let table = self.table("main", "file_viewing_stats")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "canvas_type"],
                columns: &["last_viewed_timestamp_ms", "views", "viewtime_ms"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "file_viewing_stats";
                let canvas: i64 = column(row, 1, T)?;
                let canvas = u8::try_from(canvas)
                    .ok()
                    .and_then(CanvasType::from_code)
                    .ok_or_else(|| LegacyError::bad_value(T, format!("unknown canvas {canvas}")))?;
                Ok(ViewingStats {
                    hash_id: column(row, 0, T)?,
                    canvas,
                    last_viewed: timestamp_column(row, 2, T)?,
                    views: column(row, 3, T)?,
                    viewtime_ms: column(row, 4, T)?,
                })
            }),
        ))
    }

    /// Files removed from every local domain whose file on disk has not yet
    /// been deleted (`deferred_physical_file_deletes`). The importer should
    /// not copy these.
    pub fn deferred_physical_file_deletes(&self) -> Result<Rows<'_, HashId>> {
        Ok(self.hash_id_rows(&self.table("main", "deferred_physical_file_deletes")?))
    }

    /// As [`LegacyDb::deferred_physical_file_deletes`], for thumbnails.
    pub fn deferred_physical_thumbnail_deletes(&self) -> Result<Rows<'_, HashId>> {
        Ok(self.hash_id_rows(&self.table("main", "deferred_physical_thumbnail_deletes")?))
    }
}
