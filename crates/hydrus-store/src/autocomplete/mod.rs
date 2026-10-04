//! Tag autocomplete: which tags match what a user has typed, with counts.
//!
//! [`AutocompleteInput`] parses what was typed; under a tag service's
//! [`AutocompleteRules`] it yields a [`TagQuery`], which [`search_tags`] runs
//! over a [`TagSearchScope`] (file domains × tag services, storage or display
//! tags). The matching rules reproduce the reference's (see [`pattern`]).

pub mod input;
pub mod pattern;
pub mod search;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use hydrus_core::ServiceKey;

pub use input::{AutocompleteInput, AutocompleteRules, TagQuery};
pub use search::{
    CountDomain, CountRange, TagDisplayType, TagMatch, TagSearchScope, search_tags,
    search_tags_for_write,
};

use crate::settings::Setting;

/// Autocomplete rules for the tag services that have non-default ones, by
/// tag service key (hex). "All known tags" can have its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutocompleteSettings {
    pub services: BTreeMap<String, AutocompleteRules>,
}

impl AutocompleteSettings {
    /// The rules for a tag service.
    pub fn rules(&self, service: &ServiceKey) -> AutocompleteRules {
        self.services
            .get(&service.to_hex())
            .copied()
            .unwrap_or_default()
    }
}

impl Setting for AutocompleteSettings {
    const KEY: &'static str = "tag_autocomplete";
}
