//! Sorting a page's files as the reference's pages sort them
//! (`MediaList.Sort`): the fallback sort, then the page's, each stable,
//! by a system sort (as [`super::sort`] does in its page mode), by the
//! tags in some namespaces, or by rating.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use roaring::RoaringBitmap;

use hydrus_core::pages::{PageSort, PageSortBy};
use hydrus_core::sort::{HumanSortKey, human_sort_key};
use hydrus_core::{ContentStatus, HashId, TagId};
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
            Some(by) => sort::sort_in(env, &bitmap, FileSort { by, order }, Mode::Page(files)),
            None => Ok(files.to_vec()),
        },
        PageSortBy::Namespaces {
            namespaces,
            tag_display_type,
        } => {
            let keys = namespace_keys(env, files, namespaces, *tag_display_type)?;
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
    tag_display_type: i64,
) -> Result<HashMap<u32, Vec<Vec<HumanSortKey>>>> {
    use hydrus_store::tag_display::{TagDisplayFilters, TagView};
    let ids: Vec<HashId> = files.iter().map(|&id| HashId(id)).collect();
    let batch = hydrus_store::media::load(
        env.conn,
        &env.snapshot.services,
        Some(&env.snapshot.display),
        &ids,
    )?;
    let view = match tag_display_type {
        TAG_DISPLAY_SINGLE_MEDIA => Some(TagView::SingleMedia),
        TAG_DISPLAY_SELECTION_LIST => Some(TagView::SelectionList),
        _ => None,
    };
    let hidden = match view {
        Some(view) => {
            let filters: TagDisplayFilters = hydrus_store::settings::get(env.conn)?;
            Some(filters.by_service(view, &env.snapshot.services))
        }
        None => None,
    };
    let mut keys = HashMap::with_capacity(batch.results.len());
    for m in &batch.results {
        let mut displayed: BTreeSet<TagId> = BTreeSet::new();
        for (&service, service_tags) in &m.tags {
            let graph = env.snapshot.display.get(service);
            for status in [ContentStatus::Current, ContentStatus::Pending] {
                for &stored in service_tags.by_status.get(&status).into_iter().flatten() {
                    displayed.extend(graph.display_tags(stored).filter(|t| {
                        hidden.as_ref().is_none_or(|hidden| {
                            batch
                                .tags
                                .get(t)
                                .is_none_or(|tag| hidden.shows(service, tag.as_str()))
                        })
                    }));
                }
            }
        }
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
