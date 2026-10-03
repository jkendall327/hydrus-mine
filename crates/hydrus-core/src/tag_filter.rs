//! Tag filters: white/blacklist rules over tag *slices*.
//!
//! A slice is one of: `""` (every unnamespaced tag), `":"` (every namespaced
//! tag), `"ns:"` (every tag in namespace `ns`), or a whole tag. More specific
//! rules win: tag rules over namespace rules over the two catch-alls. Used for
//! import filtering and Client API search restrictions.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::numbers::human_int;
use crate::tag::split_tag;

/// Whether a slice is allowed or blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterRule {
    Whitelist,
    Blacklist,
}

pub const UNNAMESPACED: &str = "";
pub const NAMESPACED: &str = ":";

/// Above this many rules, descriptions summarise instead of listing.
const TOO_MANY_RULES: usize = 12;

/// A set of rules; tags without a matching rule are allowed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagFilter {
    rules: BTreeMap<String, FilterRule>,
}

fn is_namespace_slice(slice: &str) -> bool {
    slice != UNNAMESPACED
        && slice != NAMESPACED
        && slice.ends_with(':')
        && slice.matches(':').count() == 1
}

fn pretty_slice(slice: &str) -> String {
    match slice {
        UNNAMESPACED => "unnamespaced tags".into(),
        NAMESPACED => "namespaced tags".into(),
        s if is_namespace_slice(s) => format!("'{}' tags", &s[..s.len() - 1]),
        s => s.into(),
    }
}

impl TagFilter {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_rule(mut self, slice: impl Into<String>, rule: FilterRule) -> Self {
        self.set_rule(slice, rule);
        self
    }

    pub fn set_rule(&mut self, slice: impl Into<String>, rule: FilterRule) {
        self.rules.insert(slice.into(), rule);
    }

    pub fn rules(&self) -> impl Iterator<Item = (&str, FilterRule)> {
        self.rules.iter().map(|(s, r)| (s.as_str(), *r))
    }

    /// Whether no rule blocks anything.
    pub fn allows_everything(&self) -> bool {
        self.rules.values().all(|r| *r == FilterRule::Whitelist)
    }

    /// Whether `tag` passes. With `apply_unnamespaced_rules_to_namespaced`,
    /// a rule on `blue eyes` also applies to `character:blue eyes`.
    pub fn tag_ok(&self, tag: &str, apply_unnamespaced_rules_to_namespaced: bool) -> bool {
        let (namespace, subtag) = split_tag(tag);
        let decide = |slice: &str| self.rules.get(slice).map(|r| *r == FilterRule::Whitelist);

        if let Some(ok) = decide(tag) {
            return ok;
        }
        if apply_unnamespaced_rules_to_namespaced
            && !namespace.is_empty()
            && let Some(ok) = decide(subtag)
        {
            return ok;
        }
        if namespace.is_empty() {
            decide(UNNAMESPACED).unwrap_or(true)
        } else {
            decide(&format!("{namespace}:"))
                .or_else(|| decide(NAMESPACED))
                .unwrap_or(true)
        }
    }

    /// The rules' slices: (blacklisted, whitelisted), each sorted.
    fn sides(&self) -> (Vec<&str>, Vec<&str>) {
        let mut black = Vec::new();
        let mut white = Vec::new();
        for (slice, rule) in &self.rules {
            match rule {
                FilterRule::Blacklist => black.push(slice.as_str()),
                FilterRule::Whitelist => white.push(slice.as_str()),
            }
        }
        (black, white)
    }

    /// What the filter blocks, as a blacklist ("blacklisting on goblin,
    /// orc"; `ToBlacklistString`).
    pub fn to_blacklist_string(&self) -> String {
        let (black, white) = self.sides();
        if black.is_empty() {
            return "no blacklist set".into();
        }
        let list = |slices: &[&str]| {
            slices
                .iter()
                .map(|s| pretty_slice(s))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let both: BTreeSet<&str> = [UNNAMESPACED, NAMESPACED].into_iter().collect();
        let mut text = if black.iter().copied().collect::<BTreeSet<_>>() == both {
            "blacklisting on any tags".to_owned()
        } else if black.len() > TOO_MANY_RULES {
            format!("blacklisting on {} rules", human_int(black.len() as u64))
        } else {
            format!("blacklisting on {}", list(&black))
        };
        if white.len() > TOO_MANY_RULES {
            text.push_str(&format!(
                " except {} other rules",
                human_int(white.len() as u64)
            ));
        } else if !white.is_empty() {
            text.push_str(&format!(" except {}", list(&white)));
        }
        text
    }

    /// What the filter lets through, as a tag import's "get tags" says it
    /// ("all tags except goblin"; `ToFilterString`).
    pub fn to_filter_string(&self) -> String {
        let (black, white) = self.sides();
        if black.is_empty() {
            return "all tags".into();
        }
        let list = |slices: &[&str]| {
            slices
                .iter()
                .map(|s| pretty_slice(s))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let catch_all = |s: &&str| *s == UNNAMESPACED || *s == NAMESPACED;
        let functional_black: Vec<&str> = black.iter().copied().filter(|s| !catch_all(s)).collect();
        let functional_white: Vec<&str> = white.iter().copied().filter(|s| !catch_all(s)).collect();
        let still_filtering = |text: &mut String| {
            if functional_black.len() > TOO_MANY_RULES {
                text.push_str(&format!(
                    " while still filtering on {} rules",
                    human_int(functional_black.len() as u64)
                ));
            } else if !functional_black.is_empty() {
                text.push_str(&format!(
                    " while still filtering out {}",
                    list(&functional_black)
                ));
            }
        };
        let has_unnamespaced = black.contains(&UNNAMESPACED);
        let has_namespaced = black.contains(&NAMESPACED);
        if has_unnamespaced && has_namespaced {
            let mut text = if white.is_empty() {
                "no tags!".to_owned()
            } else if white.len() > TOO_MANY_RULES {
                format!("only adding on {} rules", human_int(white.len() as u64))
            } else {
                format!("only adding {}", list(&white))
            };
            if !white.is_empty() {
                still_filtering(&mut text);
            }
            text
        } else if has_unnamespaced || has_namespaced {
            let mut text = if has_unnamespaced {
                "all namespaced tags".to_owned()
            } else {
                "all unnamespaced tags".to_owned()
            };
            if white.len() > TOO_MANY_RULES {
                text.push_str(&format!(
                    " and {} other rules",
                    human_int(functional_white.len() as u64)
                ));
            } else if !white.is_empty() {
                text.push_str(&format!(" and {}", list(&functional_white)));
            }
            still_filtering(&mut text);
            text
        } else {
            let mut text = if black.len() > TOO_MANY_RULES {
                format!("all tags except on {} rules", human_int(black.len() as u64))
            } else {
                format!("all tags except {}", list(&black))
            };
            if functional_white.len() > TOO_MANY_RULES {
                text.push_str(&format!(
                    " while still allowing on {} rules",
                    human_int(functional_white.len() as u64)
                ));
            } else if !functional_white.is_empty() {
                text.push_str(&format!(
                    " while still allowing {}",
                    list(&functional_white)
                ));
            }
            text
        }
    }

    /// A sentence describing what the filter permits, e.g. "only allowing safe".
    pub fn to_permitted_string(&self) -> String {
        let mut blacklist: Vec<&str> = Vec::new();
        let mut whitelist: Vec<&str> = Vec::new();
        for (slice, rule) in &self.rules {
            match rule {
                FilterRule::Blacklist => blacklist.push(slice),
                FilterRule::Whitelist => whitelist.push(slice),
            }
        }
        let is_catch_all = |s: &&str| *s == UNNAMESPACED || *s == NAMESPACED;
        let functional_black: Vec<&str> = blacklist
            .iter()
            .copied()
            .filter(|s| !is_catch_all(s))
            .collect();
        let functional_white: Vec<&str> = whitelist
            .iter()
            .copied()
            .filter(|s| !is_catch_all(s))
            .collect();
        let list = |slices: &[&str]| {
            slices
                .iter()
                .map(|s| pretty_slice(s))
                .collect::<Vec<_>>()
                .join(", ")
        };

        if blacklist.is_empty() {
            return "allowing all tags".into();
        }
        let blocks_all = blacklist.contains(&UNNAMESPACED) && blacklist.contains(&NAMESPACED);
        let mut text;
        if blocks_all {
            if whitelist.is_empty() {
                text = "not allowing any tags".into();
            } else {
                text = if whitelist.len() > TOO_MANY_RULES {
                    format!("only allowing on {} rules", whitelist.len())
                } else {
                    format!("only allowing {}", list(&whitelist))
                };
                if !functional_black.is_empty() {
                    if functional_black.len() > TOO_MANY_RULES {
                        text +=
                            &format!(" while still disllowing {} rules", functional_black.len());
                    } else {
                        text += &format!(" while still disallowing {}", list(&functional_black));
                    }
                }
            }
        } else if blacklist.contains(&UNNAMESPACED) || blacklist.contains(&NAMESPACED) {
            text = if blacklist.contains(&UNNAMESPACED) {
                "allowing all namespaced tags".into()
            } else {
                "allowing all unnamespaced tags".into()
            };
            if whitelist.len() > TOO_MANY_RULES {
                text += &format!(" and {} other rules", functional_white.len());
            } else if !whitelist.is_empty() {
                text += &format!(" and {}", list(&functional_white));
            }
            if !functional_black.is_empty() {
                text += &format!(" while still disallowing {}", list(&functional_black));
            }
        } else {
            text = if blacklist.len() > TOO_MANY_RULES {
                format!("allowing all tags except on {} rules", blacklist.len())
            } else {
                format!("allowing all tags except {}", list(&blacklist))
            };
            if !functional_white.is_empty() {
                text += &format!(" while still allowing {}", list(&functional_white));
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_rules_win() {
        let f = TagFilter::new()
            .with_rule(UNNAMESPACED, FilterRule::Blacklist)
            .with_rule(NAMESPACED, FilterRule::Blacklist)
            .with_rule("safe", FilterRule::Whitelist)
            .with_rule("creator:", FilterRule::Whitelist)
            .with_rule("creator:bad", FilterRule::Blacklist);
        assert!(f.tag_ok("safe", false));
        assert!(!f.tag_ok("unsafe", false));
        assert!(f.tag_ok("creator:someone", false));
        assert!(!f.tag_ok("creator:bad", false));
        assert!(!f.tag_ok("series:x", false));
        assert!(!f.tag_ok("rating:safe", false));
        assert!(f.tag_ok("rating:safe", true));
        assert!(!f.allows_everything());
        assert_eq!(
            f.to_permitted_string(),
            "only allowing 'creator' tags, safe while still disallowing creator:bad"
        );
        assert_eq!(TagFilter::new().to_permitted_string(), "allowing all tags");
    }
}
