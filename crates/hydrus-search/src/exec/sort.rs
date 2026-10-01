//! Sorting search results.
//!
//! Each sort type reads one key per file and sorts stably, starting from
//! ascending file id, so files with equal keys stay in file id order in
//! both directions (as the reference's stable sort of an id-ordered table
//! does). Files with no value where the reference has no row (e.g. no
//! import time in the sorted domain, never viewed, not archived) go last
//! in both directions; a present-but-unknown value (a file with no width)
//! sorts as -1, as in the reference.

use std::cmp::Ordering;
use std::collections::HashMap;

use rand::seq::SliceRandom as _;
use roaring::RoaringBitmap;
use rusqlite::types::FromSql;

use hydrus_core::{CanvasType, HashId, ServiceType};

use super::Result;
use super::context::{Env, TagScope, tag_service};
use super::sql;

/// What to sort files by. Codes are the Client API's `file_sort_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortBy {
    FileSize,
    Duration,
    ImportTime,
    Mime,
    Random,
    Width,
    Height,
    Ratio,
    NumPixels,
    /// Tags displayed in the search's tag domain, current and pending.
    NumTags,
    MediaViews,
    MediaViewtime,
    ApproxBitrate,
    HasAudio,
    ModifiedTime,
    Framerate,
    NumFrames,
    NumCollectionFiles,
    LastViewedTime,
    ArchivedTime,
    Hash,
    PixelHash,
    Blurhash,
    AverageColourLightness,
    AverageColourChromaticMagnitude,
    AverageColourGreenRed,
    AverageColourBlueYellow,
    AverageColourHue,
}

impl SortBy {
    pub const ALL: [SortBy; 28] = [
        SortBy::FileSize,
        SortBy::Duration,
        SortBy::ImportTime,
        SortBy::Mime,
        SortBy::Random,
        SortBy::Width,
        SortBy::Height,
        SortBy::Ratio,
        SortBy::NumPixels,
        SortBy::NumTags,
        SortBy::MediaViews,
        SortBy::MediaViewtime,
        SortBy::ApproxBitrate,
        SortBy::HasAudio,
        SortBy::ModifiedTime,
        SortBy::Framerate,
        SortBy::NumFrames,
        SortBy::NumCollectionFiles,
        SortBy::LastViewedTime,
        SortBy::ArchivedTime,
        SortBy::Hash,
        SortBy::PixelHash,
        SortBy::Blurhash,
        SortBy::AverageColourLightness,
        SortBy::AverageColourChromaticMagnitude,
        SortBy::AverageColourGreenRed,
        SortBy::AverageColourBlueYellow,
        SortBy::AverageColourHue,
    ];

    /// The reference's `CC.SORT_FILES_BY_*` code.
    pub fn code(self) -> u8 {
        Self::ALL
            .iter()
            .position(|s| *s == self)
            .expect("every sort is listed") as u8
    }

    pub fn from_code(code: i64) -> Option<SortBy> {
        usize::try_from(code)
            .ok()
            .and_then(|i| Self::ALL.get(i))
            .copied()
    }
}

/// Ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// How to sort search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileSort {
    pub by: SortBy,
    pub order: SortOrder,
}

impl Default for FileSort {
    /// Oldest imports first, the Client API's default.
    fn default() -> Self {
        Self {
            by: SortBy::ImportTime,
            order: SortOrder::Ascending,
        }
    }
}

/// A sort key: up to three numbers compared in order.
type Key = [f64; 3];

fn key(a: f64) -> Key {
    [a, 0.0, 0.0]
}

fn cmp_keys(a: &Key, b: &Key) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.total_cmp(y))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// How a sort orders ties and files with no value.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Mode<'a> {
    /// As the database sorts a search (the Client API's): ties in file id
    /// order, and files with no value last in both directions.
    Database,
    /// As a page sorts its files (`MediaSort.Sort`): ties in this order
    /// (files not in it after, by id), and files with no value where the
    /// page reads one (never viewed as 0 views, no time as -1) sorted with
    /// the rest.
    Page(&'a [u32]),
}

/// The key a page gives a file with no row for `by`, where it gives one
/// (`deal_with_none` and its kin); `None` where such files go last in both
/// directions, as in the database.
fn page_missing(by: SortBy) -> Option<Key> {
    match by {
        SortBy::MediaViews | SortBy::MediaViewtime => Some(key(0.0)),
        SortBy::FileSize
        | SortBy::Duration
        | SortBy::Width
        | SortBy::Height
        | SortBy::NumFrames
        | SortBy::Mime
        | SortBy::Ratio
        | SortBy::NumPixels
        | SortBy::Framerate
        | SortBy::LastViewedTime
        | SortBy::ModifiedTime => Some(key(-1.0)),
        SortBy::ApproxBitrate => Some([-1.0, -1.0, 0.0]),
        _ => None,
    }
}

/// Sort `rows` by key, stably, from file id order (or, for a page, its
/// order); files of `files` without a row take `missing` (a page's), else
/// follow in that order.
fn order<K: Clone>(
    files: &RoaringBitmap,
    mut rows: Vec<(u32, K)>,
    descending: bool,
    cmp: impl Fn(&K, &K) -> Ordering,
    mode: Mode<'_>,
    missing: Option<K>,
) -> Vec<u32> {
    let rank: HashMap<u32, usize> = match mode {
        Mode::Database => HashMap::new(),
        Mode::Page(base) => base.iter().enumerate().map(|(i, &id)| (id, i)).collect(),
    };
    let place = |id: u32| (rank.get(&id).copied().unwrap_or(usize::MAX), id);
    if let (Mode::Page(_), Some(missing)) = (mode, missing) {
        let with: RoaringBitmap = rows.iter().map(|(id, _)| *id).collect();
        rows.extend(
            files
                .iter()
                .filter(|id| !with.contains(*id))
                .map(|id| (id, missing.clone())),
        );
    }
    rows.sort_by_key(|(id, _)| place(*id));
    if descending {
        rows.sort_by(|a, b| cmp(&b.1, &a.1));
    } else {
        rows.sort_by(|a, b| cmp(&a.1, &b.1));
    }
    let mut with_rows = RoaringBitmap::new();
    let mut out: Vec<u32> = Vec::with_capacity(files.len() as usize);
    for (id, _) in rows {
        if with_rows.insert(id) {
            out.push(id);
        }
    }
    let mut rest: Vec<u32> = files.iter().filter(|id| !with_rows.contains(*id)).collect();
    rest.sort_by_key(|&id| place(id));
    out.extend(rest);
    out
}

/// The domain's whole import order, when reading it (or having it cached)
/// beats reading import times for just `files`.
fn import_order(
    env: &Env<'_>,
    service: hydrus_core::ServiceId,
    files: &RoaringBitmap,
) -> Result<Option<std::sync::Arc<Vec<u32>>>> {
    use super::context::Strategy;
    let cache = env.domain_cache();
    match env.strategy {
        Strategy::AlwaysProbe => return Ok(None),
        Strategy::AlwaysScan => return Ok(Some(cache.import_order(env.conn, service)?)),
        Strategy::Auto => {}
    }
    if let Some(order) = cache.cached_import_order(service) {
        // checking a file against the domain's order in memory costs about
        // a hundredth of reading its import time
        return Ok((files.len() >= order.len() as u64 / 100).then_some(order));
    }
    let size = cache.size_estimate(env.conn, service, false)?;
    if super::context::probe_domain(files.len(), size) {
        return Ok(None);
    }
    Ok(Some(cache.import_order(env.conn, service)?))
}

/// `files` sorted by import time into `service`, given its import order:
/// the same order as [`order`] makes of each file's (import time, id) key.
fn in_order(
    env: &Env<'_>,
    service: hydrus_core::ServiceId,
    files: &RoaringBitmap,
    order: &[u32],
    descending: bool,
) -> Result<Vec<u32>> {
    let mut out: Vec<u32> = Vec::with_capacity(files.len() as usize);
    let wanted = |id: &&u32| files.contains(**id);
    if descending {
        out.extend(order.iter().rev().filter(wanted));
    } else {
        out.extend(order.iter().filter(wanted));
    }
    if out.len() as u64 != files.len() {
        let in_domain = env.domain_cache().files(env.conn, service, false)?;
        out.extend(files.iter().filter(|id| !in_domain.contains(*id)));
    }
    Ok(out)
}

/// Read `(hash_id, value)` rows for `files` from `select` (a query over a
/// table whose first column is the hash id, ending in a `WHERE`), probing
/// or scanning `table` as appropriate.
fn rows<T: FromSql>(
    env: &Env<'_>,
    select: &str,
    table: &str,
    params: &[&dyn rusqlite::ToSql],
    files: &RoaringBitmap,
) -> Result<Vec<(u32, T)>> {
    let mut out = Vec::with_capacity(files.len() as usize);
    let scan = !env.prefer_probe(files.len(), sql::table_rows(env.conn, table));
    if scan {
        let mut stmt = env.conn.prepare_cached(select)?;
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            let id = sql::hash_id(row, 0)?;
            if files.contains(id) {
                out.push((id, row.get(1)?));
            }
        }
    } else {
        let column = select
            .strip_prefix("SELECT ")
            .and_then(|s| s.split([',', ' ']).next())
            .unwrap_or("hash_id");
        let mut stmt = env.conn.prepare_cached(&format!(
            "{select} AND {column} IN rarray(?{})",
            params.len() + 1
        ))?;
        sql::for_each_chunk(files, |chunk| {
            let mut all = params.to_vec();
            all.push(&chunk);
            let mut rows = stmt.query(all.as_slice())?;
            while let Some(row) = rows.next()? {
                out.push((sql::hash_id(row, 0)?, row.get(1)?));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

/// A numeric `files` column, with NULL as -1.
fn file_column(env: &Env<'_>, expr: &str, files: &RoaringBitmap) -> Result<Vec<(u32, Key)>> {
    Ok(rows::<Option<f64>>(
        env,
        &format!("SELECT hash_id, {expr} FROM files WHERE 1"),
        "files",
        &[],
        files,
    )?
    .into_iter()
    .map(|(id, v)| (id, key(v.unwrap_or(-1.0))))
    .collect())
}

/// Several `files` columns at once.
fn file_columns(
    env: &Env<'_>,
    columns: &str,
    files: &RoaringBitmap,
) -> Result<Vec<(u32, [Option<f64>; 5])>> {
    let mut out = Vec::new();
    let mut stmt = env.conn.prepare_cached(&format!(
        "SELECT hash_id, {columns} FROM files WHERE hash_id IN rarray(?)"
    ))?;
    let n = columns.split(',').count();
    sql::for_each_chunk(files, |chunk| {
        let mut rows = stmt.query([chunk])?;
        while let Some(row) = rows.next()? {
            let mut values = [None; 5];
            for (i, v) in values.iter_mut().enumerate().take(n) {
                *v = row.get::<_, Option<f64>>(i + 1)?;
            }
            out.push((sql::hash_id(row, 0)?, values));
        }
        Ok(())
    })?;
    Ok(out)
}

/// Sort `files` by `sort`, as the database sorts a search.
pub(crate) fn sort(env: &Env<'_>, files: &RoaringBitmap, sort: FileSort) -> Result<Vec<u32>> {
    sort_in(env, files, sort, Mode::Database)
}

/// Sort `files` by `sort`, as the database or a page sorts them.
#[allow(clippy::too_many_lines)]
pub(crate) fn sort_in(
    env: &Env<'_>,
    files: &RoaringBitmap,
    sort: FileSort,
    mode: Mode<'_>,
) -> Result<Vec<u32>> {
    let desc = sort.order == SortOrder::Descending;
    let page = matches!(mode, Mode::Page(_));
    let missing = if page { page_missing(sort.by) } else { None };
    let by_key = |rows: Vec<(u32, Key)>| order(files, rows, desc, cmp_keys, mode, missing);
    Ok(match sort.by {
        SortBy::FileSize => by_key(file_column(env, "size", files)?),
        SortBy::Duration => by_key(file_column(env, "duration_ms", files)?),
        SortBy::Width => by_key(file_column(env, "width", files)?),
        SortBy::Height => by_key(file_column(env, "height", files)?),
        SortBy::NumFrames => by_key(file_column(env, "num_frames", files)?),
        SortBy::Mime => by_key(file_column(env, "coalesce(forced_mime, mime)", files)?),
        SortBy::HasAudio => {
            // audio first when ascending; files without info are silent
            let audio: HashMap<u32, Key> =
                file_column(env, "has_audio", files)?.into_iter().collect();
            let rows = files
                .iter()
                .map(|id| (id, key(-audio.get(&id).map_or(0.0, |k| k[0].max(0.0)))))
                .collect();
            by_key(rows)
        }
        SortBy::Ratio => by_key(
            file_columns(env, "width, height", files)?
                .into_iter()
                .map(|(id, [w, h, ..])| {
                    let r = match (w, h) {
                        (Some(w), Some(h)) if w != 0.0 && h != 0.0 => w / h,
                        _ => -1.0,
                    };
                    (id, key(r))
                })
                .collect(),
        ),
        SortBy::NumPixels => by_key(
            file_columns(env, "width, height", files)?
                .into_iter()
                .map(|(id, [w, h, ..])| {
                    let p = match (w, h) {
                        (Some(w), Some(h)) if w != 0.0 && h != 0.0 => w * h,
                        _ => -1.0,
                    };
                    (id, key(p))
                })
                .collect(),
        ),
        SortBy::Framerate => by_key(
            file_columns(env, "num_frames, duration_ms", files)?
                .into_iter()
                .map(|(id, [frames, duration, ..])| {
                    let r = match (frames, duration) {
                        (Some(f), Some(d)) if f > 0.0 && d > 0.0 => f / d,
                        _ => -1.0,
                    };
                    (id, key(r))
                })
                .collect(),
        ),
        SortBy::ApproxBitrate => by_key(
            file_columns(env, "duration_ms, num_frames, size, width, height", files)?
                .into_iter()
                .map(|(id, [duration, frames, size, width, height])| {
                    (id, bitrate(duration, frames, size, width, height))
                })
                .collect(),
        ),
        SortBy::ImportTime => {
            let service = env.location.single_current.or_else(|| {
                env.service_of_type(ServiceType::HydrusLocalFileStorage)
                    .map(|s| s.id)
            });
            if let Some(service) = service
                && let Some(order) = import_order(env, service, files)?
            {
                let sorted = in_order(env, service, files, &order, desc)?;
                if !page {
                    return Ok(sorted);
                }
                // a page sorts files not in the domain as imported at -1,
                // by id
                let in_domain = env.domain_cache().files(env.conn, service, false)?;
                let (mut inside, outside): (Vec<u32>, Vec<u32>) =
                    sorted.into_iter().partition(|id| in_domain.contains(*id));
                return Ok(if desc {
                    inside.extend(outside.into_iter().rev());
                    inside
                } else {
                    let mut out = outside;
                    out.extend(inside);
                    out
                });
            }
            let rows: Vec<(u32, Option<i64>)> = match service {
                Some(service) => rows(
                    env,
                    "SELECT hash_id, added_ms FROM file_domain_current WHERE service_id = ?",
                    "file_domain_current",
                    &[&service],
                    files,
                )?,
                None => Vec::new(),
            };
            // the file id breaks ties between files imported together
            let mut keys: Vec<(u32, Key)> = rows
                .into_iter()
                .map(|(id, t)| (id, [t.unwrap_or(-1) as f64, f64::from(id), 0.0]))
                .collect();
            if page {
                let with: RoaringBitmap = keys.iter().map(|(id, _)| *id).collect();
                keys.extend(
                    files
                        .iter()
                        .filter(|id| !with.contains(*id))
                        .map(|id| (id, [-1.0, f64::from(id), 0.0])),
                );
            }
            by_key(keys)
        }
        SortBy::ModifiedTime => {
            let mut earliest: HashMap<u32, i64> = HashMap::new();
            for (select, table) in [
                (
                    "SELECT hash_id, file_modified_ms FROM files WHERE file_modified_ms IS NOT NULL",
                    "files",
                ),
                (
                    "SELECT hash_id, modified_ms FROM file_domain_modified WHERE 1",
                    "file_domain_modified",
                ),
            ] {
                for (id, t) in rows::<i64>(env, select, table, &[], files)? {
                    let e = earliest.entry(id).or_insert(t);
                    *e = (*e).min(t);
                }
            }
            by_key(
                earliest
                    .into_iter()
                    .map(|(id, t)| (id, key(t as f64)))
                    .collect(),
            )
        }
        SortBy::LastViewedTime => by_key(
            rows::<Option<i64>>(
                env,
                "SELECT hash_id, last_viewed_ms FROM file_viewing_stats WHERE canvas_type = ?",
                "file_viewing_stats",
                &[&CanvasType::MediaViewer.code()],
                files,
            )?
            .into_iter()
            .map(|(id, t)| (id, key(t.unwrap_or(-1) as f64)))
            .collect(),
        ),
        SortBy::ArchivedTime => {
            let archived = rows::<Option<i64>>(
                env,
                "SELECT hash_id, archived_ms FROM file_archived WHERE 1",
                "file_archived",
                &[],
                files,
            )?;
            if page {
                // a page sorts the inbox before the archive, then by when
                let inbox: RoaringBitmap = rows::<i64>(
                    env,
                    "SELECT hash_id, 1 FROM file_inbox WHERE 1",
                    "file_inbox",
                    &[],
                    files,
                )?
                .into_iter()
                .map(|(id, _)| id)
                .collect();
                let when: HashMap<u32, i64> = archived
                    .into_iter()
                    .map(|(id, t)| (id, t.unwrap_or(-1)))
                    .collect();
                by_key(
                    files
                        .iter()
                        .map(|id| {
                            let archive = if inbox.contains(id) { 0.0 } else { 1.0 };
                            (
                                id,
                                [archive, when.get(&id).copied().unwrap_or(-1) as f64, 0.0],
                            )
                        })
                        .collect(),
                )
            } else {
                by_key(
                    archived
                        .into_iter()
                        .map(|(id, t)| (id, key(t.unwrap_or(-1) as f64)))
                        .collect(),
                )
            }
        }
        SortBy::MediaViews | SortBy::MediaViewtime => {
            let column = if sort.by == SortBy::MediaViews {
                "views"
            } else {
                "viewtime_ms"
            };
            let canvases = sql::int_array(
                env.viewing
                    .interesting_canvases
                    .iter()
                    .map(|c| u32::from(c.code())),
            );
            let mut totals: HashMap<u32, f64> = HashMap::new();
            for (id, n) in rows::<i64>(
                env,
                &format!(
                    "SELECT hash_id, {column} FROM file_viewing_stats WHERE canvas_type IN rarray(?)"
                ),
                "file_viewing_stats",
                &[&canvases],
                files,
            )? {
                *totals.entry(id).or_default() += n as f64;
            }
            // files never seen in those viewers have no row and go last
            by_key(totals.into_iter().map(|(id, n)| (id, key(n))).collect())
        }
        SortBy::NumTags => {
            let scope = TagScope {
                services: env
                    .tags
                    .services
                    .iter()
                    .map(|s| tag_service(env.snapshot, s.id))
                    .collect(),
                statuses: vec![
                    hydrus_core::ContentStatus::Current,
                    hydrus_core::ContentStatus::Pending,
                ],
            };
            let counts: HashMap<u32, u64> =
                super::leaf::display_tag_counts(&env.with_tags(scope), None, files)?
                    .into_iter()
                    .collect();
            by_key(
                files
                    .iter()
                    .map(|id| (id, key(counts.get(&id).copied().unwrap_or(0) as f64)))
                    .collect(),
            )
        }
        // (every file is one file, not a collection)
        SortBy::NumCollectionFiles => by_key(files.iter().map(|id| (id, key(1.0))).collect()),
        SortBy::Random => {
            let mut ids: Vec<u32> = files.iter().collect();
            ids.shuffle(&mut rand::rng());
            ids
        }
        SortBy::Hash => {
            let ids: Vec<HashId> = files.iter().map(HashId).collect();
            let mut rows: Vec<(u32, [u8; 32])> = Vec::with_capacity(ids.len());
            for chunk in ids.chunks(sql::CHUNK) {
                rows.extend(
                    hydrus_store::master::hashes(env.conn, chunk)?
                        .into_iter()
                        .map(|(id, h)| (id.get(), h.0)),
                );
            }
            order(files, rows, desc, Ord::cmp, mode, None)
        }
        SortBy::PixelHash => {
            let rows = rows::<Option<Vec<u8>>>(
                env,
                "SELECT hash_id, pixel_hash FROM files WHERE pixel_hash IS NOT NULL",
                "files",
                &[],
                files,
            )?
            .into_iter()
            .filter_map(|(id, h)| Some((id, h?)))
            .collect();
            order(files, rows, desc, Ord::cmp, mode, None)
        }
        SortBy::Blurhash
        | SortBy::AverageColourLightness
        | SortBy::AverageColourChromaticMagnitude
        | SortBy::AverageColourGreenRed
        | SortBy::AverageColourBlueYellow
        | SortBy::AverageColourHue => {
            let blurhashes: Vec<(u32, String)> = rows::<Option<String>>(
                env,
                "SELECT hash_id, blurhash FROM files WHERE blurhash IS NOT NULL",
                "files",
                &[],
                files,
            )?
            .into_iter()
            .filter_map(|(id, b)| Some((id, b?)))
            .collect();
            if sort.by == SortBy::Blurhash {
                order(files, blurhashes, desc, Ord::cmp, mode, None)
            } else {
                let rows = blurhashes
                    .into_iter()
                    .filter_map(|(id, b)| Some((id, colour_key(sort.by, &b, desc)?)))
                    .collect();
                by_key(rows)
            }
        }
    })
}

/// The reference's approximate bitrate key: bytes per millisecond, then
/// per frame (or per pixel for still images).
fn bitrate(
    duration: Option<f64>,
    frames: Option<f64>,
    size: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> Key {
    let size = size.filter(|s| *s != 0.0);
    let (duration_rate, frame_rate) = match (duration.filter(|d| *d != 0.0), size) {
        (_, None) => (-1.0, -1.0),
        (None, Some(size)) => match (width, height) {
            (Some(w), Some(h)) if w != 0.0 && h != 0.0 => (0.0, size / (w * h)),
            (Some(_), Some(_)) => (0.0, -1.0),
            _ => (0.0, 0.0),
        },
        (Some(duration), Some(size)) => {
            let rate = size / duration;
            match frames.filter(|f| *f != 0.0) {
                Some(f) => (rate, rate / f),
                None => (rate, 0.0),
            }
        }
    };
    [duration_rate, frame_rate, 0.0]
}

const BASE83: &str =
    "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

/// An sRGB colour, each channel 0-255.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgb {
    red: f64,
    green: f64,
    blue: f64,
}

/// A blurhash's average colour (its DC component).
fn average_colour(blurhash: &str) -> Option<Rgb> {
    let dc = blurhash.get(2..6)?;
    let mut value: u64 = 0;
    for c in dc.chars() {
        value = value * 83 + BASE83.find(c)? as u64;
    }
    let channel = |shift: u32| ((value >> shift) & 0xFF) as f64;
    Some(Rgb {
        red: channel(16),
        green: channel(8),
        blue: channel(0),
    })
}

/// CIE L*a*b* (D65), as the reference computes it.
struct Lab {
    lightness: f64,
    green_red: f64,
    blue_yellow: f64,
}

fn lab(colour: Rgb) -> Lab {
    let linear = |v: f64| {
        let v = v / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (red, green, blue) = (
        linear(colour.red),
        linear(colour.green),
        linear(colour.blue),
    );
    let cie_x = red * 0.412_456_4 + green * 0.357_576_1 + blue * 0.180_437_5;
    let cie_y = red * 0.212_672_9 + green * 0.715_152_2 + blue * 0.072_175_0;
    let cie_z = red * 0.019_333_9 + green * 0.119_192_0 + blue * 0.950_304_1;
    let curve = |t: f64| {
        if t > 0.008_856 {
            t.powf(1.0 / 3.0)
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (
        curve(cie_x / 0.950_47),
        curve(cie_y / 1.0),
        curve(cie_z / 1.088_83),
    );
    Lab {
        lightness: 116.0 * fy - 16.0,
        green_red: 500.0 * (fx - fy),
        blue_yellow: 200.0 * (fy - fz),
    }
}

/// Hue (degrees) and saturation, as the reference computes them.
fn hue_saturation(colour: Rgb) -> (f64, f64) {
    let (red, green, blue) = (
        colour.red / 255.0,
        colour.green / 255.0,
        colour.blue / 255.0,
    );
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    if delta <= 0.0 {
        return (0.0, 0.0);
    }
    let lightness = f64::midpoint(max, min);
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    // the first channel that is the maximum decides the sector
    let hue = if red >= green && red >= blue {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if green >= blue {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };
    (hue * 60.0, saturation)
}

/// The reference's average-colour sort keys.
fn colour_key(by: SortBy, blurhash: &str, descending: bool) -> Option<Key> {
    let colour = average_colour(blurhash)?;
    let lab = lab(colour);
    let chroma = lab.green_red * lab.green_red + lab.blue_yellow * lab.blue_yellow;
    Some(match by {
        SortBy::AverageColourLightness => [lab.lightness, chroma, 0.0],
        SortBy::AverageColourChromaticMagnitude => [chroma, lab.lightness, 0.0],
        SortBy::AverageColourGreenRed => [lab.green_red, -lab.lightness, 0.0],
        SortBy::AverageColourBlueYellow => [lab.blue_yellow, -lab.lightness, 0.0],
        SortBy::AverageColourHue => {
            let (hue, saturation) = hue_saturation(colour);
            // greys go last in either direction
            let greys = if saturation < 0.03 {
                if descending { -1.0 } else { 1.0 }
            } else {
                0.0
            };
            [greys, hue, -saturation]
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_codes_match_the_reference() {
        let constants = hydrus_testkit::fixture_json("constants.json");
        let codes = constants["client_int_constants"].as_object().unwrap();
        let names = [
            ("FILESIZE", SortBy::FileSize),
            ("DURATION", SortBy::Duration),
            ("IMPORT_TIME", SortBy::ImportTime),
            ("MIME", SortBy::Mime),
            ("RANDOM", SortBy::Random),
            ("WIDTH", SortBy::Width),
            ("HEIGHT", SortBy::Height),
            ("RATIO", SortBy::Ratio),
            ("NUM_PIXELS", SortBy::NumPixels),
            ("NUM_TAGS", SortBy::NumTags),
            ("MEDIA_VIEWS", SortBy::MediaViews),
            ("MEDIA_VIEWTIME", SortBy::MediaViewtime),
            ("APPROX_BITRATE", SortBy::ApproxBitrate),
            ("HAS_AUDIO", SortBy::HasAudio),
            ("FILE_MODIFIED_TIMESTAMP", SortBy::ModifiedTime),
            ("FRAMERATE", SortBy::Framerate),
            ("NUM_FRAMES", SortBy::NumFrames),
            ("NUM_COLLECTION_FILES", SortBy::NumCollectionFiles),
            ("LAST_VIEWED_TIME", SortBy::LastViewedTime),
            ("ARCHIVED_TIMESTAMP", SortBy::ArchivedTime),
            ("HASH", SortBy::Hash),
            ("PIXEL_HASH", SortBy::PixelHash),
            ("BLURHASH", SortBy::Blurhash),
            ("AVERAGE_COLOUR_LIGHTNESS", SortBy::AverageColourLightness),
            (
                "AVERAGE_COLOUR_CHROMATIC_MAGNITUDE",
                SortBy::AverageColourChromaticMagnitude,
            ),
            (
                "AVERAGE_COLOUR_CHROMATICITY_GREEN_RED",
                SortBy::AverageColourGreenRed,
            ),
            (
                "AVERAGE_COLOUR_CHROMATICITY_BLUE_YELLOW",
                SortBy::AverageColourBlueYellow,
            ),
            ("AVERAGE_COLOUR_HUE", SortBy::AverageColourHue),
        ];
        let reference: Vec<_> = codes
            .keys()
            .filter(|k| k.starts_with("SORT_FILES_BY_"))
            .collect();
        assert_eq!(reference.len(), names.len());
        for (name, sort) in names {
            let code = codes[&format!("SORT_FILES_BY_{name}")].as_i64().unwrap();
            assert_eq!(i64::from(sort.code()), code, "{name}");
            assert_eq!(SortBy::from_code(code), Some(sort));
        }
        assert_eq!(SortBy::from_code(28), None);
        assert_eq!(SortBy::from_code(-1), None);
    }

    #[test]
    fn ties_keep_id_order_in_both_directions() {
        let files: RoaringBitmap = [1, 2, 3, 4, 5].into_iter().collect();
        let rows = vec![(3, key(1.0)), (1, key(1.0)), (2, key(0.0)), (4, key(2.0))];
        let db = Mode::Database;
        assert_eq!(
            order(&files, rows.clone(), false, cmp_keys, db, None),
            [2, 1, 3, 4, 5]
        );
        assert_eq!(
            order(&files, rows, true, cmp_keys, db, None),
            [4, 1, 3, 2, 5]
        );
    }

    #[test]
    fn a_page_s_ties_keep_its_order_and_its_files_without_values_its_default() {
        let files: RoaringBitmap = [1, 2, 3, 4, 5].into_iter().collect();
        let rows = vec![(3, key(1.0)), (1, key(1.0)), (2, key(0.0)), (4, key(2.0))];
        let page = Mode::Page(&[5, 4, 3, 2, 1]);
        // 3 and 1 tie, in the page's order; 5 has no value, so goes last
        assert_eq!(
            order(&files, rows.clone(), false, cmp_keys, page, None),
            [2, 3, 1, 4, 5]
        );
        assert_eq!(
            order(&files, rows.clone(), true, cmp_keys, page, None),
            [4, 3, 1, 2, 5]
        );
        // unless the page gives it one
        assert_eq!(
            order(&files, rows, false, cmp_keys, page, Some(key(-1.0))),
            [5, 2, 3, 1, 4]
        );
    }

    #[test]
    fn colour_keys_match_the_reference() {
        // expected values from hydrus.core.files.images.HydrusBlurhash
        let blurhash = "UeIisF.Yr=w0+LtKWGjbLTm:RWXPV~xCnif}";
        assert_eq!(
            average_colour(blurhash),
            Some(Rgb {
                red: 161.0,
                green: 189.0,
                blue: 99.0
            })
        );
        let lightness = colour_key(SortBy::AverageColourLightness, blurhash, false).unwrap();
        assert!(
            (lightness[0] - 72.808_168_332_941_05).abs() < 1e-9,
            "{lightness:?}"
        );
        assert!(
            (lightness[1] - 2_345.878_198_532_727_8).abs() < 1e-6,
            "{lightness:?}"
        );
        let green_red = colour_key(SortBy::AverageColourGreenRed, blurhash, false).unwrap();
        assert!(
            (green_red[0] + 23.908_172_443_974_92).abs() < 1e-9,
            "{green_red:?}"
        );
        let blue_yellow = colour_key(SortBy::AverageColourBlueYellow, blurhash, false).unwrap();
        assert!(
            (blue_yellow[0] - 42.122_173_364_178_23).abs() < 1e-9,
            "{blue_yellow:?}"
        );
        let hue = colour_key(SortBy::AverageColourHue, blurhash, false).unwrap();
        assert!(hue[0].abs() < f64::EPSILON, "a colour, not a grey");
        assert!((hue[1] - 78.666_666_666_666_69).abs() < 1e-9, "{hue:?}");
        assert!((hue[2] + 0.405_405_405_405_405_43).abs() < 1e-12, "{hue:?}");
        assert_eq!(average_colour("abc"), None);
    }
}
