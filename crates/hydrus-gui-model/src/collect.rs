//! The page's collect control, as the reference's (`CollectComboCtrl`,
//! `MediaCollectControl`): the namespaces of the options' namespace sorts,
//! then the star rating services, each checked to collect by it, and
//! whether files that match none collect together.

use hydrus_core::ServiceKey;
use hydrus_core::ServiceType;
use hydrus_core::pages::{PageCollect, PageSortBy, SortSettings};

/// What a choice collects by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectBy {
    Namespace(String),
    Rating(ServiceKey),
}

/// A choice in the collect control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectChoice {
    pub name: String,
    pub by: CollectBy,
}

impl CollectChoice {
    /// Whether `collect` collects by this.
    pub fn checked(&self, collect: &PageCollect) -> bool {
        match &self.by {
            CollectBy::Namespace(namespace) => collect.namespaces.contains(namespace),
            CollectBy::Rating(key) => collect.ratings.contains(key),
        }
    }
}

/// The choices (`_ReinitialiseChoices`): the namespaces in the options'
/// namespace sorts, in order, then the like/dislike and numerical rating
/// services, each kind by name.
pub fn choices(store: &hydrus_store::Store) -> Vec<CollectChoice> {
    let sorts: SortSettings = store.read(hydrus_store::settings::get).unwrap_or_default();
    let mut namespaces: Vec<String> = sorts
        .namespace_sorts
        .into_iter()
        .flat_map(|sort| match sort.by {
            PageSortBy::Namespaces { namespaces, .. } => namespaces,
            _ => Vec::new(),
        })
        .collect();
    namespaces.sort();
    namespaces.dedup();
    let mut out: Vec<CollectChoice> = namespaces
        .into_iter()
        .map(|namespace| CollectChoice {
            name: namespace.clone(),
            by: CollectBy::Namespace(namespace),
        })
        .collect();
    let snapshot = store.snapshot();
    // (`STAR_RATINGS_SERVICES`, a kind at a time, as the services manager
    // lists them: by name)
    for kind in [
        ServiceType::LocalRatingLike,
        ServiceType::LocalRatingNumerical,
        ServiceType::RatingLikeRepository,
        ServiceType::RatingNumericalRepository,
    ] {
        let mut services: Vec<_> = snapshot
            .services
            .all()
            .filter(|s| s.service_type() == kind)
            .collect();
        services.sort_by_key(|s| s.name.to_lowercase());
        out.extend(services.into_iter().map(|s| CollectChoice {
            name: s.name.clone(),
            by: CollectBy::Rating(s.key.clone()),
        }));
    }
    out
}

/// The control's label (`GetValues`): "collect by" the checked choices'
/// names, or "no collections".
pub fn label(choices: &[CollectChoice], collect: &PageCollect) -> String {
    let checked: Vec<&str> = choices
        .iter()
        .filter(|c| c.checked(collect))
        .map(|c| c.name.as_str())
        .collect();
    if checked.is_empty() {
        "no collections".to_owned()
    } else {
        format!("collect by {}", checked.join("-"))
    }
}

/// The collect with a choice checked or not (`CollectValuesChanged`): the
/// checked choices' namespaces and services, in the choices' order.
pub fn toggled(
    choices: &[CollectChoice],
    collect: &PageCollect,
    index: usize,
    on: bool,
) -> PageCollect {
    let mut out = PageCollect {
        namespaces: Vec::new(),
        ratings: Vec::new(),
        collect_unmatched: collect.collect_unmatched,
    };
    for (i, choice) in choices.iter().enumerate() {
        let checked = if i == index {
            on
        } else {
            choice.checked(collect)
        };
        if !checked {
            continue;
        }
        match &choice.by {
            CollectBy::Namespace(namespace) => out.namespaces.push(namespace.clone()),
            CollectBy::Rating(key) => out.ratings.push(key.clone()),
        }
    }
    out
}

/// The collect with unmatched files collected together or left single
/// (`SetCollectUnmatched`, which also takes the checked choices afresh).
pub fn with_unmatched(
    choices: &[CollectChoice],
    collect: &PageCollect,
    collect_unmatched: bool,
) -> PageCollect {
    let collect = PageCollect {
        collect_unmatched,
        ..collect.clone()
    };
    toggled(choices, &collect, usize::MAX, false)
}
