//! Tag filters (`HydrusTags.TagFilter`, serialisable type 44).
//!
//! A tag filter is a list of rules, each whitelisting or blacklisting a
//! *tag slice*: a whole tag (`"blue eyes"`, `"series:metroid"`), a namespace
//! (`"series:"`), all namespaced tags (`":"`) or all unnamespaced tags
//! (`""`). They are used for Client API search restrictions, tag display
//! filters and import filters.

use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::tag::split_tag;

use super::util::{DecodeResult, int, list, malformed, string, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType::TAG_FILTER;

/// Whether a rule lets matching tags through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagRule {
    /// `HC.FILTER_WHITELIST` (0).
    Allow,
    /// `HC.FILTER_BLACKLIST` (1).
    Block,
}

impl TagRule {
    pub const fn code(self) -> i64 {
        match self {
            TagRule::Allow => 0,
            TagRule::Block => 1,
        }
    }

    pub const fn from_code(code: i64) -> Option<TagRule> {
        match code {
            0 => Some(TagRule::Allow),
            1 => Some(TagRule::Block),
            _ => None,
        }
    }
}

/// A decoded tag filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagFilter {
    /// The rules in stored order. The reference loads them into a dict, so a
    /// later rule for the same slice replaces an earlier one.
    pub rules: Vec<(String, TagRule)>,
}

impl TagFilter {
    /// Decode a tag filter object.
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<TagFilter> {
        object.expect_kind(KIND)?;
        object.check_not_future()?;
        Self::from_info(&object.info())
    }

    /// Decode a tag filter stored as a nested tuple.
    pub fn from_tuple(value: &PyJson) -> DecodeResult<TagFilter> {
        Self::from_object(&SerialisableObject::from_tuple(value)?)
    }

    fn from_info(info: &PyJson) -> DecodeResult<TagFilter> {
        let rules = list(KIND, info, "rules")?
            .iter()
            .map(|rule| {
                let [slice, code] = tuple::<2>(KIND, rule, "rule")?;
                let slice = string(KIND, slice, "tag slice")?;
                let code = int(KIND, code, "rule")?;
                let rule = TagRule::from_code(code)
                    .ok_or_else(|| malformed(KIND, format!("unknown rule {code}")))?;
                Ok((slice, rule))
            })
            .collect::<DecodeResult<_>>()?;
        Ok(TagFilter { rules })
    }

    /// The effective rules, one per slice, as the reference sees them.
    pub fn effective_rules(&self) -> BTreeMap<&str, TagRule> {
        self.rules.iter().map(|(s, r)| (s.as_str(), *r)).collect()
    }

    /// Whether the filter lets everything through (it has no blocking rule).
    pub fn allows_everything(&self) -> bool {
        self.effective_rules()
            .values()
            .all(|rule| *rule == TagRule::Allow)
    }

    /// Precompute the lookup sets for testing many tags.
    pub fn compile(&self) -> CompiledTagFilter {
        let mut compiled = CompiledTagFilter::default();
        for (slice, rule) in self.effective_rules() {
            let allow = rule == TagRule::Allow;
            if slice.is_empty() {
                if allow {
                    compiled.all_unnamespaced_allowed = true;
                } else {
                    compiled.all_unnamespaced_blocked = true;
                }
                compiled.namespaces_interesting = true;
            } else if slice == ":" {
                if allow {
                    compiled.all_namespaced_allowed = true;
                } else {
                    compiled.all_namespaced_blocked = true;
                }
                compiled.namespaces_interesting = true;
            } else if is_namespace_slice(slice) {
                let namespace = slice[..slice.len() - 1].to_owned();
                if allow {
                    compiled.namespaces_allowed.insert(namespace);
                } else {
                    compiled.namespaces_blocked.insert(namespace);
                }
                compiled.namespaces_interesting = true;
            } else {
                if allow {
                    compiled.tags_allowed.insert(slice.to_owned());
                } else {
                    compiled.tags_blocked.insert(slice.to_owned());
                }
                compiled.tags_interesting = true;
            }
        }
        compiled
    }

    /// Whether `tag` passes the filter (`TagFilter.TagOK`).
    pub fn allows(&self, tag: &str, apply_unnamespaced_rules_to_namespaced_tags: bool) -> bool {
        self.compile()
            .allows(tag, apply_unnamespaced_rules_to_namespaced_tags)
    }
}

/// `"series:"` is a namespace slice; `""` and `":"` are the special slices,
/// and anything with more than one colon is a whole tag.
fn is_namespace_slice(slice: &str) -> bool {
    !slice.is_empty()
        && slice != ":"
        && slice.ends_with(':')
        && slice.bytes().filter(|b| *b == b':').count() == 1
}

/// A tag filter prepared for testing tags, mirroring the reference's rule cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompiledTagFilter {
    all_unnamespaced_allowed: bool,
    all_namespaced_allowed: bool,
    namespaces_allowed: BTreeSet<String>,
    tags_allowed: BTreeSet<String>,
    all_unnamespaced_blocked: bool,
    all_namespaced_blocked: bool,
    namespaces_blocked: BTreeSet<String>,
    tags_blocked: BTreeSet<String>,
    namespaces_interesting: bool,
    tags_interesting: bool,
}

impl CompiledTagFilter {
    /// Whether `tag` passes. Exact tag rules win over namespace rules, which
    /// win over the "all (un)namespaced" rules; with no applicable rule a tag
    /// passes.
    pub fn allows(&self, tag: &str, apply_unnamespaced_rules_to_namespaced_tags: bool) -> bool {
        if self.tags_interesting {
            if self.tags_allowed.contains(tag) {
                return true;
            }
            if self.tags_blocked.contains(tag) {
                return false;
            }
            if apply_unnamespaced_rules_to_namespaced_tags {
                let (namespace, subtag) = split_tag(tag);
                if !namespace.is_empty() {
                    if self.tags_allowed.contains(subtag) {
                        return true;
                    }
                    if self.tags_blocked.contains(subtag) {
                        return false;
                    }
                }
            }
        }
        if self.namespaces_interesting {
            let (namespace, _) = split_tag(tag);
            if namespace.is_empty() {
                if self.all_unnamespaced_allowed {
                    return true;
                }
                if self.all_unnamespaced_blocked {
                    return false;
                }
            } else {
                if self.namespaces_allowed.contains(namespace) {
                    return true;
                }
                if self.namespaces_blocked.contains(namespace) {
                    return false;
                }
                if self.all_namespaced_allowed {
                    return true;
                }
                if self.all_namespaced_blocked {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(rules: &str) -> TagFilter {
        TagFilter::from_tuple(&PyJson::parse(&format!("[44, 1, {rules}]")).unwrap()).unwrap()
    }

    #[test]
    fn exceptions_beat_blanket_rules() {
        // "everything blocked except safe", as the fixture's restricted API key has
        let f = filter(r#"[["", 1], [":", 1], ["safe", 0]]"#);
        assert!(f.allows("safe", false));
        assert!(!f.allows("explicit", false));
        assert!(!f.allows("series:metroid", false));
        assert!(!f.allows("rating:safe", false));
        assert!(f.allows("rating:safe", true));
        assert!(!f.allows_everything());
    }

    #[test]
    fn namespace_rules() {
        let f = filter(r#"[[":", 1], ["series:", 0], ["series:bad", 1]]"#);
        assert!(f.allows("series:metroid", false));
        assert!(!f.allows("series:bad", false));
        assert!(!f.allows("character:samus aran", false));
        assert!(f.allows("unnamespaced", false));
        assert!(filter("[]").allows_everything());
    }

    #[test]
    fn later_rules_replace_earlier_ones() {
        let f = filter(r#"[["a", 0], ["a", 1]]"#);
        assert!(!f.allows("a", false));
    }
}
