//! Remote services: repository processing state, IPFS pins, remote thumbnails.
//!
//! A hydrus repository publishes its content as numbered *update files*,
//! which the client downloads into the "repository updates" file domain and
//! then processes into its own tables. The processing state lives in
//! per-repository tables:
//!
//! * `repository_updates_N` — which update files belong to which update index
//! * `repository_unregistered_updates_N` — update files whose type the
//!   client has not yet determined (so it cannot process them yet)
//! * `repository_updates_processed_N` — for each update file and content
//!   type, whether it has been processed
//! * `external_master.repository_hash_id_map_N` / `repository_tag_id_map_N`
//!   — the repository's own ids for hashes and tags, mapped to local ids
//!
//! Importing these lets a new client carry on syncing where the reference
//! left off instead of reprocessing a repository from scratch (which for
//! the public tag repository takes days).

use hydrus_core::{ContentType, HashId, ServiceId, ServiceType, TagId, TextId};

use super::column;
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};
use crate::rows::{Paging, Rows};

/// Whether an update file has been processed for one content type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateProcessed {
    pub hash_id: HashId,
    pub content_type: ContentType,
    pub processed: bool,
}

/// A directory of files pinned to an IPFS service (`service_directories`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDirectory {
    pub service_id: ServiceId,
    /// The directory's multihash, interned as a text.
    pub directory_id: TextId,
    pub num_files: i64,
    pub total_size: i64,
    pub note: String,
}

impl LegacyDb {
    fn repository_table(&self, service: ServiceId, schema: &str, prefix: &str) -> Result<String> {
        self.service_table(
            service,
            schema,
            prefix,
            ServiceType::is_repository,
            "a repository",
        )
    }

    /// Update files per update index (`repository_updates_N`).
    pub fn repository_updates(&self, service: ServiceId) -> Result<Rows<'_, (i64, HashId)>> {
        let table = self.repository_table(service, "main", "repository_updates")?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["update_index", "hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(move |row| Ok((column(row, 0, &name)?, column(row, 1, &name)?))),
        ))
    }

    /// Update files not yet registered (`repository_unregistered_updates_N`).
    pub fn repository_unregistered_updates(&self, service: ServiceId) -> Result<Rows<'_, HashId>> {
        let table = self.repository_table(service, "main", "repository_unregistered_updates")?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(move |row| column(row, 0, &name)),
        ))
    }

    /// Processing state per update file and content type
    /// (`repository_updates_processed_N`).
    pub fn repository_updates_processed(
        &self,
        service: ServiceId,
    ) -> Result<Rows<'_, UpdateProcessed>> {
        let table = self.repository_table(service, "main", "repository_updates_processed")?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "content_type"],
                columns: &["processed"],
                join: "",
            },
            Box::new(move |row| {
                let code: i64 = column(row, 1, &name)?;
                let content_type = u8::try_from(code)
                    .ok()
                    .and_then(ContentType::from_code)
                    .ok_or_else(|| {
                        LegacyError::bad_value(&name, format!("unknown content type {code}"))
                    })?;
                let processed: i64 = column(row, 2, &name)?;
                Ok(UpdateProcessed {
                    hash_id: column(row, 0, &name)?,
                    content_type,
                    processed: processed != 0,
                })
            }),
        ))
    }

    /// The repository's hash ids mapped to local hash ids
    /// (`repository_hash_id_map_N`).
    pub fn repository_hash_id_map(&self, service: ServiceId) -> Result<Rows<'_, (i64, HashId)>> {
        let table = self.repository_table(service, "external_master", "repository_hash_id_map")?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_hash_id"],
                columns: &["hash_id"],
                join: "",
            },
            Box::new(move |row| Ok((column(row, 0, &name)?, column(row, 1, &name)?))),
        ))
    }

    /// The repository's tag ids mapped to local tag ids
    /// (`repository_tag_id_map_N`).
    pub fn repository_tag_id_map(&self, service: ServiceId) -> Result<Rows<'_, (i64, TagId)>> {
        let table = self.repository_table(service, "external_master", "repository_tag_id_map")?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_tag_id"],
                columns: &["tag_id"],
                join: "",
            },
            Box::new(move |row| Ok((column(row, 0, &name)?, column(row, 1, &name)?))),
        ))
    }

    /// Remote files whose thumbnails the client has downloaded
    /// (`remote_thumbnails`).
    pub fn remote_thumbnails(&self) -> Result<Rows<'_, (ServiceId, HashId)>> {
        let table = self.table("main", "remote_thumbnails")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(|row| {
                Ok((
                    column(row, 0, "remote_thumbnails")?,
                    column(row, 1, "remote_thumbnails")?,
                ))
            }),
        ))
    }

    /// IPFS multihashes of pinned files (`service_filenames`).
    pub fn service_filenames(&self) -> Result<Rows<'_, (ServiceId, HashId, String)>> {
        let table = self.table("main", "service_filenames")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "hash_id"],
                columns: &["filename"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "service_filenames";
                Ok((column(row, 0, T)?, column(row, 1, T)?, column(row, 2, T)?))
            }),
        ))
    }

    /// Directories pinned to IPFS services (`service_directories`).
    pub fn service_directories(&self) -> Result<Rows<'_, ServiceDirectory>> {
        let table = self.table("main", "service_directories")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "directory_id"],
                columns: &["num_files", "total_size", "note"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "service_directories";
                Ok(ServiceDirectory {
                    service_id: column(row, 0, T)?,
                    directory_id: column(row, 1, T)?,
                    num_files: column(row, 2, T)?,
                    total_size: column(row, 3, T)?,
                    note: column(row, 4, T)?,
                })
            }),
        ))
    }

    /// Which files are in which pinned IPFS directory
    /// (`service_directory_file_map`).
    pub fn service_directory_files(&self) -> Result<Rows<'_, (ServiceId, TextId, HashId)>> {
        let table = self.table("main", "service_directory_file_map")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "directory_id", "hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "service_directory_file_map";
                Ok((column(row, 0, T)?, column(row, 1, T)?, column(row, 2, T)?))
            }),
        ))
    }
}
