//! The sort control's choices, named and ordered as the reference's
//! (`CC.sort_type_string_lookup`, `MediaSort.GetSortOrderStrings`).

use hydrus_core::pages::PageSortBy;
use hydrus_search::{SortBy, SortOrder};

/// One sort type as the control offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortChoice {
    pub by: SortBy,
    /// e.g. `time: import time`.
    pub name: String,
    /// The two orders' names, ascending first (e.g. `oldest first`,
    /// `newest first`).
    pub orders: [&'static str; 2],
    /// The order chosen with the type.
    pub default_order: SortOrder,
}

/// (group, name, ascending, descending, default order)
fn facts(
    by: SortBy,
) -> (
    Option<&'static str>,
    &'static str,
    &'static str,
    &'static str,
    SortOrder,
) {
    use SortOrder::{Ascending as A, Descending as D};
    match by {
        SortBy::NumCollectionFiles => (
            Some("collections"),
            "number of files in collection",
            "fewest first",
            "most first",
            D,
        ),
        SortBy::Width => (
            Some("dimensions"),
            "width",
            "slimmest first",
            "widest first",
            A,
        ),
        SortBy::Height => (
            Some("dimensions"),
            "height",
            "shortest first",
            "tallest first",
            A,
        ),
        SortBy::NumPixels => (
            Some("dimensions"),
            "number of pixels",
            "ascending",
            "descending",
            D,
        ),
        SortBy::Ratio => (
            Some("dimensions"),
            "resolution ratio",
            "tallest first",
            "widest first",
            A,
        ),
        SortBy::Duration => (
            Some("duration"),
            "duration",
            "shortest first",
            "longest first",
            D,
        ),
        SortBy::Framerate => (
            Some("duration"),
            "framerate",
            "slowest first",
            "fastest first",
            D,
        ),
        SortBy::NumFrames => (
            Some("duration"),
            "number of frames",
            "smallest first",
            "largest first",
            D,
        ),
        SortBy::ApproxBitrate => (
            Some("file"),
            "approximate bitrate",
            "smallest first",
            "largest first",
            D,
        ),
        SortBy::FileSize => (
            Some("file"),
            "filesize",
            "smallest first",
            "largest first",
            D,
        ),
        SortBy::Hash => (
            Some("file"),
            "hash - sha256",
            "lexicographic",
            "reverse lexicographic",
            A,
        ),
        SortBy::PixelHash => (
            Some("file"),
            "hash - pixel hash",
            "lexicographic",
            "reverse lexicographic",
            A,
        ),
        SortBy::Blurhash => (
            Some("file"),
            "hash - blurhash",
            "lexicographic",
            "reverse lexicographic",
            A,
        ),
        SortBy::Mime => (Some("file"), "filetype", "filetype", "filetype", A),
        SortBy::HasAudio => (Some("file"), "has audio", "audio first", "silent first", A),
        SortBy::AverageColourLightness => (
            Some("average colour"),
            "lightness",
            "darkest first",
            "lightest first",
            D,
        ),
        SortBy::AverageColourChromaticMagnitude => (
            Some("average colour"),
            "chromatic magnitude",
            "greys first",
            "colours first",
            D,
        ),
        SortBy::AverageColourGreenRed => (
            Some("average colour"),
            "balance - green-red",
            "greens first",
            "reds first",
            A,
        ),
        SortBy::AverageColourBlueYellow => (
            Some("average colour"),
            "balance - blue-yellow",
            "blues first",
            "yellows first",
            A,
        ),
        SortBy::AverageColourHue => (
            Some("average colour"),
            "hue",
            "rainbow - red first",
            "rainbow - purple first",
            A,
        ),
        SortBy::Random => (None, "random", "random", "random", A),
        SortBy::NumTags => (Some("tags"), "number of tags", "ascending", "descending", A),
        SortBy::ImportTime => (
            Some("time"),
            "import time",
            "oldest first",
            "newest first",
            D,
        ),
        SortBy::ModifiedTime => (
            Some("time"),
            "modified time",
            "oldest first",
            "newest first",
            D,
        ),
        SortBy::ArchivedTime => (
            Some("time"),
            "archived time",
            "oldest first",
            "newest first",
            D,
        ),
        SortBy::LastViewedTime => (
            Some("time"),
            "last viewed time",
            "oldest first",
            "newest first",
            D,
        ),
        SortBy::MediaViews => (Some("views"), "views", "ascending", "descending", D),
        SortBy::MediaViewtime => (Some("views"), "viewtime", "ascending", "descending", D),
    }
}

/// Every sort type, in the control's order: by name, but width before
/// height (`CC.SYSTEM_SORT_TYPES_SORT_CONTROL_SORTED`).
pub fn choices() -> Vec<SortChoice> {
    let mut choices: Vec<SortChoice> = SortBy::ALL
        .iter()
        .map(|&by| {
            let (group, name, ascending, descending, default_order) = facts(by);
            SortChoice {
                by,
                name: match group {
                    Some(group) => format!("{group}: {name}"),
                    None => name.to_owned(),
                },
                orders: [ascending, descending],
                default_order,
            }
        })
        .collect();
    let key = |c: &SortChoice| match c.by {
        SortBy::Width => "dimensions 0".to_owned(),
        SortBy::Height => "dimensions 1".to_owned(),
        _ => c.name.clone(),
    };
    choices.sort_by_key(key);
    choices
}

/// A sort type the control offers a page, with the store's: a system
/// sort, one of the options' namespace sorts, or a rating service's
/// (`_PopulateSortMenuOrList`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageChoice {
    pub by: PageSortBy,
    /// e.g. `time: import time`, `tags: series-creator-title`, `rating:
    /// favourites`.
    pub name: String,
    /// The two orders' names, ascending first.
    pub orders: [&'static str; 2],
    /// Whether the order chosen with the type is ascending.
    pub default_ascending: bool,
}

/// The sort types offered for a page sorted by `current`: the system ones
/// in the control's order, then the options' namespace sorts, then each
/// rating service; and `current`, if it is none of those (a custom
/// namespace sort, say).
pub fn page_choices(store: &hydrus_store::Store, current: &PageSortBy) -> Vec<PageChoice> {
    let mut out: Vec<PageChoice> = choices()
        .into_iter()
        .map(|c| PageChoice {
            by: PageSortBy::System(i64::from(c.by.code())),
            name: c.name,
            orders: c.orders,
            default_ascending: c.default_order == SortOrder::Ascending,
        })
        .collect();
    let sorts: hydrus_core::pages::SortSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let snapshot = store.snapshot();
    // (by name, as the reference's services manager lists them)
    let mut ratings: Vec<_> = snapshot
        .services
        .all()
        .filter(|s| s.service_type().is_rating_service())
        .collect();
    ratings.sort_by_key(|s| s.name.to_lowercase());
    let ratings = ratings
        .into_iter()
        .map(|s| PageSortBy::Rating(s.key.clone()));
    let mut others: Vec<PageSortBy> = sorts
        .namespace_sorts
        .into_iter()
        .map(|s| s.by)
        .chain(ratings)
        .collect();
    if !out.iter().any(|c| c.by == *current) && !others.contains(current) {
        others.push(current.clone());
    }
    out.extend(
        others
            .into_iter()
            .map(|by| other_choice(&snapshot.services, by)),
    );
    out
}

/// Wheel traversal omits an appended custom current value, as the Qt menu does.
pub fn known_choice_count(store: &hydrus_store::Store) -> usize {
    page_choices(
        store,
        &PageSortBy::System(i64::from(SortBy::FileSize.code())),
    )
    .len()
}

/// Changing type retains the order when its actual two choice labels are equal.
pub fn type_ascending(
    current: &hydrus_core::pages::PageSort,
    previous: &[PageChoice],
    chosen: &PageChoice,
) -> bool {
    if previous
        .iter()
        .find(|choice| choice.by == current.by)
        .is_some_and(|choice| choice.orders == chosen.orders)
    {
        current.ascending
    } else {
        chosen.default_ascending
    }
}

/// A page's sort as the reference writes it (`MediaSort.ToString`): "sort
/// by time: import time, newest first".
pub fn sort_text(store: &hydrus_store::Store, sort: &hydrus_core::pages::PageSort) -> String {
    let choices = page_choices(store, &sort.by);
    let Some(choice) = choices.iter().find(|c| c.by == sort.by) else {
        return String::new();
    };
    let order = choice.orders[usize::from(!sort.ascending)];
    format!("sort by {}, {order}", choice.name)
}

/// A namespace or rating sort as the control offers it
/// (`GetSortTypeString`, `GetSortOrderStrings`).
fn other_choice(services: &hydrus_store::services::ServiceRegistry, by: PageSortBy) -> PageChoice {
    match &by {
        PageSortBy::Namespaces { namespaces, .. } => PageChoice {
            name: format!("tags: {}", namespaces.join("-")),
            by,
            orders: ["a-z", "z-a"],
            default_ascending: true,
        },
        PageSortBy::Rating(key) => PageChoice {
            name: format!(
                "rating: {}",
                services
                    .by_key(key)
                    .map_or("unknown service", |s| s.name.as_str())
            ),
            by,
            orders: ["ascending", "descending"],
            default_ascending: false,
        },
        PageSortBy::System(code) => PageChoice {
            name: format!("unknown sort {code}"),
            by,
            orders: ["ascending", "descending"],
            default_ascending: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_are_named_and_ordered_as_the_reference_orders_them() {
        let names: Vec<String> = choices().into_iter().map(|c| c.name).collect();
        assert_eq!(names.len(), SortBy::ALL.len());
        assert_eq!(
            names[..8],
            [
                "average colour: balance - blue-yellow",
                "average colour: balance - green-red",
                "average colour: chromatic magnitude",
                "average colour: hue",
                "average colour: lightness",
                "collections: number of files in collection",
                "dimensions: width",
                "dimensions: height",
            ]
        );
        assert_eq!(names.last().unwrap(), "views: viewtime");
    }
}
