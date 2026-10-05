//! Captured local-domain transfers. Confirmation preferences are read on invocation.
use hydrus_core::{HashId, ServiceId, ServiceKey, ServiceType};
use hydrus_store::content::{DomainRoles, TransferKind};
use hydrus_store::settings::LocalTransferPreferences;
use hydrus_store::{Result, Store, StoreError};

/// An immutable actionable selection; later selection changes cannot retarget it.
#[derive(Debug, Clone)]
pub struct Transfer {
    pub kind: TransferKind,
    pub destination: ServiceKey,
    pub source: Option<ServiceKey>,
    pub files: Vec<HashId>,
    pub question: String,
    pub confirm: bool,
}
impl Transfer {
    pub fn load(
        store: &Store,
        kind: TransferKind,
        destination: ServiceId,
        source: Option<ServiceId>,
        files: &[HashId],
    ) -> Result<Option<Self>> {
        let snapshot = store.snapshot();
        let domains = DomainRoles::new(&snapshot.services)?;
        let dest = snapshot.services.get(destination)?;
        let source = source.map(|id| snapshot.services.get(id)).transpose()?;
        if dest.service_type() != ServiceType::LocalFileDomain
            || source
                .as_ref()
                .is_some_and(|s| s.service_type() != ServiceType::LocalFileDomain)
        {
            return Err(StoreError::Invalid(
                "Local migration requires local file domains.".into(),
            ));
        }
        if kind != TransferKind::Copy && source.is_none() {
            return Err(StoreError::Invalid(
                "A file move requires a source domain.".into(),
            ));
        }
        let current = store.read(|c| hydrus_store::media::current_domains(c, files))?;
        // The reference copy question counts destination-absent media before
        // filtering out non-local media for the migration worker.
        let copy_count = files
            .iter()
            .filter(|f| {
                current
                    .get(*f)
                    .is_none_or(|domains| !domains.contains(&destination))
            })
            .count();
        let files = files
            .iter()
            .copied()
            .filter(|f| {
                current.get(f).is_some_and(|current| {
                    current.contains(&domains.combined_local_media)
                        && (kind == TransferKind::Merge || !current.contains(&destination))
                        && source.as_ref().is_none_or(|s| current.contains(&s.id))
                })
            })
            .collect::<Vec<_>>();
        if files.is_empty() || source.as_ref().is_some_and(|s| s.id == destination) {
            return Ok(None);
        }
        let prefs: LocalTransferPreferences = store.read(hydrus_store::settings::get)?;
        let count = hydrus_core::numbers::human_int(if kind == TransferKind::Copy {
            copy_count
        } else {
            files.len()
        } as u64);
        let (question, confirm) = if let Some(source) = &source {
            let verb = if kind == TransferKind::Merge {
                "Move-merge"
            } else {
                "Move"
            };
            (
                format!(
                    "{verb} {count} files from {} to {}?",
                    source.name, dest.name
                ),
                prefs.move_files,
            )
        } else {
            (format!("Add {count} files to {}?", dest.name), prefs.copy)
        };
        Ok(Some(Self {
            kind,
            destination: dest.key.clone(),
            source: source.map(|s| s.key.clone()),
            files,
            question,
            confirm,
        }))
    }
    pub fn apply(&self, store: &Store) -> Result<Vec<HashId>> {
        let transfer = self.clone();
        store.write_content(move |writer| {
            writer.transfer_local_files(
                transfer.kind,
                &transfer.destination,
                transfer.source.as_ref(),
                &transfer.files,
            )
        })
    }
}
