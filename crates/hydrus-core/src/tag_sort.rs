//! Sorting tags for a tag list, as the reference does
//! (`ClientTagSorting.SortTags`): by tag, subtag or count, optionally
//! grouped by namespace.

use crate::sort::{HumanSortKey, human_sort_key};
use crate::tag::split_tag;

/// What a tag list sorts by (`ClientTagSorting.SORT_BY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TagSortType {
    Tag,
    Subtag,
    Count,
}

/// How tags are grouped (`ClientTagSorting.GROUP_BY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TagGroupBy {
    Nothing,
    /// Namespaced tags first, by namespace a-z, then unnamespaced.
    NamespaceAz,
    /// By the user's namespace order (`user_namespace_group_by_sort`).
    NamespaceUser,
}

/// A tag list's sort (`ClientTagSorting.TagSort`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TagSort {
    pub sort_type: TagSortType,
    pub ascending: bool,
    pub group_by: TagGroupBy,
}

impl TagSort {
    /// The reference's default for search pages and the media viewer
    /// (`STATICGetTextASCUserGroupedDefault`).
    pub const DEFAULT: TagSort = TagSort {
        sort_type: TagSortType::Tag,
        ascending: true,
        group_by: TagGroupBy::NamespaceUser,
    };
}

/// The reference's default `user_namespace_group_by_sort` (`""` stands for
/// unnamespaced tags, `":"` for any other namespace).
pub fn default_user_namespaces() -> Vec<String> {
    ["creator", "series", "character", "species", "", "meta"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// `lexicographic_key`: namespace then subtag, an unnamespaced tag sorting
/// by its subtag in both places.
fn lexicographic_key(tag: &str) -> (HumanSortKey, HumanSortKey) {
    let (namespace, subtag) = split_tag(tag);
    let subtag = human_sort_key(subtag);
    if namespace.is_empty() {
        (subtag.clone(), subtag)
    } else {
        (human_sort_key(namespace), subtag)
    }
}

/// A group's key; `None` sorts before `Some` (as Python's shorter tuple does).
type GroupKey = (usize, Option<HumanSortKey>);

fn namespace_az_key(tag: &str) -> GroupKey {
    match split_tag(tag).0 {
        "" => (1, None),
        namespace => (0, Some(human_sort_key(namespace))),
    }
}

fn namespace_user_key(tag: &str, namespaces: &[String]) -> GroupKey {
    let no_namespace = namespaces.len() + 1;
    let any_namespace = namespaces
        .iter()
        .position(|n| n == ":")
        .unwrap_or(namespaces.len());
    let namespace = split_tag(tag).0;
    if let Some(index) = namespaces.iter().position(|n| n == namespace) {
        (index, None)
    } else if namespace.is_empty() {
        (no_namespace, None)
    } else {
        (any_namespace, Some(human_sort_key(namespace)))
    }
}

/// Sort `items` (a tag and its count each) as the reference's tag lists do,
/// given the user's namespace order for [`TagGroupBy::NamespaceUser`].
pub fn sort_tags<T>(
    sort: &TagSort,
    items: &mut [T],
    tag: impl Fn(&T) -> &str,
    count: impl Fn(&T) -> u64,
    user_namespaces: &[String],
) {
    // the reference sorts in passes (each stable), the last pass deciding most
    let by = |items: &mut [T], reverse: bool, key: &dyn Fn(&T) -> _| {
        if reverse {
            items.sort_by_cached_key(|item| std::cmp::Reverse(key(item)));
        } else {
            items.sort_by_cached_key(key);
        }
    };
    let descending = !sort.ascending;
    match sort.sort_type {
        TagSortType::Count => {
            // a-z among equal counts, whichever way counts go
            let lexicographic = |item: &T| lexicographic_key(tag(item));
            by(items, !descending, &lexicographic);
            if descending {
                items.sort_by_cached_key(|item| std::cmp::Reverse(count(item)));
            } else {
                items.sort_by_cached_key(|item| count(item));
            }
        }
        TagSortType::Tag => by(items, descending, &|item: &T| lexicographic_key(tag(item))),
        TagSortType::Subtag => {
            let subtag = |item: &T| human_sort_key(split_tag(tag(item)).1);
            if descending {
                items.sort_by_cached_key(|item| std::cmp::Reverse(subtag(item)));
            } else {
                items.sort_by_cached_key(subtag);
            }
        }
    }
    if sort.sort_type != TagSortType::Subtag {
        match sort.group_by {
            TagGroupBy::Nothing => {}
            TagGroupBy::NamespaceAz => items.sort_by_cached_key(|item| namespace_az_key(tag(item))),
            TagGroupBy::NamespaceUser => {
                items.sort_by_cached_key(|item| namespace_user_key(tag(item), user_namespaces));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(sort: TagSort, tags: &[(&str, u64)]) -> Vec<String> {
        let mut items: Vec<(String, u64)> =
            tags.iter().map(|(t, c)| ((*t).to_owned(), *c)).collect();
        sort_tags(
            &sort,
            &mut items,
            |i| &i.0,
            |i| i.1,
            &default_user_namespaces(),
        );
        items.into_iter().map(|(t, _)| t).collect()
    }

    const TAGS: &[(&str, u64)] = &[
        ("blue eyes", 3),
        ("series:metroid", 5),
        ("meta:highres", 9),
        ("character:samus aran", 5),
        ("page:10", 1),
        ("page:2", 1),
        ("creator:someone", 2),
        ("artist:nobody", 7),
        ("smile", 3),
    ];

    #[test]
    fn the_default_groups_by_the_users_namespaces() {
        assert_eq!(
            sorted(TagSort::DEFAULT, TAGS),
            [
                "creator:someone",
                "series:metroid",
                "character:samus aran",
                "blue eyes",
                "smile",
                "meta:highres",
                "artist:nobody",
                "page:2",
                "page:10",
            ]
        );
    }

    #[test]
    fn counts_sort_with_a_to_z_among_equals() {
        let sort = TagSort {
            sort_type: TagSortType::Count,
            ascending: false,
            group_by: TagGroupBy::Nothing,
        };
        assert_eq!(
            sorted(sort, TAGS),
            [
                "meta:highres",
                "artist:nobody",
                "character:samus aran",
                "series:metroid",
                "blue eyes",
                "smile",
                "creator:someone",
                "page:2",
                "page:10",
            ]
        );
    }
}
