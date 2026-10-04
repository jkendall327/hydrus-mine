//! Sorting a page's files as the reference's pages sort them
//! (`MediaList.Sort`): the fallback sort, then the page's, each stable,
//! by a system sort (as [`super::sort`] does in its page mode), by the
//! tags in some namespaces, or by rating.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use roaring::RoaringBitmap;

use hydrus_core::pages::{PageSort, PageSortBy};
use hydrus_core::sort::{HumanSortKey, human_sort_key};
use hydrus_core::{HashId, TagId};
use hydrus_store::media::Rating;

use super::Result;
use super::context::Env;
use super::sort::{self, FileSort, Mode, SortBy, SortOrder};

/// `ClientTags.TAG_DISPLAY_SINGLE_MEDIA`.
const TAG_DISPLAY_SINGLE_MEDIA: i64 = 2;
/// `ClientTags.TAG_DISPLAY_SELECTION_LIST`.
const TAG_DISPLAY_SELECTION_LIST: i64 = 3;

/// Sort `files` (in the page's order) by `sort`, from that order. A
/// system sort we don't know leaves them as they are.
pub(crate) fn sort_page(env: &Env<'_>, files: &[u32], sort: &PageSort) -> Result<Vec<u32>> {
    let bitmap: RoaringBitmap = files.iter().copied().collect();
    let order = if sort.ascending {
        SortOrder::Ascending
    } else {
        SortOrder::Descending
    };
    match &sort.by {
        PageSortBy::System(code) => match SortBy::from_code(*code) {
            Some(SortBy::NumTags) => {
                let ids: Vec<HashId> = files.iter().map(|&id| HashId(id)).collect();
                let batch = hydrus_store::media::load(
                    env.conn,
                    &env.snapshot.services,
                    Some(&env.snapshot.display),
                    &ids,
                )?;
                let tags = tag_ids_for_sort(env, &batch, sort)?;
                let keys = tags
                    .into_iter()
                    .map(|(id, tags)| (id.0, tags.len()))
                    .collect();
                Ok(stable(files, &keys, !sort.ascending))
            }
            Some(by) => sort::sort_in(env, &bitmap, FileSort { by, order }, Mode::Page(files)),
            None => Ok(files.to_vec()),
        },
        PageSortBy::Namespaces { namespaces, .. } => {
            let keys = namespace_keys(env, files, namespaces, sort)?;
            Ok(stable(files, &keys, !sort.ascending))
        }
        PageSortBy::Rating(service) => {
            let keys = rating_keys(env, files, service)?;
            Ok(stable(files, &keys, !sort.ascending))
        }
    }
}

/// `files` sorted stably by their keys (as Python's `sort(key, reverse)`:
/// reversing keeps ties in their order).
fn stable<K: Ord>(files: &[u32], keys: &HashMap<u32, K>, reverse: bool) -> Vec<u32> {
    let mut out = files.to_vec();
    out.sort_by(|a, b| {
        let ordering = keys.get(a).cmp(&keys.get(b));
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
    out
}

/// Each file's tags in each namespace, as `GetComparableNamespaceSlice`
/// has them: the subtags of its current and pending tags in all known
/// tags, as the display type shows them, in human order.
fn namespace_keys(
    env: &Env<'_>,
    files: &[u32],
    namespaces: &[String],
    sort: &PageSort,
) -> Result<HashMap<u32, Vec<Vec<HumanSortKey>>>> {
    let ids: Vec<HashId> = files.iter().map(|&id| HashId(id)).collect();
    let batch = hydrus_store::media::load(
        env.conn,
        &env.snapshot.services,
        Some(&env.snapshot.display),
        &ids,
    )?;
    let displayed = tag_ids_for_sort(env, &batch, sort)?;
    let mut keys = HashMap::with_capacity(batch.results.len());
    for m in &batch.results {
        let displayed = &displayed[&m.hash_id];
        let pairs: Vec<(&str, &str)> = displayed
            .iter()
            .filter_map(|t| batch.tags.get(t))
            .map(hydrus_core::Tag::split)
            .collect();
        let key = namespaces
            .iter()
            .map(|wanted| {
                let mut subtags: Vec<HumanSortKey> = pairs
                    .iter()
                    .filter(|(namespace, _)| namespace == wanted)
                    .map(|(_, subtag)| human_sort_key(subtag))
                    .collect();
                subtags.sort();
                subtags
            })
            .collect();
        keys.insert(m.hash_id.0, key);
    }
    Ok(keys)
}

/// Displayed current and pending tags in the sort's independent service.
/// Search context toggles and display service do not change MediaSort keys.
pub(crate) fn tag_ids_for_sort(
    env: &Env<'_>,
    batch: &hydrus_store::media::MediaBatch,
    sort: &PageSort,
) -> Result<HashMap<HashId, BTreeSet<TagId>>> {
    use hydrus_store::tag_display::{TagDisplayFilters, TagView};
    let uses_tags = matches!(sort.by, PageSortBy::Namespaces { .. })
        || matches!(sort.by, PageSortBy::System(code) if SortBy::from_code(code) == Some(SortBy::NumTags));
    if !uses_tags {
        return Ok(batch
            .results
            .iter()
            .map(|media| (media.hash_id, BTreeSet::new()))
            .collect());
    }
    let context =
        hydrus_core::search::context::TagContext::new(sort.tag_context.service.clone(), true, true);
    let scope = super::context::resolve_tags(env.snapshot, &context)?;
    let view = match &sort.by {
        PageSortBy::Namespaces {
            tag_display_type: TAG_DISPLAY_SINGLE_MEDIA,
            ..
        } => Some(TagView::SingleMedia),
        PageSortBy::Namespaces {
            tag_display_type: TAG_DISPLAY_SELECTION_LIST,
            ..
        } => Some(TagView::SelectionList),
        _ => None,
    };
    let hidden = match view {
        Some(view) => {
            let filters: TagDisplayFilters = hydrus_store::settings::get(env.conn)?;
            Some(filters.by_service(view, &env.snapshot.services))
        }
        None => None,
    };
    Ok(batch
        .results
        .iter()
        .map(|media| {
            let mut displayed = BTreeSet::new();
            for service in &scope.services {
                if let Some(mapped) = media.tags.get(&service.id) {
                    for status in &scope.statuses {
                        for &stored in mapped.by_status.get(status).into_iter().flatten() {
                            displayed.extend(service.graph.display_tags(stored).filter(|tag| {
                                hidden.as_ref().is_none_or(|hidden| {
                                    batch
                                        .tags
                                        .get(tag)
                                        .is_none_or(|tag| hidden.shows(service.id, tag.as_str()))
                                })
                            }));
                        }
                    }
                }
            }
            (media.hash_id, displayed)
        })
        .collect())
}

/// A rating as a page sorts by it: the number, or -1 for none.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RatingKey(f64);

impl Eq for RatingKey {}

impl PartialOrd for RatingKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RatingKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Each file's rating on `service` (`GetRating` and `deal_with_none`: an
/// unrated file is 0 on an inc/dec service, else -1).
fn rating_keys(
    env: &Env<'_>,
    files: &[u32],
    service: &hydrus_core::ServiceKey,
) -> Result<HashMap<u32, RatingKey>> {
    let service = env.snapshot.services.by_key(service).ok();
    let unrated = match service.map(|s| s.service_type()) {
        Some(hydrus_core::ServiceType::LocalRatingIncDec) => 0.0,
        _ => -1.0,
    };
    let service = service.map(|s| s.id);
    let ids: Vec<HashId> = files.iter().map(|&id| HashId(id)).collect();
    let batch = hydrus_store::media::load(env.conn, &env.snapshot.services, None, &ids)?;
    Ok(batch
        .results
        .iter()
        .map(|m| {
            let rating = service
                .and_then(|s| m.ratings.get(&s))
                .map_or(unrated, |r| match r {
                    Rating::Fraction(f) => *f,
                    Rating::IncDec(n) => *n as f64,
                });
            (m.hash_id.0, RatingKey(rating))
        })
        .collect())
}
