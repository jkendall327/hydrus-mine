//! Subscriptions (type 88) with their query headers (87), and each query's
//! history: its log container (86) holding a gallery log (67) of gallery
//! seeds (66) and a file seed cache (8) of file seeds (57); plus checker
//! options (52).
//!
//! Every version of the seeds and headers that a v688 database can hold is
//! read (older versions only lack fields, which get the reference's
//! defaults). A subscription saved before hydrus v670's import options
//! overhaul (version 1-3) has old-style import options, converted as the
//! reference converts them (see [`legacy_import_options`]).

use hydrus_core::import_options::{ImportOptionsSlice, TagImportOptions};
use hydrus_core::subscriptions::CheckerOptions;

use super::domain::{expect, nested_list};
use super::legacy_import_options;
use super::util::{
    DecodeResult, boolean, float, int, list, malformed, nested, opt_int, opt_string, string,
    strings, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const SUBSCRIPTION: SerialisableType = SerialisableType(88);
const QUERY_HEADER: SerialisableType = SerialisableType(87);
const LOG_CONTAINER: SerialisableType = SerialisableType(86);
const GALLERY_LOG: SerialisableType = SerialisableType(67);
const GALLERY_SEED: SerialisableType = SerialisableType(66);
const FILE_SEED_CACHE: SerialisableType = SerialisableType(8);
const FILE_SEED: SerialisableType = SerialisableType(57);
const CHECKER_OPTIONS: SerialisableType = SerialisableType(52);
const SERVICE_KEYS_TO_TAGS: SerialisableType = SerialisableType(77);

/// A file seed as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyFileSeed {
    /// 0: a path, 1: a URL.
    pub seed_type: i64,
    pub data: String,
    /// What makes two seeds the same: the URL normalised, or the path.
    /// `None` for a URL seed saved before version 8, which the reference
    /// normalises with its URL classes as it loads it (the data itself if
    /// that fails).
    pub data_for_comparison: Option<String>,
    pub created: i64,
    pub modified: i64,
    pub source_time: Option<i64>,
    pub status: i64,
    pub note: String,
    pub referral_url: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub external_filterable_tags: Vec<String>,
    /// `(service key hex, tags)`.
    pub external_additional_tags: Vec<(String, Vec<String>)>,
    pub primary_urls: Vec<String>,
    pub source_urls: Vec<String>,
    pub tags: Vec<String>,
    pub notes: Vec<(String, String)>,
    /// `(hash type, hex)`.
    pub hashes: Vec<(String, String)>,
}

/// A gallery seed as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyGallerySeed {
    pub url: String,
    pub can_generate_more_pages: bool,
    pub external_filterable_tags: Vec<String>,
    pub external_additional_tags: Vec<(String, Vec<String>)>,
    pub created: i64,
    pub modified: i64,
    pub status: i64,
    pub note: String,
    pub referral_url: Option<String>,
    pub request_headers: Vec<(String, String)>,
}

/// A query's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryLog {
    pub name: String,
    pub gallery_seeds: Vec<LegacyGallerySeed>,
    pub file_seeds: Vec<LegacyFileSeed>,
}

/// A subscription query.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryHeader {
    /// The name its [`QueryLog`] is stored under.
    pub log_name: String,
    pub query_text: String,
    pub display_name: Option<String>,
    pub check_now: bool,
    pub last_check_time: i64,
    pub next_check_time: i64,
    pub paused: bool,
    /// `ClientImporting.CHECKER_STATUS_*`: 0 ok, 1 dead.
    pub checker_status: i64,
    pub file_seed_compaction_number: u64,
    pub gallery_seed_compaction_number: u64,
    /// Tags for everything the query finds.
    pub tag_import_options: TagImportOptions,
    /// What of old-style options could not be converted (and why); the
    /// defaults stand in.
    pub unconverted: Option<String>,
}

/// A subscription.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacySubscription {
    pub name: String,
    pub gug_key: String,
    pub gug_name: String,
    pub queries: Vec<QueryHeader>,
    pub checker: CheckerOptions,
    pub initial_file_limit: Option<i64>,
    pub periodic_file_limit: Option<i64>,
    pub this_is_a_random_sample: bool,
    pub paused: bool,
    /// Converted as the reference converts them if saved in the old style
    /// (before version 4).
    pub import_options: ImportOptionsSlice,
    /// What of old-style options (its own or its queries') could not be
    /// converted, and why; the defaults stand in.
    pub unconverted: Vec<String>,
    pub no_work_until: i64,
    pub no_work_until_reason: String,
    pub show_a_popup_while_working: bool,
    pub publish_files_to_popup_button: bool,
    pub publish_files_to_page: bool,
    pub publish_label_override: Option<String>,
    pub merge_query_publish_events: bool,
}

fn headers(k: SerialisableType, value: &PyJson) -> DecodeResult<Vec<(String, String)>> {
    match value {
        PyJson::Object(entries) => entries
            .iter()
            .map(|(name, v)| Ok((name.clone(), string(k, v, "request header")?)))
            .collect(),
        PyJson::Null => Ok(Vec::new()),
        _ => Err(malformed(k, "request headers are not a dictionary")),
    }
}

/// `ClientTags.ServiceKeysToTags` (type 77).
pub(crate) fn service_keys_to_tags(
    k: SerialisableType,
    value: &PyJson,
) -> DecodeResult<Vec<(String, Vec<String>)>> {
    let object = nested(k, value, "service keys to tags")?;
    object.expect_kind(SERVICE_KEYS_TO_TAGS)?;
    list(SERVICE_KEYS_TO_TAGS, &object.info(), "service keys to tags")?
        .iter()
        .map(|pair| {
            let [key, tags] = tuple::<2>(SERVICE_KEYS_TO_TAGS, pair, "service tags")?;
            Ok((
                string(SERVICE_KEYS_TO_TAGS, key, "service key")?,
                strings(SERVICE_KEYS_TO_TAGS, tags, "tags")?,
            ))
        })
        .collect()
}

fn pairs(k: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<Vec<(String, String)>> {
    list(k, value, what)?
        .iter()
        .map(|pair| {
            let [a, b] = tuple::<2>(k, pair, what)?;
            Ok((string(k, a, what)?, string(k, b, what)?))
        })
        .collect()
}

/// Decode a file seed, of any version.
pub fn file_seed(object: &SerialisableObject) -> DecodeResult<LegacyFileSeed> {
    let k = FILE_SEED;
    expect(object, k, &[1, 2, 3, 4, 5, 6, 7, 8])?;
    let info = object.info();
    let items = list(k, &info, "file seed")?;
    let v = object.version;
    // (the layout each version added a field to)
    let mut i = 0;
    let mut next = |what: &str| -> DecodeResult<&PyJson> {
        let item = items
            .get(i)
            .ok_or_else(|| malformed(k, format!("file seed has no {what}")))?;
        i += 1;
        Ok(item)
    };
    let seed_type = int(k, next("type")?, "type")?;
    let data = string(k, next("data")?, "data")?;
    let data_for_comparison = if v >= 8 {
        // (the reference loads a missing one as the data itself)
        Some(
            opt_string(k, next("comparison data")?, "comparison data")?
                .unwrap_or_else(|| data.clone()),
        )
    } else if seed_type == 0 {
        // a path is its own comparison data; a URL's is its normalised form,
        // which needs the URL classes
        Some(data.clone())
    } else {
        None
    };
    let created = int(k, next("created")?, "created")?;
    let modified = int(k, next("modified")?, "modified")?;
    let source_time = opt_int(k, next("source time")?, "source time")?;
    let status = int(k, next("status")?, "status")?;
    let note = string(k, next("note")?, "note")?;
    let referral_url = if v >= 2 {
        opt_string(k, next("referral url")?, "referral url")?
    } else {
        None
    };
    let request_headers = if v >= 7 {
        headers(k, next("request headers")?)?
    } else {
        Vec::new()
    };
    let external_filterable_tags = if v >= 4 {
        strings(
            k,
            next("external filterable tags")?,
            "external filterable tags",
        )?
    } else {
        Vec::new()
    };
    let external_additional_tags = if v >= 3 {
        service_keys_to_tags(k, next("external additional tags")?)?
    } else {
        Vec::new()
    };
    let primary_urls = strings(k, next("urls")?, "urls")?;
    let source_urls = if v >= 5 {
        strings(k, next("source urls")?, "source urls")?
    } else {
        Vec::new()
    };
    let tags = strings(k, next("tags")?, "tags")?;
    let notes = if v >= 6 {
        pairs(k, next("notes")?, "notes")?
    } else {
        Vec::new()
    };
    let hashes = pairs(k, next("hashes")?, "hashes")?;
    Ok(LegacyFileSeed {
        seed_type,
        data,
        data_for_comparison,
        created,
        modified,
        source_time,
        status,
        note,
        referral_url,
        request_headers,
        external_filterable_tags,
        external_additional_tags,
        primary_urls,
        source_urls,
        tags,
        notes,
        hashes,
    })
}

/// Decode a gallery seed, of any version.
pub fn gallery_seed(object: &SerialisableObject) -> DecodeResult<LegacyGallerySeed> {
    let k = GALLERY_SEED;
    expect(object, k, &[1, 2, 3, 4])?;
    let info = object.info();
    let items = list(k, &info, "gallery seed")?;
    let v = object.version;
    let mut i = 0;
    let mut next = |what: &str| -> DecodeResult<&PyJson> {
        let item = items
            .get(i)
            .ok_or_else(|| malformed(k, format!("gallery seed has no {what}")))?;
        i += 1;
        Ok(item)
    };
    let url = string(k, next("url")?, "url")?;
    let can_generate_more_pages = boolean(
        k,
        next("can generate more pages")?,
        "can generate more pages",
    )?;
    let external_filterable_tags = if v >= 3 {
        strings(
            k,
            next("external filterable tags")?,
            "external filterable tags",
        )?
    } else {
        Vec::new()
    };
    let external_additional_tags = if v >= 2 {
        service_keys_to_tags(k, next("external additional tags")?)?
    } else {
        Vec::new()
    };
    let created = int(k, next("created")?, "created")?;
    let modified = int(k, next("modified")?, "modified")?;
    let status = int(k, next("status")?, "status")?;
    let note = string(k, next("note")?, "note")?;
    let referral_url = opt_string(k, next("referral url")?, "referral url")?;
    let request_headers = if v >= 4 {
        headers(k, next("request headers")?)?
    } else {
        Vec::new()
    };
    Ok(LegacyGallerySeed {
        url,
        can_generate_more_pages,
        external_filterable_tags,
        external_additional_tags,
        created,
        modified,
        status,
        note,
        referral_url,
        request_headers,
    })
}

/// Decode a file seed cache (8). Versions 1-7 (a list of `[text, info
/// dictionary]` rows, from before file seeds were objects) are upgraded as
/// `FileSeedCache._UpdateSerialisableInfo` does.
pub fn file_seed_cache(cache: &SerialisableObject) -> DecodeResult<Vec<LegacyFileSeed>> {
    expect(cache, FILE_SEED_CACHE, &[1, 2, 3, 4, 5, 6, 7, 8])?;
    if cache.version < 8 {
        return old_file_seed_cache(cache);
    }
    nested_list(FILE_SEED_CACHE, &cache.info(), "file seed cache")?
        .iter()
        .map(file_seed)
        .collect()
}

fn old_file_seed_cache(cache: &SerialisableObject) -> DecodeResult<Vec<LegacyFileSeed>> {
    let k = FILE_SEED_CACHE;
    let version = cache.version;
    let info = cache.info();
    let mut seen = std::collections::HashSet::new();
    let mut identities = std::collections::HashSet::new();
    let mut out = Vec::new();
    for row in list(k, &info, "file seed cache")? {
        let [text, fields] = tuple::<2>(k, row, "file seed row")?;
        let mut text = string(k, text, "file seed text")?;
        // (version 4 dropped repeats, keeping the first)
        if version <= 4 && !seen.insert(text.clone()) {
            continue;
        }
        let PyJson::Object(fields) = fields else {
            return Err(malformed(k, "file seed info is not a dictionary"));
        };
        let field = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
                .ok_or_else(|| malformed(k, format!("file seed info has no {name}")))
        };
        if version <= 6 {
            text = text.replace("//media.tumblr.com", "//data.tumblr.com");
        }
        let seed_type = i64::from(text.starts_with("http"));
        // (a cache cannot hold a seed twice: the reference drops repeats,
        // keeping the first, the first time it indexes them)
        if !identities.insert((seed_type, text.clone())) {
            continue;
        }
        let note = field("note")?;
        // (version 1 notes could be anything; the upgrade `str()`ed them)
        let note = match note {
            PyJson::Str(note) => note.clone(),
            other if version == 1 => other
                .py_str()
                .ok_or_else(|| malformed(k, "file seed note cannot be converted to text"))?,
            _ => return Err(malformed(k, "file seed note is not text")),
        };
        let source_time = if version <= 5 {
            None
        } else {
            opt_int(k, field("source_timestamp")?, "source time")?
        };
        out.push(LegacyFileSeed {
            seed_type,
            data_for_comparison: (seed_type == 0).then(|| text.clone()),
            data: text,
            created: int(k, field("added_timestamp")?, "created")?,
            modified: int(k, field("last_modified_timestamp")?, "modified")?,
            source_time,
            status: int(k, field("status")?, "status")?,
            note,
            referral_url: None,
            request_headers: Vec::new(),
            external_filterable_tags: Vec::new(),
            external_additional_tags: Vec::new(),
            primary_urls: Vec::new(),
            source_urls: Vec::new(),
            tags: Vec::new(),
            notes: Vec::new(),
            hashes: Vec::new(),
        });
    }
    Ok(out)
}

/// Decode a gallery log (67).
pub fn gallery_seed_log(log: &SerialisableObject) -> DecodeResult<Vec<LegacyGallerySeed>> {
    expect(log, GALLERY_LOG, &[1])?;
    nested_list(GALLERY_LOG, &log.info(), "gallery log")?
        .iter()
        .map(gallery_seed)
        .collect()
}

/// Decode a query's log container.
pub fn query_log(object: &SerialisableObject) -> DecodeResult<QueryLog> {
    let k = LOG_CONTAINER;
    expect(object, k, &[1])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(k, "query log has no name"))?;
    let info = object.info();
    let [gallery_log, file_seed_cache_value] = tuple::<2>(k, &info, "query log")?;
    let gallery_seeds = gallery_seed_log(&nested(k, gallery_log, "gallery log")?)?;
    let file_seeds = file_seed_cache(&nested(k, file_seed_cache_value, "file seed cache")?)?;
    Ok(QueryLog {
        name,
        gallery_seeds,
        file_seeds,
    })
}

/// Decode checker options.
pub fn checker_options(object: &SerialisableObject) -> DecodeResult<CheckerOptions> {
    let k = CHECKER_OPTIONS;
    expect(object, k, &[1])?;
    let info = object.info();
    let [per_check, faster, slower, death] = tuple::<4>(k, &info, "checker options")?;
    let [death_files, death_period] = tuple::<2>(k, death, "death file velocity")?;
    Ok(CheckerOptions {
        intended_files_per_check: float(k, per_check, "intended files per check")?,
        never_faster_than: int(k, faster, "never faster than")?,
        never_slower_than: int(k, slower, "never slower than")?,
        death_file_velocity: (
            int(k, death_files, "death files")?,
            int(k, death_period, "death period")?,
        ),
    })
}

/// Decode a query header, of any version.
pub fn query_header(object: &SerialisableObject) -> DecodeResult<QueryHeader> {
    let k = QUERY_HEADER;
    expect(object, k, &[1, 2, 3])?;
    let info = object.info();
    let items = list(k, &info, "query header")?;
    let v = object.version;
    let mut i = 0;
    let mut next = |what: &str| -> DecodeResult<&PyJson> {
        let item = items
            .get(i)
            .ok_or_else(|| malformed(k, format!("query header has no {what}")))?;
        i += 1;
        Ok(item)
    };
    let log_name = string(k, next("log name")?, "log name")?;
    let query_text = string(k, next("query text")?, "query text")?;
    let display_name = opt_string(k, next("display name")?, "display name")?;
    let check_now = boolean(k, next("check now")?, "check now")?;
    let last_check_time = int(k, next("last check time")?, "last check time")?;
    let next_check_time = int(k, next("next check time")?, "next check time")?;
    let paused = boolean(k, next("paused")?, "paused")?;
    let checker_status = int(k, next("checker status")?, "checker status")?;
    next("log container status")?;
    next("file seed cache status")?;
    let (file_seed_compaction_number, gallery_seed_compaction_number) = if v >= 2 {
        let files = int(k, next("file compaction")?, "file compaction")?;
        let galleries = int(k, next("gallery compaction")?, "gallery compaction")?;
        (
            u64::try_from(files).unwrap_or(250),
            u64::try_from(galleries).unwrap_or(100),
        )
    } else {
        (250, 100)
    };
    let tag_options = next("tag import options")?;
    let tag_options = nested(k, tag_options, "tag import options")?;
    let mut unconverted = None;
    let tag_import_options = if v >= 3 {
        super::import_options::tags(&tag_options)?
    } else {
        // old-style tag import options, of which only the tags are kept (as
        // version 3 did)
        match legacy_import_options::tag_import_options(&tag_options) {
            Ok(options) => options.tags,
            Err(e) => {
                unconverted = Some(format!("tag import options ({e})"));
                TagImportOptions::default()
            }
        }
    };
    Ok(QueryHeader {
        log_name,
        query_text,
        display_name,
        check_now,
        last_check_time,
        next_check_time,
        paused,
        checker_status,
        file_seed_compaction_number,
        gallery_seed_compaction_number,
        tag_import_options,
        unconverted,
    })
}

/// Decode a subscription, of any version.
pub fn subscription(object: &SerialisableObject) -> DecodeResult<LegacySubscription> {
    let k = SUBSCRIPTION;
    expect(object, k, &[1, 2, 3, 4])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(k, "subscription has no name"))?;
    let info = object.info();
    let items = list(k, &info, "subscription")?;
    let v = object.version;
    let mut i = 0;
    let mut next = |what: &str| -> DecodeResult<&PyJson> {
        let item = items
            .get(i)
            .ok_or_else(|| malformed(k, format!("subscription has no {what}")))?;
        i += 1;
        Ok(item)
    };
    let [gug_key, gug_name] = tuple::<2>(k, next("gug")?, "gug key and name")?;
    let gug_key = string(k, gug_key, "gug key")?;
    let gug_name = string(k, gug_name, "gug name")?;
    let queries: Vec<QueryHeader> = list(k, next("query headers")?, "query headers")?
        .iter()
        .map(|q| query_header(&nested(k, q, "query header")?))
        .collect::<DecodeResult<_>>()?;
    let checker = checker_options(&nested(k, next("checker options")?, "checker options")?)?;
    let initial_file_limit = opt_int(k, next("initial file limit")?, "initial file limit")?;
    let periodic_file_limit = opt_int(k, next("periodic file limit")?, "periodic file limit")?;
    let this_is_a_random_sample = if v >= 2 {
        boolean(k, next("random sample")?, "random sample")?
    } else {
        false
    };
    let paused = boolean(k, next("paused")?, "paused")?;
    let mut unconverted: Vec<String> = Vec::new();
    let import_options = if v >= 4 {
        super::import_options::slice(&nested(k, next("import options")?, "import options")?)?
    } else {
        // file and tag import options, and (from version 3) note import
        // options, in the old style
        let file = nested(k, next("file import options")?, "file import options")?;
        let tags = nested(k, next("tag import options")?, "tag import options")?;
        let notes = if v >= 3 {
            Some(nested(
                k,
                next("note import options")?,
                "note import options",
            )?)
        } else {
            None
        };
        let converted = (|| -> DecodeResult<ImportOptionsSlice> {
            let notes = notes
                .map(|n| legacy_import_options::note_import_options(&n))
                .transpose()?;
            Ok(legacy_import_options::convert(
                Some(&legacy_import_options::file_import_options(&file)?),
                Some(&legacy_import_options::tag_import_options(&tags)?),
                notes.as_ref(),
            ))
        })();
        converted.unwrap_or_else(|e| {
            unconverted.push(format!("import options ({e})"));
            ImportOptionsSlice::default()
        })
    };
    let no_work_until = int(k, next("no work until")?, "no work until")?;
    let no_work_until_reason = string(k, next("no work until reason")?, "no work until reason")?;
    let b = |value: &PyJson, what: &str| boolean(k, value, what);
    unconverted.extend(queries.iter().filter_map(|q| {
        q.unconverted
            .as_ref()
            .map(|what| format!("the {what} of query \"{}\"", q.query_text))
    }));
    Ok(LegacySubscription {
        unconverted,
        name,
        gug_key,
        gug_name,
        queries,
        checker,
        initial_file_limit,
        periodic_file_limit,
        this_is_a_random_sample,
        paused,
        import_options,
        no_work_until,
        no_work_until_reason,
        show_a_popup_while_working: b(next("show popup")?, "show popup")?,
        publish_files_to_popup_button: b(next("publish to popup")?, "publish to popup")?,
        publish_files_to_page: b(next("publish to page")?, "publish to page")?,
        publish_label_override: opt_string(k, next("label override")?, "label override")?,
        merge_query_publish_events: b(next("merge publish events")?, "merge publish events")?,
    })
}
