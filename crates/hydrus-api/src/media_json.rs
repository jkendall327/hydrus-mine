//! File metadata as the API presents it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Map, Value as Json, json};

use hydrus_core::sort::{HumanSortKey, human_sort_key};
use hydrus_core::{
    CanvasType, ContentStatus, ServiceId, ServiceType, Sha256, Tag, TagId, TimestampMs,
};
use hydrus_store::Snapshot;
use hydrus_store::media::{FileFlags, MediaResult, Rating};
use hydrus_store::services::ServiceKind;

/// Options that shape each metadata row.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetadataOptions {
    pub include_notes: bool,
    pub include_milliseconds: bool,
    pub include_blurhash: bool,
    pub hide_service_keys_tags: bool,
    /// Classify each known URL (`detailed_url_information`).
    pub detailed_urls: bool,
}

/// Seconds as the API reports them: whole seconds, or float seconds when
/// milliseconds were asked for.
fn api_time(t: Option<TimestampMs>, include_milliseconds: bool) -> Json {
    match t {
        None => Json::Null,
        Some(t) if include_milliseconds => json!(t.as_secs_f64()),
        Some(t) => json!(t.secs()),
    }
}

/// The identifier-only row for a hash we have no record of.
pub fn missing_row(hash: &Sha256) -> Json {
    json!({ "file_id": null, "hash": hash.to_hex() })
}

/// The row for `only_return_basic_information`.
pub fn basic_row(m: &MediaResult, include_blurhash: bool) -> Json {
    let mut row = Map::new();
    row.insert("file_id".into(), json!(m.hash_id.get()));
    row.insert("hash".into(), json!(m.hash.to_hex()));
    if let Some(info) = &m.info {
        insert_basic_info(&mut row, info);
        if include_blurhash {
            row.insert("blurhash".into(), json!(info.blurhash));
        }
    }
    Json::Object(row)
}

fn insert_basic_info(row: &mut Map<String, Json>, info: &hydrus_store::media::FileInfo) {
    row.insert("size".into(), json!(info.size));
    row.insert("mime".into(), json!(info.mime.mimetype()));
    row.insert("filetype_human".into(), json!(info.mime.human_name()));
    row.insert("filetype_enum".into(), json!(info.mime.code()));
    row.insert("ext".into(), json!(info.mime.extension().unwrap_or("")));
    row.insert("width".into(), json!(info.width));
    row.insert("height".into(), json!(info.height));
    row.insert("duration".into(), json!(info.duration_ms));
    row.insert("num_frames".into(), json!(info.num_frames));
    row.insert("num_words".into(), json!(info.num_words));
    row.insert("has_audio".into(), json!(info.has_audio));
    row.insert(
        "filetype_forced".into(),
        json!(info.original_mime.is_some()),
    );
    if let Some(original) = info.original_mime {
        row.insert("original_mime".into(), json!(original.mimetype()));
    }
}

/// The tags of a batch of files, with their presentation order worked out
/// once for the whole batch.
#[derive(Debug)]
pub struct TagNames<'a> {
    names: &'a HashMap<TagId, Tag>,
    keys: HashMap<TagId, HumanSortKey>,
}

impl<'a> TagNames<'a> {
    pub fn new(names: &'a HashMap<TagId, Tag>) -> Self {
        let keys = names
            .iter()
            .map(|(id, tag)| (*id, human_sort_key(tag.as_str())))
            .collect();
        Self { names, keys }
    }

    /// Human order, and plain order among tags human order can't tell
    /// apart (`HumanTextSort` of a sorted list).
    fn sorted(&self, ids: &BTreeSet<TagId>) -> Vec<&str> {
        let mut known: Vec<(&HumanSortKey, &str)> = ids
            .iter()
            .filter_map(|id| Some((self.keys.get(id)?, self.names.get(id)?.as_str())))
            .collect();
        known.sort_unstable();
        known.into_iter().map(|(_, name)| name).collect()
    }
}

/// Tags of one file, in one service, by status, sorted for presentation.
fn statuses_json(
    by_status: &BTreeMap<ContentStatus, BTreeSet<TagId>>,
    names: &TagNames<'_>,
) -> Json {
    let mut out = Map::new();
    for (status, ids) in by_status {
        let tags = names.sorted(ids);
        if tags.is_empty() {
            continue;
        }
        out.insert(status.code().to_string(), json!(tags));
    }
    Json::Object(out)
}

/// The full metadata row.
pub fn full_row(
    snapshot: &Snapshot,
    m: &MediaResult,
    tag_names: &TagNames<'_>,
    opts: MetadataOptions,
) -> Json {
    let services = &snapshot.services;
    let ms = opts.include_milliseconds;
    let mut row = Map::new();
    row.insert("file_id".into(), json!(m.hash_id.get()));
    row.insert("hash".into(), json!(m.hash.to_hex()));
    let flags = m.info.as_ref().map_or(FileFlags::default(), |i| i.flags);
    if let Some(info) = &m.info {
        insert_basic_info(&mut row, info);
        row.insert("blurhash".into(), json!(info.blurhash));
        row.insert(
            "pixel_hash".into(),
            json!(info.pixel_hash.map(|h| h.to_hex())),
        );
        if info.mime.has_thumbnail()
            && let (Some(w), Some(h)) = (info.width, info.height)
            && w > 0
            && h > 0
        {
            let (tw, th) = snapshot.thumbnails.resolution(Some(w), Some(h));
            row.insert("thumbnail_width".into(), json!(tw));
            row.insert("thumbnail_height".into(), json!(th));
        }
    }
    if opts.include_notes {
        let notes: Map<String, Json> = m.notes.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        row.insert("notes".into(), Json::Object(notes));
    }

    let describe = |id: ServiceId| -> Option<(String, Map<String, Json>)> {
        let s = services.get(id).ok()?;
        let mut d = Map::new();
        d.insert("name".into(), json!(s.name));
        d.insert("type".into(), json!(s.service_type().code()));
        d.insert("type_pretty".into(), json!(s.service_type().name()));
        Some((s.key.to_hex(), d))
    };
    let mut current = Map::new();
    for loc in &m.current {
        if let Some((key, mut d)) = describe(loc.service) {
            d.insert("time_imported".into(), api_time(loc.added, ms));
            current.insert(key, Json::Object(d));
        }
    }
    let mut deleted = Map::new();
    for loc in &m.deleted {
        if let Some((key, mut d)) = describe(loc.service) {
            d.insert("time_deleted".into(), api_time(loc.deleted, ms));
            d.insert("time_imported".into(), api_time(loc.originally_added, ms));
            deleted.insert(key, Json::Object(d));
        }
    }
    row.insert(
        "file_services".into(),
        json!({ "current": current, "deleted": deleted }),
    );

    row.insert("time_modified".into(), api_time(m.aggregate_modified(), ms));
    let mut details = Map::new();
    for (domain, t) in &m.domain_modified {
        details.insert(domain.clone(), api_time(Some(*t), ms));
    }
    if let Some(local) = m.info.as_ref().and_then(|i| i.file_modified) {
        details.insert("local".into(), api_time(Some(local), ms));
    }
    row.insert("time_modified_details".into(), Json::Object(details));

    row.insert("is_inbox".into(), json!(m.inbox));
    if !m.inbox && m.archived.is_some() {
        row.insert("time_archived".into(), api_time(m.archived, ms));
    }
    let builtin =
        |kind: fn(&ServiceKind) -> bool| services.all().find(|s| kind(&s.kind)).map(|s| s.id);
    let local_storage = builtin(|k| matches!(k, ServiceKind::LocalFileStorage));
    let trash = builtin(|k| matches!(k, ServiceKind::Trash));
    let local_media = builtin(|k| matches!(k, ServiceKind::CombinedLocalMedia));
    let is_trashed = trash.is_some_and(|t| m.is_current_in(t));
    row.insert(
        "is_local".into(),
        json!(local_storage.is_some_and(|s| m.is_current_in(s))),
    );
    row.insert("is_trashed".into(), json!(is_trashed));
    row.insert(
        "is_deleted".into(),
        json!(is_trashed || local_media.is_some_and(|s| m.is_deleted_from(s))),
    );
    for (name, flag) in [
        ("has_transparency", FileFlags::TRANSPARENCY),
        ("has_exif", FileFlags::EXIF),
        ("has_xmp", FileFlags::XMP),
        ("has_iptc", FileFlags::IPTC),
        (
            "has_human_readable_embedded_metadata",
            FileFlags::HUMAN_READABLE_METADATA,
        ),
        ("has_software_source", FileFlags::SOFTWARE_SOURCE),
        ("has_icc_profile", FileFlags::ICC_PROFILE),
    ] {
        row.insert(name.into(), json!(flags.has(flag)));
    }
    row.insert("known_urls".into(), json!(m.urls));
    if opts.detailed_urls {
        row.insert(
            "detailed_known_urls".into(),
            detailed_urls(snapshot, &m.urls),
        );
    }
    row.insert("ipfs_multihashes".into(), json!({}));

    let mut ratings = Map::new();
    for service in services.all() {
        let value = match (&service.kind, m.ratings.get(&service.id)) {
            (ServiceKind::RatingLike(_), Some(Rating::Fraction(v))) => json!(*v > 0.5),
            (ServiceKind::RatingNumerical(c), Some(Rating::Fraction(v))) => json!(c.stars(*v)),
            (ServiceKind::RatingIncDec(_), Some(Rating::IncDec(v))) => json!(v),
            (ServiceKind::RatingIncDec(_), _) => json!(0),
            (k, _) if k.service_type().is_rating_service() => Json::Null,
            _ => continue,
        };
        ratings.insert(service.key.to_hex(), value);
    }
    row.insert("ratings".into(), Json::Object(ratings));

    row.insert("tags".into(), tags_json(snapshot, m, tag_names));

    let mut viewing = Vec::new();
    for canvas in [
        CanvasType::MediaViewer,
        CanvasType::Preview,
        CanvasType::ClientApi,
    ] {
        let stats = m.viewing.iter().find(|v| v.canvas == canvas);
        viewing.push(json!({
            "canvas_type": canvas.code(),
            "canvas_type_pretty": canvas_name(canvas),
            "views": stats.map_or(0, |s| s.views),
            "viewtime": stats.map_or(0.0, |s| s.viewtime_ms as f64 / 1000.0),
            "last_viewed_timestamp": stats.and_then(|s| s.last_viewed).map(TimestampMs::as_secs_f64),
        }));
    }
    row.insert("file_viewing_statistics".into(), json!(viewing));
    Json::Object(row)
}

/// Per tag service (and "all known tags"): storage and display tags by status.
fn tags_json(snapshot: &Snapshot, m: &MediaResult, names: &TagNames<'_>) -> Json {
    let services = &snapshot.services;
    let mut out = Map::new();
    let mut all_storage: BTreeMap<ContentStatus, BTreeSet<TagId>> = BTreeMap::new();
    let mut all_display: BTreeMap<ContentStatus, BTreeSet<TagId>> = BTreeMap::new();
    let entry = |name: &str,
                 service_type: ServiceType,
                 storage: &BTreeMap<_, _>,
                 display: &BTreeMap<_, _>| {
        json!({
            "name": name,
            "type": service_type.code(),
            "type_pretty": service_type.name(),
            "storage_tags": statuses_json(storage, names),
            "display_tags": statuses_json(display, names),
        })
    };
    for service in services.tag_services() {
        let graph = snapshot.display.get(service.id);
        let mut storage: BTreeMap<ContentStatus, BTreeSet<TagId>> = BTreeMap::new();
        let mut display: BTreeMap<ContentStatus, BTreeSet<TagId>> = BTreeMap::new();
        if let Some(service_tags) = m.tags.get(&service.id) {
            for (status, ids) in &service_tags.by_status {
                storage
                    .entry(*status)
                    .or_default()
                    .extend(ids.iter().copied());
                let shown = display.entry(*status).or_default();
                match status {
                    ContentStatus::Current | ContentStatus::Pending => {
                        shown.extend(ids.iter().flat_map(|t| graph.display_tags(*t)));
                    }
                    // siblings and parents only apply to tags a file has
                    ContentStatus::Deleted | ContentStatus::Petitioned => {
                        shown.extend(ids.iter().copied());
                    }
                }
            }
        }
        for (status, ids) in &storage {
            all_storage.entry(*status).or_default().extend(ids);
        }
        for (status, ids) in &display {
            all_display.entry(*status).or_default().extend(ids);
        }
        out.insert(
            service.key.to_hex(),
            entry(&service.name, service.service_type(), &storage, &display),
        );
    }
    if let Some(all_known) = services.of_type(ServiceType::CombinedTag).next() {
        out.insert(
            all_known.key.to_hex(),
            entry(
                &all_known.name,
                ServiceType::CombinedTag,
                &all_storage,
                &all_display,
            ),
        );
    }
    Json::Object(out)
}

fn canvas_name(canvas: CanvasType) -> &'static str {
    match canvas {
        CanvasType::MediaViewer => "media viewer",
        CanvasType::Preview => "preview viewer",
        CanvasType::DuplicatesFilter => "duplicates filter",
        CanvasType::ArchiveDeleteFilter => "archive/delete filter",
        CanvasType::ClientApi => "client api viewer",
        CanvasType::Dialog => "dialog",
    }
}

/// Each URL normalised and classified, as `/add_urls/get_url_info` does;
/// URLs that can't be normalised are left out, as the reference does.
fn detailed_urls(snapshot: &Snapshot, urls: &[String]) -> Json {
    let classes = &snapshot.url_classes;
    let mut out = Vec::new();
    for url in urls {
        let Ok(normalised) = classes.normalise(url, false) else {
            continue;
        };
        let capability = classes.parse_capability(&normalised);
        let mut detail = json!({
            "normalised_url": normalised,
            "url_type": capability.url_type.code(),
            "url_type_string": capability.url_type.name(),
            "match_name": capability.match_name,
            "can_parse": capability.parser.is_ok(),
        });
        if let Err(reason) = capability.parser {
            detail["cannot_parse_reason"] = json!(reason);
        }
        out.push(detail);
    }
    Json::Array(out)
}
