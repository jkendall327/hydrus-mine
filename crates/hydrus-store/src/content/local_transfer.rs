//! Reference local migration: restore/add destination, then remove the source.
use super::ContentWriter;
use crate::{Result, StoreError};
use hydrus_core::{HashId, ServiceKey, ServiceType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
    Merge,
}

impl ContentWriter<'_> {
    /// Revalidate captured files and stable service keys inside the transaction.
    /// Strict moves skip an existing destination; merge moves include it.
    pub fn transfer_local_files(
        &mut self,
        kind: TransferKind,
        destination: &ServiceKey,
        source: Option<&ServiceKey>,
        files: &[HashId],
    ) -> Result<Vec<HashId>> {
        let domain = |key: &ServiceKey| -> Result<_> {
            let service = self.snapshot().services.by_key(key)?;
            if service.service_type() != ServiceType::LocalFileDomain {
                return Err(StoreError::Invalid(
                    "Local migration requires local file domains.".into(),
                ));
            }
            Ok((service.id, service.name.clone()))
        };
        let (destination, name) = domain(destination)?;
        let source = source.map(domain).transpose()?.map(|(id, _)| id);
        if kind != TransferKind::Copy && source.is_none() {
            return Err(StoreError::Invalid(
                "A file move requires a source domain.".into(),
            ));
        }
        if source == Some(destination) {
            return Ok(Vec::new());
        }
        let mut files = self.filter_current(self.roles().combined_local_media, files)?;
        if let Some(source) = source {
            let current = self.filter_current(source, &files)?;
            files.retain(|f| current.contains(f));
        }
        if kind != TransferKind::Merge {
            let current = self.filter_current(destination, &files)?;
            files.retain(|f| !current.contains(f));
        }
        let deleted = self.filter_deleted(destination, &files)?;
        self.undelete_files(destination, &deleted)?;
        let now = self.now_ms();
        let rows = files
            .iter()
            .map(|&file| (file, Some(now)))
            .collect::<Vec<_>>();
        self.add_files(destination, &rows)?;
        if let Some(source) = source
            && kind != TransferKind::Copy
        {
            self.delete_files(source, &files, Some(&format!("Moved to {name}")))?;
        }
        Ok(files)
    }
}
