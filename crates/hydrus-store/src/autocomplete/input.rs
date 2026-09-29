//! What a user typed into a tag autocomplete box, and the search it asks for.
//!
//! The raw text is cleaned like a tag, a leading `-` makes the search
//! exclusive, runs of `*` collapse, and the subtag is reduced to its
//! [searchable](crate::text::searchable_subtag) form. Whether a search is run
//! at all depends on the tag service's [`AutocompleteRules`]: "fetch all"
//! searches (`*`, `series:*`, `series:`) are refused unless allowed.

use serde::{Deserialize, Serialize};

use hydrus_core::tag::{clean_tag, split_tag};

use crate::text::searchable_tag;

/// Per-tag-service rules for which autocomplete searches run. Everything is
/// off by default, as in the reference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutocompleteRules {
    /// `char` also searches namespaces, finding `character:samus aran`.
    pub search_namespaces_into_full_tags: bool,
    /// `samus` searches `*:samus*`, i.e. in every namespace.
    pub unnamespaced_search_gives_any_namespace_wildcards: bool,
    /// `series:` alone fetches every `series:` tag.
    pub namespace_bare_fetch_all_allowed: bool,
    /// `series:*` fetches every `series:` tag.
    pub namespace_fetch_all_allowed: bool,
    /// `*` fetches every tag.
    pub fetch_all_allowed: bool,
}

/// Autocomplete input, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteInput {
    /// Whether the text looks like a system predicate, which tag autocomplete
    /// never searches for.
    system: bool,
    inclusive: bool,
    /// The cleaned text with runs of `*` collapsed.
    content: String,
}

/// A tag search autocomplete will run: the searchable text, always ending in
/// a wildcard (e.g. `samus ar*`, `series:met*`, `*eyes*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagQuery {
    pub text: String,
    /// Also match namespaces that start with the text (see
    /// [`AutocompleteRules::search_namespaces_into_full_tags`]).
    pub search_namespaces_into_full_tags: bool,
}

fn collapse_wildcards(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if !(c == '*' && out.ends_with('*')) {
            out.push(c);
        }
    }
    out
}

impl AutocompleteInput {
    pub fn parse(raw: &str) -> Self {
        Self {
            system: raw.starts_with("system:"),
            inclusive: !raw.starts_with('-'),
            content: collapse_wildcards(&clean_tag(raw)),
        }
    }

    /// Whether the user asked for tags to *exclude* (typed a leading `-`).
    pub fn inclusive(&self) -> bool {
        self.inclusive
    }

    /// The cleaned-up search text, as shown back to the user.
    pub fn search_text(&self) -> String {
        self.search_text_with(None, false)
    }

    /// The search to run, or `None` if this input doesn't search for tags
    /// under these rules.
    pub fn tag_query(&self, rules: &AutocompleteRules) -> Option<TagQuery> {
        if self.system {
            return None;
        }
        let text = self.search_text_with(Some(rules), false);
        if text.is_empty() {
            return None;
        }
        let (namespace, subtag) = split_tag(&text);
        let any_namespace = namespace.is_empty() || namespace == "*";
        let bare_namespace_fetch_all = !any_namespace && subtag.is_empty();
        let namespace_fetch_all = !any_namespace && subtag == "*";
        let fetch_all = any_namespace && subtag == "*";
        let bare_ok =
            rules.namespace_bare_fetch_all_allowed || rules.search_namespaces_into_full_tags;
        let namespace_ok = bare_ok || rules.namespace_fetch_all_allowed;
        if (bare_namespace_fetch_all && !bare_ok)
            || (namespace_fetch_all && !namespace_ok)
            || (fetch_all && !rules.fetch_all_allowed)
        {
            return None;
        }
        Some(TagQuery {
            text: self.search_text_with(Some(rules), true),
            search_namespaces_into_full_tags: rules.search_namespaces_into_full_tags,
        })
    }

    /// The reference's `_GetSearchText`: `rules` enables the "unnamespaced
    /// means any namespace" conversion, `autocompleting` appends a wildcard.
    fn search_text_with(&self, rules: Option<&AutocompleteRules>, autocompleting: bool) -> String {
        if self.content.is_empty() {
            return String::new();
        }
        let mut text = searchable_tag(&self.content);
        if rules.is_some_and(|r| r.unnamespaced_search_gives_any_namespace_wildcards)
            && !text.contains(':')
        {
            if text.is_empty() {
                return text;
            }
            text = format!("*:{text}");
        }
        if autocompleting {
            let (namespace, subtag) = split_tag(&text);
            if (!namespace.is_empty() || !subtag.is_empty()) && !subtag.ends_with('*') {
                text.push('*');
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(raw: &str) -> Option<String> {
        AutocompleteInput::parse(raw)
            .tag_query(&AutocompleteRules::default())
            .map(|q| q.text)
    }

    #[test]
    fn parses_like_the_reference() {
        let p = AutocompleteInput::parse("-Blue_Eyes");
        assert!(!p.inclusive());
        assert_eq!(p.search_text(), "blue eyes");
        assert_eq!(query("-Blue_Eyes").as_deref(), Some("blue eyes*"));
        assert_eq!(query("::").as_deref(), Some("::*"));
        assert_eq!(query(":)").as_deref(), Some("::*"));
        assert_eq!(query("*eyes").as_deref(), Some("*eyes*"));
        assert_eq!(query("ser*:met").as_deref(), Some("ser*:met*"));
        // fetch-alls are refused by default
        for raw in [
            "*",
            "character:",
            "character:*",
            "**:*",
            "system:inbox",
            "  -  ",
        ] {
            assert_eq!(query(raw), None, "{raw}");
        }
        assert_eq!(
            AutocompleteInput::parse("system:inbox").search_text(),
            "inbox"
        );
    }

    #[test]
    fn rules_open_up_fetch_alls() {
        let rules = AutocompleteRules {
            namespace_fetch_all_allowed: true,
            fetch_all_allowed: true,
            unnamespaced_search_gives_any_namespace_wildcards: true,
            ..AutocompleteRules::default()
        };
        let q = |raw: &str| {
            AutocompleteInput::parse(raw)
                .tag_query(&rules)
                .map(|q| q.text)
        };
        assert_eq!(q("character:*").as_deref(), Some("character:*"));
        assert_eq!(q("*").as_deref(), Some("*:*"));
        assert_eq!(q("samus").as_deref(), Some("*:samus*"));
        assert_eq!(q("character:"), None);
    }
}
