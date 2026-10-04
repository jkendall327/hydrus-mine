//! Collecting a page's files (`MediaList.Collect`) and sorting the files
//! and collections that make (`MediaList.Sort`), as the reference's pages
//! do.
//!
//! Files group by their tags in the collect's namespaces and the values
//! of their ratings on its services; those matching none stay single
//! unless they collect together too. Every group is a collection, even of
//! one file. A collection's files are sorted among themselves, then the
//! page's files and collections, with each sort's view of a collection: its
//! files' sizes, durations and frames summed, its largest file's
//! resolution, its latest import, change and archive times, its files'
//! views summed, the tags of any of them, its display file's hashes (its
//! first, sorted), and its first file's ratings (as collected).

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use hydrus_core::pages::{PageCollect, PageMedia, PageSort, PageSortBy};
use hydrus_core::sort::{HumanSortKey, human_sort_key};
use hydrus_core::{CanvasType, ContentStatus, HashId, ServiceId, Tag, TagId};
use hydrus_store::media::{MediaResult, Rating};

use super::Result;
use super::context::Env;
use super::page_sort;
use super::sort::{SortBy, colour_key};

/// Collect `files` (in the page's order) by `collect`, and sort them; the
/// page's domains are `current`.
pub(crate) fn collect_and_sort(
    env: &Env<'_>,
    current: &[ServiceId],
    files: &[HashId],
    collect: &PageCollect,
    sort: &PageSort,
    fallback: Option<&PageSort>,
) -> Result<Vec<PageMedia>> {
    let batch = hydrus_store::media::load(
        env.conn,
        &env.snapshot.services,
        Some(&env.snapshot.display),
        files,
    )?;
    let results: HashMap<HashId, &MediaResult> =
        batch.results.iter().map(|m| (m.hash_id, m)).collect();
    let tags = |file: HashId| -> BTreeSet<TagId> {
        results
            .get(&file)
            .map(|m| displayed(env, m))
            .unwrap_or_default()
    };
    // The reference's GetNamespaceSlice always unions current and pending;
    // its collect context selects the service, independent of search toggles.
    let collect_context = hydrus_core::search::context::TagContext::new(
        collect.tag_context.service.clone(),
        true,
        true,
    );
    let collect_scope = super::context::resolve_tags(env.snapshot, &collect_context)?;
    let collect_tags = |file: HashId| -> BTreeSet<TagId> {
        let Some(media) = results.get(&file) else {
            return BTreeSet::new();
        };
        let mut tags = BTreeSet::new();
        for service in &collect_scope.services {
            if let Some(mapped) = media.tags.get(&service.id) {
                for status in &collect_scope.statuses {
                    for &stored in mapped.by_status.get(status).into_iter().flatten() {
                        tags.extend(service.graph.display_tags(stored));
                    }
                }
            }
        }
        tags
    };
    let names = &batch.tags;
    // group, in the page's order
    let ratings: Vec<ServiceId> = collect
        .ratings
        .iter()
        .filter_map(|key| env.snapshot.services.by_key(key).ok().map(|s| s.id))
        .collect();
    let prefixes: Vec<String> = collect.namespaces.iter().map(|n| format!("{n}:")).collect();
    let mut groups: Vec<(GroupKey, Vec<HashId>)> = Vec::new();
    let mut at: HashMap<GroupKey, usize> = HashMap::new();
    for &file in files {
        let slice: BTreeSet<String> = collect_tags(file)
            .iter()
            .filter_map(|t| names.get(t))
            .map(Tag::as_str)
            .filter(|tag| prefixes.iter().any(|p| tag.starts_with(p.as_str())))
            .map(str::to_owned)
            .collect();
        // (the values only: `GetStarRatingSlice`)
        let values: BTreeSet<u64> = results
            .get(&file)
            .map(|m| {
                ratings
                    .iter()
                    .filter_map(|s| m.ratings.get(s))
                    .map(|r| rating_value(*r).to_bits())
                    .collect()
            })
            .unwrap_or_default();
        let key = (slice, values);
        if let Some(&i) = at.get(&key) {
            groups[i].1.push(file);
        } else {
            at.insert(key.clone(), groups.len());
            groups.push((key, vec![file]));
        }
    }
    let mut items: Vec<Item> = Vec::new();
    let mut collections: Vec<Vec<HashId>> = Vec::new();
    for ((slice, values), members) in groups {
        if slice.is_empty() && values.is_empty() && !collect.collect_unmatched {
            items.extend(members.into_iter().map(Item::File));
        } else {
            collections.push(members);
        }
    }
    for members in collections {
        // (its first file, as collected, gives its ratings and breaks
        // resolution ties; then its files are sorted)
        let first = members[0];
        let mut order: Vec<u32> = members.iter().map(|h| h.0).collect();
        for sort in fallback.into_iter().chain([sort]) {
            order = page_sort::sort_page(env, &order, sort)?;
        }
        items.push(Item::Collection {
            files: order.into_iter().map(HashId).collect(),
            as_collected: members,
            first,
        });
    }
    let facts: Vec<Facts> = items
        .iter()
        .map(|item| Facts::of(env, current, item, &results, &tags))
        .collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    for sort in fallback.into_iter().chain([sort]) {
        let keys: Vec<Vec<Part>> = facts
            .iter()
            .map(|f| item_key(env, f, sort, names))
            .collect();
        let reverse = !sort.ascending;
        order.sort_by(|&a, &b| {
            let ordering = keys[a].cmp(&keys[b]);
            if reverse {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    Ok(order
        .into_iter()
        .map(|i| match &items[i] {
            Item::File(file) => PageMedia::File(*file),
            Item::Collection { files, .. } => PageMedia::Collection(files.clone()),
        })
        .collect())
}

type GroupKey = (BTreeSet<String>, BTreeSet<u64>);

enum Item {
    File(HashId),
    Collection {
        /// Sorted.
        files: Vec<HashId>,
        as_collected: Vec<HashId>,
        first: HashId,
    },
}

/// A file's current and pending tags in all known tags, as displayed.
fn displayed(env: &Env<'_>, m: &MediaResult) -> BTreeSet<TagId> {
    let mut out = BTreeSet::new();
    for (&service, service_tags) in &m.tags {
        let graph = env.snapshot.display.get(service);
        for status in [ContentStatus::Current, ContentStatus::Pending] {
            for &stored in service_tags.by_status.get(&status).into_iter().flatten() {
                out.extend(graph.display_tags(stored));
            }
        }
    }
    out
}

/// A rating as a number (`GetRating`'s).
fn rating_value(rating: Rating) -> f64 {
    match rating {
        Rating::Fraction(f) => f,
        Rating::IncDec(n) => n as f64,
    }
}

/// What a page's sorts read of a file or a collection.
struct Facts {
    collection: bool,
    num_files: usize,
    earliest: u32,
    size: Option<f64>,
    duration: Option<f64>,
    num_frames: Option<f64>,
    resolution: (Option<f64>, Option<f64>),
    /// (a collection's is 0)
    mime: u8,
    audio: bool,
    imported: Option<i64>,
    modified: Option<i64>,
    archived: bool,
    archived_at: Option<i64>,
    views: u64,
    viewtime: u64,
    last_viewed: Option<i64>,
    /// The display file's.
    hash: String,
    pixel_hash: Option<Vec<u8>>,
    blurhash: Option<String>,
    tags: BTreeSet<TagId>,
    ratings: HashMap<ServiceId, Rating>,
}

impl Facts {
    fn of(
        env: &Env<'_>,
        current: &[ServiceId],
        item: &Item,
        results: &HashMap<HashId, &MediaResult>,
        tags: &dyn Fn(HashId) -> BTreeSet<TagId>,
    ) -> Facts {
        let interesting = &env.viewing.interesting_canvases;
        // (`GetBestCurrentTimestamp`: the earliest in the page's domains)
        let imported = |m: &MediaResult| {
            current
                .iter()
                .filter_map(|&s| m.added_to(s).map(|t| t.0))
                .min()
        };
        let viewing = |m: &MediaResult| {
            let mut views = 0;
            let mut viewtime = 0;
            let mut last = None;
            for v in &m.viewing {
                if interesting.contains(&v.canvas) {
                    views += v.views;
                    viewtime += v.viewtime_ms;
                }
                if v.canvas == CanvasType::MediaViewer {
                    last = v.last_viewed.map(|t| t.0);
                }
            }
            (views, viewtime, last)
        };
        let single = |file: HashId| -> Facts {
            let Some(m) = results.get(&file) else {
                return Facts::empty(file);
            };
            let info = m.info.as_ref();
            let (views, viewtime, last_viewed) = viewing(m);
            Facts {
                collection: false,
                num_files: 1,
                earliest: file.0,
                size: info.map(|i| i.size as f64),
                duration: info.and_then(|i| i.duration_ms).map(|d| d as f64),
                num_frames: info.and_then(|i| i.num_frames).map(|n| n as f64),
                resolution: (
                    info.and_then(|i| i.width).map(f64::from),
                    info.and_then(|i| i.height).map(f64::from),
                ),
                mime: info.map_or(0, |i| i.mime.code()),
                audio: info.is_some_and(|i| i.has_audio),
                imported: imported(m),
                modified: m.aggregate_modified().map(|t| t.0),
                archived: !m.inbox,
                archived_at: m.archived.map(|t| t.0),
                views,
                viewtime,
                last_viewed,
                hash: hex(&m.hash.0),
                pixel_hash: info.and_then(|i| i.pixel_hash).map(|h| h.0.to_vec()),
                blurhash: info.and_then(|i| i.blurhash.clone()),
                tags: tags(file),
                ratings: m.ratings.clone(),
            }
        };
        match item {
            Item::File(file) => single(*file),
            Item::Collection {
                files,
                as_collected,
                first,
            } => {
                let each: Vec<Facts> = as_collected.iter().map(|&f| single(f)).collect();
                let display = single(files[0]);
                let durations: f64 = each
                    .iter()
                    .filter_map(|f| f.duration.filter(|&d| d > 0.0))
                    .sum();
                // the first largest, as collected
                let mut resolution = (None, None);
                let mut leader = 0.0;
                for f in &each {
                    if let (Some(w), Some(h)) = f.resolution
                        && w * h > leader
                    {
                        resolution = (Some(w), Some(h));
                        leader = w * h;
                    }
                }
                let max = |of: &dyn Fn(&Facts) -> Option<i64>| each.iter().filter_map(of).max();
                Facts {
                    collection: true,
                    num_files: each.len(),
                    earliest: each.iter().map(|f| f.earliest).min().unwrap_or(0),
                    size: Some(each.iter().filter_map(|f| f.size).sum()),
                    duration: (durations > 0.0).then_some(durations),
                    num_frames: Some(each.iter().filter_map(|f| f.num_frames).sum()),
                    resolution: match resolution {
                        (None, _) | (_, None) => (Some(0.0), Some(0.0)),
                        r => r,
                    },
                    mime: 0,
                    audio: each.iter().any(|f| f.audio),
                    imported: max(&|f| f.imported),
                    modified: max(&|f| f.modified),
                    // (a collection's locations aren't in the inbox)
                    archived: true,
                    archived_at: max(&|f| f.archived_at),
                    views: each.iter().map(|f| f.views).sum(),
                    viewtime: each.iter().map(|f| f.viewtime).sum(),
                    last_viewed: max(&|f| f.last_viewed),
                    hash: display.hash,
                    pixel_hash: display.pixel_hash,
                    blurhash: display.blurhash,
                    tags: each.iter().flat_map(|f| f.tags.iter().copied()).collect(),
                    ratings: results
                        .get(first)
                        .map(|m| m.ratings.clone())
                        .unwrap_or_default(),
                }
            }
        }
    }

    fn empty(file: HashId) -> Facts {
        Facts {
            collection: false,
            num_files: 1,
            earliest: file.0,
            size: None,
            duration: None,
            num_frames: None,
            resolution: (None, None),
            mime: 0,
            audio: false,
            imported: None,
            modified: None,
            archived: false,
            archived_at: None,
            views: 0,
            viewtime: 0,
            last_viewed: None,
            hash: String::new(),
            pixel_hash: None,
            blurhash: None,
            tags: BTreeSet::new(),
            ratings: HashMap::new(),
        }
    }

    /// `HasUsefulResolution`'s width and height.
    fn useful_resolution(&self) -> Option<(f64, f64)> {
        match self.resolution {
            (Some(w), Some(h)) if w != 0.0 && h != 0.0 => Some((w, h)),
            _ => None,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// One part of a sort key; each sort type keeps each part's kind.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Num(f64),
    Bytes(Vec<u8>),
    Text(String),
    Tags(Vec<HumanSortKey>),
}

impl Eq for Part {}

impl PartialOrd for Part {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Part {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Part::Num(a), Part::Num(b)) => a.total_cmp(b),
            (Part::Bytes(a), Part::Bytes(b)) => a.cmp(b),
            (Part::Text(a), Part::Text(b)) => a.cmp(b),
            (Part::Tags(a), Part::Tags(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }
    }
}

/// `deal_with_none`.
fn or_minus_one(v: Option<f64>) -> Part {
    Part::Num(v.unwrap_or(-1.0))
}

/// The key `sort` gives a file or collection (`GetSortKeyAndReverse`).
#[allow(clippy::too_many_lines)]
fn item_key(env: &Env<'_>, f: &Facts, sort: &PageSort, names: &HashMap<TagId, Tag>) -> Vec<Part> {
    let reverse = !sort.ascending;
    // (sorted last both ways: the reference's `(0 if reverse else 1, ...)`)
    let present = |is: bool| Part::Num(if is == reverse { 1.0 } else { 0.0 });
    let by = match &sort.by {
        PageSortBy::System(code) => match SortBy::from_code(*code) {
            Some(by) => by,
            None => return Vec::new(),
        },
        PageSortBy::Namespaces { namespaces, .. } => {
            let pairs: Vec<(&str, &str)> = f
                .tags
                .iter()
                .filter_map(|t| names.get(t))
                .map(Tag::split)
                .collect();
            return namespaces
                .iter()
                .map(|wanted| {
                    let mut subtags: Vec<HumanSortKey> = pairs
                        .iter()
                        .filter(|(namespace, _)| namespace == wanted)
                        .map(|(_, subtag)| human_sort_key(subtag))
                        .collect();
                    subtags.sort();
                    Part::Tags(subtags)
                })
                .collect();
        }
        PageSortBy::Rating(key) => {
            let service = env.snapshot.services.by_key(key).ok();
            let unrated = match service.map(|s| s.service_type()) {
                Some(hydrus_core::ServiceType::LocalRatingIncDec) => 0.0,
                _ => -1.0,
            };
            let rating = service
                .and_then(|s| f.ratings.get(&s.id))
                .map_or(unrated, |r| rating_value(*r));
            return vec![Part::Num(rating)];
        }
    };
    let (width, height) = f.resolution;
    match by {
        SortBy::FileSize => vec![or_minus_one(f.size)],
        SortBy::Duration => vec![or_minus_one(f.duration)],
        SortBy::ImportTime => vec![
            or_minus_one(f.imported.map(|t| t as f64)),
            Part::Num(f64::from(f.earliest)),
        ],
        SortBy::Mime => vec![Part::Num(f64::from(f.mime))],
        SortBy::Random => vec![],
        SortBy::Width => vec![or_minus_one(width)],
        SortBy::Height => vec![or_minus_one(height)],
        SortBy::Ratio => vec![Part::Num(
            f.useful_resolution().map_or(-1.0, |(w, h)| w / h),
        )],
        SortBy::NumPixels => vec![Part::Num(match (width, height) {
            (Some(w), Some(h)) => w * h,
            _ => -1.0,
        })],
        SortBy::NumTags => vec![Part::Num(f.tags.len() as f64)],
        SortBy::MediaViews => vec![Part::Num(f.views as f64)],
        SortBy::MediaViewtime => vec![Part::Num(f.viewtime as f64)],
        SortBy::ApproxBitrate => {
            let (duration_bitrate, frame_bitrate) = bitrate(f);
            vec![Part::Num(duration_bitrate), Part::Num(frame_bitrate)]
        }
        SortBy::HasAudio => vec![Part::Num(if f.audio { -1.0 } else { 0.0 })],
        SortBy::ModifiedTime => vec![or_minus_one(f.modified.map(|t| t as f64))],
        SortBy::Framerate => {
            let rate = match (f.duration, f.num_frames) {
                (Some(d), Some(n)) if d > 0.0 && n > 0.0 => n / (d / 1000.0),
                _ => -1.0,
            };
            vec![Part::Num(rate)]
        }
        SortBy::NumFrames => vec![or_minus_one(f.num_frames)],
        SortBy::NumCollectionFiles => vec![
            Part::Num(f.num_files as f64),
            Part::Num(if f.collection { 1.0 } else { 0.0 }),
        ],
        SortBy::LastViewedTime => vec![or_minus_one(f.last_viewed.map(|t| t as f64))],
        SortBy::ArchivedTime => vec![
            Part::Num(if f.archived { 1.0 } else { 0.0 }),
            or_minus_one(f.archived_at.map(|t| t as f64)),
        ],
        SortBy::Hash => vec![Part::Text(f.hash.clone())],
        SortBy::PixelHash => match &f.pixel_hash {
            Some(h) => vec![present(true), Part::Bytes(h.clone())],
            None => vec![present(false), Part::Bytes(vec![0xff; 32])],
        },
        SortBy::Blurhash => match &f.blurhash {
            Some(b) => vec![present(true), Part::Text(b.clone())],
            None => vec![present(false), Part::Text(String::new())],
        },
        SortBy::AverageColourLightness
        | SortBy::AverageColourChromaticMagnitude
        | SortBy::AverageColourGreenRed
        | SortBy::AverageColourBlueYellow
        | SortBy::AverageColourHue => {
            match f
                .blurhash
                .as_deref()
                .and_then(|b| colour_key(by, b, reverse))
            {
                Some(k) => {
                    let mut out = vec![present(true)];
                    out.extend(k.iter().map(|&v| Part::Num(v)));
                    out
                }
                None => vec![present(false)],
            }
        }
    }
}

/// The reference's approximate bitrate key, of a file or a collection.
fn bitrate(f: &Facts) -> (f64, f64) {
    let size = f.size.unwrap_or(0.0);
    let useful = f.useful_resolution();
    match f.duration {
        None | Some(0.0) => {
            if size == 0.0 {
                (-1.0, -1.0)
            } else {
                let frame = match useful {
                    Some((w, h)) => size / (w * h),
                    None => -1.0,
                };
                (0.0, frame)
            }
        }
        Some(duration) => {
            if size == 0.0 {
                (-1.0, -1.0)
            } else {
                let per_ms = size / duration;
                let frame = match f.num_frames {
                    Some(n) if n != 0.0 => per_ms / n,
                    _ => 0.0,
                };
                (per_ms, frame)
            }
        }
    }
}
