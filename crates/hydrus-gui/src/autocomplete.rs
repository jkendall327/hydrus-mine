//! The search box's autocomplete: the tags matching what has been typed,
//! with their counts, as the reference's read autocomplete lists them
//! (display tags, all known tags, in all my files); and before anything is
//! typed, the system predicates that need no more input, with theirs.

use std::sync::Arc;

use hydrus_core::ServiceKey;
use hydrus_core::service::builtin_keys;
use hydrus_store::Store;
use hydrus_store::autocomplete::{
    self, AutocompleteInput, AutocompleteSettings, CountDomain, TagDisplayType, TagSearchScope,
};

/// One suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// As listed, e.g. `blue eyes (12)`.
    pub label: String,
    /// What choosing it adds to the search, e.g. `blue eyes` or `-blue eyes`.
    pub predicate: String,
}

pub struct Autocomplete {
    store: Arc<Store>,
    text: String,
    suggestions: Vec<Suggestion>,
    highlighted: usize,
}

impl std::fmt::Debug for Autocomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Autocomplete")
            .field("text", &self.text)
            .field("suggestions", &self.suggestions.len())
            .field("highlighted", &self.highlighted)
            .finish_non_exhaustive()
    }
}

impl Autocomplete {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            text: String::new(),
            suggestions: Vec::new(),
            highlighted: 0,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    pub fn highlighted(&self) -> Option<usize> {
        (!self.suggestions.is_empty()).then_some(self.highlighted)
    }

    /// Move the highlight up (negative) or down, staying in the list.
    pub fn move_highlight(&mut self, by: isize) {
        if let Some(last) = self.suggestions.len().checked_sub(1) {
            self.highlighted = self.highlighted.saturating_add_signed(by).min(last);
        }
    }

    /// What pressing enter adds: the highlighted suggestion, else the text
    /// as typed (a system predicate, say).
    pub fn chosen(&self) -> Option<String> {
        match self.highlighted() {
            Some(i) => Some(self.suggestions[i].predicate.clone()),
            None => Some(self.text.trim().to_owned()).filter(|t| !t.is_empty()),
        }
    }

    pub fn clear(&mut self) {
        self.set_text("");
    }

    /// The text changed: search again.
    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.highlighted = 0;
        self.suggestions = self.search().unwrap_or_default();
    }

    fn search(&self) -> Option<Vec<Suggestion>> {
        if self.text.trim().is_empty() {
            return self.system_predicates();
        }
        let input = AutocompleteInput::parse(&self.text);
        let snapshot = self.store.snapshot();
        let registry = &snapshot.services;
        let domain = registry
            .builtin(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)
            .ok()?
            .id;
        let all_known_tags = ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec());
        let matches = self
            .store
            .read(|conn| {
                let rules = hydrus_store::settings::get::<AutocompleteSettings>(conn)?
                    .rules(&all_known_tags);
                let Some(query) = input.tag_query(&rules) else {
                    return Ok(Vec::new());
                };
                let scope = TagSearchScope {
                    domains: vec![CountDomain {
                        service: domain,
                        exact: true,
                    }],
                    tag_service: None,
                    display: TagDisplayType::Display,
                    include_current: true,
                    include_pending: true,
                };
                autocomplete::search_tags(conn, registry, &snapshot.display, &scope, &query)
            })
            .ok()?;
        // the exact match first, then the most used
        let typed = input.search_text();
        let mut matches = matches;
        matches.sort_by(|a, b| {
            (b.tag == typed)
                .cmp(&(a.tag == typed))
                .then(b.count.min_total().cmp(&a.count.min_total()))
                .then(a.tag.cmp(&b.tag))
        });
        let sign = if input.inclusive() { "" } else { "-" };
        Some(
            matches
                .into_iter()
                .map(|m| Suggestion {
                    label: format!("{sign}{} {}", m.tag, m.count.suffix()),
                    predicate: format!("{sign}{}", m.tag),
                })
                .collect(),
        )
    }

    /// `system:everything`, `system:inbox` and `system:archive`, with how
    /// many files each finds in all my files (`_GetFileSystemPredicates`).
    fn system_predicates(&self) -> Option<Vec<Suggestion>> {
        let snapshot = self.store.snapshot();
        let domain = snapshot
            .services
            .builtin(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)
            .ok()?
            .id;
        let (everything, inbox): (i64, i64) = self
            .store
            .read(|conn| {
                let everything = conn.query_row(
                    "SELECT count(*) FROM file_domain_current WHERE service_id = ?",
                    [domain],
                    |r| r.get(0),
                )?;
                let inbox = conn.query_row(
                    "SELECT count(*) FROM file_inbox AS i
                     JOIN file_domain_current AS d ON d.hash_id = i.hash_id AND d.service_id = ?",
                    [domain],
                    |r| r.get(0),
                )?;
                Ok((everything, inbox))
            })
            .ok()?;
        Some(
            [
                ("system:everything", everything),
                ("system:inbox", inbox),
                ("system:archive", everything - inbox),
            ]
            .into_iter()
            .map(|(predicate, count)| Suggestion {
                label: format!("{predicate} ({count})"),
                predicate: predicate.to_owned(),
            })
            .collect(),
        )
    }
}
