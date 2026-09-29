//! The domain a search runs over: which files (file domains) and which tags
//! (tag domain, current and/or pending) it looks at, plus its predicates.

use std::collections::BTreeSet;

use hydrus_core::ServiceKey;
use hydrus_core::service::builtin_keys;

use crate::predicate::Predicate;

/// The file domains a search covers: files currently in any of `current`, or
/// deleted from any of `deleted`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocationContext {
    current: BTreeSet<ServiceKey>,
    deleted: BTreeSet<ServiceKey>,
}

impl LocationContext {
    /// Build a location context. Like the reference, a context that includes
    /// "all known files" is exactly "all known files".
    pub fn new(
        current: impl IntoIterator<Item = ServiceKey>,
        deleted: impl IntoIterator<Item = ServiceKey>,
    ) -> Self {
        let all_known_files = ServiceKey::new(builtin_keys::COMBINED_FILE.to_vec());
        let current: BTreeSet<ServiceKey> = current.into_iter().collect();
        if current.contains(&all_known_files) {
            return Self {
                current: [all_known_files].into_iter().collect(),
                deleted: BTreeSet::new(),
            };
        }
        Self {
            current,
            deleted: deleted.into_iter().collect(),
        }
    }

    /// Files currently in one domain.
    pub fn single(service: ServiceKey) -> Self {
        Self::new([service], [])
    }

    pub fn current(&self) -> &BTreeSet<ServiceKey> {
        &self.current
    }

    pub fn deleted(&self) -> &BTreeSet<ServiceKey> {
        &self.deleted
    }

    pub fn is_all_known_files(&self) -> bool {
        self.current
            .iter()
            .any(|k| k.as_bytes() == builtin_keys::COMBINED_FILE)
    }
}

impl Default for LocationContext {
    /// All local media, the default for Client API searches.
    fn default() -> Self {
        Self::single(ServiceKey::new(
            builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
        ))
    }
}

/// The tag domain a search reads tags from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TagContext {
    /// The tag service searched.
    pub service: ServiceKey,
    pub include_current: bool,
    pub include_pending: bool,
    /// The tag service whose siblings and parents are applied for display.
    pub display_service: ServiceKey,
}

impl TagContext {
    /// Search one tag service, with its own siblings and parents.
    pub fn new(service: ServiceKey, include_current: bool, include_pending: bool) -> Self {
        Self {
            display_service: service.clone(),
            service,
            include_current,
            include_pending,
        }
    }

    pub fn is_all_known_tags(&self) -> bool {
        self.service.as_bytes() == builtin_keys::COMBINED_TAG
    }
}

impl Default for TagContext {
    /// All known tags, current and pending.
    fn default() -> Self {
        Self::new(
            ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()),
            true,
            true,
        )
    }
}

/// A complete file search: where to look and what to match.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSearchContext {
    pub location: LocationContext,
    pub tags: TagContext,
    /// Files must satisfy all of these.
    pub predicates: Vec<Predicate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_known_files_absorbs_other_domains() {
        let all = ServiceKey::new(builtin_keys::COMBINED_FILE.to_vec());
        let trash = ServiceKey::new(builtin_keys::TRASH.to_vec());
        let ctx = LocationContext::new([all.clone(), trash.clone()], [trash]);
        assert!(ctx.is_all_known_files());
        assert_eq!(ctx.current().len(), 1);
        assert!(ctx.deleted().is_empty());
    }

    #[test]
    fn defaults_match_the_client_api() {
        let ctx = FileSearchContext::default();
        assert_eq!(
            ctx.location
                .current()
                .iter()
                .next()
                .map(ServiceKey::as_bytes),
            Some(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)
        );
        assert!(ctx.tags.is_all_known_tags());
        assert!(ctx.tags.include_current && ctx.tags.include_pending);
    }
}
