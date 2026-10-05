//! Thumbnail clear-delete-record captures eligibility before the question.
use hydrus_core::HashId;
use hydrus_store::{Store, content::DomainRoles};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Eligible selected identities captured before the reference's confirmation.
pub struct Plan {
    /// Ordered files with a physical local-storage deletion record.
    pub files: Vec<HashId>,
}

impl Plan {
    /// `selected` is already flattened; retain its order for 64-record writes.
    pub fn capture(store: &Store, selected: &[HashId]) -> hydrus_store::Result<Self> {
        let roles = DomainRoles::new(&store.snapshot().services)?;
        let deleted = store.read(|conn| {
            hydrus_store::media::deleted_from(conn, selected, roles.local_file_storage)
        })?;
        Ok(Self {
            files: selected
                .iter()
                .copied()
                .filter(|file| deleted.contains(file))
                .collect(),
        })
    }

    /// The reference's exact question, including its singular grammar.
    pub fn question(&self) -> String {
        format!(
            "Clear the deletion record for {} previously deleted files?.",
            hydrus_core::numbers::human_int(self.files.len() as u64)
        )
    }

    /// Each accepted batch commits independently, as reference content writes do.
    /// Do not recapture eligibility after the question: new deletions stay outside
    /// this plan. The existing writer preserves trash and current file membership.
    pub fn apply(&self, store: &Store) -> hydrus_store::Result<()> {
        for batch in self.files.chunks(64) {
            let batch = batch.to_vec();
            store.write_content(move |writer| writer.clear_local_delete_records(Some(&batch)))?;
        }
        Ok(())
    }
}
